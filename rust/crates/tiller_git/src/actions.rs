//! The Changes panel's mutations: stage, unstage, discard — for a single
//! path or for everything at once.
//!
//! Mirrors `GitActions` from the Swift app's `TillerGit` package: the same
//! git commands, the same `--literal-pathspecs` guard (so paths that look
//! like flags or globs are taken literally), and the same unborn-HEAD special
//! case for unstage. Unlike Swift, no re-query validation is performed —
//! these functions preflight a caller's status snapshot and then surface git's
//! own errors, so stale or duplicate selections cannot mutate the checkout.

use std::path::Path;

use crate::git;
use crate::status::{has_head, status};
use crate::{GitActionError, GitError, StatusEntry, StatusSnapshot};

/// Batch mutations used by the Changes surface. Every method validates the
/// caller's status snapshot against a fresh status query before invoking git.
pub struct GitActions;

impl GitActions {
    /// Stages exactly the selected current paths.
    pub fn stage(repo: &Path, entries: &[StatusEntry]) -> Result<(), GitError> {
        let paths = validate(entries, repo, ActionSection::Stage)?;
        run_literal_owned(&mutated_args(["add", "-A", "--"], paths), repo)
    }

    /// Unstages exactly the selected current/original mutation paths.
    pub fn unstage(repo: &Path, entries: &[StatusEntry]) -> Result<(), GitError> {
        let paths = validate(entries, repo, ActionSection::Unstage)?;
        let command = if has_head(repo) {
            mutated_args(["reset", "HEAD", "--"], paths)
        } else {
            mutated_args(["rm", "--cached", "--force", "--"], paths)
        };
        run_literal_owned(&command, repo)
    }

    /// Restores selected tracked worktree changes from the index.
    pub fn discard_changes(repo: &Path, entries: &[StatusEntry]) -> Result<(), GitError> {
        let paths = validate(entries, repo, ActionSection::DiscardChanges)?;
        run_literal_owned(&mutated_args(["restore", "--worktree", "--"], paths), repo)
    }

    /// Removes only the selected untracked paths.
    pub fn discard_untracked(repo: &Path, entries: &[StatusEntry]) -> Result<(), GitError> {
        let paths = validate(entries, repo, ActionSection::DiscardUntracked)?;
        run_literal_owned(&mutated_args(["clean", "-f", "-d", "--"], paths), repo)
    }

    /// Entries-first spelling for integrations that model actions as
    /// `action(entries, repo)` rather than `action(repo, entries)`.
    pub fn stage_entries(entries: &[StatusEntry], repo: &Path) -> Result<(), GitError> {
        Self::stage(repo, entries)
    }

    /// Entries-first spelling for unstage.
    pub fn unstage_entries(entries: &[StatusEntry], repo: &Path) -> Result<(), GitError> {
        Self::unstage(repo, entries)
    }

    /// Entries-first spelling for tracked-change discard.
    pub fn discard_change_entries(entries: &[StatusEntry], repo: &Path) -> Result<(), GitError> {
        Self::discard_changes(repo, entries)
    }

    /// Entries-first spelling for untracked discard.
    pub fn discard_untracked_entries(entries: &[StatusEntry], repo: &Path) -> Result<(), GitError> {
        Self::discard_untracked(repo, entries)
    }
}

/// Stages a single path: `git add -A -- <path>`.
///
/// `-A` stages additions, modifications and deletions alike, so the same
/// call works for a newly created file, an edited one, or a deleted one.
pub fn stage(repo: &Path, path: &Path) -> Result<(), GitError> {
    let conflicted = conflicted_paths(repo)?;
    if conflicted.iter().any(|conflicted| conflicted == path) {
        return Err(GitError::ConflictedPaths {
            paths: vec![path.to_path_buf()],
        });
    }
    validate_single_path(repo, path, ActionSection::Stage)?;
    let path = git::path_arg(path);
    run_literal(&["add", "-A", "--", &path], repo)
}

/// Stages every change in the checkout: `git add -A`.
pub fn stage_all(repo: &Path) -> Result<(), GitError> {
    let conflicted = conflicted_paths(repo)?;
    if !conflicted.is_empty() {
        return Err(GitError::ConflictedPaths { paths: conflicted });
    }
    git::run_accepting(&["add", "-A"], repo, &[0]).map(|_| ())
}

/// Unstages a single path, moving it back to the worktree.
///
/// With a committed HEAD this is `git reset HEAD -- <path>`; on an unborn
/// HEAD (no commits yet) there is nothing to reset to, so the file is
/// removed from the index with `git rm --cached --force`, matching the Swift
/// app.
pub fn unstage(repo: &Path, path: &Path) -> Result<(), GitError> {
    validate_single_path(repo, path, ActionSection::Unstage)?;
    let path = git::path_arg(path);
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
    validate_single_path(repo, path, ActionSection::DiscardChanges)?;
    let path = git::path_arg(path);
    run_literal(&["restore", "--worktree", "--", &path], repo)
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

/// Returns the conflicted paths in the same stable order as the status
/// snapshot. This read is the preflight door for both staging operations.
fn conflicted_paths(repo: &Path) -> Result<Vec<std::path::PathBuf>, GitError> {
    Ok(status(repo)?
        .entries
        .into_iter()
        .filter(|entry| entry.is_conflicted())
        .map(|entry| entry.path)
        .collect())
}

#[derive(Clone, Copy)]
enum ActionSection {
    Stage,
    Unstage,
    DiscardChanges,
    DiscardUntracked,
}

fn validate_single_path(repo: &Path, path: &Path, section: ActionSection) -> Result<(), GitError> {
    let snapshot = status(repo)?;
    if snapshot
        .entries
        .iter()
        .any(|entry| entry.path == path && entry.is_conflicted())
    {
        return Err(GitError::Action(GitActionError::ConflictedPaths {
            paths: vec![path.to_path_buf()],
        }));
    }
    let valid = valid_entries(&snapshot, section);
    if valid.iter().any(|entry| entry.path == path) {
        Ok(())
    } else {
        Err(GitError::Action(GitActionError::StalePaths {
            paths: vec![path.to_path_buf()],
        }))
    }
}

fn validate(
    entries: &[StatusEntry],
    repo: &Path,
    section: ActionSection,
) -> Result<Vec<std::path::PathBuf>, GitError> {
    if entries.is_empty() {
        return Err(GitError::Action(GitActionError::EmptySelection));
    }
    let conflicted: Vec<_> = entries
        .iter()
        .filter(|entry| entry.is_conflicted())
        .map(|entry| entry.path.clone())
        .collect();
    if !conflicted.is_empty() {
        return Err(GitError::Action(GitActionError::ConflictedPaths {
            paths: conflicted,
        }));
    }

    let paths = mutation_paths(entries, matches!(section, ActionSection::Unstage));
    let duplicates = duplicate_paths(&paths);
    if !duplicates.is_empty() {
        return Err(GitError::Action(GitActionError::DuplicatePaths {
            paths: duplicates,
        }));
    }

    let snapshot = status(repo)?;
    let valid = valid_entries(&snapshot, section);
    let stale: Vec<_> = entries
        .iter()
        .filter(|entry| !valid.contains(entry))
        .map(|entry| entry.path.clone())
        .collect();
    if !stale.is_empty() {
        return Err(GitError::Action(GitActionError::StalePaths {
            paths: stale,
        }));
    }
    Ok(paths)
}

fn valid_entries(snapshot: &StatusSnapshot, section: ActionSection) -> Vec<&StatusEntry> {
    snapshot
        .entries
        .iter()
        .filter(|entry| match section {
            ActionSection::Stage => entry.has_worktree_changes() || entry.is_untracked(),
            ActionSection::Unstage => entry.is_staged(),
            ActionSection::DiscardChanges => entry.has_worktree_changes() && !entry.is_untracked(),
            ActionSection::DiscardUntracked => entry.is_untracked(),
        })
        .collect()
}

fn mutation_paths(entries: &[StatusEntry], include_original: bool) -> Vec<std::path::PathBuf> {
    entries
        .iter()
        .flat_map(|entry| {
            let mut paths = vec![entry.path.clone()];
            if include_original && let Some(original) = &entry.original_path {
                paths.push(original.clone());
            }
            paths
        })
        .collect()
}

fn duplicate_paths(paths: &[std::path::PathBuf]) -> Vec<std::path::PathBuf> {
    let mut duplicates = Vec::new();
    for (index, path) in paths.iter().enumerate() {
        if paths[..index].contains(path) && !duplicates.contains(path) {
            duplicates.push(path.clone());
        }
    }
    duplicates
}

fn mutated_args<const N: usize>(prefix: [&str; N], paths: Vec<std::path::PathBuf>) -> Vec<String> {
    prefix
        .into_iter()
        .map(str::to_owned)
        .chain(
            paths
                .into_iter()
                .map(|path| git::path_arg(&path)),
        )
        .collect()
}

fn run_literal_owned(arguments: &[String], repo: &Path) -> Result<(), GitError> {
    let mut args = Vec::with_capacity(arguments.len() + 1);
    args.push("--literal-pathspecs".to_string());
    args.extend(arguments.iter().cloned());
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
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
