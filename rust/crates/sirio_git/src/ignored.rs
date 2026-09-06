//! The Files tree's ignored-path model: which worktree paths the ignore
//! rules (`.gitignore` and friends) exclude, so the panel can dim them.
//!
//! Deliberately separate from [`crate::status`]: adding `--ignored` to the
//! shared `git status` call would have leaked ignored entries into the
//! Changes panel as untracked ones (porcelain's `!` records carry no XY
//! states of their own).

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::GitError;
use crate::git;

/// The set of ignored paths in a worktree, as reported by
/// `git ls-files --others --ignored --exclude-standard --directory`.
///
/// Individually ignored files appear as themselves; a directory whose
/// entire contents are ignored appears once, as the directory (with a
/// trailing `/`), and none of its children are listed. Membership is
/// therefore an ancestor walk, not a plain lookup — see
/// [`IgnoredPaths::is_ignored`].
#[derive(Clone, Debug, Default)]
pub struct IgnoredPaths {
    set: HashSet<PathBuf>,
}

impl IgnoredPaths {
    /// Whether the worktree-relative `relative` is ignored: either listed
    /// itself, or living under a listed directory. `Path` equality and
    /// hashing are component-wise, so `target/` from git matches both a
    /// lookup of `target` and the ancestor of `target/debug/app`.
    pub fn is_ignored(&self, relative: &Path) -> bool {
        relative
            .ancestors()
            .filter(|ancestor| !ancestor.as_os_str().is_empty())
            .any(|ancestor| self.set.contains(ancestor))
    }
}

/// The ignored paths of `repo`'s worktree. `--directory` keeps the output
/// small on real checkouts: a fully-ignored tree like `target/` is one
/// record, not one per file beneath it.
pub fn ignored_paths(repo: &Path) -> Result<IgnoredPaths, GitError> {
    let output = git::run_accepting(
        &[
            "ls-files",
            "-z",
            "--others",
            "--ignored",
            "--exclude-standard",
            "--directory",
        ],
        repo,
        &[0],
    )?;
    Ok(parse_ignored(&output.stdout))
}

/// Parses raw `git ls-files -z` output: NUL-separated paths, kept as raw
/// bytes by `-z` so spaces and non-ASCII need no quoting round-trip.
pub fn parse_ignored(output: &[u8]) -> IgnoredPaths {
    IgnoredPaths {
        set: output
            .split(|byte| *byte == 0)
            .filter(|record| !record.is_empty())
            .map(|record| PathBuf::from(String::from_utf8_lossy(record).into_owned()))
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_ignored_file_and_its_exact_path_match() {
        let ignored = parse_ignored(b".env\0notes with spaces.txt\0");

        assert!(ignored.is_ignored(Path::new(".env")));
        assert!(ignored.is_ignored(Path::new("notes with spaces.txt")));
        assert!(!ignored.is_ignored(Path::new("src/main.rs")));
    }

    #[test]
    fn an_ignored_directory_covers_itself_and_everything_beneath_it() {
        // git emits a fully-ignored directory with a trailing slash and
        // never lists its children.
        let ignored = parse_ignored(b"target/\0");

        assert!(ignored.is_ignored(Path::new("target")));
        assert!(ignored.is_ignored(Path::new("target/debug")));
        assert!(ignored.is_ignored(Path::new("target/debug/app")));
        assert!(!ignored.is_ignored(Path::new("target-notes.md")));
        assert!(!ignored.is_ignored(Path::new("src")));
    }

    #[test]
    fn empty_output_and_the_default_ignore_nothing() {
        assert!(!parse_ignored(b"").is_ignored(Path::new("anything")));
        assert!(!IgnoredPaths::default().is_ignored(Path::new("anything")));
    }
}
