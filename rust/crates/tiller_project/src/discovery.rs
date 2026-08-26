//! Real discovery of projects and worktrees from disk, via `git`.
//!
//! The contract mirrors the Swift app's `TillerGit` package:
//!
//! - [`is_git_repository`] is the pure gate: a `.git` entry (directory for a
//!   normal repo, file for a linked worktree/submodule) at the project root.
//!   No shell-out, so it is safe to call anywhere.
//! - [`discover_worktrees`] shells out to `git worktree list --porcelain` and
//!   parses the machine format. The first block is the primary (main)
//!   worktree. A detached HEAD appears as a `detached` label with no `branch`
//!   line; `locked` and `prunable` entries carry optional reasons.
//! - [`current_branch`] answers "what branch is this checkout on right now"
//!   for any checkout path, returning `None` for a detached HEAD.
//! - [`read_head_label`] answers the same question without spawning `git`
//!   at all — one read of the checkout's resolved `HEAD` file — so live
//!   branch labels stay affordable on small machines.

use std::path::Path;

use crate::{GitError, git};

/// One worktree as reported by `git worktree list --porcelain`.
///
/// This is the *discovered* form — what git told us — before it is fitted
/// with ids and a display branch label by [`crate::Workspace::load_project`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiscoveredWorktree {
    /// Absolute path of the checkout directory.
    pub path: std::path::PathBuf,
    /// Full commit id HEAD points at, when git reported one.
    pub head: Option<String>,
    /// Local branch name when the checkout is on a branch; `None` when HEAD
    /// is detached (the porcelain `detached` label / missing `branch` line).
    pub branch: Option<String>,
    /// True for the main worktree (the first porcelain block).
    pub is_primary: bool,
    /// Whether the worktree is locked (`git worktree lock`).
    pub locked: bool,
    /// Whether git considers the worktree prunable.
    pub prunable: bool,
}

/// The result of discovering a directory as a project.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiscoveredProject {
    /// Whether the directory is a git repository.
    pub is_git: bool,
    /// The repository's worktrees; empty when `is_git` is false.
    pub worktrees: Vec<DiscoveredWorktree>,
}

/// True if `path` contains a `.git` entry — a directory for a normal repo, a
/// file for a linked worktree/submodule. Matches `GitRepoDetection` from the
/// Swift app byte for byte in behavior (both count as git).
pub fn is_git_repository(path: &Path) -> bool {
    path.join(".git").exists()
}

/// Discovers a directory as a project: git detection plus, for git repos, the
/// full worktree list. A non-git directory yields an empty worktree list
/// without ever touching `git` — like the Swift app's `addProject`.
pub fn discover_project(root: &Path) -> Result<DiscoveredProject, GitError> {
    if !is_git_repository(root) {
        return Ok(DiscoveredProject {
            is_git: false,
            worktrees: Vec::new(),
        });
    }

    Ok(DiscoveredProject {
        is_git: true,
        worktrees: discover_worktrees(root)?,
    })
}

/// Lists a git repository's worktrees by running
/// `git worktree list --porcelain` in `repo_path` and parsing the output.
///
/// Returns an error for a non-git directory (git exits 128 with a fatal
/// message); callers should gate on [`is_git_repository`] first, as the
/// Swift app does.
pub fn discover_worktrees(repo_path: &Path) -> Result<Vec<DiscoveredWorktree>, GitError> {
    let stdout = git::run_success(&["worktree", "list", "--porcelain"], repo_path)?;
    Ok(parse_worktree_list(&stdout))
}

/// Determines the current branch of a checkout.
///
/// Runs `git branch --show-current` in `path`. Returns `Some(branch)` when
/// the checkout is on a branch and `None` when HEAD is detached (git prints
/// nothing and still exits 0). Matches the Swift app's use of `--show-current`
/// semantics for branch detection.
pub fn current_branch(path: &Path) -> Result<Option<String>, GitError> {
    let stdout = git::run_success(&["branch", "--show-current"], path)?;
    let branch = stdout.trim();
    Ok(if branch.is_empty() {
        None
    } else {
        Some(branch.to_string())
    })
}

/// Answers what label this checkout shows right now, by reading its git
/// directory's `HEAD` file directly.
///
/// The git directory is resolved the way git itself lays it out: a `.git`
/// directory beside the checkout *is* it (`<path>/.git`), while a linked
/// worktree instead carries a `.git` **file** whose body is
/// `gitdir: <path to that worktree's admin directory>` — absolute, or
/// relative to the checkout itself. Branch HEAD (`ref: refs/heads/<name>`)
/// yields everything after the prefix, trimmed — a branch name may contain
/// slashes and arrives whole. A raw commit id (detached HEAD) yields its
/// first seven characters, matching `short_head`'s detached convention in
/// the app shell.
///
/// This is the process-free half of keeping branch labels live (#114):
/// unlike [`current_branch`] it never spawns `git`, so affording one file
/// read per row at rare, user-driven moments stays within a Raspberry Pi
/// 5's budget. Anything missing, unreadable or unparseable is `None` —
/// never a process, never a guess, never a blanked label.
pub fn read_head_label(path: &Path) -> Option<String> {
    let dot_git = path.join(".git");
    let git_dir = if dot_git.is_dir() {
        dot_git
    } else {
        let link = std::fs::read_to_string(&dot_git).ok()?;
        let target = link.strip_prefix("gitdir:")?.trim();
        if target.is_empty() {
            return None;
        }
        let target = Path::new(target);
        if target.is_absolute() {
            target.to_path_buf()
        } else {
            path.join(target)
        }
    };

    let head = std::fs::read_to_string(git_dir.join("HEAD")).ok()?;
    if let Some(branch) = head.strip_prefix("ref: refs/heads/") {
        let branch = branch.trim();
        if branch.is_empty() {
            return None;
        }
        Some(branch.to_string())
    } else {
        // Not a branch ref: only a raw commit id counts. Anything else
        // (another ref namespace, garbage) is unparseable here.
        let sha = head.trim();
        let bytes = sha.as_bytes();
        if bytes.len() >= 7 && bytes.iter().all(|b| b.is_ascii_hexdigit()) {
            Some(sha[..7].to_string())
        } else {
            None
        }
    }
}

/// Parses the machine-readable output of `git worktree list --porcelain`.
///
/// Blocks are separated by blank lines; the first block is the main worktree
/// (so `is_primary` is true exactly for it). Within a block:
///
/// ```text
/// worktree <absolute path>
/// HEAD <full commit sha>
/// branch refs/heads/<name> | detached | bare
/// locked [reason]
/// prunable [reason]
/// ```
///
/// The `branch` line is absent for a detached HEAD (which is instead marked
/// by a bare `detached` label), and reasons on `locked`/`prunable` are
/// free-form — a reason is the rest of the line, which may be empty. Unknown
/// lines are ignored so the parser survives forward-compatible format
/// additions.
pub fn parse_worktree_list(output: &str) -> Vec<DiscoveredWorktree> {
    let mut worktrees = Vec::new();
    let mut path: Option<std::path::PathBuf> = None;
    let mut head: Option<String> = None;
    let mut branch: Option<String> = None;
    let mut locked = false;
    let mut prunable = false;

    for line in output.lines() {
        if line.is_empty() {
            if let Some(path) = path.take() {
                worktrees.push(DiscoveredWorktree {
                    path,
                    head: head.take(),
                    branch: branch.take(),
                    is_primary: worktrees.is_empty(),
                    locked,
                    prunable,
                });
                locked = false;
                prunable = false;
            }
            continue;
        }

        if let Some(rest) = line.strip_prefix("worktree ") {
            path = Some(rest.into());
        } else if let Some(rest) = line.strip_prefix("HEAD ") {
            head = Some(rest.to_string());
        } else if let Some(rest) = line.strip_prefix("branch refs/heads/") {
            branch = Some(rest.to_string());
        } else if line == "detached" || line == "bare" {
            // A detached or bare checkout has no local branch.
            branch = None;
        } else if line == "locked" || line.starts_with("locked ") {
            locked = true;
        } else if line == "prunable" || line.starts_with("prunable ") {
            prunable = true;
        }
        // Unknown lines are ignored for forward compatibility.
    }

    // A trailing block without a closing blank line.
    if let Some(path) = path {
        worktrees.push(DiscoveredWorktree {
            path,
            head: head.take(),
            branch: branch.take(),
            is_primary: worktrees.is_empty(),
            locked,
            prunable,
        });
    }

    worktrees
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// A realistic `git worktree list --porcelain` capture: a primary
    /// checkout on `main`, a linked worktree on a branch, a detached-HEAD
    /// entry (no `branch` line, `detached` label), a locked entry with a
    /// reason, and a prunable entry with git's documented reason text. The
    /// block shapes are byte-faithful to real git output (verified against
    /// git 2.53).
    fn sample_porcelain() -> &'static str {
        concat!(
            "worktree /Users/me/tiller\n",
            "HEAD 1638c08b4ffb127b9852cbab3124df6a374d07f7\n",
            "branch refs/heads/main\n",
            "\n",
            "worktree /Users/me/tiller-feature\n",
            "HEAD 1638c08b4ffb127b9852cbab3124df6a374d07f7\n",
            "branch refs/heads/feature/login\n",
            "\n",
            "worktree /Users/me/tiller-detached\n",
            "HEAD a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0\n",
            "detached\n",
            "\n",
            "worktree /Users/me/tiller-locked\n",
            "HEAD 1638c08b4ffb127b9852cbab3124df6a374d07f7\n",
            "branch refs/heads/locked-branch\n",
            "locked in use by backup\n",
            "\n",
            "worktree /Users/me/tiller-prunable\n",
            "HEAD 1638c08b4ffb127b9852cbab3124df6a374d07f7\n",
            "branch refs/heads/prunable-branch\n",
            "prunable gitdir file points to non-existent location\n",
            "\n",
        )
    }

    #[test]
    fn parses_primary_linked_detached_locked_and_prunable_entries() {
        let worktrees = parse_worktree_list(sample_porcelain());

        assert_eq!(worktrees.len(), 5);

        // Primary (main) worktree.
        assert_eq!(worktrees[0].path, PathBuf::from("/Users/me/tiller"));
        assert_eq!(worktrees[0].branch.as_deref(), Some("main"));
        assert!(worktrees[0].is_primary);
        assert!(!worktrees[0].locked);
        assert!(!worktrees[0].prunable);

        // Linked worktree on a branch with a slash.
        assert_eq!(worktrees[1].path, PathBuf::from("/Users/me/tiller-feature"));
        assert_eq!(worktrees[1].branch.as_deref(), Some("feature/login"));
        assert!(!worktrees[1].is_primary);

        // Detached HEAD: no branch line, `detached` label, head preserved.
        assert_eq!(
            worktrees[2].path,
            PathBuf::from("/Users/me/tiller-detached")
        );
        assert_eq!(worktrees[2].branch, None);
        assert_eq!(
            worktrees[2].head.as_deref(),
            Some("a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0")
        );
        assert!(!worktrees[2].is_primary);

        // Locked entry with a reason.
        assert_eq!(worktrees[3].path, PathBuf::from("/Users/me/tiller-locked"));
        assert!(worktrees[3].locked);
        assert!(!worktrees[3].prunable);
        assert_eq!(worktrees[3].branch.as_deref(), Some("locked-branch"));

        // Prunable entry with git's free-form reason.
        assert_eq!(
            worktrees[4].path,
            PathBuf::from("/Users/me/tiller-prunable")
        );
        assert!(worktrees[4].prunable);
        assert!(!worktrees[4].locked);
        assert_eq!(worktrees[4].branch.as_deref(), Some("prunable-branch"));
    }

    #[test]
    fn parses_blocks_without_trailing_blank_line() {
        let worktrees = parse_worktree_list(
            "worktree /a\nHEAD 1638c08b4ffb127b9852cbab3124df6a374d07f7\nbranch refs/heads/main\n\
             \n\
             worktree /b\nHEAD 1638c08b4ffb127b9852cbab3124df6a374d07f7\nbranch refs/heads/x\n",
        );

        assert_eq!(worktrees.len(), 2);
        assert!(worktrees[0].is_primary);
        assert!(!worktrees[1].is_primary);
        assert_eq!(worktrees[1].branch.as_deref(), Some("x"));
    }

    #[test]
    fn ignores_unknown_lines_and_empty_output() {
        assert!(parse_worktree_list("").is_empty());

        let worktrees = parse_worktree_list(
            "worktree /a\nHEAD 1638c08b4ffb127b9852cbab3124df6a374d07f7\nbranch refs/heads/main\n\
             some-future-attribute value\n\n",
        );
        assert_eq!(worktrees.len(), 1);
        assert_eq!(worktrees[0].branch.as_deref(), Some("main"));
    }

    #[test]
    fn bare_repo_entry_reports_no_branch() {
        let worktrees = parse_worktree_list(
            "worktree /Users/me/tiller.git\nHEAD 1638c08b4ffb127b9852cbab3124df6a374d07f7\nbare\n",
        );
        assert_eq!(worktrees.len(), 1);
        assert!(worktrees[0].is_primary);
        assert_eq!(worktrees[0].branch, None);
    }
}
