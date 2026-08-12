//! Unified-diff parsing and per-file line counts, matching the Swift app's
//! `GitDiff` and `GitDiffStats`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::GitError;
use crate::git;
use crate::status::{StatusEntry, has_head};

/// The origin of one diff line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiffOrigin {
    Context,
    Addition,
    Deletion,
}

/// One line of a hunk.
///
/// Both line numbers are carried so a side-by-side renderer can show them
/// next to each other: a context line has both, a deletion has no new number,
/// an addition has no old one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiffLine {
    pub origin: DiffOrigin,
    /// Line number in the old (HEAD) file, `None` for additions.
    pub old_line_number: Option<usize>,
    /// Line number in the new (worktree) file, `None` for deletions.
    pub new_line_number: Option<usize>,
    /// The line's content, without the diff marker (`+`/`-`/space) and with
    /// any trailing CR stripped (CRLF checkouts).
    pub content: String,
}

/// One hunk of a unified diff.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hunk {
    /// The raw header, e.g. `@@ -257,11 +257,23 @@ and trailing context`.
    pub header: String,
    /// Start line in the old file (the number after `-`).
    pub old_start: usize,
    /// Number of old lines covered by the hunk.
    pub old_lines: usize,
    /// Start line in the new file (the number after `+`).
    pub new_start: usize,
    /// Number of new lines covered by the hunk.
    pub new_lines: usize,
    /// The hunk's lines, in order.
    pub lines: Vec<DiffLine>,
}

/// The parsed unified diff for a single path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileDiff {
    /// The path the diff is for (the current path of the entry).
    pub path: PathBuf,
    /// The hunks of the diff, in order.
    pub hunks: Vec<Hunk>,
    /// Total added lines across all hunks.
    pub additions: usize,
    /// Total deleted lines across all hunks.
    pub deletions: usize,
    /// Whether git reported the file as binary (line counts are meaningless).
    pub is_binary: bool,
    /// Whether the diff mentions a submodule commit change.
    pub is_submodule: bool,
}

/// Per-file line counts, cheap enough to fetch for every changed file at
/// once. Binary files report zero counts with `is_binary` set, matching git's
/// `-` in numstat.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DiffStat {
    pub additions: usize,
    pub deletions: usize,
    pub is_binary: bool,
}

/// The unified context git emits when asked for the whole file: large enough
/// that every line lands in one hunk. Not `usize::MAX` — git multiplies the
/// value internally and would overflow.
pub const WHOLE_FILE_CONTEXT_LINES: usize = 100_000;

/// The default context lines for the Changes panel's hunk view.
pub const DEFAULT_CONTEXT_LINES: usize = 3;

/// Loads the unified diff for one status entry.
///
/// Mirrors `GitDiff.load` from the Swift app:
/// - untracked files and checkouts with an unborn HEAD have no HEAD version
///   to diff against, so they are diffed against `/dev/null` via
///   `git diff --no-index`;
/// - everything else diffs against `HEAD`, passing both the current path and
///   the rename source (a single-path pathspec would defeat git's rename
///   detection and render a rename as delete+add).
pub fn diff_entry(
    repo: &Path,
    entry: &StatusEntry,
    context_lines: usize,
) -> Result<FileDiff, GitError> {
    let unified = format!("--unified={context_lines}");
    let result = if entry.is_untracked() || !has_head(repo) {
        let absolute = repo.join(&entry.path);
        git::run_accepting(
            &[
                "diff",
                "--no-color",
                "--no-ext-diff",
                "--no-index",
                &unified,
                "--",
                "/dev/null",
                absolute.to_str().unwrap_or_default(),
            ],
            repo,
            &[0, 1],
        )
    } else {
        let mut pathspecs = Vec::new();
        if let Some(original) = &entry.original_path {
            pathspecs.push(original.to_string_lossy().into_owned());
        }
        pathspecs.push(entry.path.to_string_lossy().into_owned());
        let mut args = vec![
            "diff".to_string(),
            "--no-color".to_string(),
            "--no-ext-diff".to_string(),
            unified,
            "HEAD".to_string(),
            "--".to_string(),
        ];
        args.extend(pathspecs);
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        git::run_accepting(&args, repo, &[0])
    }?;

    Ok(parse_diff(&result.stdout_string(), &entry.path))
}

/// Parses the text of a unified diff into hunks.
///
/// Handles the full git diff preamble (`diff --git`, `index`, `--- a/`,
/// `+++ b/`, `new file mode`, `rename from/to`, `similarity index`,
/// `Binary files ...`, `GIT binary patch`, `\ No newline at end of file`) as
/// metadata that is skipped, and hunks (`@@ -a,b +c,d @@ trailing`) as the
/// structured part. Trailing CR characters are stripped from content lines so
/// CRLF checkouts diff cleanly.
pub fn parse_diff(output: &str, path: &Path) -> FileDiff {
    let mut hunks: Vec<Hunk> = Vec::new();
    let mut additions = 0usize;
    let mut deletions = 0usize;
    let mut is_binary = false;
    let mut is_submodule = false;

    // Cursor state inside the current hunk.
    let mut old_line: Option<usize> = None;
    let mut new_line: Option<usize> = None;

    for raw in output.lines() {
        if raw.is_empty() {
            continue;
        }

        if raw.starts_with("@@")
            && let Some(header) = parse_hunk_header(raw)
        {
            if let Some(old) = header.old_start {
                old_line = Some(old);
            }
            if let Some(new) = header.new_start {
                new_line = Some(new);
            }
            hunks.push(Hunk {
                header: raw.to_string(),
                old_start: header.old_start.unwrap_or(0),
                old_lines: header.old_lines.unwrap_or(0),
                new_start: header.new_start.unwrap_or(0),
                new_lines: header.new_lines.unwrap_or(0),
                lines: Vec::new(),
            });
            continue;
        }

        if raw.starts_with("diff --git ")
            || raw.starts_with("index ")
            || raw.starts_with("--- ")
            || raw.starts_with("+++ ")
            || raw.starts_with("new file mode ")
            || raw.starts_with("deleted file mode ")
            || raw.starts_with("old mode ")
            || raw.starts_with("new mode ")
            || raw.starts_with("rename from ")
            || raw.starts_with("rename to ")
            || raw.starts_with("similarity index ")
            || raw.starts_with("dissimilarity index ")
            || raw == "\\ No newline at end of file"
        {
            continue;
        }

        if raw.starts_with("Binary files ") || raw == "GIT binary patch" {
            is_binary = true;
            continue;
        }

        // A hunk-content line (context / addition / deletion).
        let current_hunk = hunks.last_mut();
        let content = raw
            .strip_prefix('+')
            .map(|rest| (DiffOrigin::Addition, rest));
        let content = content.or_else(|| {
            raw.strip_prefix('-')
                .map(|rest| (DiffOrigin::Deletion, rest))
        });
        let content = content.or_else(|| {
            raw.strip_prefix(' ')
                .map(|rest| (DiffOrigin::Context, rest))
        });

        let Some((origin, rest)) = content else {
            // Unknown preamble line (e.g. a `Subproject commit` line outside a
            // hunk); skip.
            continue;
        };

        let content = rest.strip_suffix('\r').unwrap_or(rest).to_string();
        if content.starts_with("Subproject commit ") {
            is_submodule = true;
        }

        let line = match origin {
            DiffOrigin::Addition => {
                additions += 1;
                DiffLine {
                    origin,
                    old_line_number: None,
                    new_line_number: new_line,
                    content,
                }
            }
            DiffOrigin::Deletion => {
                deletions += 1;
                DiffLine {
                    origin,
                    old_line_number: old_line,
                    new_line_number: None,
                    content,
                }
            }
            DiffOrigin::Context => DiffLine {
                origin,
                old_line_number: old_line,
                new_line_number: new_line,
                content,
            },
        };

        match origin {
            DiffOrigin::Addition => new_line = new_line.map(|n| n + 1),
            DiffOrigin::Deletion => old_line = old_line.map(|n| n + 1),
            DiffOrigin::Context => {
                old_line = old_line.map(|n| n + 1);
                new_line = new_line.map(|n| n + 1);
            }
        }

        if let Some(hunk) = current_hunk {
            hunk.lines.push(line);
        }
    }

    FileDiff {
        path: path.to_path_buf(),
        hunks,
        additions,
        deletions,
        is_binary,
        is_submodule,
    }
}

/// The parsed fields of a `@@ -a,b +c,d @@ trailing` header. Counts default
/// to 1 when omitted (`@@ -1 +1 @@`), and either side may be absent.
struct HunkHeader {
    old_start: Option<usize>,
    old_lines: Option<usize>,
    new_start: Option<usize>,
    new_lines: Option<usize>,
}

fn parse_hunk_header(line: &str) -> Option<HunkHeader> {
    let mut fields = line.split_whitespace();
    let _at = fields.next()?; // "@@"
    let old = fields.next()?; // "-257,11"
    let new = fields.next()?; // "+257,23"

    let parse_side = |field: &str| -> Option<(Option<usize>, Option<usize>)> {
        let digits = field.get(1..)?;
        let mut parts = digits.split(',');
        let start = parts.next()?.parse::<usize>().ok();
        // An omitted count in `@@ -1 +1 @@` means 1.
        let count = match parts.next() {
            Some(count) => count.parse::<usize>().ok(),
            None => Some(1),
        };
        Some((start, count))
    };

    let (old_start, old_lines) = parse_side(old)?;
    let (new_start, new_lines) = parse_side(new)?;
    Some(HunkHeader {
        old_start,
        old_lines,
        new_start,
        new_lines,
    })
}

/// Line counts for every changed file, in one git invocation plus a cheap
/// read per untracked file. Untracked files do not appear in `--numstat`
/// against HEAD at all, so their additions are counted from disk — mirroring
/// `GitDiffStats.stats` from the Swift app.
pub fn stats(repo: &Path, entries: &[StatusEntry]) -> Result<HashMap<PathBuf, DiffStat>, GitError> {
    let mut result = HashMap::new();

    if has_head(repo) {
        let output = git::run_accepting(
            &[
                "diff",
                "--numstat",
                "-z",
                "--no-color",
                "--no-ext-diff",
                "HEAD",
            ],
            repo,
            &[0],
        )?;
        result = parse_numstat(&output.stdout_string());
    }

    for entry in entries.iter().filter(|entry| entry.is_untracked()) {
        let absolute = repo.join(&entry.path);
        if let Some(lines) = line_count(&absolute) {
            result.insert(
                entry.path.clone(),
                DiffStat {
                    additions: lines,
                    deletions: 0,
                    is_binary: false,
                },
            );
        }
    }

    Ok(result)
}

/// Parses `git diff --numstat -z` output.
///
/// Records are `adds\tdels\tpath\0`; binary files report `-` for both counts;
/// renames report an empty path followed by two records — `\0<old>\0<new>\0`
/// — so the stat is keyed by the new path.
pub fn parse_numstat(output: &str) -> HashMap<PathBuf, DiffStat> {
    let parts: Vec<&str> = output.split('\0').collect();
    let mut stats = HashMap::new();
    let mut index = 0;

    while index < parts.len() {
        let record = parts[index];
        if record.is_empty() {
            index += 1;
            continue;
        }

        let mut fields = record.split('\t');
        let Some(adds) = fields.next() else {
            index += 1;
            continue;
        };
        let Some(dels) = fields.next() else {
            index += 1;
            continue;
        };
        let Some(path_value) = fields.next() else {
            index += 1;
            continue;
        };

        let is_binary = adds == "-";
        let additions = adds.parse::<usize>().unwrap_or(0);
        let deletions = dels.parse::<usize>().unwrap_or(0);

        let path: PathBuf = if path_value.is_empty() {
            // Rename: the new path is two records further on.
            if index + 2 >= parts.len() {
                break;
            }
            index += 3;
            PathBuf::from(parts[index - 1])
        } else {
            index += 1;
            PathBuf::from(path_value)
        };

        stats.insert(
            path,
            DiffStat {
                additions,
                deletions,
                is_binary,
            },
        );
    }

    stats
}

/// Counts the lines of a UTF-8 file, mirroring the Swift app's snapshot
/// limit (500 KB) and returning `None` for anything larger or non-UTF-8.
fn line_count(path: &Path) -> Option<usize> {
    let data = std::fs::read(path).ok()?;
    if data.len() > 500_000 {
        return None;
    }
    let text = std::str::from_utf8(&data).ok()?;
    if text.is_empty() {
        return Some(0);
    }
    if text.ends_with('\n') {
        Some(text.split('\n').count() - 1)
    } else {
        Some(text.split('\n').count())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_hunks_with_line_numbers() {
        let patch = concat!(
            "diff --git a/f.txt b/f.txt\n",
            "index 1111111..2222222 100644\n",
            "--- a/f.txt\n",
            "+++ b/f.txt\n",
            "@@ -1,3 +1,4 @@ some trailing context\n",
            " first\n",
            "-second\n",
            "+SECOND\n",
            " third\n",
            "+fourth\n",
        );

        let diff = parse_diff(patch, Path::new("f.txt"));

        assert_eq!(diff.hunks.len(), 1);
        let hunk = &diff.hunks[0];
        assert_eq!(hunk.header, "@@ -1,3 +1,4 @@ some trailing context");
        assert_eq!(hunk.old_start, 1);
        assert_eq!(hunk.old_lines, 3);
        assert_eq!(hunk.new_start, 1);
        assert_eq!(hunk.new_lines, 4);

        assert_eq!(hunk.lines.len(), 5);
        assert_eq!(hunk.lines[0].origin, DiffOrigin::Context);
        assert_eq!(hunk.lines[0].old_line_number, Some(1));
        assert_eq!(hunk.lines[0].new_line_number, Some(1));
        assert_eq!(hunk.lines[0].content, "first");

        assert_eq!(hunk.lines[1].origin, DiffOrigin::Deletion);
        assert_eq!(hunk.lines[1].old_line_number, Some(2));
        assert_eq!(hunk.lines[1].new_line_number, None);
        assert_eq!(hunk.lines[1].content, "second");

        assert_eq!(hunk.lines[2].origin, DiffOrigin::Addition);
        assert_eq!(hunk.lines[2].old_line_number, None);
        assert_eq!(hunk.lines[2].new_line_number, Some(2));
        assert_eq!(hunk.lines[2].content, "SECOND");

        assert_eq!(diff.additions, 2);
        assert_eq!(diff.deletions, 1);
        assert!(!diff.is_binary);
    }

    #[test]
    fn strips_carriage_returns_from_crlf_lines() {
        let patch = "@@ -1,3 +1,4 @@\n a\r\n-b\r\n+B\r\n c\r\n+d\r\n";
        let diff = parse_diff(patch, Path::new("crlf.txt"));

        let hunk = &diff.hunks[0];
        assert_eq!(hunk.lines[0].content, "a");
        assert_eq!(hunk.lines[1].content, "b");
        assert_eq!(hunk.lines[2].content, "B");
        assert!(!hunk.lines[2].content.contains('\r'));
        assert_eq!(diff.additions, 2);
        assert_eq!(diff.deletions, 1);
    }

    #[test]
    fn handles_single_number_hunk_headers() {
        // `@@ -1 +1 @@` — counts default to 1.
        let patch = "@@ -1 +1 @@\n-old\n+new\n";
        let diff = parse_diff(patch, Path::new("f.txt"));

        let hunk = &diff.hunks[0];
        assert_eq!(hunk.old_start, 1);
        assert_eq!(hunk.old_lines, 1);
        assert_eq!(hunk.new_start, 1);
        assert_eq!(hunk.new_lines, 1);
        assert_eq!(hunk.lines.len(), 2);
    }

    #[test]
    fn handles_zero_zero_hunk_for_new_files() {
        let patch = "@@ -0,0 +1,2 @@\n+one\n+two\n";
        let diff = parse_diff(patch, Path::new("new.txt"));

        let hunk = &diff.hunks[0];
        assert_eq!(hunk.old_start, 0);
        assert_eq!(hunk.old_lines, 0);
        assert_eq!(hunk.new_start, 1);
        assert_eq!(diff.additions, 2);
    }

    #[test]
    fn marks_binary_diffs() {
        let patch = "diff --git a/blob.bin b/blob.bin\nindex 1111111..2222222 100644\nBinary files a/blob.bin and b/blob.bin differ\n";
        let diff = parse_diff(patch, Path::new("blob.bin"));

        assert!(diff.is_binary);
        assert!(diff.hunks.is_empty());
        assert_eq!(diff.additions, 0);
    }

    #[test]
    fn multiple_hunks_accumulate_line_numbers() {
        let patch = concat!(
            "@@ -1,2 +1,2 @@\n",
            " a\n",
            " b\n",
            "@@ -10,1 +11,1 @@\n",
            "-ten\n",
            "+TEN\n",
        );
        let diff = parse_diff(patch, Path::new("f.txt"));

        assert_eq!(diff.hunks.len(), 2);
        let second = &diff.hunks[1];
        assert_eq!(second.old_start, 10);
        assert_eq!(second.new_start, 11);
        assert_eq!(second.lines[0].old_line_number, Some(10));
        assert_eq!(second.lines[1].new_line_number, Some(11));
    }

    #[test]
    fn parses_numstat_with_binary_and_rename() {
        // Real shape from `git diff --numstat -z HEAD`: a rename record has an
        // empty path field followed by two NUL-separated records (old, new).
        let output =
            "2\t1\tfile one.txt\x00-\t-\tblob.bin\x000\t0\t\x00renamed.txt\x00renamed2.txt\x00";
        let stats = parse_numstat(output);

        assert_eq!(stats.len(), 3);
        assert_eq!(
            stats[&PathBuf::from("file one.txt")],
            DiffStat {
                additions: 2,
                deletions: 1,
                is_binary: false
            }
        );
        assert_eq!(
            stats[&PathBuf::from("blob.bin")],
            DiffStat {
                additions: 0,
                deletions: 0,
                is_binary: true
            }
        );
        // Rename: keyed by the NEW path.
        assert_eq!(
            stats[&PathBuf::from("renamed2.txt")],
            DiffStat {
                additions: 0,
                deletions: 0,
                is_binary: false
            }
        );
    }

    #[test]
    fn empty_numstat_is_empty() {
        assert!(parse_numstat("").is_empty());
    }
}
