//! Git worktree creation and removal, ported from
//! `TillerGit/GitWorktrees.swift` with one deliberate divergence: removal
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

/// Removes a worktree at `path` from `repo`.
///
/// Deliberately without `--force`: git refuses to remove a worktree with
/// uncommitted changes, and that refusal is surfaced to the user rather
/// than silently discarding their work.
pub fn remove_worktree(repo: &Path, path: &Path) -> Result<(), WorktreeError> {
    let path = git::path_arg(path);
    git::run_accepting(&["worktree", "remove", path.as_str()], repo, &[0])?;
    Ok(())
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

    #[test]
    fn derives_paths_like_the_swift_app() {
        assert_eq!(
            derive_worktree_path(Path::new("/Users/me"), "tiller", "feature/login"),
            PathBuf::from("/Users/me/tiller-feature/login")
        );
        assert_eq!(
            derive_worktree_path(Path::new("/Users/me"), "tiller", "my branch"),
            PathBuf::from("/Users/me/tiller-my branch")
        );
    }

    #[test]
    fn resolves_parent_directory_with_override_and_sibling_default() {
        let root = Path::new("/Users/me/projects/tiller");
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
            "tiller-init-git-test-{}-{}",
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
