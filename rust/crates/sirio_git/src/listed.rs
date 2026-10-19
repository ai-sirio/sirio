//! The files a worktree search reads: tracked and untracked, ignore rules
//! applied.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::GitError;
use crate::git;

/// Every file of `worktree` git lists as tracked or as untracked and not
/// ignored (`ls-files -z --cached --others --exclude-standard`), relative to
/// it. A tracked file deleted from disk is still listed, and an unmerged file
/// appears once per stage in git's output but once here.
pub fn listed_files(worktree: &Path) -> Result<Vec<PathBuf>, GitError> {
    let output = git::run_accepting(
        &[
            "ls-files",
            "-z",
            "--cached",
            "--others",
            "--exclude-standard",
        ],
        worktree,
        &[0],
    )?;
    Ok(parse_listed(&output.stdout))
}

fn parse_listed(output: &[u8]) -> Vec<PathBuf> {
    let mut seen = HashSet::new();
    output
        .split(|byte| *byte == 0)
        .filter(|record| !record.is_empty())
        .map(|record| PathBuf::from(String::from_utf8_lossy(record).into_owned()))
        .filter(|path| seen.insert(path.clone()))
        .collect()
}
