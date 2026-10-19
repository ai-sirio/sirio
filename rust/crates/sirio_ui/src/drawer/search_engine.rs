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

use ely_gpui_component::editor::{FindOptions, Finder, GlobFilter, Hit};

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
/// the cap stopped it, whether a cancel did (a cancelled outcome is partial and
/// must never feed Replace all), and the error that did (a pattern that does
/// not compile, or git failing to list the files).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SearchOutcome {
    pub files: Vec<FileResult>,
    pub skipped: BTreeMap<Skip, usize>,
    pub capped: bool,
    pub cancelled: bool,
    pub error: Option<String>,
}

/// The matches of `query` in `lf_text`, one `FoundHit` per line a match starts on.
pub fn hits_in(lf_text: &str, query: &SearchQuery) -> Result<Vec<FoundHit>, String> {
    Ok(hits_found(lf_text, &compile(query)?))
}

/// `hits_in` with the query already compiled, for a run over many files.
fn hits_found(lf_text: &str, finder: &Finder) -> Vec<FoundHit> {
    let mut found: Vec<FoundHit> = Vec::new();
    // The line of the last match: its number, where it starts and where it
    // ends (at its `\n`, or the end of the text). Matches ascend, so the walk
    // only moves forward and reads each line once: the cost is linear in the
    // text plus the matches, however long a line is.
    let line_end_from =
        |from: usize| lf_text[from..].find('\n').map_or(lf_text.len(), |at| from + at);
    let (mut line, mut line_start, mut line_end) = (0usize, 0usize, line_end_from(0));
    for range in finder.find_all(lf_text) {
        // A match that is itself a `\n` starts at its line's end and stays on it.
        while range.start > line_end {
            line += 1;
            line_start = line_end + 1;
            line_end = line_end_from(line_start);
        }
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
    found
}

/// `lf_text` with every match of `query` replaced by `with`, literally, and
/// how many there were.
pub fn replace_in(
    lf_text: &str,
    query: &SearchQuery,
    with: &str,
) -> Result<(String, usize), String> {
    let matches = compile(query)?.find_all(lf_text);
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

fn compile(query: &SearchQuery) -> Result<Finder, String> {
    Finder::new(&query.text, query.options)
}

/// Searches the worktree's listed files (`sirio_git::listed_files`).
/// `buffers` holds the dirty tabs' LF text by worktree-relative path and wins
/// over the disk; disk text is normalised to LF first. The query is compiled
/// before anything is listed, so a pattern that does not compile is the
/// outcome's `error` whatever the files are. The run stops at [`MATCH_CAP`]
/// matches (`capped`; the file that crosses it keeps its hits up to it) or
/// when `cancel` is set (`cancelled`), checked between files. Symlinks are
/// skipped without a word, as ripgrep does by default. `on_file` hears each
/// file with a match as it is found; the same results end up in the outcome.
pub fn search_worktree(
    worktree: &Path,
    query: &SearchQuery,
    buffers: &HashMap<PathBuf, String>,
    cancel: &AtomicBool,
    on_file: impl FnMut(FileResult),
) -> SearchOutcome {
    let finder = match compile(query) {
        Ok(finder) => finder,
        Err(error) => return failed(error),
    };
    match sirio_git::listed_files(worktree) {
        Ok(files) => walk(worktree, files, query, &finder, buffers, cancel, on_file),
        Err(error) => failed(error.to_string()),
    }
}

fn failed(error: String) -> SearchOutcome {
    SearchOutcome {
        error: Some(error),
        ..SearchOutcome::default()
    }
}

/// What reading one listed file came to.
enum Read {
    Text(String),
    /// Not a regular file any more (deleted, a directory such as a
    /// submodule's) or a symlink: nothing to report.
    Gone,
    Skipped(Skip),
}

fn read_listed(path: &Path) -> Read {
    use std::io::Read as _;

    // `symlink_metadata`, not `metadata`: a committed link to a file outside
    // the worktree must not put that file's lines in the results, and a link
    // to a file inside would list the same lines twice.
    match std::fs::symlink_metadata(path) {
        // A tracked file deleted from disk is still listed.
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Read::Gone,
        Err(_) => return Read::Skipped(Skip::Unreadable),
        Ok(metadata) if !metadata.file_type().is_file() => return Read::Gone,
        Ok(_) => {}
    }
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Read::Gone,
        Err(_) => return Read::Skipped(Skip::Unreadable),
    };
    // The opened file's size, then a read that stops one byte past the bound:
    // a file that grows between the two still reads as too large, and one that
    // reports no size is never read whole.
    match file.metadata() {
        Ok(metadata) if !metadata.is_file() => return Read::Gone,
        Ok(metadata) if metadata.len() > MAX_FILE_BYTES => return Read::Skipped(Skip::TooLarge),
        Ok(_) => {}
        Err(_) => return Read::Skipped(Skip::Unreadable),
    }
    let mut bytes = Vec::new();
    if file.take(MAX_FILE_BYTES + 1).read_to_end(&mut bytes).is_err() {
        return Read::Skipped(Skip::Unreadable);
    }
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Read::Skipped(Skip::TooLarge);
    }
    match String::from_utf8(bytes) {
        Err(_) => Read::Skipped(Skip::NotUtf8),
        Ok(text) if is_binary(text.as_bytes()) => Read::Skipped(Skip::Binary),
        Ok(text) => Read::Text(LineEnding::normalize(&text)),
    }
}

/// `search_worktree` over a given list of relative paths, for the tests: no git.
#[cfg(test)]
pub(crate) fn search_files(
    worktree: &Path,
    files: Vec<PathBuf>,
    query: &SearchQuery,
    buffers: &HashMap<PathBuf, String>,
    cancel: &AtomicBool,
    on_file: impl FnMut(FileResult),
) -> SearchOutcome {
    match compile(query) {
        Ok(finder) => walk(worktree, files, query, &finder, buffers, cancel, on_file),
        Err(error) => failed(error),
    }
}

fn walk(
    worktree: &Path,
    files: Vec<PathBuf>,
    query: &SearchQuery,
    finder: &Finder,
    buffers: &HashMap<PathBuf, String>,
    cancel: &AtomicBool,
    mut on_file: impl FnMut(FileResult),
) -> SearchOutcome {
    let mut outcome = SearchOutcome::default();
    if query.text.is_empty() {
        return outcome;
    }
    // Compiled once for the run; a glob that cannot be compiled logs here once.
    let globs = GlobFilter::new(&query.include, &query.exclude);
    let mut total = 0usize;
    for relative in files {
        if cancel.load(Ordering::Relaxed) {
            outcome.cancelled = true;
            break;
        }
        // Globs are matched against `/`-separated paths on every platform.
        let name = relative.to_string_lossy().replace('\\', "/");
        if !globs.passes(&name) {
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
        let mut hits = hits_found(text, finder);
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
    //!   (`an_invalid_regex_in_a_run_is_the_outcomes_error`), above all when no
    //!   file is read (`an_invalid_regex_is_reported_even_when_no_file_is_read`);
    //! - one long line makes the walk quadratic, each match rescanning the line
    //!   (`one_long_line_with_many_matches_is_walked_once`);
    //! - a symlink is followed: a link to a file outside the worktree puts that
    //!   file's lines in the results, a link to one inside lists them twice
    //!   (`a_symlink_to_a_file_outside_the_worktree_is_not_searched`,
    //!   `a_symlink_to_a_file_inside_is_not_counted_twice`);
    //! - exactly the cap is called capped, or one more is not
    //!   (`exactly_the_cap_is_not_capped_and_one_more_is`);
    //! - a run the cancel flag stopped looks finished (`a_cancelled_run_says_so`);
    //! - `replace_in` splits a multi-byte character, or sees the `\r` of a CRLF
    //!   file (`replacing_multibyte_text_never_splits_a_character`,
    //!   `replacing_in_text_from_a_crlf_file_gives_lf_text`).

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
        // Counted as Binary if it were read; the exclude must keep it unread.
        std::fs::write(dir.join("b.rs"), b"a\0").unwrap();
        // And the include must keep this one unread.
        std::fs::write(dir.join("c.md"), b"a\0").unwrap();
        let query = SearchQuery {
            text: "a".into(),
            include: "*.rs".into(),
            exclude: "b.rs".into(),
            ..SearchQuery::default()
        };

        let outcome = run(&dir, &["a.rs", "a.md", "b.rs", "c.md"], &query, &HashMap::new());

        let paths: Vec<_> = outcome.files.iter().map(|file| file.path.clone()).collect();
        assert_eq!(paths, vec![PathBuf::from("a.rs")]);
        assert!(outcome.skipped.is_empty());
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
        assert!(outcome.cancelled);
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

    #[test]
    fn a_cancelled_run_says_so() {
        let dir = scratch("cancelled-flag");
        std::fs::write(dir.join("a.txt"), "a\n").unwrap();
        let cancel = AtomicBool::new(true);
        let files = vec![PathBuf::from("a.txt")];

        let none = HashMap::new();

        let stopped = search_files(&dir, files.clone(), &query("a"), &none, &cancel, |_| {});
        cancel.store(false, Ordering::Relaxed);
        let whole = search_files(&dir, files, &query("a"), &none, &cancel, |_| {});

        assert!(stopped.cancelled && stopped.files.is_empty());
        assert!(!whole.cancelled && whole.files.len() == 1);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn an_invalid_regex_is_reported_even_when_no_file_is_read() {
        let regex = FindOptions { regex: true, ..Default::default() };
        let dir = scratch("badregex-empty");
        let bad = with("(", regex);

        let never = AtomicBool::new(false);
        let no_files = search_files(&dir, Vec::new(), &bad, &HashMap::new(), &never, |_| {});
        let all_skipped = run(&dir, &["gone.txt"], &bad, &HashMap::new());

        assert!(no_files.error.is_some());
        assert!(all_skipped.error.is_some());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn one_long_line_with_many_matches_is_walked_once() {
        // 200 KB on one line, 50 000 matches: a per-match rescan of the line
        // is quadratic here. No timing is asserted; the run must just finish
        // with every match found.
        let text = "abc ".repeat(50_000);

        let found = hits_in(&text, &query("abc")).unwrap();

        assert_eq!(found.len(), 1);
        assert_eq!(found[0].hit.ranges.len(), 50_000);
        assert_eq!(found[0].ranges_in_file[49_999], 199_996..199_999);
        assert_eq!(found[0].hit.ranges[49_999], 199_996..199_999);
    }

    #[cfg(unix)]
    #[test]
    fn a_symlink_to_a_file_outside_the_worktree_is_not_searched() {
        let outside = scratch("outside");
        std::fs::write(outside.join("secret.txt"), "needle\n").unwrap();
        let dir = scratch("link-out");
        std::os::unix::fs::symlink(outside.join("secret.txt"), dir.join("link.txt")).unwrap();

        let outcome = run(&dir, &["link.txt"], &query("needle"), &HashMap::new());

        assert!(outcome.files.is_empty());
        assert!(outcome.skipped.is_empty());
        let _ = std::fs::remove_dir_all(dir);
        let _ = std::fs::remove_dir_all(outside);
    }

    #[cfg(unix)]
    #[test]
    fn a_symlink_to_a_file_inside_is_not_counted_twice() {
        let dir = scratch("link-in");
        std::fs::write(dir.join("real.txt"), "needle\n").unwrap();
        std::os::unix::fs::symlink("real.txt", dir.join("link.txt")).unwrap();

        let files = ["link.txt", "real.txt"];
        let outcome = run(&dir, &files, &query("needle"), &HashMap::new());

        let paths: Vec<_> = outcome.files.iter().map(|file| file.path.clone()).collect();
        assert_eq!(paths, vec![PathBuf::from("real.txt")]);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn exactly_the_cap_is_not_capped_and_one_more_is() {
        let dir = scratch("exact-cap");
        std::fs::write(dir.join("exact.txt"), "x\n".repeat(MATCH_CAP)).unwrap();
        std::fs::write(dir.join("over.txt"), "x\n".repeat(MATCH_CAP + 1)).unwrap();
        let total = |outcome: &SearchOutcome| -> usize {
            outcome
                .files
                .iter()
                .flat_map(|file| &file.hits)
                .map(|found| found.ranges_in_file.len())
                .sum()
        };

        let exact = run(&dir, &["exact.txt"], &query("x"), &HashMap::new());
        let over = run(&dir, &["over.txt"], &query("x"), &HashMap::new());

        assert!(!exact.capped);
        assert_eq!(total(&exact), MATCH_CAP);
        assert!(over.capped);
        assert_eq!(total(&over), MATCH_CAP);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn replacing_multibyte_text_never_splits_a_character() {
        let (text, count) = replace_in("è😀漢 x è😀漢\n", &query("😀漢"), "é").unwrap();
        assert_eq!((text.as_str(), count), ("èé x èé\n", 2));

        let (text, count) = replace_in("è😀漢\n", &query("è"), "😀😀").unwrap();
        assert_eq!((text.as_str(), count), ("😀😀😀漢\n", 1));
    }

    #[test]
    fn replacing_in_text_from_a_crlf_file_gives_lf_text() {
        let regex = FindOptions { regex: true, ..Default::default() };
        let lf = LineEnding::normalize("a\r\nold\r\nb\r\n");

        let (text, count) = replace_in(&lf, &query("old"), "new").unwrap();
        assert_eq!((text.as_str(), count), ("a\nnew\nb\n", 1));

        let (text, count) = replace_in(&lf, &with(r"a\nold", regex), "z").unwrap();
        assert_eq!((text.as_str(), count), ("z\nb\n", 1));
    }
}
