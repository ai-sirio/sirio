//! The Changes panel's status model: what `git status` reports for a
//! checkout, parsed from `git status --porcelain=v2 -z`.

use std::fmt;
use std::path::PathBuf;

use crate::GitError;
use crate::git;

/// What kind of change git reports for a path, in the index or the worktree.
///
/// Mirrors `GitFileState` from the Swift app's `SirioGit` package.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum StatusKind {
    Modified,
    Added,
    Deleted,
    Renamed,
    Copied,
    TypeChanged,
    Unmerged,
    Untracked,
}

/// One changed path, with its index and worktree states.
///
/// A single file can carry two states at once (e.g. staged *and* further
/// modified): `index_status` and `worktree_status` are independent, exactly
/// like the two status columns of porcelain. Mirrors `GitStatusEntry` from the
/// Swift app.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StatusEntry {
    /// Repo-relative path of the file, as it exists now.
    pub path: PathBuf,
    /// The path a renamed/copied entry came from (rename source).
    pub original_path: Option<PathBuf>,
    /// The file's state in the index (staged), if any.
    pub index_status: Option<StatusKind>,
    /// The file's state in the worktree, if any.
    pub worktree_status: Option<StatusKind>,
}

impl StatusEntry {
    /// Whether the file has staged (index) changes.
    pub fn is_staged(&self) -> bool {
        matches!(self.index_status, Some(status) if status != StatusKind::Untracked)
    }

    /// Whether the file has unstaged worktree changes.
    pub fn has_worktree_changes(&self) -> bool {
        matches!(self.worktree_status, Some(status) if status != StatusKind::Untracked)
    }

    /// Whether the file is untracked (not yet known to git).
    pub fn is_untracked(&self) -> bool {
        self.index_status == Some(StatusKind::Untracked)
            || self.worktree_status == Some(StatusKind::Untracked)
    }

    /// Whether the file is in a conflicted (unmerged) state.
    pub fn is_conflicted(&self) -> bool {
        self.index_status == Some(StatusKind::Unmerged)
            || self.worktree_status == Some(StatusKind::Unmerged)
    }
}

/// A full `git status` snapshot for a checkout.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StatusSnapshot {
    /// All changed paths, sorted by path.
    pub entries: Vec<StatusEntry>,
}

impl StatusSnapshot {
    /// The empty snapshot.
    pub fn empty() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Entries with staged (index) changes.
    pub fn staged(&self) -> Vec<&StatusEntry> {
        self.entries
            .iter()
            .filter(|entry| entry.is_staged())
            .collect()
    }

    /// Entries with unstaged worktree changes (untracked excluded, matching
    /// the Swift app's "Changes" section).
    pub fn changes(&self) -> Vec<&StatusEntry> {
        self.entries
            .iter()
            .filter(|entry| entry.has_worktree_changes() && !entry.is_untracked())
            .collect()
    }

    /// Untracked entries.
    pub fn untracked(&self) -> Vec<&StatusEntry> {
        self.entries
            .iter()
            .filter(|entry| entry.is_untracked())
            .collect()
    }

    /// Whether there are no changes at all.
    pub fn is_clean(&self) -> bool {
        self.entries.is_empty()
    }
}

/// An error while parsing porcelain v2 output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StatusParseError {
    /// A record was shorter than the porcelain format requires.
    MalformedRecord,
    /// A rename record was not followed by the rename source path.
    MissingRenameSource {
        /// The renamed path that lacks its source.
        path: String,
    },
    /// A status byte outside the porcelain vocabulary.
    UnsupportedStatus(u8),
}

impl fmt::Display for StatusParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StatusParseError::MalformedRecord => {
                write!(f, "git returned a malformed porcelain status record")
            }
            StatusParseError::MissingRenameSource { path } => {
                write!(f, "git omitted the rename source for {path}")
            }
            StatusParseError::UnsupportedStatus(byte) => {
                write!(f, "git returned unsupported status byte {byte}")
            }
        }
    }
}

impl std::error::Error for StatusParseError {}

/// Loads the status of `repo` by running `git status --porcelain=v2 -z
/// --untracked-files=all`.
///
/// Porcelain v2 (not v1) is used on purpose: it reports the index and
/// worktree states independently (a file can be staged *and* further
/// modified), it carries rename detection results, and with `-z` paths are
/// NUL-terminated raw bytes, so paths with spaces or non-ASCII characters
/// need no quoting round-trip.
pub fn status(repo: &std::path::Path) -> Result<StatusSnapshot, GitError> {
    let _perf = sirio_perf::span("git.status", 0);
    let output = git::run_accepting(
        &["status", "--porcelain=v2", "-z", "--untracked-files=all"],
        repo,
        &[0],
    )?;
    parse_status(&output.stdout).map_err(|error| GitError::InvalidOutput {
        message: error.to_string(),
    })
}

/// Parses raw `git status --porcelain=v2 -z` output into a snapshot.
///
/// Records are NUL-separated. The format:
///
/// ```text
/// 1 <XY> <sub> <mH> <mI> <mW> <hH> <hI> <path>            ordinary change
/// 2 <XY> <sub> <mH> <mI> <mW> <hH> <hI> <X><score> <path> rename/copy
/// u <XY> <sub> <m1> <m2> <m3> <mW> <h1> <h2> <h3> <path>  unmerged
/// ? <path>                                                untracked
/// ```
///
/// For rename/copy records the original path is the *next* NUL-terminated
/// record. Paths are everything after the fixed metadata fields, so they may
/// contain spaces. `# ` lines (branch headers) are ignored. Because paths are
/// read as raw bytes, fields are located by counting space separators, not by
/// splitting the path itself.
pub fn parse_status(output: &[u8]) -> Result<StatusSnapshot, StatusParseError> {
    let records: Vec<&[u8]> = output
        .split(|byte| *byte == 0)
        .filter(|record| !record.is_empty())
        .collect();

    let mut entries = Vec::new();
    let mut index = 0;
    while index < records.len() {
        let record = records[index];
        if record.starts_with(b"# ") {
            index += 1;
            continue;
        }

        match record.first() {
            Some(b'1') => {
                entries.push(parse_ordinary_record(record)?);
                index += 1;
            }
            Some(b'2') => {
                let mut entry = parse_rename_record(record)?;
                index += 1;
                if index >= records.len() {
                    return Err(StatusParseError::MissingRenameSource {
                        path: entry.path.to_string_lossy().into_owned(),
                    });
                }
                entry.original_path = Some(PathBuf::from(
                    String::from_utf8_lossy(records[index]).into_owned(),
                ));
                index += 1;
                entries.push(entry);
            }
            Some(b'u') => {
                entries.push(parse_unmerged_record(record)?);
                index += 1;
            }
            Some(b'?') | Some(b'!') => {
                // Untracked (?) or ignored (!) file: v2 carries no X/Y for
                // these. Both states are set to Untracked, mirroring how the
                // Swift parser reads v1's `??`.
                entries.push(StatusEntry {
                    path: PathBuf::from(String::from_utf8_lossy(&record[2..]).into_owned()),
                    original_path: None,
                    index_status: Some(StatusKind::Untracked),
                    worktree_status: Some(StatusKind::Untracked),
                });
                index += 1;
            }
            _ => return Err(StatusParseError::MalformedRecord),
        }
    }

    entries.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(StatusSnapshot { entries })
}

/// Parses a `1 <XY> <sub> <mH> <mI> <mW> <hH> <hI> <path>` record.
fn parse_ordinary_record(record: &[u8]) -> Result<StatusEntry, StatusParseError> {
    // The path is everything after the 8th space separator, so it may itself
    // contain spaces.
    let fields = splitn_str(record, 9);
    let Some(fields) = fields else {
        return Err(StatusParseError::MalformedRecord);
    };
    let (index_status, worktree_status, path) = parse_xy_and_path(fields, 1, 8)?;
    Ok(StatusEntry {
        path,
        original_path: None,
        index_status,
        worktree_status,
    })
}

/// Parses a `2 <XY> <sub> <mH> <mI> <mW> <hH> <hI> <X><score> <path>` record.
/// The rename source is attached by the caller from the following record.
fn parse_rename_record(record: &[u8]) -> Result<StatusEntry, StatusParseError> {
    // The path is everything after the 9th space separator (the 8th field is
    // the `<X><score>` token, e.g. `R100`).
    let fields = splitn_str(record, 10);
    let Some(fields) = fields else {
        return Err(StatusParseError::MalformedRecord);
    };
    let (index_status, worktree_status, path) = parse_xy_and_path(fields, 1, 9)?;
    Ok(StatusEntry {
        path,
        original_path: None,
        index_status,
        worktree_status,
    })
}

/// Parses a `u <XY> <sub> <m1> <m2> <m3> <mW> <h1> <h2> <h3> <path>` record.
fn parse_unmerged_record(record: &[u8]) -> Result<StatusEntry, StatusParseError> {
    // The path is everything after the 10th space separator.
    let fields = splitn_str(record, 11);
    let Some(fields) = fields else {
        return Err(StatusParseError::MalformedRecord);
    };
    let (index_status, worktree_status, path) = parse_xy_and_path(fields, 1, 10)?;
    Ok(StatusEntry {
        path,
        original_path: None,
        index_status,
        worktree_status,
    })
}

/// Splits a record into at most `n` space-separated fields, returning the
/// field list only if the record had at least `n` separators (i.e. all
/// metadata fields are present). The final field is the raw remainder of the
/// record — the path — and may contain spaces.
fn splitn_str(record: &[u8], n: usize) -> Option<Vec<&[u8]>> {
    let mut fields = Vec::with_capacity(n);
    let mut rest = record;
    for _ in 1..n {
        let position = rest.iter().position(|byte| *byte == b' ')?;
        fields.push(&rest[..position]);
        rest = &rest[position + 1..];
    }
    fields.push(rest);
    Some(fields)
}

/// Extracts the XY status pair and the path (the last field) from a parsed
/// record. `xy_index` is the field index of the XY token (1 for all record
/// kinds), `path_index` the index of the path field.
fn parse_xy_and_path(
    fields: Vec<&[u8]>,
    xy_index: usize,
    path_index: usize,
) -> Result<(Option<StatusKind>, Option<StatusKind>, PathBuf), StatusParseError> {
    let xy = fields[xy_index];
    if xy.len() != 2 {
        return Err(StatusParseError::MalformedRecord);
    }
    let index_status = state_for(xy[0])?;
    let worktree_status = state_for(xy[1])?;
    let path = PathBuf::from(String::from_utf8_lossy(fields[path_index]).into_owned());
    Ok((index_status, worktree_status, path))
}

/// Maps a porcelain v2 status byte to a [`StatusKind`]. `.` (unmodified)
/// maps to `None`.
fn state_for(byte: u8) -> Result<Option<StatusKind>, StatusParseError> {
    let kind = match byte {
        b'.' => return Ok(None),
        b'M' => StatusKind::Modified,
        b'A' => StatusKind::Added,
        b'D' => StatusKind::Deleted,
        b'R' => StatusKind::Renamed,
        b'C' => StatusKind::Copied,
        b'T' => StatusKind::TypeChanged,
        b'U' => StatusKind::Unmerged,
        _ => return Err(StatusParseError::UnsupportedStatus(byte)),
    };
    Ok(Some(kind))
}

/// Whether the checkout has a commit yet (i.e. HEAD resolves). `false` for a
/// repo created with `git init` and nothing committed — an unborn HEAD.
/// Mirrors `GitRepository.hasHead` from the Swift app.
pub fn has_head(repo: &std::path::Path) -> bool {
    git::run_accepting(&["rev-parse", "--verify", "--quiet", "HEAD"], repo, &[0, 1])
        .is_ok_and(|output| output.is_success())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn path(value: &str) -> PathBuf {
        PathBuf::from(value)
    }

    #[test]
    fn parses_modified_untracked_and_rename_records() {
        // Real output shape from git 2.53, paths with spaces kept raw by -z.
        let mut output = Vec::new();
        output.extend_from_slice(b"1 .M N... 100644 100644 100644 83db48f84ec878fbfb30b46d16630e944e34f205 83db48f84ec878fbfb30b46d16630e944e34f205 file one.txt\0");
        output.extend_from_slice(b"2 R. N... 100644 100644 100644 3367afdbbf91e638efe983616377c60477cc6612 3367afdbbf91e638efe983616377c60477cc6612 R100 renamed2.txt\0renamed.txt\0");
        output.extend_from_slice(b"1 AM N... 000000 100644 100644 0000000000000000000000000000000000000000 3ed3870aac84a296bb0711eaf37096151f0c0378 staged.txt\0");
        output.extend_from_slice(b"? untracked.txt\0");

        let snapshot = parse_status(&output).expect("parses");
        // Sorted by path: file one.txt, renamed2.txt, staged.txt, untracked.txt.
        assert_eq!(snapshot.entries.len(), 4);

        let modified = &snapshot.entries[0];
        assert_eq!(modified.path, path("file one.txt"));
        assert_eq!(modified.index_status, None);
        assert_eq!(modified.worktree_status, Some(StatusKind::Modified));
        assert!(!modified.is_staged());
        assert!(modified.has_worktree_changes());

        let rename = &snapshot.entries[1];
        assert_eq!(rename.path, path("renamed2.txt"));
        assert_eq!(
            rename.original_path.as_deref(),
            Some(Path::new("renamed.txt"))
        );
        assert_eq!(rename.index_status, Some(StatusKind::Renamed));
        assert!(rename.is_staged());

        let both = &snapshot.entries[2];
        assert_eq!(both.path, path("staged.txt"));
        assert_eq!(both.index_status, Some(StatusKind::Added));
        assert_eq!(both.worktree_status, Some(StatusKind::Modified));
        assert!(both.is_staged());
        assert!(both.has_worktree_changes());

        let untracked = &snapshot.entries[3];
        assert_eq!(untracked.path, path("untracked.txt"));
        assert!(untracked.is_untracked());
        assert!(!untracked.is_staged());
    }

    #[test]
    fn parses_non_ascii_paths() {
        let mut output = Vec::new();
        output.extend_from_slice("1 .M N... 100644 100644 100644 83db48f84ec878fbfb30b46d16630e944e34f205 83db48f84ec878fbfb30b46d16630e944e34f205 café-ünïcode.txt\0".as_bytes());

        let snapshot = parse_status(&output).expect("parses");
        assert_eq!(snapshot.entries[0].path, path("café-ünïcode.txt"));
    }

    #[test]
    fn parses_unmerged_record() {
        // Real shape from git 2.53: `u <XY> <sub> <m1> <m2> <m3> <mW> <h1>
        // <h2> <h3> <path>` — four modes, three stage hashes.
        let mut output = Vec::new();
        output.extend_from_slice(
            b"u UU N... 100644 100644 100644 100644 df967b96a579e45a18b8251732d16804b2e56a55 ba2906d0666cf726c7eaadd2cd3db615dedfdf3a 2299c37978265a95cbe835a4b0f0bbf15aad5549 f.txt\0",
        );

        let snapshot = parse_status(&output).expect("parses");
        let entry = &snapshot.entries[0];
        assert_eq!(entry.path, path("f.txt"));
        assert!(entry.is_conflicted());
        assert_eq!(entry.index_status, Some(StatusKind::Unmerged));
        assert_eq!(entry.worktree_status, Some(StatusKind::Unmerged));
    }

    #[test]
    fn ignores_branch_header_lines() {
        let mut output = Vec::new();
        output.extend_from_slice(b"# branch.oid 3ff09b86479b467fcb2fb3628cf2e6d4c6d9bba8\0");
        output.extend_from_slice(b"# branch.head other\0");
        output.extend_from_slice(b"? new.txt\0");

        let snapshot = parse_status(&output).expect("parses");
        assert_eq!(snapshot.entries.len(), 1);
        assert_eq!(snapshot.entries[0].path, path("new.txt"));
    }

    #[test]
    fn empty_output_is_clean() {
        let snapshot = parse_status(b"").expect("parses");
        assert!(snapshot.is_clean());
        assert!(snapshot.entries.is_empty());
    }

    #[test]
    fn malformed_record_is_rejected() {
        assert_eq!(
            parse_status(b"garbage record here\0"),
            Err(StatusParseError::MalformedRecord)
        );
    }

    #[test]
    fn missing_rename_source_is_rejected() {
        let output = b"2 R. N... 100644 100644 100644 3367afdbbf91e638efe983616377c60477cc6612 3367afdbbf91e638efe983616377c60477cc6612 R100 renamed2.txt\0";
        assert_eq!(
            parse_status(output),
            Err(StatusParseError::MissingRenameSource {
                path: "renamed2.txt".to_string()
            })
        );
    }

    #[test]
    fn unsupported_status_byte_is_rejected() {
        let output = b"1 ZM N... 100644 100644 100644 83db48f84ec878fbfb30b46d16630e944e34f205 83db48f84ec878fbfb30b46d16630e944e34f205 x.txt\0";
        assert_eq!(
            parse_status(output),
            Err(StatusParseError::UnsupportedStatus(b'Z'))
        );
    }

    #[test]
    fn snapshot_sections_split_staged_changes_and_untracked() {
        let mut output = Vec::new();
        output.extend_from_slice(b"1 M. N... 100644 100644 100644 83db48f84ec878fbfb30b46d16630e944e34f205 3ed3870aac84a296bb0711eaf37096151f0c0378 staged-only.txt\0");
        output.extend_from_slice(b"1 .M N... 100644 100644 100644 83db48f84ec878fbfb30b46d16630e944e34f205 83db48f84ec878fbfb30b46d16630e944e34f205 worktree-only.txt\0");
        output.extend_from_slice(b"? new.txt\0");

        let snapshot = parse_status(&output).expect("parses");
        assert_eq!(snapshot.staged().len(), 1);
        assert_eq!(snapshot.staged()[0].path, path("staged-only.txt"));
        assert_eq!(snapshot.changes().len(), 1);
        assert_eq!(snapshot.changes()[0].path, path("worktree-only.txt"));
        assert_eq!(snapshot.untracked().len(), 1);
        assert_eq!(snapshot.untracked()[0].path, path("new.txt"));
        assert!(!snapshot.is_clean());
    }
}
