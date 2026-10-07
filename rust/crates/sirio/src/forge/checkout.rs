//! Checking a change request out into a worktree (change requests C1, spec
//! C §5): read the forge, gather what git knows, let `handoff::decide`
//! choose, then run the git steps. Runs off the GPUI thread. Nothing it
//! already created is deleted when a later step fails.

use std::path::{Path, PathBuf};

use sirio_forge::ChangeRef;
use sirio_git::{FastForward, FetchError, RemoteOutcome};
use sirio_ui::forge_source::ChangeRequestSource;
use sirio_ui::handoff::{self, BranchFacts, Facts, ListedRemote, Plan, PushTarget, Start, Target};

use super::{ForgeHub, choose_remote, fetch_timeout};

pub(crate) struct CheckoutRequest {
    pub(crate) reference: ChangeRef,
    /// The project's main checkout, where git runs.
    pub(crate) repo: PathBuf,
    pub(crate) project_name: String,
    pub(crate) location_override: Option<PathBuf>,
}

pub(crate) struct CheckoutDone {
    pub(crate) path: PathBuf,
    pub(crate) branch: String,
    pub(crate) title: String,
    pub(crate) detail: String,
}

fn fetch_message(error: FetchError) -> String {
    match error {
        FetchError::TimedOut => "the fetch timed out".to_string(),
        FetchError::Failed { detail } => detail,
    }
}

fn name_of(path: &Path) -> String {
    path.file_name().map_or_else(|| path.display().to_string(), |name| name.to_string_lossy().into_owned())
}

impl ForgeHub {
    pub(crate) fn checkout(&self, request: &CheckoutRequest) -> Result<CheckoutDone, String> {
        let reference = &request.reference;
        let repo = request.repo.as_path();
        let client = self
            .client_for(reference)
            .map_err(|_| format!("Sirio is not connected to {}", reference.host))?;
        let header = client
            .header(reference.number)
            .map_err(|error| format!("could not read {}: {error}", reference.label()))?;
        let revisions = header
            .revisions
            .clone()
            .ok_or_else(|| format!("{} names no head commit", reference.label()))?;

        let remotes = sirio_git::list_remotes(repo);
        let listed_name = choose_remote(&remotes, &reference.host, &reference.project)
            .ok_or_else(|| format!("no remote of this project points at {}", reference.project))?;
        let listed = ListedRemote {
            url: remotes.iter().find(|(name, _)| *name == listed_name).map(|(_, url)| url.clone()).unwrap_or_default(),
            name: listed_name,
        };
        let target = handoff::target(&header.summary.source_branch, &listed, header.head.as_ref());

        // The head commit, through B1's fetch, so ancestry can be judged
        // before anything is created.
        self.revisions
            .ensure(repo, reference, &revisions, None)
            .map_err(|error| format!("could not fetch the head of {}: {error}", reference.label()))?;

        let facts = self.facts(request, &target, &remotes, &revisions.head_sha)?;
        let done = |path: PathBuf, detail: String| CheckoutDone {
            branch: target.branch.clone(),
            title: header.summary.title.clone(),
            path,
            detail,
        };
        match handoff::decide(&target, &facts) {
            Plan::Refuse(refusal) => Err(refusal.message()),
            Plan::Reuse { worktree, .. } => {
                let detail = self.sync(repo, &worktree, &target)?;
                Ok(done(worktree, detail))
            }
            Plan::Create { path, branch, start, add_remote } => {
                if let Some((name, url)) = &add_remote {
                    match sirio_git::ensure_remote(repo, name, url) {
                        Ok(RemoteOutcome::Added | RemoteOutcome::AlreadyThere) => {}
                        Ok(RemoteOutcome::Conflict { existing }) => {
                            return Err(format!("the remote {name} already points at {existing}"));
                        }
                        Err(error) => return Err(format!("adding the remote {name} failed: {error}")),
                    }
                }
                if let Some((remote, remote_branch)) = target.remote_and_branch() {
                    sirio_git::fetch_branch(repo, remote, remote_branch, fetch_timeout())
                        .map_err(|error| format!("fetching {remote}/{remote_branch} failed: {}", fetch_message(error)))?;
                }
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent)
                        .map_err(|error| format!("creating {} failed: {error}", parent.display()))?;
                }
                let detail = match &start {
                    Start::Track { upstream } => {
                        sirio_git::create_worktree_tracking(repo, &branch, &path, upstream)
                            .map_err(|error| format!("creating the worktree failed: {error}"))?;
                        format!("Created {} on {branch}, tracking {upstream}", name_of(&path))
                    }
                    Start::At { commit } => {
                        sirio_git::create_worktree_at(repo, &branch, &path, commit)
                            .map_err(|error| format!("creating the worktree failed: {error}"))?;
                        let reason = match &target.push {
                            PushTarget::ReadOnly(reason) => reason.message(),
                            _ => "",
                        };
                        format!("Created {} on {branch} (read-only: {reason})", name_of(&path))
                    }
                    Start::Existing { set_upstream } => {
                        sirio_git::create_worktree(repo, &branch, &path, None)
                            .map_err(|error| format!("creating the worktree failed: {error}"))?;
                        if let Some(upstream) = set_upstream {
                            sirio_git::set_upstream(repo, &branch, upstream)
                                .map_err(|error| format!("setting the upstream failed: {error}"))?;
                        }
                        let to = target.upstream().unwrap_or_else(|| revisions.head_sha.clone());
                        let moved = sirio_git::fast_forward(&path, &to)
                            .map_err(|error| format!("fast-forwarding failed: {error}"))?;
                        format!("Created {} on the existing {branch}{}", name_of(&path), sync_words(&moved, &to))
                    }
                };
                self.map_fork_push(repo, &target)?;
                Ok(done(path, detail))
            }
        }
    }

    fn facts(
        &self,
        request: &CheckoutRequest,
        target: &Target,
        remotes: &[(String, String)],
        head_sha: &str,
    ) -> Result<Facts, String> {
        let repo = request.repo.as_path();
        let mut linked = None;
        for path in self.settings.linked_worktrees(&request.reference) {
            if path.exists() {
                linked = Some(path);
                break;
            }
            // A linked worktree deleted outside Sirio: forget it.
            self.settings.drop_change_request_link(&path);
        }
        let checked_out = sirio_git::worktree_for_branch(repo, &target.branch)
            .map_err(|error| format!("reading the worktrees failed: {error}"))?
            .map(|path| {
                let upstream = sirio_git::upstream_of(repo, &target.branch)
                    .ok()
                    .flatten()
                    .map(|upstream| format!("{}/{}", upstream.remote, upstream.branch));
                (path, upstream)
            });
        let branch = if checked_out.is_some() {
            None
        } else {
            sirio_git::local_branch(repo, &target.branch)
                .map_err(|error| format!("reading the branch {} failed: {error}", target.branch))?
                .map(|local| BranchFacts {
                    reaches_head: local.sha == head_sha
                        || sirio_git::is_ancestor(repo, &local.sha, head_sha).unwrap_or(false),
                    upstream: local.upstream.map(|upstream| format!("{}/{}", upstream.remote, upstream.branch)),
                    sha: local.sha,
                })
        };
        let fork_remote_url = match &target.push {
            PushTarget::Fork { remote, .. } => {
                remotes.iter().find(|(name, _)| name == remote).map(|(_, url)| url.clone())
            }
            _ => None,
        };
        let parent = sirio_git::resolve_parent_directory(repo, request.location_override.as_deref());
        let new_path = sirio_git::derive_worktree_path(
            &parent,
            &request.project_name,
            &handoff::worktree_dir_branch(&target.branch),
        );
        Ok(Facts {
            linked,
            checked_out,
            branch,
            fork_remote_url,
            new_path_taken: new_path.exists(),
            new_path,
            head_sha: head_sha.to_string(),
        })
    }

    /// Brings a reused worktree up to its upstream when it is clean and behind.
    fn sync(&self, repo: &Path, worktree: &Path, target: &Target) -> Result<String, String> {
        let Some((remote, branch)) = target.remote_and_branch() else {
            let reason = match &target.push {
                PushTarget::ReadOnly(reason) => reason.message(),
                _ => "",
            };
            return Ok(format!("Reused {} (read-only: {reason})", name_of(worktree)));
        };
        self.map_fork_push(repo, target)?;
        sirio_git::fetch_branch(repo, remote, branch, fetch_timeout())
            .map_err(|error| format!("fetching {remote}/{branch} failed: {}", fetch_message(error)))?;
        let upstream = format!("{remote}/{branch}");
        let moved = sirio_git::fast_forward(worktree, &upstream)
            .map_err(|error| format!("fast-forwarding failed: {error}"))?;
        Ok(format!("Reused {}{}", name_of(worktree), sync_words(&moved, &upstream)))
    }

    /// A fork's local branch is `<owner>/<branch>`: map it so a plain
    /// `git push` lands on the fork's `<branch>` (`push.default=simple`
    /// refuses a push to another name otherwise).
    fn map_fork_push(&self, repo: &Path, target: &Target) -> Result<(), String> {
        if let PushTarget::Fork { remote, branch, .. } = &target.push {
            sirio_git::ensure_push_refspec(repo, remote, &target.branch, branch)
                .map_err(|error| format!("mapping {} to {remote}/{branch} failed: {error}", target.branch))?;
        }
        Ok(())
    }
}

fn sync_words(moved: &FastForward, upstream: &str) -> String {
    match moved {
        FastForward::UpToDate => format!(": up to date with {upstream}"),
        FastForward::Advanced { to } => format!(": fast-forwarded to {}", &to[..to.len().min(8)]),
        FastForward::Dirty => ": it has uncommitted changes, so it was left as it is".to_string(),
        FastForward::Diverged => format!(": it has diverged from {upstream}, so it was left as it is"),
    }
}
