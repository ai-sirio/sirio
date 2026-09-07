//! Git worktree creation and removal, ported from
//! `SirioGit/GitWorktrees.swift` with one deliberate divergence: removal
//! does NOT pass `--force`, so git's own refusal to remove a worktree with
//! uncommitted changes is surfaced to the user instead of silently
//! discarding their work.
//!
//! All calls go through the bounded-timeout runner, and callers are
//! expected to invoke them off the render thread (the UI wraps them in a
//! background task).

use std::path::{Path, PathBuf};

use crate::GitError;
use crate::git;

/// A failure creating or removing a git worktree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorktreeError {
    /// A worktree for this branch already exists — creating another one
    /// would be a broken duplicate, so it is refused before touching git.
    BranchAlreadyCheckedOut {
        /// The branch that is already checked out.
        branch: String,
        /// The existing worktree's checkout path.
        path: PathBuf,
    },
    /// The git command failed; stderr carries git's own reason (a branch
    /// that already exists without a worktree, an unborn HEAD, a worktree
    /// with uncommitted changes, ...).
    Git(GitError),
}

impl std::fmt::Display for WorktreeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WorktreeError::BranchAlreadyCheckedOut { branch, path } => write!(
                f,
                "branch '{branch}' already has a worktree at {}",
                path.display()
            ),
            WorktreeError::Git(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for WorktreeError {}

impl From<GitError> for WorktreeError {
    fn from(error: GitError) -> Self {
        WorktreeError::Git(error)
    }
}

/// Creates a worktree for `branch` at `path` inside `repo`.
///
/// The branch is created from `base` when given, else from HEAD — matching
/// `WorktreeDefaults.resolveBase` in the Swift app (the primary worktree's
/// branch, or HEAD when none). If the branch already exists (but has no
/// worktree), the new worktree attaches to it instead of failing.
///
/// Refuses up front, with [`WorktreeError::BranchAlreadyCheckedOut`], when
/// the branch already has a worktree — a second one would be broken.
pub fn create_worktree(
    repo: &Path,
    branch: &str,
    path: &Path,
    base: Option<&str>,
) -> Result<(), WorktreeError> {
    // Pre-check: a branch that already has a worktree must be reported
    // clearly, not turned into a broken duplicate by git.
    if let Some(existing) = worktree_for_branch(repo, branch)? {
        return Err(WorktreeError::BranchAlreadyCheckedOut {
            branch: branch.to_string(),
            path: existing,
        });
    }

    // A branch that exists without a worktree is attached to, not created.
    let branch_exists = git::run_accepting(
        &[
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("refs/heads/{branch}"),
        ],
        repo,
        &[0, 1],
    )
    .is_ok_and(|output| output.is_success());

    let mut arguments = vec!["worktree", "add"];
    let path_string = git::path_arg(path);
    if branch_exists {
        // Attach to the existing branch: `worktree add <path> <branch>`.
        arguments.push(path_string.as_ref());
        arguments.push(branch);
    } else {
        // Create the branch: `worktree add -b <branch> <path> [<base>]`.
        arguments.push("-b");
        arguments.push(branch);
        arguments.push(path_string.as_ref());
        if let Some(base) = base {
            arguments.push(base);
        }
    }
    git::run_accepting(&arguments, repo, &[0])?;
    Ok(())
}

/// Removes a worktree at `path` from `repo`, then deletes its `branch`.
///
/// Deliberately without `--force`: git refuses to remove a worktree with
/// uncommitted changes, and that refusal is surfaced to the user rather
/// than silently discarding their work. Branch deletion is best-effort after
/// the worktree has been removed: a refusal there is logged, but does not
/// turn the already-completed worktree removal into an error.
pub fn remove_worktree(repo: &Path, path: &Path, branch: &str) -> Result<(), WorktreeError> {
    let path = git::path_arg(path);
    git::run_accepting(&["worktree", "remove", path.as_str()], repo, &[0])?;
    if let Err(error) = git::run_accepting(&["branch", "-D", branch], repo, &[0]) {
        eprintln!("[git] failed to delete branch '{branch}' after removing worktree: {error}");
    }
    Ok(())
}

/// The remote branch a local branch tracks (its upstream).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpstreamBranch {
    /// The remote's name (`origin`).
    pub remote: String,
    /// The branch name on that remote, without the `refs/heads/` prefix.
    pub branch: String,
}

/// The deadline for the one call in this module that talks to a remote:
/// a push over the network legitimately outlives the local default.
const REMOTE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);

/// The upstream `branch` tracks in `repo`, if it has one.
///
/// A branch with no upstream — or no branch at all — answers `None`; only a
/// git failure is an error. Read from `for-each-ref` rather than
/// `rev-parse @{upstream}` so a missing upstream is a plain empty answer,
/// not a non-zero exit to disambiguate from a real failure.
pub fn upstream_of(repo: &Path, branch: &str) -> Result<Option<UpstreamBranch>, GitError> {
    let refname = format!("refs/heads/{branch}");
    let output = git::run_accepting(
        &[
            "for-each-ref",
            "--format=%(upstream:remotename)\t%(upstream:remoteref)",
            &refname,
        ],
        repo,
        &[0],
    )?;
    let text = output.stdout_string();
    let Some((remote, remote_ref)) = text.lines().next().and_then(|line| line.split_once('\t'))
    else {
        return Ok(None);
    };
    let remote = remote.trim();
    let remote_ref = remote_ref.trim();
    if remote.is_empty() || remote_ref.is_empty() {
        return Ok(None);
    }
    Ok(Some(UpstreamBranch {
        remote: remote.to_string(),
        branch: remote_ref
            .strip_prefix("refs/heads/")
            .unwrap_or(remote_ref)
            .to_string(),
    }))
}

/// Deletes `upstream`'s branch on its remote: `git push <remote> --delete
/// <branch>`. Nothing local changes; git's refusal (offline, no permission,
/// already gone) is surfaced as the error.
pub fn delete_remote_branch(repo: &Path, upstream: &UpstreamBranch) -> Result<(), GitError> {
    git::run_accepting_with_timeout(
        &["push", &upstream.remote, "--delete", &upstream.branch],
        repo,
        &[0],
        REMOTE_TIMEOUT,
    )?;
    Ok(())
}

/// Deletes the branch on its remote, then removes the worktree and its local
/// branch like [`remove_worktree`].
///
/// Remote first, on purpose: a push that fails (offline, refused) leaves the
/// checkout untouched, so the user can fall back to a disk-only removal
/// with nothing lost. A worktree removal refused afterwards — uncommitted
/// changes — leaves the local branch and its work in place, re-pushable.
pub fn remove_worktree_and_remote_branch(
    repo: &Path,
    path: &Path,
    branch: &str,
    upstream: &UpstreamBranch,
) -> Result<(), WorktreeError> {
    delete_remote_branch(repo, upstream)?;
    remove_worktree(repo, path, branch)
}

/// Initializes `directory` as a Git repository without creating a commit.
///
/// This is the sidebar's explicit "Initialize Git" transition for folder
/// projects. The caller owns the follow-up discovery and UI refresh.
pub fn init_repository(directory: &Path) -> Result<(), GitError> {
    git::run_accepting(&["init"], directory, &[0]).map(|_| ())
}

/// The checkout path of the worktree currently on `branch`, if any.
fn worktree_for_branch(repo: &Path, branch: &str) -> Result<Option<PathBuf>, WorktreeError> {
    let output = git::run_accepting(&["worktree", "list", "--porcelain"], repo, &[0])?;
    let mut current_path: Option<PathBuf> = None;
    for line in output.stdout_string().lines() {
        if let Some(path) = line.strip_prefix("worktree ") {
            current_path = Some(PathBuf::from(path));
        } else if let Some(worktree_branch) = line.strip_prefix("branch refs/heads/")
            && worktree_branch == branch
        {
            return Ok(current_path);
        }
    }
    Ok(None)
}

/// The parent directory for a new worktree's folder: an explicit
/// per-project override wins; otherwise the project root's sibling
/// directory — matching `WorktreeDefaults.resolveParentDirectory` in the
/// Swift app.
pub fn resolve_parent_directory(root: &Path, override_dir: Option<&Path>) -> PathBuf {
    override_dir.map(Path::to_path_buf).unwrap_or_else(|| {
        root.parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| root.to_path_buf())
    })
}

/// The folder name for a new worktree: `{project-name}-{branch}`, matching
/// the Swift app's `addWorktree`. A branch with a slash becomes a nested
/// directory, which git supports.
pub fn derive_worktree_path(parent_dir: &Path, project_name: &str, branch: &str) -> PathBuf {
    parent_dir.join(format!("{project_name}-{branch}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn git_ok(cwd: &Path, args: &[&str]) {
        let status = std::process::Command::new("git")
            .args(args)
            .current_dir(cwd)
            .status()
            .expect("git runs");
        assert!(status.success(), "git {args:?} failed in {}", cwd.display());
    }

    /// The branch heads `origin` holds, one `<sha>\t<ref>` line each.
    fn remote_heads(repo: &Path) -> String {
        let output = std::process::Command::new("git")
            .args(["ls-remote", "--heads", "origin"])
            .current_dir(repo)
            .output()
            .expect("git runs");
        assert!(output.status.success(), "ls-remote fails");
        String::from_utf8_lossy(&output.stdout).into_owned()
    }

    /// A fresh repository with one commit on `main`, pushed with `-u` to a
    /// bare `origin` beside it. Returns `(root, repo)`; the root holds both
    /// and is the caller's to delete. Uncanonicalized on purpose: git
    /// refuses `\?\` paths on Windows.
    fn repo_with_bare_origin(tag: &str) -> (PathBuf, PathBuf) {
        let root = std::env::temp_dir().join(format!(
            "sirio-worktree-remote-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        let repo = root.join("repo");
        let origin = root.join("origin.git");
        std::fs::create_dir_all(&repo).expect("create repo");
        std::fs::create_dir_all(&origin).expect("create origin");
        git_ok(&origin, &["init", "-q", "--bare"]);
        git_ok(&repo, &["init", "-q", "-b", "main"]);
        git_ok(&repo, &["config", "user.email", "test@sirio.dev"]);
        git_ok(&repo, &["config", "user.name", "Sirio Test"]);
        std::fs::write(repo.join("file.txt"), "one\n").expect("write");
        git_ok(&repo, &["add", "-A"]);
        git_ok(&repo, &["commit", "-q", "-m", "root"]);
        git_ok(
            &repo,
            &[
                "remote",
                "add",
                "origin",
                origin.to_str().expect("utf-8 path"),
            ],
        );
        git_ok(&repo, &["push", "-q", "-u", "origin", "main"]);
        (root, repo)
    }

    #[test]
    fn upstream_of_reports_the_tracked_remote_branch_or_none() {
        let (root, repo) = repo_with_bare_origin("upstream");
        assert_eq!(
            upstream_of(&repo, "main").expect("git answers"),
            Some(UpstreamBranch {
                remote: "origin".to_string(),
                branch: "main".to_string(),
            }),
            "main was pushed with -u, so it tracks origin/main"
        );
        assert_eq!(
            upstream_of(&repo, "missing").expect("git answers"),
            None,
            "a branch that does not exist has no upstream"
        );

        let worktree = root.join("wt-feature");
        create_worktree(&repo, "feature", &worktree, None).expect("worktree created");
        assert_eq!(
            upstream_of(&repo, "feature").expect("git answers"),
            None,
            "a branch never pushed tracks nothing"
        );
        git_ok(&worktree, &["push", "-q", "-u", "origin", "feature"]);
        assert_eq!(
            upstream_of(&repo, "feature").expect("git answers"),
            Some(UpstreamBranch {
                remote: "origin".to_string(),
                branch: "feature".to_string(),
            }),
            "after push -u the branch tracks origin/feature"
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn remove_worktree_and_remote_branch_deletes_the_checkout_and_both_branches() {
        let (root, repo) = repo_with_bare_origin("remove-remote");
        let worktree = root.join("wt-feature");
        create_worktree(&repo, "feature", &worktree, None).expect("worktree created");
        git_ok(&worktree, &["push", "-q", "-u", "origin", "feature"]);
        assert!(
            remote_heads(&repo).contains("refs/heads/feature"),
            "the fixture pushed feature to origin"
        );

        let upstream = upstream_of(&repo, "feature")
            .expect("git answers")
            .expect("feature tracks origin/feature");
        remove_worktree_and_remote_branch(&repo, &worktree, "feature", &upstream)
            .expect("both removals succeed");

        assert!(!worktree.exists(), "the checkout is gone from disk");
        assert!(
            !remote_heads(&repo).contains("refs/heads/feature"),
            "the remote branch is gone from origin"
        );
        let local = git::run_accepting(&["branch", "--list", "feature"], &repo, &[0])
            .expect("git answers")
            .stdout_string();
        assert!(
            local.trim().is_empty(),
            "the local branch is gone too, got {local:?}"
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn a_refused_remote_deletion_leaves_the_worktree_untouched() {
        let (root, repo) = repo_with_bare_origin("remote-refused");
        let worktree = root.join("wt-feature");
        create_worktree(&repo, "feature", &worktree, None).expect("worktree created");

        let bogus = UpstreamBranch {
            remote: "nowhere".to_string(),
            branch: "feature".to_string(),
        };
        let result = remove_worktree_and_remote_branch(&repo, &worktree, "feature", &bogus);
        assert!(result.is_err(), "a push to an unknown remote fails");

        assert!(
            worktree.exists(),
            "remote first: a failed push must not have touched the checkout"
        );
        assert!(
            worktree_for_branch(&repo, "feature")
                .expect("git answers")
                .is_some(),
            "git still lists the feature worktree"
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn derives_paths_like_the_swift_app() {
        assert_eq!(
            derive_worktree_path(Path::new("/Users/me"), "sirio", "feature/login"),
            PathBuf::from("/Users/me/sirio-feature/login")
        );
        assert_eq!(
            derive_worktree_path(Path::new("/Users/me"), "sirio", "my branch"),
            PathBuf::from("/Users/me/sirio-my branch")
        );
    }

    #[test]
    fn resolves_parent_directory_with_override_and_sibling_default() {
        let root = Path::new("/Users/me/projects/sirio");
        assert_eq!(
            resolve_parent_directory(root, Some(Path::new("/worktrees"))),
            PathBuf::from("/worktrees")
        );
        assert_eq!(
            resolve_parent_directory(root, None),
            PathBuf::from("/Users/me/projects")
        );
    }

    #[test]
    fn initializes_a_folder_as_a_git_repository() {
        let root = std::env::temp_dir().join(format!(
            "sirio-init-git-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).expect("create folder");
        init_repository(&root).expect("git init succeeds");
        assert!(root.join(".git").exists());
        let _ = std::fs::remove_dir_all(root);
    }
}
