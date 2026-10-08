//! Checking a change request out into a worktree (change requests C1, spec
//! C §5): read the forge, gather what git knows, let `handoff::decide`
//! choose, then run the git steps. Runs off the GPUI thread. Nothing it
//! already created is deleted when a later step fails.

use std::path::{Path, PathBuf};

use sirio_forge::{ChangeHeader, ChangeRef, Forge, Revisions};
use sirio_git::{FastForward, FetchError, RemoteOutcome};
use sirio_ui::forge_source::ChangeRequestSource;
use sirio_ui::handoff::context::push_words;
use sirio_ui::handoff::{self, BranchFacts, Facts, ListedRemote, LocalSource, Plan, PushTarget, Start, Target};

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
    /// Where the agent may push, in Sirio's words (`push_words`).
    pub(crate) push: String,
}

/// What `ForgeHub::plan` decided, before any git step has run.
pub(crate) struct Planned {
    pub(crate) header: ChangeHeader,
    pub(crate) revisions: Revisions,
    pub(crate) target: Target,
    pub(crate) plan: Plan,
}

fn fetch_message(error: FetchError) -> String {
    match error {
        FetchError::TimedOut => "the fetch timed out".to_string(),
        FetchError::Failed { detail } => detail,
    }
}

pub(super) fn name_of(path: &Path) -> String {
    path.file_name().map_or_else(|| path.display().to_string(), |name| name.to_string_lossy().into_owned())
}

impl ForgeHub {
    pub(crate) fn checkout(&self, request: &CheckoutRequest) -> Result<CheckoutDone, String> {
        let Planned { header, revisions, target, plan } = self.plan(request, false)?;
        let repo = request.repo.as_path();
        let done = |path: PathBuf, detail: String| CheckoutDone {
            branch: target.branch.clone(),
            title: header.summary.title.clone(),
            push: push_words(&target),
            path,
            detail,
        };
        match plan {
            Plan::Refuse(refusal) => Err(refusal.message()),
            Plan::Reuse { worktree, .. } => {
                let detail = self.sync(repo, &worktree, &target, &revisions.head_sha)?;
                Ok(done(worktree, detail))
            }
            Plan::Create { path, branch, start, add_remote } => {
                if let Some((name, url)) = &add_remote {
                    let fork_branch = match &target.push {
                        PushTarget::Fork { branch, .. } => branch.as_str(),
                        _ => "",
                    };
                    match sirio_git::ensure_remote(repo, name, url, fork_branch) {
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
                        // The upstream first: git sets it on a branch no
                        // worktree has checked out, so a failure here leaves
                        // nothing new behind for a retry to refuse.
                        if let Some(upstream) = set_upstream {
                            sirio_git::set_upstream(repo, &branch, upstream)
                                .map_err(|error| format!("setting the upstream failed: {error}"))?;
                        }
                        sirio_git::create_worktree(repo, &branch, &path, None)
                            .map_err(|error| format!("creating the worktree failed: {error}"))?;
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

    /// Everything `checkout` decides before a git step runs: the forge's
    /// header and revisions, the target, the facts git knows, and the plan.
    /// Fetches the head through B1 (sirio's own refs, nothing a worktree
    /// sees), so ancestry can be judged. `dry` only reads the worktree
    /// registrations and change request links: a missing one is treated as
    /// absent and left in place, which is what a preview wants.
    pub(crate) fn plan(&self, request: &CheckoutRequest, dry: bool) -> Result<Planned, String> {
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
        let every: Vec<ListedRemote> = remotes
            .iter()
            .map(|(name, url)| ListedRemote { name: name.clone(), url: url.clone() })
            .collect();
        // The local branch when the head repository is gone (the forge drops
        // the owner with it): the change request's own number, never a name
        // the user may have picked for their own branch.
        let gone_branch = match reference.forge {
            Forge::GitHub => format!("pr-{}", reference.number),
            Forge::GitLab => format!("mr-{}", reference.number),
        };
        // The local branch of the source name decides whether the viewer's own
        // fork may take it (see `handoff::target`).
        let local = match sirio_git::local_branch(repo, &header.summary.source_branch)
            .map_err(|error| format!("reading the branch {} failed: {error}", header.summary.source_branch))?
        {
            None => LocalSource::Absent,
            Some(local) => match local.upstream {
                Some(upstream) => LocalSource::Tracks(format!("{}/{}", upstream.remote, upstream.branch)),
                None => LocalSource::Untracked,
            },
        };
        let target = handoff::target(
            &header.summary.source_branch,
            reference.number,
            &listed,
            &every,
            header.head.as_ref(),
            &gone_branch,
            &local,
        );

        // The head commit, through B1's fetch, so ancestry can be judged
        // before anything is created.
        self.revisions
            .ensure(repo, reference, &revisions, None)
            .map_err(|error| format!("could not fetch the head of {}: {error}", reference.label()))?;

        let facts = self.facts(request, &target, &remotes, &revisions.head_sha, dry)?;
        let plan = handoff::decide(&target, &facts);
        Ok(Planned { header, revisions, target, plan })
    }

    fn facts(
        &self,
        request: &CheckoutRequest,
        target: &Target,
        remotes: &[(String, String)],
        head_sha: &str,
        dry: bool,
    ) -> Result<Facts, String> {
        let repo = request.repo.as_path();
        let mut linked = None;
        for path in self.settings.linked_worktrees(&request.reference) {
            if path.exists() {
                linked = Some(path);
                break;
            }
            // A linked worktree deleted outside Sirio: forget it (a dry run
            // only reads, so it leaves the link and treats it as absent).
            if !dry {
                self.settings.drop_change_request_link(&path);
            }
        }
        let checked_out = match sirio_git::worktree_for_branch(repo, &target.branch)
            .map_err(|error| format!("reading the worktrees failed: {error}"))?
        {
            // Git still lists a folder deleted outside it, on its branch: that
            // registration is removed, and the branch is free to be checked out.
            Some(path) if !path.exists() => {
                if !dry {
                    sirio_git::remove_missing_worktree(repo, &path).map_err(|error| {
                        format!("forgetting the deleted worktree {} failed: {error}", path.display())
                    })?;
                }
                None
            }
            Some(path) => {
                let upstream = sirio_git::upstream_of(repo, &target.branch)
                    .ok()
                    .flatten()
                    .map(|upstream| format!("{}/{}", upstream.remote, upstream.branch));
                Some((path, upstream))
            }
            None => None,
        };
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

    /// Brings a reused worktree up to its upstream (or, for a read-only
    /// target, to the change request's head) when it is clean and behind.
    /// Only the branch the worktree has checked out is ever moved, and only
    /// when it is `target.branch`: a worktree switched elsewhere is left alone.
    fn sync(&self, repo: &Path, worktree: &Path, target: &Target, head_sha: &str) -> Result<String, String> {
        let name = name_of(worktree);
        // A read that fails is a failure, not "another branch": the worktree
        // may well be on the target.
        let current = sirio_project::current_branch(worktree)
            .map_err(|error| format!("reading the branch of {name} failed: {error}"))?;
        let on_target = current.as_deref() == Some(target.branch.as_str());
        let Some((remote, branch)) = target.remote_and_branch() else {
            let reason = match &target.push {
                PushTarget::ReadOnly(reason) => reason.message(),
                _ => "",
            };
            if !on_target {
                return Ok(format!("Reused {name} (read-only: {reason}){}", left_alone(current.as_deref())));
            }
            let moved = sirio_git::fast_forward(worktree, head_sha)
                .map_err(|error| format!("fast-forwarding failed: {error}"))?;
            return Ok(format!(
                "Reused {name} (read-only: {reason}){}",
                sync_words(&moved, "the change request's head")
            ));
        };
        // A fork that accepts pushes now, though the worktree was made read-only:
        // its remote is added, fetched, and the branch gets its upstream before
        // the mapping that a plain push relies on is written.
        if let PushTarget::Fork { url, .. } = &target.push {
            match sirio_git::ensure_remote(repo, remote, url, branch) {
                Ok(RemoteOutcome::Added | RemoteOutcome::AlreadyThere) => {}
                Ok(RemoteOutcome::Conflict { existing }) => {
                    return Err(format!("the remote {remote} already points at {existing}"));
                }
                Err(error) => return Err(format!("adding the remote {remote} failed: {error}")),
            }
        }
        sirio_git::fetch_branch(repo, remote, branch, fetch_timeout())
            .map_err(|error| format!("fetching {remote}/{branch} failed: {}", fetch_message(error)))?;
        let upstream = format!("{remote}/{branch}");
        if on_target
            && sirio_git::upstream_of(repo, &target.branch)
                .map_err(|error| format!("reading the upstream of {} failed: {error}", target.branch))?
                .is_none()
        {
            sirio_git::set_upstream(repo, &target.branch, &upstream)
                .map_err(|error| format!("setting the upstream failed: {error}"))?;
        }
        self.map_fork_push(repo, target)?;
        if !on_target {
            return Ok(format!("Reused {name}{}", left_alone(current.as_deref())));
        }
        let upstream = format!("{remote}/{branch}");
        let moved = sirio_git::fast_forward(worktree, &upstream)
            .map_err(|error| format!("fast-forwarding failed: {error}"))?;
        Ok(format!("Reused {name}{}", sync_words(&moved, &upstream)))
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

/// Named where a reused worktree is not on the change request's branch.
const ON_ANOTHER_BRANCH: &str = ": it is on another branch, so it was left as it is";
/// Named where a reused worktree has a detached HEAD.
const ON_DETACHED_HEAD: &str = ": it is on a detached HEAD, so it was left as it is";

fn left_alone(current: Option<&str>) -> &'static str {
    if current.is_some() { ON_ANOTHER_BRANCH } else { ON_DETACHED_HEAD }
}

fn sync_words(moved: &FastForward, upstream: &str) -> String {
    match moved {
        FastForward::UpToDate => format!(": up to date with {upstream}"),
        FastForward::Advanced { to } => format!(": fast-forwarded to {}", &to[..to.len().min(8)]),
        FastForward::Dirty => ": it has uncommitted changes, so it was left as it is".to_string(),
        FastForward::Diverged => format!(": it has diverged from {upstream}, so it was left as it is"),
    }
}
