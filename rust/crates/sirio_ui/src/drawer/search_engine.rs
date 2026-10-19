//! The drawer's Search engine: the in-file find's matcher run over a worktree.
//!
//! Pure functions, no UI. `hits_in` and `replace_in` work on one file's
//! LF-normalised text; `search_worktree` lists the files, reads them (an open,
//! unsaved tab's text wins over the disk) and stops at a cap or a cancel.
//! Every range is a byte range in the LF text, so a CRLF file is matched as LF
//! and `è😀漢` is never split.

use std::collections::{BTreeMap, HashMap};
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use ely_gpui_component::editor::{FindOptions, Hit, find_all, passes};

use crate::editor::{LineEnding, MAX_FILE_BYTES, is_binary};

/// How many matches a run keeps before it stops and says so.
pub const MATCH_CAP: usize = 20_000;

/// What to look for and in which files: the text with its options, and the
/// comma-separated globs the path must pass.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SearchQuery {
    pub text: String,
    pub options: FindOptions,
    pub include: String,
    pub exclude: String,
}

/// One line that a match starts on. `ranges_in_file` holds the same matches'
/// unclipped byte ranges in the file's LF text, which `hit.ranges` clips to
/// the line.
#[derive(Clone, Debug, PartialEq)]
pub struct FoundHit {
    pub hit: Hit,
    pub ranges_in_file: Vec<Range<usize>>,
}

/// Why a listed file was not searched.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Skip {
    TooLarge,
    Binary,
    NotUtf8,
    Unreadable,
}

/// A file with at least one match.
#[derive(Clone, Debug, PartialEq)]
pub struct FileResult {
    pub path: PathBuf,
    pub hits: Vec<FoundHit>,
}

/// A run's end: the files that matched, the files left unread and why, whether
/// the cap stopped it, and the error that did (a pattern that does not
/// compile, or git failing to list the files).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SearchOutcome {
    pub files: Vec<FileResult>,
    pub skipped: BTreeMap<Skip, usize>,
    pub capped: bool,
    pub error: Option<String>,
}

/// The matches of `query` in `lf_text`, one `FoundHit` per line a match starts on.
pub fn hits_in(lf_text: &str, query: &SearchQuery) -> Result<Vec<FoundHit>, String> {
    let matches = find_all(lf_text, &query.text, query.options)?;
    let mut found: Vec<FoundHit> = Vec::new();
    // The line the last match started on, with the byte offset it begins at,
    // so the lines between two matches are counted once.
    let (mut line, mut line_start, mut scanned) = (0usize, 0usize, 0usize);
    for range in matches {
        for (offset, _) in lf_text[scanned..range.start].match_indices('\n') {
            line += 1;
            line_start = scanned + offset + 1;
        }
        scanned = range.start;
        let line_end = lf_text[line_start..]
            .find('\n')
            .map_or(lf_text.len(), |at| line_start + at);
        let clipped = range.start - line_start..range.end.min(line_end) - line_start;
        match found.last_mut() {
            Some(last) if last.hit.line == line => {
                last.hit.ranges.push(clipped);
                last.ranges_in_file.push(range);
            }
            _ => found.push(FoundHit {
                hit: Hit {
                    line,
                    text: lf_text[line_start..line_end].to_string().into(),
                    ranges: vec![clipped],
                },
                ranges_in_file: vec![range],
            }),
        }
    }
    Ok(found)
}

/// `lf_text` with every match of `query` replaced by `with`, literally, and
/// how many there were.
pub fn replace_in(
    lf_text: &str,
    query: &SearchQuery,
    with: &str,
) -> Result<(String, usize), String> {
    let matches = find_all(lf_text, &query.text, query.options)?;
    let mut replaced = String::with_capacity(lf_text.len());
    let mut at = 0;
    for range in &matches {
        replaced.push_str(&lf_text[at..range.start]);
        replaced.push_str(with);
        at = range.end;
    }
    replaced.push_str(&lf_text[at..]);
    Ok((replaced, matches.len()))
}

/// Searches the worktree's listed files (`sirio_git::listed_files`).
/// `buffers` holds the dirty tabs' LF text by worktree-relative path and wins
/// over the disk; disk text is normalised to LF first. The run stops at
/// [`MATCH_CAP`] matches (`capped`; the file that crosses it keeps its hits up
/// to it) or when `cancel` is set, checked between files. `on_file` hears each
/// file with a match as it is found; the same results end up in the outcome.
pub fn search_worktree(
    worktree: &Path,
    query: &SearchQuery,
    buffers: &HashMap<PathBuf, String>,
    cancel: &AtomicBool,
    on_file: impl FnMut(FileResult),
) -> SearchOutcome {
    match sirio_git::listed_files(worktree) {
        Ok(files) => search_files(worktree, files, query, buffers, cancel, on_file),
        Err(error) => SearchOutcome {
            error: Some(error.to_string()),
            ..SearchOutcome::default()
        },
    }
}

/// What reading one listed file came to.
enum Read {
    Text(String),
    /// Not a file any more, or not a regular one (a submodule's directory):
    /// nothing to report.
    Gone,
    Skipped(Skip),
}

fn read_listed(path: &Path) -> Read {
    let Ok(metadata) = std::fs::metadata(path) else {
        // A tracked file deleted from disk is still listed.
        return match std::fs::symlink_metadata(path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Read::Gone,
            _ => Read::Skipped(Skip::Unreadable),
        };
    };
    if !metadata.is_file() {
        return Read::Gone;
    }
    if metadata.len() > MAX_FILE_BYTES {
        return Read::Skipped(Skip::TooLarge);
    }
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Read::Gone,
        Err(_) => return Read::Skipped(Skip::Unreadable),
    };
    // The file may have grown between the metadata and the read.
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Read::Skipped(Skip::TooLarge);
    }
    match String::from_utf8(bytes) {
        Err(_) => Read::Skipped(Skip::NotUtf8),
        Ok(text) if is_binary(text.as_bytes()) => Read::Skipped(Skip::Binary),
        Ok(text) => Read::Text(LineEnding::normalize(&text)),
    }
}

/// `search_worktree` over a given list of relative paths.
pub(crate) fn search_files(
    worktree: &Path,
    files: Vec<PathBuf>,
    query: &SearchQuery,
    buffers: &HashMap<PathBuf, String>,
    cancel: &AtomicBool,
    mut on_file: impl FnMut(FileResult),
) -> SearchOutcome {
    let mut outcome = SearchOutcome::default();
    if query.text.is_empty() {
        return outcome;
    }
    let mut total = 0usize;
    for relative in files {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        // Globs are matched against `/`-separated paths on every platform.
        let name = relative.to_string_lossy().replace('\\', "/");
        if !passes(&name, &query.include, &query.exclude) {
            continue;
        }
        let owned;
        let text = match buffers.get(&relative) {
            Some(text) => text,
            None => match read_listed(&worktree.join(&relative)) {
                Read::Text(text) => {
                    owned = text;
                    &owned
                }
                Read::Gone => continue,
                Read::Skipped(reason) => {
                    *outcome.skipped.entry(reason).or_default() += 1;
                    continue;
                }
            },
        };
        let mut hits = match hits_in(text, query) {
            Ok(hits) => hits,
            Err(error) => {
                outcome.error = Some(error);
                break;
            }
        };
        let in_file: usize = hits.iter().map(|found| found.ranges_in_file.len()).sum();
        if in_file == 0 {
            continue;
        }
        let room = MATCH_CAP - total;
        if in_file > room {
            outcome.capped = true;
            truncate_matches(&mut hits, room);
        }
        total += in_file.min(room);
        if !hits.is_empty() {
            let result = FileResult { path: relative, hits };
            on_file(result.clone());
            outcome.files.push(result);
        }
        if outcome.capped {
            break;
        }
    }
    outcome
}

/// Keeps the first `keep` matches of `hits`, dropping a hit left with none.
fn truncate_matches(hits: &mut Vec<FoundHit>, keep: usize) {
    let mut left = keep;
    hits.retain_mut(|found| {
        let take = found.ranges_in_file.len().min(left);
        found.ranges_in_file.truncate(take);
        found.hit.ranges.truncate(take);
        left -= take;
        take > 0
    });
}

#[cfg(test)]
mod tests {
    //! Failure list, written before the code. Each line is a way the engine
    //! could go wrong, and the test that notices:
    //!
    //! - a match is reported on the wrong line or at the wrong columns
    //!   (`a_literal_match_is_found_with_its_line_and_range`);
    //! - the options are ignored or inverted, so `Foo` matches `foo` with case
    //!   on, a word search finds a substring, or a regex is taken literally
    //!   (`case_word_and_regex_mean_what_find_all_means`);
    //! - two matches on one line become two hits, or the second is lost
    //!   (`two_matches_on_one_line_are_one_hit_with_two_ranges`);
    //! - a match spanning lines runs past its line's text, which the panel
    //!   would slice out of bounds, or is reported on its last line
    //!   (`a_match_spanning_lines_is_one_hit_on_its_first_line_clipped_to_its_end`);
    //! - a range lands inside `è😀漢` (`multibyte_ranges_fall_on_char_boundaries`);
    //! - a pattern that does not compile panics instead of reaching the
    //!   panel's error line (`an_invalid_regex_is_an_error_not_a_panic`);
    //! - the replacement is read as a template, so `$1` vanishes
    //!   (`replaces_every_match_literally_and_counts`);
    //! - a multi-line match is replaced only up to its first line
    //!   (`a_multiline_match_is_replaced_whole`);
    //! - a text with no match is returned changed or counted
    //!   (`no_match_returns_the_text_unchanged_and_zero`);
    //! - a file over the cap, a binary file or a non-UTF-8 file is searched,
    //!   or is skipped without being counted (`unsearchable_files_are_skipped_and_counted`);
    //! - a tracked file deleted from disk is counted as unreadable, which
    //!   would put a warning on every checkout with a deletion
    //!   (`a_file_deleted_from_disk_is_dropped_silently`);
    //! - an unsaved tab's text loses to the disk's (`an_open_buffer_wins_over_the_disk`);
    //! - a CRLF file is matched with its `\r` in the text
    //!   (`a_crlf_file_is_matched_as_lf`);
    //! - the include and exclude globs are applied after reading, or not at all
    //!   (`the_globs_choose_which_files_are_read`);
    //! - the run goes on past the cap, or the file that crosses it loses its
    //!   hits up to it (`the_cap_stops_the_run_and_keeps_the_crossing_files_hits`);
    //! - a set cancel flag is ignored (`a_cancelled_run_reads_nothing_more`);
    //! - a pattern that does not compile is silent
    //!   (`an_invalid_regex_in_a_run_is_the_outcomes_error`).

    use super::*;

    fn query(text: &str) -> SearchQuery {
        SearchQuery {
            text: text.into(),
            ..SearchQuery::default()
        }
    }

    fn with(text: &str, options: FindOptions) -> SearchQuery {
        SearchQuery {
            text: text.into(),
            options,
            ..SearchQuery::default()
        }
    }

    fn scratch(tag: &str) -> PathBuf {
        use std::sync::atomic::AtomicU64;
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "sirio-search-engine-{tag}-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn run(
        dir: &Path,
        files: &[&str],
        query: &SearchQuery,
        buffers: &HashMap<PathBuf, String>,
    ) -> SearchOutcome {
        let files = files.iter().map(PathBuf::from).collect();
        search_files(dir, files, query, buffers, &AtomicBool::new(false), |_| {})
    }

    #[test]
    fn a_literal_match_is_found_with_its_line_and_range() {
        let found = hits_in("one\ntwo foo\nthree\n", &query("foo")).unwrap();

        assert_eq!(found.len(), 1);
        assert_eq!(found[0].hit.line, 1);
        assert_eq!(found[0].hit.text.as_ref(), "two foo");
        assert_eq!(found[0].hit.ranges, vec![4..7]);
        assert_eq!(found[0].ranges_in_file, vec![8..11]);
    }

    #[test]
    fn case_word_and_regex_mean_what_find_all_means() {
        let text = "Foo foo foobar\n";
        let count = |query: &SearchQuery| {
            hits_in(text, query)
                .unwrap()
                .iter()
                .map(|found| found.ranges_in_file.len())
                .sum::<usize>()
        };

        assert_eq!(count(&query("foo")), 3);
        assert_eq!(count(&with("foo", FindOptions { case: true, ..Default::default() })), 2);
        assert_eq!(count(&with("foo", FindOptions { word: true, ..Default::default() })), 2);
        assert_eq!(count(&with("fo+", FindOptions { regex: true, ..Default::default() })), 3);
        assert_eq!(count(&query("fo+")), 0);
    }

    #[test]
    fn two_matches_on_one_line_are_one_hit_with_two_ranges() {
        let found = hits_in("ab ab\nab\n", &query("ab")).unwrap();

        assert_eq!(found.len(), 2);
        assert_eq!(found[0].hit.line, 0);
        assert_eq!(found[0].hit.ranges, vec![0..2, 3..5]);
        assert_eq!(found[0].ranges_in_file, vec![0..2, 3..5]);
        assert_eq!(found[1].hit.line, 1);
        assert_eq!(found[1].ranges_in_file, vec![6..8]);
    }

    #[test]
    fn a_match_spanning_lines_is_one_hit_on_its_first_line_clipped_to_its_end() {
        let regex = FindOptions { regex: true, ..Default::default() };
        let spanning = with(r"start\nmiddle", regex);
        let found = hits_in("x start\nmiddle end\nafter\n", &spanning).unwrap();

        assert_eq!(found.len(), 1);
        assert_eq!(found[0].hit.line, 0);
        assert_eq!(found[0].hit.text.as_ref(), "x start");
        assert_eq!(found[0].hit.ranges, vec![2..7]);
        assert_eq!(found[0].ranges_in_file, vec![2..14]);
    }

    #[test]
    fn multibyte_ranges_fall_on_char_boundaries() {
        let text = "è😀漢 and è😀漢\n";
        let found = hits_in(text, &query("😀漢")).unwrap();

        assert_eq!(found.len(), 1);
        for range in &found[0].ranges_in_file {
            assert!(text.is_char_boundary(range.start) && text.is_char_boundary(range.end));
        }
        for range in &found[0].hit.ranges {
            let line = found[0].hit.text.as_ref();
            assert_eq!(&line[range.clone()], "😀漢");
        }
        assert_eq!(found[0].hit.ranges.len(), 2);
    }

    #[test]
    fn an_invalid_regex_is_an_error_not_a_panic() {
        let regex = FindOptions { regex: true, ..Default::default() };

        assert!(hits_in("a(b\n", &with("(", regex)).is_err());
        assert!(replace_in("a(b\n", &with("(", regex), "x").is_err());
    }

    #[test]
    fn replaces_every_match_literally_and_counts() {
        let regex = FindOptions { regex: true, ..Default::default() };
        let (text, count) = replace_in("a1 b2\n", &with(r"(\w)(\d)", regex), "$1-$2").unwrap();

        assert_eq!(text, "$1-$2 $1-$2\n");
        assert_eq!(count, 2);
    }

    #[test]
    fn a_multiline_match_is_replaced_whole() {
        let regex = FindOptions { regex: true, ..Default::default() };
        let spanning = with(r"old\nold2", regex);
        let (text, count) = replace_in("keep\nold\nold2\nkeep\n", &spanning, "new").unwrap();

        assert_eq!(text, "keep\nnew\nkeep\n");
        assert_eq!(count, 1);
    }

    #[test]
    fn no_match_returns_the_text_unchanged_and_zero() {
        let (text, count) = replace_in("alpha\n", &query("zzz"), "x").unwrap();

        assert_eq!((text.as_str(), count), ("alpha\n", 0));
    }

    #[test]
    fn unsearchable_files_are_skipped_and_counted() {
        let dir = scratch("skips");
        std::fs::write(dir.join("big.txt"), vec![b'a'; MAX_FILE_BYTES as usize + 1]).unwrap();
        std::fs::write(dir.join("nul.bin"), b"a\0a").unwrap();
        std::fs::write(dir.join("latin1.txt"), b"caf\xe9 a").unwrap();
        std::fs::write(dir.join("ok.txt"), "a\n").unwrap();

        let outcome = run(
            &dir,
            &["big.txt", "nul.bin", "latin1.txt", "ok.txt"],
            &query("a"),
            &HashMap::new(),
        );

        assert_eq!(outcome.files.len(), 1);
        assert_eq!(outcome.files[0].path, PathBuf::from("ok.txt"));
        assert_eq!(outcome.skipped.get(&Skip::TooLarge), Some(&1));
        assert_eq!(outcome.skipped.get(&Skip::Binary), Some(&1));
        assert_eq!(outcome.skipped.get(&Skip::NotUtf8), Some(&1));
        assert_eq!(outcome.skipped.get(&Skip::Unreadable), None);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_file_deleted_from_disk_is_dropped_silently() {
        let dir = scratch("deleted");
        std::fs::write(dir.join("here.txt"), "a\n").unwrap();

        let outcome = run(&dir, &["gone.txt", "here.txt"], &query("a"), &HashMap::new());

        assert_eq!(outcome.files.len(), 1);
        assert!(outcome.skipped.is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn an_open_buffer_wins_over_the_disk() {
        let dir = scratch("buffer");
        std::fs::write(dir.join("a.txt"), "on disk\n").unwrap();
        std::fs::write(dir.join("b.txt"), "unsaved\n").unwrap();
        let buffers = HashMap::from([
            (PathBuf::from("a.txt"), "typed unsaved\n".to_string()),
            (PathBuf::from("b.txt"), "typed\n".to_string()),
        ]);

        let outcome = run(&dir, &["a.txt", "b.txt"], &query("unsaved"), &buffers);

        assert_eq!(outcome.files.len(), 1);
        assert_eq!(outcome.files[0].path, PathBuf::from("a.txt"));
        assert_eq!(outcome.files[0].hits[0].hit.text.as_ref(), "typed unsaved");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_crlf_file_is_matched_as_lf() {
        let dir = scratch("crlf");
        std::fs::write(dir.join("w.txt"), "one\r\ntwo\r\n").unwrap();

        let outcome = run(&dir, &["w.txt"], &query("one\ntwo"), &HashMap::new());

        let hit = &outcome.files[0].hits[0];
        assert_eq!((hit.hit.line, hit.hit.text.as_ref()), (0, "one"));
        assert_eq!(hit.ranges_in_file, vec![0..7]);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_globs_choose_which_files_are_read() {
        let dir = scratch("globs");
        std::fs::write(dir.join("a.rs"), "a\n").unwrap();
        std::fs::write(dir.join("a.md"), "a\n").unwrap();
        std::fs::write(dir.join("b.rs"), "a\n").unwrap();
        // Unreadable if it were read: a directory named like a file.
        std::fs::create_dir(dir.join("skipped.txt")).unwrap();
        let query = SearchQuery {
            text: "a".into(),
            include: "*.rs".into(),
            exclude: "b.rs".into(),
            ..SearchQuery::default()
        };

        let outcome = run(&dir, &["a.rs", "a.md", "b.rs"], &query, &HashMap::new());

        let paths: Vec<_> = outcome.files.iter().map(|file| file.path.clone()).collect();
        assert_eq!(paths, vec![PathBuf::from("a.rs")]);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_cap_stops_the_run_and_keeps_the_crossing_files_hits() {
        let dir = scratch("cap");
        let body = "x\n".repeat(9_000);
        for name in ["1.txt", "2.txt", "3.txt", "4.txt"] {
            std::fs::write(dir.join(name), &body).unwrap();
        }
        let mut streamed = Vec::new();

        let outcome = search_files(
            &dir,
            ["1.txt", "2.txt", "3.txt", "4.txt"].iter().map(PathBuf::from).collect(),
            &query("x"),
            &HashMap::new(),
            &AtomicBool::new(false),
            |file| streamed.push(file.path),
        );

        let total: usize = outcome
            .files
            .iter()
            .flat_map(|file| &file.hits)
            .map(|found| found.ranges_in_file.len())
            .sum();
        assert!(outcome.capped);
        assert_eq!(total, MATCH_CAP);
        let paths: Vec<_> = outcome.files.iter().map(|file| file.path.clone()).collect();
        let expected: Vec<_> = ["1.txt", "2.txt", "3.txt"].iter().map(PathBuf::from).collect();
        assert_eq!(paths, expected);
        assert_eq!(streamed, paths);
        assert_eq!(outcome.files[2].hits.len(), 2_000);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_cancelled_run_reads_nothing_more() {
        let dir = scratch("cancel");
        std::fs::write(dir.join("a.txt"), "a\n").unwrap();
        std::fs::write(dir.join("b.txt"), "a\n").unwrap();
        let cancel = AtomicBool::new(false);
        let mut seen = 0;

        let outcome = search_files(
            &dir,
            vec![PathBuf::from("a.txt"), PathBuf::from("b.txt")],
            &query("a"),
            &HashMap::new(),
            &cancel,
            |_| {
                seen += 1;
                cancel.store(true, Ordering::Relaxed);
            },
        );

        assert_eq!(seen, 1);
        assert_eq!(outcome.files.len(), 1);
        assert!(!outcome.capped);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn an_invalid_regex_in_a_run_is_the_outcomes_error() {
        let dir = scratch("badregex");
        std::fs::write(dir.join("a.txt"), "a(\n").unwrap();
        let regex = FindOptions { regex: true, ..Default::default() };

        let outcome = run(&dir, &["a.txt"], &with("(", regex), &HashMap::new());

        assert!(outcome.error.is_some());
        assert!(outcome.files.is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }
}
