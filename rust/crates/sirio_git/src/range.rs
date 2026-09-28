//! A change request's diff, read from two commits already in the object
//! store (spec §4). The list of files and their counts cost two cheap git
//! processes; one file's diff is read only when its row is opened, because a
//! process's captured output is capped (`DEFAULT_OUTPUT_LIMIT_BYTES`) and a
//! whole-change-request patch can exceed it.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::diff::{DiffStat, FileDiff, parse_diff, parse_numstat};
use crate::error::GitError;
use crate::git;

/// A full commit id: 40 (SHA-1) or 64 (SHA-256) hex digits. The one check
/// between a string a forge sent and git's command line, where a leading `-`
/// would be an option.
pub fn is_commit_id(value: &str) -> bool {
    matches!(value.len(), 40 | 64) && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// One file a range touches: git's status letter, its path (the new one for a
/// rename or copy) and the old one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RangeFile {
    pub status: char,
    pub path: PathBuf,
    pub old_path: Option<PathBuf>,
}

fn require_commit_id(value: &str) -> Result<(), GitError> {
    if is_commit_id(value) {
        Ok(())
    } else {
        Err(GitError::CommandFailed {
            code: 128,
            stderr: format!("not a commit id: {value:?}"),
        })
    }
}

/// `base...head`: the changes on `head` since it left `base`'s history — what
/// a forge shows, and why `base` need not be `head`'s parent.
fn range_spec(base: &str, head: &str) -> Result<String, GitError> {
    require_commit_id(base)?;
    require_commit_id(head)?;
    Ok(format!("{base}...{head}"))
}

/// The files `base...head` changes.
pub fn range_files(repo: &Path, base: &str, head: &str) -> Result<Vec<RangeFile>, GitError> {
    let spec = range_spec(base, head)?;
    let output = git::run_accepting(
        &[
            "-c", "core.quotePath=false", "diff", "--name-status", "-z", "-M", "--no-color",
            "--no-ext-diff", &spec, "--",
        ],
        repo,
        &[0],
    )?;
    Ok(parse_name_status_z(&output.stdout))
}

/// Per-file counts for `base...head`, keyed by the new path.
pub fn range_stats(
    repo: &Path,
    base: &str,
    head: &str,
) -> Result<HashMap<PathBuf, DiffStat>, GitError> {
    let spec = range_spec(base, head)?;
    let output = git::run_accepting(
        &[
            "-c", "core.quotePath=false", "diff", "--numstat", "-z", "-M", "--no-color",
            "--no-ext-diff", &spec, "--",
        ],
        repo,
        &[0],
    )?;
    Ok(parse_numstat(&output.stdout_string()))
}

/// One file's diff in `base...head`. A renamed file passes both paths, or
/// git would render the rename as a deletion plus an addition.
pub fn range_file_diff(
    repo: &Path,
    base: &str,
    head: &str,
    path: &Path,
    old_path: Option<&Path>,
    context_lines: usize,
) -> Result<FileDiff, GitError> {
    let spec = range_spec(base, head)?;
    let mut args = vec![
        "-c".to_string(),
        "core.quotePath=false".to_string(),
        "diff".to_string(),
        "--no-color".to_string(),
        "--no-ext-diff".to_string(),
        "-M".to_string(),
        format!("-U{context_lines}"),
        "--patch".to_string(),
        spec,
        "--".to_string(),
        format!(":(literal){}", git::path_arg(path)),
    ];
    if let Some(old) = old_path {
        args.push(format!(":(literal){}", git::path_arg(old)));
    }
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let output = git::run_accepting(&args, repo, &[0])?;
    Ok(parse_diff(&output.stdout_string(), path))
}

/// A file's bytes at `sha`, exactly as stored (no filters, no text
/// conversion). A missing path is git's own error.
pub fn show_blob(repo: &Path, sha: &str, path: &Path) -> Result<Vec<u8>, GitError> {
    require_commit_id(sha)?;
    let object = format!("{sha}:{}", git::path_arg(path));
    let output = git::run_accepting(&["cat-file", "blob", &object], repo, &[0])?;
    Ok(output.stdout)
}

/// Parses `git diff --name-status -z -M`: records are `STATUS\0path\0`, and
/// `R<score>`/`C<score>` carry `old\0new\0`. Only NUL separates, so a path
/// with a newline, quote or space survives. A truncated record ends the
/// list instead of panicking.
fn parse_name_status_z(output: &[u8]) -> Vec<RangeFile> {
    // The last record ends in a NUL too; without this, the empty field after
    // it would be read as the missing path of a truncated record.
    let body = output.strip_suffix(&[0]).unwrap_or(output);
    let mut fields = body
        .split(|byte| *byte == 0)
        .map(|field| String::from_utf8_lossy(field).into_owned());
    let mut files = Vec::new();
    while let Some(status) = fields.next() {
        let Some(letter) = status.chars().next() else {
            continue;
        };
        if matches!(letter, 'R' | 'C') {
            let (Some(old), Some(new)) = (fields.next(), fields.next()) else {
                break;
            };
            files.push(RangeFile {
                status: letter,
                path: PathBuf::from(new),
                old_path: Some(PathBuf::from(old)),
            });
        } else {
            let Some(path) = fields.next() else {
                break;
            };
            files.push(RangeFile {
                status: letter,
                path: PathBuf::from(path),
                old_path: None,
            });
        }
    }
    files
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(status: char, path: &str, old: Option<&str>) -> RangeFile {
        RangeFile {
            status,
            path: PathBuf::from(path),
            old_path: old.map(PathBuf::from),
        }
    }

    #[test]
    fn nothing_gives_no_files() {
        assert_eq!(parse_name_status_z(b""), vec![]);
    }

    #[test]
    fn plain_statuses_carry_one_path() {
        let parsed = parse_name_status_z(b"M\0a.txt\0A\0dir/b.txt\0D\0c.txt\0T\0d.txt\0");
        assert_eq!(
            parsed,
            vec![
                file('M', "a.txt", None),
                file('A', "dir/b.txt", None),
                file('D', "c.txt", None),
                file('T', "d.txt", None),
            ]
        );
    }

    #[test]
    fn a_rename_or_copy_carries_both_paths_new_one_first_in_path() {
        let parsed = parse_name_status_z(b"R100\0old name.md\0new name.md\0C75\0x\0y\0");
        assert_eq!(
            parsed,
            vec![
                file('R', "new name.md", Some("old name.md")),
                file('C', "y", Some("x")),
            ]
        );
    }

    #[test]
    fn only_nul_separates_so_newlines_spaces_and_unicode_survive() {
        let parsed = parse_name_status_z("M\0we\nird \"q\" ünï.txt\0".as_bytes());
        assert_eq!(parsed, vec![file('M', "we\nird \"q\" ünï.txt", None)]);
    }

    #[test]
    fn a_truncated_record_stops_without_panicking() {
        assert_eq!(parse_name_status_z(b"R100\0only-old\0"), vec![]);
        assert_eq!(parse_name_status_z(b"M\0"), vec![]);
        assert_eq!(
            parse_name_status_z(b"M\0ok\0R90\0old\0"),
            vec![file('M', "ok", None)]
        );
    }
}
