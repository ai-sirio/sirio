//! The git steps that check a change request out into a worktree (change
//! requests C1). Remote steps are non-interactive and use git's own
//! credentials; nothing here rewrites an existing branch or remote.

use std::path::Path;
use std::time::Duration;

use crate::error::GitError;
use crate::fetch::{FetchError, redact_credentials};
use crate::git;
use crate::worktree::{UpstreamBranch, WorktreeError, upstream_of};

/// `git check-ref-format --branch` without running git for the obvious
/// refusals, so a name never reaches a command line as an option.
fn valid_branch(repo: &Path, branch: &str) -> bool {
    !branch.is_empty()
        && !branch.starts_with('-')
        && git::run_accepting(&["check-ref-format", "--branch", branch], repo, &[0]).is_ok()
}

pub fn fetch_branch(
    repo: &Path,
    remote: &str,
    branch: &str,
    timeout: Duration,
) -> Result<(), FetchError> {
    if remote.starts_with('-') || !valid_branch(repo, branch) {
        return Err(FetchError::Failed {
            detail: format!("invalid branch name: {branch}"),
        });
    }
    let refspec = format!("+refs/heads/{branch}:refs/remotes/{remote}/{branch}");
    let args = [
        "-c",
        "gc.auto=0",
        "-c",
        "maintenance.auto=false",
        "fetch",
        "--no-tags",
        "--quiet",
        "--no-recurse-submodules",
        "--",
        remote,
        refspec.as_str(),
    ];
    // `run_remote` does not judge the exit status: a git that ran and failed
    // comes back as `Ok` with a non-zero status, so it is checked here.
    match git::run_remote(&args, repo, timeout) {
        Ok(output) if output.is_success() => Ok(()),
        Ok(output) => Err(FetchError::Failed {
            detail: redact_credentials(output.stderr.trim()),
        }),
        Err(GitError::TimedOut { .. }) => Err(FetchError::TimedOut),
        Err(error) => Err(FetchError::Failed {
            detail: redact_credentials(&error.to_string()),
        }),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RemoteOutcome {
    Added,
    AlreadyThere,
    Conflict { existing: String },
}

pub fn ensure_remote(repo: &Path, name: &str, url: &str) -> Result<RemoteOutcome, GitError> {
    // `name` comes from `handoff::fork_remote_name` and never starts with `-`.
    // The raw configured value, not `get-url`: `get-url` expands
    // `url.<base>.insteadOf`, so a rewritten remote would read back as a
    // different URL than the one that was added.
    let key = format!("remote.{name}.url");
    let existing = git::run_accepting(&["config", "--get", &key], repo, &[0, 1])?;
    let existing = existing.stdout_string().trim().to_string();
    if existing.is_empty() {
        git::run_accepting(&["remote", "add", name, url], repo, &[0])?;
        return Ok(RemoteOutcome::Added);
    }
    if existing == url {
        Ok(RemoteOutcome::AlreadyThere)
    } else {
        Ok(RemoteOutcome::Conflict { existing })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalBranch {
    pub sha: String,
    pub upstream: Option<UpstreamBranch>,
}

pub fn local_branch(repo: &Path, branch: &str) -> Result<Option<LocalBranch>, GitError> {
    let refname = format!("refs/heads/{branch}");
    let output = git::run_accepting(&["rev-parse", "--verify", "--quiet", &refname], repo, &[0, 1])?;
    let sha = output.stdout_string().trim().to_string();
    if sha.is_empty() {
        return Ok(None);
    }
    Ok(Some(LocalBranch {
        sha,
        upstream: upstream_of(repo, branch)?,
    }))
}

pub fn is_ancestor(repo: &Path, ancestor: &str, descendant: &str) -> Result<bool, GitError> {
    let output = git::run_accepting(
        &["merge-base", "--is-ancestor", ancestor, descendant],
        repo,
        &[0, 1],
    )?;
    Ok(output.status == Some(0))
}

pub fn create_worktree_tracking(
    repo: &Path,
    branch: &str,
    path: &Path,
    upstream: &str,
) -> Result<(), WorktreeError> {
    let path = git::path_arg(path);
    git::run_accepting(
        &["worktree", "add", "--track", "-b", branch, &path, upstream],
        repo,
        &[0],
    )?;
    Ok(())
}

pub fn create_worktree_at(
    repo: &Path,
    branch: &str,
    path: &Path,
    commit: &str,
) -> Result<(), WorktreeError> {
    let path = git::path_arg(path);
    git::run_accepting(
        &["worktree", "add", "--no-track", "-b", branch, &path, commit],
        repo,
        &[0],
    )?;
    Ok(())
}

pub fn set_upstream(repo: &Path, branch: &str, upstream: &str) -> Result<(), GitError> {
    let flag = format!("--set-upstream-to={upstream}");
    git::run_accepting(&["branch", &flag, branch], repo, &[0])?;
    Ok(())
}

/// Makes a plain `git push` from `local` land on `remote_branch` of `remote`.
/// A fork's local branch is `<owner>/<branch>`, and `push.default=simple`
/// refuses to push a branch to another name, so the remote carries the
/// mapping (`remote.<remote>.push`). Added once; other entries are kept.
pub fn ensure_push_refspec(
    repo: &Path,
    remote: &str,
    local: &str,
    remote_branch: &str,
) -> Result<(), GitError> {
    let key = format!("remote.{remote}.push");
    let refspec = format!("refs/heads/{local}:refs/heads/{remote_branch}");
    let existing = git::run_accepting(&["config", "--get-all", &key], repo, &[0, 1])?;
    if existing
        .stdout_string()
        .lines()
        .any(|line| line.trim() == refspec)
    {
        return Ok(());
    }
    git::run_accepting(&["config", "--add", &key, &refspec], repo, &[0])?;
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FastForward {
    UpToDate,
    Advanced { to: String },
    Dirty,
    Diverged,
}

/// Moves a clean worktree forward to `target` (an upstream such as
/// `origin/feat`, or a commit); leaves a dirty or diverged one as it is.
pub fn fast_forward(worktree: &Path, target: &str) -> Result<FastForward, GitError> {
    let status = git::run_accepting(
        &["status", "--porcelain", "--untracked-files=no"],
        worktree,
        &[0],
    )?;
    let head = git::run_accepting(&["rev-parse", "HEAD"], worktree, &[0])?
        .stdout_string()
        .trim()
        .to_string();
    let wanted = git::run_accepting(&["rev-parse", "--verify", target], worktree, &[0])?
        .stdout_string()
        .trim()
        .to_string();
    if head == wanted || is_ancestor(worktree, &wanted, &head)? {
        return Ok(FastForward::UpToDate);
    }
    if !is_ancestor(worktree, &head, &wanted)? {
        return Ok(FastForward::Diverged);
    }
    if !status.stdout_string().trim().is_empty() {
        return Ok(FastForward::Dirty);
    }
    git::run_accepting(&["merge", "--ff-only", "--quiet", &wanted], worktree, &[0])?;
    Ok(FastForward::Advanced { to: wanted })
}
