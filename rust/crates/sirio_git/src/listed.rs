//! The files a worktree search reads: tracked and untracked, ignore rules
//! applied.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::GitError;
use crate::git;

/// What `listed_files` found: the files whose names are text, and how many
/// more git lists whose names are not (a path is bytes on Linux; `PathBuf`
/// here is built from text, so such a name would name a file that does not
/// exist).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Listed {
    pub files: Vec<PathBuf>,
    /// Distinct listed names that are not UTF-8: counted, never turned into a path.
    pub not_utf8: usize,
}

/// Every file of `worktree` git lists as tracked or as untracked and not
/// ignored (`ls-files -z --cached --others --exclude-standard`), relative to
/// it. A tracked file deleted from disk is still listed, and an unmerged file
/// appears once per stage in git's output but once here. A name that is not
/// UTF-8 is counted in `not_utf8`, not listed.
pub fn listed_files(worktree: &Path) -> Result<Listed, GitError> {
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

fn parse_listed(output: &[u8]) -> Listed {
    let mut seen = HashSet::new();
    let mut listed = Listed::default();
    for record in output.split(|byte| *byte == 0).filter(|record| !record.is_empty()) {
        if !seen.insert(record) {
            continue;
        }
        match std::str::from_utf8(record) {
            Ok(name) => listed.files.push(PathBuf::from(name)),
            Err(_) => listed.not_utf8 += 1,
        }
    }
    listed
}

#[cfg(test)]
mod tests {
    //! Ways the listing could lose a file without saying so:
    //! - a name that is not UTF-8 is turned into a lossy path that does not
    //!   exist, and the search drops it silently
    //!   (`a_name_that_is_not_utf8_is_counted_not_listed`);
    //! - an unmerged file's stages are listed or counted once each
    //!   (`an_unmerged_name_is_listed_once`).
    use super::*;

    #[test]
    fn a_name_that_is_not_utf8_is_counted_not_listed() {
        let listed = parse_listed(b"a.txt\0bad\xff.txt\0dir/b.txt\0bad\xff.txt\0other\xfe\0");

        assert_eq!(listed.files, vec![PathBuf::from("a.txt"), PathBuf::from("dir/b.txt")]);
        assert_eq!(listed.not_utf8, 2);
    }

    #[test]
    fn an_unmerged_name_is_listed_once() {
        let listed = parse_listed(b"a.txt\0a.txt\0a.txt\0");

        assert_eq!(listed, Listed { files: vec![PathBuf::from("a.txt")], not_utf8: 0 });
    }
}
