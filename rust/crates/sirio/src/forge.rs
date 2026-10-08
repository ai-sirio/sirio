//! The host's side of the change-request seam
//! (`sirio_ui::forge_source::ChangeRequestSource`): which forge a worktree's
//! remote is on, how Sirio signs in there, and the one `ForgeClient` per
//! project the UI reads through. What the user decided — a host's forge,
//! its means — is stored under the settings key `forge.hosts`; a token lives
//! in the credential store under `forge:<host>`, never anywhere else.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use sirio_forge::{
    ChangeRef, CliProgram, CliTransport, Forge, ForgeClient, ForgeError, ForgeTarget, HostSetting,
    Means, Resolution, Revisions, SystemProbes, TokenTransport, Transport, parse_remote_url,
    resolve,
};
use sirio_git::{FetchError, FetchRefspec};
use sirio_ui::forge_source::{
    ChangeRequestSource, Connection, HostRow, ReadyConnection, RevisionError, TokenWrite,
};
use sirio_usage::CredentialStore;

use crate::session::SessionStore;

pub(crate) fn source_owner(
    forge: Forge,
    listed: &ForgeTarget,
    origin: Option<&ForgeTarget>,
) -> Option<String> {
    let origin = origin?;
    if origin.host != listed.host || origin.project == listed.project {
        return None;
    }
    match forge {
        Forge::GitHub => origin.project.split('/').next().map(str::to_string),
        Forge::GitLab => Some(origin.project.clone()),
    }
}

pub(crate) const REVISION_REF_PREFIX: &str = "refs/sirio/change-requests/";

/// The remote of `remotes` (name, configured URL) that is the forge project
/// `host`/`project`: `upstream` first, then `origin`, then the rest by name.
pub(crate) fn choose_remote(remotes: &[(String, String)], host: &str, project: &str) -> Option<String> {
    let mut matching: Vec<&str> = remotes
        .iter()
        .filter(|(_, url)| {
            parse_remote_url(url).is_some_and(|target| {
                target.host.eq_ignore_ascii_case(host) && target.project.eq_ignore_ascii_case(project)
            })
        })
        .map(|(name, _)| name.as_str())
        .collect();
    matching.sort_by_key(|name| {
        let rank = match *name {
            "upstream" => 0,
            "origin" => 1,
            _ => 2,
        };
        (rank, name.to_string())
    });
    matching.first().map(|name| name.to_string())
}

/// `refs/sirio/change-requests/<remote>/<N>/<end>`; `end` is `head` or `base`.
pub(crate) fn revision_ref(remote: &str, number: u64, end: &str) -> String {
    format!("{REVISION_REF_PREFIX}{remote}/{number}/{end}")
}

/// `(remote, number)` of one of the refs [`revision_ref`] writes; anything
/// else — another namespace, a non-numeric or signed number, another end — is
/// `None`, and so is never deleted. Read from the right: a remote's name may
/// itself contain a slash.
pub(crate) fn parse_revision_ref(name: &str) -> Option<(String, u64)> {
    let rest = name.strip_prefix(REVISION_REF_PREFIX)?;
    let (rest, end) = rest.rsplit_once('/')?;
    if !matches!(end, "head" | "base") {
        return None;
    }
    let (remote, number) = rest.rsplit_once('/')?;
    if remote.is_empty() || number.is_empty() || !number.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    Some((remote.to_string(), number.parse().ok()?))
}

/// 120 s; `SIRIO_FORGE_FETCH_TIMEOUT_MS` shortens it in debug builds only,
/// the way `SIRIO_UPDATE_MANIFEST_URL` is honoured, so a shipped binary has
/// no such lever and the E2E can prove the timeout without waiting two minutes.
fn fetch_timeout() -> Duration {
    #[cfg(debug_assertions)]
    if let Some(millis) = std::env::var("SIRIO_FORGE_FETCH_TIMEOUT_MS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
    {
        return Duration::from_millis(millis);
    }
    Duration::from_secs(120)
}

/// Makes a change request's commits local, once at a time per change request.
#[derive(Default)]
pub(crate) struct RevisionFetcher {
    flights: Mutex<HashMap<(PathBuf, String, u64), Arc<Mutex<()>>>>,
}

impl RevisionFetcher {
    pub(crate) fn ensure(
        &self,
        worktree: &Path,
        reference: &ChangeRef,
        revisions: &Revisions,
        target_branch: Option<&str>,
    ) -> Result<(), RevisionError> {
        let local = |sha: &str| sirio_git::object_exists(worktree, sha);
        let all_local = || local(&revisions.base_sha) && local(&revisions.head_sha);
        if all_local() {
            return Ok(());
        }
        let remotes = sirio_git::list_remotes(worktree);
        let remote = choose_remote(&remotes, &reference.host, &reference.project).ok_or_else(|| {
            RevisionError::NoMatchingRemote {
                expected: format!("{}/{}", reference.host, reference.project),
            }
        })?;
        // The guarded value is `()`, so a poisoned lock protects nothing:
        // a panic while holding a turn must not close this change request
        // for the rest of the session.
        let flight = self
            .flights
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .entry((worktree.to_path_buf(), remote.clone(), reference.number))
            .or_default()
            .clone();
        let _turn = flight.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        // A request that held the turn before this one may have fetched already.
        if all_local() {
            return Ok(());
        }
        let refused = |error: FetchError| RevisionError::FetchFailed { detail: error.to_string() };
        let head_spec =
            FetchRefspec::new(&reference.head_ref(), &revision_ref(&remote, reference.number, "head")).map_err(refused)?;
        let base_spec = match target_branch {
            Some(branch) => Some(
                FetchRefspec::new(&format!("refs/heads/{branch}"), &revision_ref(&remote, reference.number, "base"))
                    .map_err(refused)?,
            ),
            None => None,
        };
        let fetch = |specs: &[FetchRefspec]| sirio_git::fetch_refs(worktree, &remote, specs, fetch_timeout());
        let both = base_spec.iter().chain(std::iter::once(&head_spec)).cloned().collect::<Vec<_>>();
        let result = match fetch(&both) {
            // git aborts a fetch as a whole when one of its refs is missing
            // on the remote, so a target branch the forge deleted (a merged
            // stacked change request, say) would take the head down with it.
            // The head ref alone usually carries the base too; when it does
            // not, the check below says which sha is gone. A timeout is not
            // retried, and the head-only error names the real problem.
            Err(FetchError::Failed { .. }) if base_spec.is_some() => fetch(std::slice::from_ref(&head_spec)),
            other => other,
        };
        result.map_err(|error| match error {
            FetchError::TimedOut => RevisionError::FetchTimedOut,
            FetchError::Failed { detail } => RevisionError::FetchFailed { detail },
        })?;
        for sha in [&revisions.head_sha, &revisions.base_sha] {
            if !local(sha) {
                return Err(RevisionError::RevisionGone { sha: sha.clone() });
            }
        }
        Ok(())
    }

    /// Deletes every revision ref no `live` change request holds. A ref only
    /// anchors objects against `git gc` — the diff and the snapshots are read
    /// by sha, and `ensure` fetches again when an object is missing — so a
    /// ref deleted under a tab that is not showing costs one refetch, never a
    /// wrong answer.
    pub(crate) fn sweep(&self, worktree: &Path, live: &[ChangeRef]) {
        let remotes = sirio_git::list_remotes(worktree);
        let keep: HashSet<(String, u64)> = live
            .iter()
            .filter_map(|reference| {
                choose_remote(&remotes, &reference.host, &reference.project)
                    .map(|remote| (remote, reference.number))
            })
            .collect();
        let Ok(refs) = sirio_git::refs_under(worktree, REVISION_REF_PREFIX) else {
            return;
        };
        for name in refs {
            if let Some(key) = parse_revision_ref(&name)
                && !keep.contains(&key)
            {
                let _ = sirio_git::delete_ref(worktree, &name);
            }
        }
    }
}

/// A token as the credential store keeps it: with the forge it was saved
/// for, so resolution can name an unknown host without probing it.
#[derive(Serialize, Deserialize)]
struct StoredToken {
    forge: Forge,
    token: String,
}

fn token_key(host: &str) -> String {
    format!("forge:{host}")
}

/// One client per host, project and means, so a client's cached viewer and
/// GitLab baseline flag survive refreshes and worktree switches.
type ClientKey = (String, String, bool);

fn client_key(target: &ForgeTarget, means: Means) -> ClientKey {
    (
        target.host.clone(),
        target.project.clone(),
        means == Means::Cli,
    )
}

mod checkout;
mod handoff;

pub(crate) use checkout::{CheckoutDone, CheckoutRequest};
pub(crate) use handoff::{HandoffAsk, HandoffDone, HandoffFailure};

/// The hub a workspace runs a checkout on. `set_source` keeps only the
/// trait object, and a checkout needs the concrete hub's git steps.
pub(crate) struct HubGlobal(pub(crate) Arc<ForgeHub>);

impl gpui::Global for HubGlobal {}

pub(crate) struct ForgeHub {
    settings: SessionStore,
    credentials: Option<CredentialStore>,
    resolutions: Mutex<HashMap<String, Resolution>>,
    clients: Mutex<HashMap<ClientKey, Arc<ForgeClient>>>,
    revisions: RevisionFetcher,
}

impl ForgeHub {
    pub(crate) fn new(settings: SessionStore, credentials: Option<CredentialStore>) -> Self {
        Self {
            settings,
            credentials,
            resolutions: Mutex::new(HashMap::new()),
            clients: Mutex::new(HashMap::new()),
            revisions: RevisionFetcher::default(),
        }
    }

    fn host_settings(&self) -> Vec<HostSetting> {
        serde_json::from_str(&self.settings.load_settings().forge_hosts).unwrap_or_default()
    }

    /// Load, change, save: `forge.hosts` is owned here, not by the Settings
    /// snapshot (see `app_settings_for_settings_save`).
    fn update_host_settings(&self, change: impl FnOnce(&mut Vec<HostSetting>)) {
        let mut settings = self.settings.load_settings();
        let mut hosts: Vec<HostSetting> =
            serde_json::from_str(&settings.forge_hosts).unwrap_or_default();
        change(&mut hosts);
        settings.forge_hosts = serde_json::to_string(&hosts).unwrap_or_else(|_| "[]".to_string());
        self.settings.save_settings(&settings);
    }

    fn stored_token(&self, host: &str) -> Option<StoredToken> {
        let raw = self.credentials.as_ref()?.get(&token_key(host))?;
        serde_json::from_str(&raw).ok()
    }

    fn resolution(&self, host: &str) -> Resolution {
        if let Some(found) = self.resolutions.lock().expect("resolutions lock").get(host) {
            return *found;
        }
        let setting = self
            .host_settings()
            .into_iter()
            .find(|setting| setting.host == host);
        let stored = self.stored_token(host).map(|stored| stored.forge);
        let resolution = resolve(host, setting.as_ref(), stored, &SystemProbes);
        self.resolutions
            .lock()
            .expect("resolutions lock")
            .insert(host.to_string(), resolution);
        resolution
    }

    fn client(
        &self,
        forge: Forge,
        means: Means,
        target: ForgeTarget,
    ) -> Result<Arc<ForgeClient>, Connection> {
        let key = client_key(&target, means);
        if let Some(client) = self.clients.lock().expect("clients lock").get(&key) {
            return Ok(client.clone());
        }
        let transport: Box<dyn Transport> = match means {
            Means::Cli => Box::new(CliTransport::new(
                CliProgram::for_forge(forge),
                &target.host,
            )),
            Means::Token => match self.stored_token(&target.host) {
                Some(stored) => Box::new(TokenTransport::new(forge, &target.host, stored.token)),
                None => {
                    return Err(Connection::NotConnected {
                        forge,
                        host: target.host,
                    });
                }
            },
        };
        let client = Arc::new(ForgeClient::new(forge, target, transport));
        self.clients
            .lock()
            .expect("clients lock")
            .insert(key, client.clone());
        Ok(client)
    }

    /// Drop what was resolved and built for `host`; the next call asks again.
    fn drop_host(&self, host: &str) {
        self.resolutions
            .lock()
            .expect("resolutions lock")
            .remove(host);
        self.clients
            .lock()
            .expect("clients lock")
            .retain(|(client_host, _, _), _| client_host != host);
    }
}

impl ChangeRequestSource for ForgeHub {
    fn connect(&self, worktree: &Path) -> Connection {
        let origin = sirio_git::remote_url(worktree, "origin");
        let listed = sirio_git::remote_url(worktree, "upstream").or_else(|| origin.clone());
        let Some(target) = listed.as_deref().and_then(parse_remote_url) else {
            return Connection::NoForgeRemote;
        };
        match self.resolution(&target.host) {
            Resolution::UnknownForge => Connection::UnknownForge { host: target.host },
            Resolution::NotConnected { forge } => Connection::NotConnected {
                forge,
                host: target.host,
            },
            Resolution::Ready { forge, means } => {
                let origin = origin.as_deref().and_then(parse_remote_url);
                let source_owner = source_owner(forge, &target, origin.as_ref());
                // A link lasts only while the worktree is on the branch it was
                // made for: a worktree switched to other work is no longer that
                // change request's checkout.
                let branch = self.current_branch(worktree);
                let linked = self
                    .settings
                    .change_request_link(worktree)
                    .filter(|(reference, linked_branch)| {
                        reference.forge == forge
                            && reference.host == target.host
                            && reference.project == target.project
                            && branch.as_deref() == Some(linked_branch.as_str())
                    })
                    .map(|(reference, _)| reference);
                match self.client(forge, means, target) {
                    Ok(client) => Connection::Ready(ReadyConnection {
                        client,
                        means,
                        branch,
                        source_owner,
                        linked,
                    }),
                    Err(connection) => connection,
                }
            }
        }
    }

    fn current_branch(&self, worktree: &Path) -> Option<String> {
        sirio_project::current_branch(worktree).ok().flatten()
    }

    fn client_for(&self, reference: &ChangeRef) -> Result<Arc<ForgeClient>, Connection> {
        match self.resolution(&reference.host) {
            Resolution::Ready { forge, means } if forge == reference.forge => self.client(
                forge,
                means,
                ForgeTarget {
                    host: reference.host.clone(),
                    project: reference.project.clone(),
                },
            ),
            Resolution::UnknownForge => Err(Connection::UnknownForge {
                host: reference.host.clone(),
            }),
            _ => Err(Connection::NotConnected {
                forge: reference.forge,
                host: reference.host.clone(),
            }),
        }
    }

    fn forget(&self, host: &str) {
        self.drop_host(host);
    }

    fn set_forge(&self, host: &str, forge: Forge) {
        self.update_host_settings(|hosts| {
            match hosts.iter_mut().find(|setting| setting.host == host) {
                Some(existing) if existing.forge != forge => {
                    existing.forge = forge;
                    existing.means = None;
                }
                Some(_) => {}
                None => hosts.push(HostSetting {
                    host: host.to_string(),
                    forge,
                    means: None,
                }),
            }
        });
        self.drop_host(host);
    }

    fn set_means(&self, host: &str, forge: Forge, means: Option<Means>) {
        self.update_host_settings(|hosts| {
            match hosts.iter_mut().find(|setting| setting.host == host) {
                Some(existing) => {
                    existing.forge = forge;
                    existing.means = means;
                }
                None => hosts.push(HostSetting {
                    host: host.to_string(),
                    forge,
                    means,
                }),
            }
        });
        self.drop_host(host);
    }

    fn save_token(&self, host: &str, forge: Forge, token: &str) -> Result<String, ForgeError> {
        let token = token.trim();
        let Some(store) = &self.credentials else {
            return Err(ForgeError::UnexpectedResponse {
                host: host.to_string(),
                detail: "there is nowhere to keep a token: no HOME or XDG_DATA_HOME".to_string(),
            });
        };
        let probe = ForgeClient::new(
            forge,
            ForgeTarget {
                host: host.to_string(),
                project: String::new(),
            },
            Box::new(TokenTransport::new(forge, host, token.to_string())),
        );
        let account = probe.viewer()?;
        let value = serde_json::to_string(&StoredToken {
            forge,
            token: token.to_string(),
        })
        .expect("a stored token serialises");
        store
            .set(&token_key(host), &value)
            .map_err(|error| ForgeError::UnexpectedResponse {
                host: host.to_string(),
                detail: error.to_string(),
            })?;
        self.drop_host(host);
        Ok(account)
    }

    fn delete_token(&self, host: &str) {
        if let Some(store) = &self.credentials {
            let _ = store.delete(&token_key(host));
        }
        self.drop_host(host);
    }

    fn hosts(&self) -> Vec<HostRow> {
        let settings = self.host_settings();
        let mut names: Vec<String> = settings
            .iter()
            .map(|setting| setting.host.clone())
            .collect();
        for host in self.resolutions.lock().expect("resolutions lock").keys() {
            if !names.contains(host) {
                names.push(host.clone());
            }
        }
        names.sort();
        names
            .into_iter()
            .map(|host| {
                let setting = settings.iter().find(|setting| setting.host == host);
                let (forge, means) = match self.resolution(&host) {
                    Resolution::Ready { forge, means } => (Some(forge), Some(means)),
                    Resolution::NotConnected { forge } => (Some(forge), None),
                    Resolution::UnknownForge => (setting.map(|setting| setting.forge), None),
                };
                let client = match (forge, means) {
                    (Some(forge), Some(means)) => self
                        .client(
                            forge,
                            means,
                            ForgeTarget {
                                host: host.clone(),
                                project: String::new(),
                            },
                        )
                        .ok(),
                    _ => None,
                };
                let account = client.as_ref().and_then(|client| client.viewer().ok());
                // Only a token has scopes to report; a CLI holds its own.
                let write = match (&client, forge, means) {
                    (Some(client), Some(forge), Some(Means::Token)) => {
                        Some(match client.token_scopes() {
                            Some(scopes) if scopes.allows_writing(forge) => TokenWrite::Yes,
                            Some(_) => TokenWrite::No,
                            None => TokenWrite::NotReported,
                        })
                    }
                    _ => None,
                };
                HostRow {
                    has_token: self.stored_token(&host).is_some(),
                    configured: setting.is_some(),
                    pinned_means: setting.and_then(|setting| setting.means),
                    host,
                    forge,
                    means,
                    account,
                    write,
                }
            })
            .collect()
    }

    fn ensure_revisions(&self, worktree: &Path, reference: &ChangeRef, revisions: &Revisions, target_branch: Option<&str>) -> Result<(), RevisionError> {
        self.revisions.ensure(worktree, reference, revisions, target_branch)
    }

    fn release_revisions(&self, worktree: &Path, live: &[ChangeRef]) {
        self.revisions.sweep(worktree, live);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(host: &str, project: &str) -> ForgeTarget {
        ForgeTarget {
            host: host.to_string(),
            project: project.to_string(),
        }
    }

    #[test]
    fn a_fork_on_github_is_named_by_its_owner() {
        let listed = target("github.com", "acme/widgets");
        let origin = target("github.com", "me/widgets");
        assert_eq!(
            source_owner(Forge::GitHub, &listed, Some(&origin)).as_deref(),
            Some("me")
        );
    }

    #[test]
    fn a_fork_on_gitlab_is_named_by_its_full_path() {
        let listed = target("gitlab.example.com", "team/sub/app");
        let origin = target("gitlab.example.com", "me/forks/app");
        assert_eq!(
            source_owner(Forge::GitLab, &listed, Some(&origin)).as_deref(),
            Some("me/forks/app")
        );
    }

    #[test]
    fn listing_origin_itself_needs_no_owner() {
        let listed = target("github.com", "acme/widgets");
        assert_eq!(
            source_owner(Forge::GitHub, &listed, Some(&listed.clone())),
            None
        );
        assert_eq!(source_owner(Forge::GitHub, &listed, None), None);
    }

    #[test]
    fn an_origin_on_another_host_says_nothing_about_this_forge() {
        let listed = target("github.com", "acme/widgets");
        let origin = target("gitlab.com", "me/widgets");
        assert_eq!(source_owner(Forge::GitHub, &listed, Some(&origin)), None);
    }

    fn remotes(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs.iter().map(|(name, url)| (name.to_string(), url.to_string())).collect()
    }

    #[test]
    fn the_matching_remote_is_chosen_upstream_then_origin_then_by_name() {
        let both = remotes(&[
            ("origin", "https://ghe.test/acme/widgets.git"),
            ("upstream", "git@ghe.test:acme/widgets.git"),
        ]);
        assert_eq!(choose_remote(&both, "ghe.test", "acme/widgets").as_deref(), Some("upstream"));
        assert_eq!(
            choose_remote(&remotes(&[("origin", "https://ghe.test/acme/widgets.git")]), "ghe.test", "acme/widgets").as_deref(),
            Some("origin")
        );
        let others = remotes(&[
            ("zed", "https://ghe.test/acme/widgets.git"),
            ("fork", "https://ghe.test/acme/widgets"),
            ("mine", "https://ghe.test/me/widgets.git"),
        ]);
        assert_eq!(choose_remote(&others, "ghe.test", "acme/widgets").as_deref(), Some("fork"));
    }

    #[test]
    fn a_remote_must_match_host_and_whole_project() {
        let list = remotes(&[
            ("origin", "https://ghe.test/acme/widgets-extra.git"),
            ("second", "https://other.test/acme/widgets.git"),
            ("local", "/srv/git/widgets.git"),
        ]);
        assert_eq!(choose_remote(&list, "ghe.test", "acme/widgets"), None);
        assert_eq!(choose_remote(&[], "ghe.test", "acme/widgets"), None);
    }

    #[test]
    fn host_and_project_compare_case_insensitively_and_subgroups_work() {
        let list = remotes(&[("origin", "https://GitHub.com/Acme/Widgets.git")]);
        assert_eq!(choose_remote(&list, "github.com", "acme/widgets").as_deref(), Some("origin"));
        let nested = remotes(&[("origin", "git@gitlab.corp:team/sub/app.git")]);
        assert_eq!(choose_remote(&nested, "gitlab.corp", "team/sub/app").as_deref(), Some("origin"));
    }

    #[test]
    fn a_revision_ref_names_its_remote_and_number_and_nothing_else_parses() {
        assert_eq!(
            parse_revision_ref("refs/sirio/change-requests/origin/7/head"),
            Some(("origin".to_string(), 7))
        );
        assert_eq!(
            parse_revision_ref("refs/sirio/change-requests/origin/7/base"),
            Some(("origin".to_string(), 7))
        );
        assert_eq!(
            parse_revision_ref("refs/sirio/change-requests/team/fork/12/head"),
            Some(("team/fork".to_string(), 12)),
            "a remote name may contain a slash"
        );
        for bad in [
            "refs/sirio/change-requests/origin/x/head",
            "refs/sirio/change-requests/origin/+7/head",
            "refs/sirio/change-requests/origin/7/tail",
            "refs/sirio/change-requests/7/head",
            "refs/sirio/change-requests//7/head",
            "refs/heads/origin/7/head",
            "refs/sirio/other/origin/7/head",
            "",
        ] {
            assert_eq!(parse_revision_ref(bad), None, "{bad:?}");
        }
    }

    // `setup()`: a forge's git side (`main`, and `refs/pull/7/head` one commit
    // ahead) and a checkout whose `origin` is *named* like the forge project but
    // *fetches* from the bare repository — `insteadOf`, which `remote_url`
    // (config --get) does not apply, so the forge is recognised and git works.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let unique = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let path = std::env::temp_dir()
                .join(format!("sirio-forge-fetch-{}-{unique}", std::process::id()));
            std::fs::create_dir_all(&path).expect("create temp dir");
            Self(std::fs::canonicalize(&path).expect("canonicalize temp dir"))
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn git(dir: &Path, args: &[&str]) -> String {
        let output = std::process::Command::new("git")
            .args(args)
            .current_dir(dir)
            .output()
            .expect("git must be installed to run these tests");
        assert!(
            output.status.success(),
            "`git {args:?}` failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    }

    fn commit_file(repo: &Path, file: &str, content: &str, message: &str) {
        std::fs::write(repo.join(file), content).expect("write fixture file");
        git(repo, &["add", "-A"]);
        git(
            repo,
            &[
                "-c", "user.email=t@example.invalid", "-c", "user.name=T",
                "-c", "commit.gpgSign=false", "commit", "-q", "-m", message,
            ],
        );
    }

    struct Setup {
        _dirs: Vec<TempDir>,
        work: PathBuf,
        base: String,
        head: String,
    }

    fn setup() -> Setup {
        let source = TempDir::new();
        git(&source.0, &["init", "-q", "-b", "main"]);
        commit_file(&source.0, "a.txt", "one\n", "first");
        commit_file(&source.0, "b.txt", "two\n", "second");
        let base = git(&source.0, &["rev-parse", "HEAD"]);
        git(&source.0, &["checkout", "-q", "-b", "pr"]);
        commit_file(&source.0, "c.txt", "three\n", "the change request");
        let head = git(&source.0, &["rev-parse", "HEAD"]);
        let holder = TempDir::new();
        let bare = holder.0.join("forge.git");
        git(
            &holder.0,
            &["clone", "-q", "--bare", source.0.to_str().unwrap(), bare.to_str().unwrap()],
        );
        git(&bare, &["update-ref", "refs/pull/7/head", &head]);
        git(&bare, &["update-ref", "-d", "refs/heads/pr"]);
        let work = TempDir::new();
        git(&work.0, &["init", "-q", "-b", "main"]);
        git(&work.0, &["config", "credential.helper", ""]);
        git(&work.0, &["remote", "add", "origin", "https://ghe.test/acme/widgets.git"]);
        git(
            &work.0,
            &[
                "config",
                &format!("url.{}.insteadOf", bare.display()),
                "https://ghe.test/acme/widgets.git",
            ],
        );
        let work_path = work.0.clone();
        Setup { _dirs: vec![source, holder, work], work: work_path, base, head }
    }

    fn reference() -> sirio_forge::ChangeRef {
        sirio_forge::ChangeRef { forge: Forge::GitHub, host: "ghe.test".into(), project: "acme/widgets".into(), number: 7 }
    }

    fn revisions_of(setup: &Setup) -> sirio_forge::Revisions {
        sirio_forge::Revisions { base_sha: setup.base.clone(), head_sha: setup.head.clone(), start_sha: None }
    }

    #[test]
    fn making_revisions_local_fetches_them_into_sirio_refs_and_names_what_it_cannot() {
        let setup = setup();
        let fetcher = RevisionFetcher::default();
        fetcher
            .ensure(&setup.work, &reference(), &revisions_of(&setup), Some("main"))
            .expect("fetch");
        assert!(sirio_git::object_exists(&setup.work, &setup.head));
        assert_eq!(
            sirio_git::refs_under(&setup.work, REVISION_REF_PREFIX).expect("refs"),
            vec![
                "refs/sirio/change-requests/origin/7/base".to_string(),
                "refs/sirio/change-requests/origin/7/head".to_string(),
            ]
        );
        // Both are local now: a second call has nothing to do, so it must not fail even with no remote to ask.
        git(&setup.work, &["remote", "remove", "origin"]);
        fetcher
            .ensure(&setup.work, &reference(), &revisions_of(&setup), Some("main"))
            .expect("nothing was missing, so nothing was fetched");
    }

    #[test]
    fn a_revision_the_forge_does_not_have_is_gone_and_a_stranger_project_has_no_remote() {
        let setup = setup();
        let fetcher = RevisionFetcher::default();
        // First, while nothing is local: once a fetch has brought both commits
        // in, `ensure` has nothing to do and never looks for a remote.
        let mut stranger = reference();
        stranger.project = "someone/else".to_string();
        assert!(matches!(
            fetcher.ensure(&setup.work, &stranger, &revisions_of(&setup), Some("main")),
            Err(RevisionError::NoMatchingRemote { .. })
        ));
        let mut missing = revisions_of(&setup);
        missing.base_sha = "0123456789abcdef0123456789abcdef01234567".to_string();
        assert_eq!(
            fetcher.ensure(&setup.work, &reference(), &missing, Some("main")),
            Err(RevisionError::RevisionGone { sha: missing.base_sha.clone() })
        );
    }

    /// A merged stacked pull request whose base branch was deleted: git
    /// refuses the whole two-refspec fetch, so the head is fetched alone and
    /// the base, an ancestor of it here, comes along.
    #[test]
    fn a_target_branch_the_forge_no_longer_has_still_lets_the_head_be_fetched() {
        let setup = setup();
        let fetcher = RevisionFetcher::default();
        fetcher
            .ensure(&setup.work, &reference(), &revisions_of(&setup), Some("a-branch-that-does-not-exist"))
            .expect("the head ref alone brings both commits in");
        assert!(sirio_git::object_exists(&setup.work, &setup.head));
        assert!(sirio_git::object_exists(&setup.work, &setup.base));
        assert_eq!(
            sirio_git::refs_under(&setup.work, REVISION_REF_PREFIX).expect("refs"),
            vec!["refs/sirio/change-requests/origin/7/head".to_string()],
            "the head ref is written, no base ref"
        );
    }

    /// A restored snapshot knows no target branch (§8): only the head ref is
    /// fetched, and nothing is written for the base.
    #[test]
    fn without_a_target_branch_only_the_head_ref_is_fetched() {
        let setup = setup();
        let fetcher = RevisionFetcher::default();
        fetcher
            .ensure(&setup.work, &reference(), &revisions_of(&setup), None)
            .expect("fetch");
        assert_eq!(
            sirio_git::refs_under(&setup.work, REVISION_REF_PREFIX).expect("refs"),
            vec!["refs/sirio/change-requests/origin/7/head".to_string()]
        );
        assert!(sirio_git::object_exists(&setup.work, &setup.base), "the base is an ancestor of the head");
    }

    #[test]
    fn sweeping_deletes_only_the_refs_no_live_change_request_holds() {
        let setup = setup();
        let fetcher = RevisionFetcher::default();
        fetcher.ensure(&setup.work, &reference(), &revisions_of(&setup), Some("main")).expect("fetch");
        // The same objects again, under another number, so there is something to keep and something to drop.
        git(&setup.work, &["update-ref", "refs/sirio/change-requests/origin/8/head", &setup.head]);
        fetcher.sweep(&setup.work, &[reference()]);
        assert_eq!(
            sirio_git::refs_under(&setup.work, REVISION_REF_PREFIX).expect("refs"),
            vec![
                "refs/sirio/change-requests/origin/7/base".to_string(),
                "refs/sirio/change-requests/origin/7/head".to_string(),
            ]
        );
        fetcher.sweep(&setup.work, &[]);
        assert!(sirio_git::refs_under(&setup.work, REVISION_REF_PREFIX).expect("refs").is_empty());
        assert!(sirio_git::object_exists(&setup.work, &setup.head), "deleting a ref never deletes the objects");
    }
}
