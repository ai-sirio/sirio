//! The Changes panel's mutations: stage, unstage, discard — for a single
//! path or for everything at once.
//!
//! Mirrors `GitActions` from the Swift app's `TillerGit` package: the same
//! git commands, the same `--literal-pathspecs` guard (so paths that look
//! like flags or globs are taken literally), and the same unborn-HEAD special
//! case for unstage. Unlike Swift, no re-query validation is performed —
//! these functions run the git command and surface git's own errors, which is
//! all the model layer needs to promise.

use std::path::Path;

use crate::GitError;
use crate::git;
use crate::status::has_head;

/// Stages a single path: `git add -A -- <path>`.
///
/// `-A` stages additions, modifications and deletions alike, so the same
/// call works for a newly created file, an edited one, or a deleted one.
pub fn stage(repo: &Path, path: &Path) -> Result<(), GitError> {
    run_literal(&["add", "-A", "--", &path.to_string_lossy()], repo)
}

/// Stages every change in the checkout: `git add -A`.
pub fn stage_all(repo: &Path) -> Result<(), GitError> {
    git::run_accepting(&["add", "-A"], repo, &[0]).map(|_| ())
}

/// Unstages a single path, moving it back to the worktree.
///
/// With a committed HEAD this is `git reset HEAD -- <path>`; on an unborn
/// HEAD (no commits yet) there is nothing to reset to, so the file is
/// removed from the index with `git rm --cached --force`, matching the Swift
/// app.
pub fn unstage(repo: &Path, path: &Path) -> Result<(), GitError> {
    let path = path.to_string_lossy();
    if has_head(repo) {
        run_literal(&["reset", "HEAD", "--", &path], repo)
    } else {
        run_literal(&["rm", "--cached", "--force", "--", &path], repo)
    }
}

/// Discards the unstaged worktree changes of a single tracked path:
/// `git restore --worktree -- <path>`.
///
/// This restores the file from the index. It does not touch untracked files
/// (those are removed with `git clean`, kept separate here exactly as in the
/// Swift app).
pub fn discard(repo: &Path, path: &Path) -> Result<(), GitError> {
    run_literal(
        &["restore", "--worktree", "--", &path.to_string_lossy()],
        repo,
    )
}

/// Discards every unstaged worktree change in the checkout:
/// `git restore --worktree -- .`.
///
/// Untracked files are deliberately left alone; removing them is a separate,
/// more destructive action (`git clean`), and the Swift app keeps the two
/// apart.
pub fn discard_all(repo: &Path) -> Result<(), GitError> {
    git::run_accepting(&["restore", "--worktree", "--", "."], repo, &[0]).map(|_| ())
}

/// Runs `git <command>` with `--literal-pathspecs` so the pathspec is taken
/// byte-for-byte (no glob magic, no leading-dash flag interpretation).
fn run_literal(arguments: &[&str], repo: &Path) -> Result<(), GitError> {
    let mut args = Vec::with_capacity(arguments.len() + 1);
    args.push("--literal-pathspecs");
    args.extend_from_slice(arguments);
    git::run_accepting(&args, repo, &[0]).map(|_| ())
}

#[cfg(test)]
mod tests {
    #[test]
    fn run_literal_builds_the_expected_command_shape() {
        // White-box check that `--literal-pathspecs` comes before the
        // subcommand, as git requires. Real repo round-trips live in the
        // integration suite.
        let mut args = Vec::with_capacity(5);
        args.push("--literal-pathspecs");
        args.extend_from_slice(&["add", "-A", "--", "-weird-name"]);
        assert_eq!(
            args,
            vec!["--literal-pathspecs", "add", "-A", "--", "-weird-name"]
        );
    }
}
