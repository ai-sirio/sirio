//! The drawer's Search engine: the in-file find's matcher run over a worktree.
//!
//! Pure functions, no UI. `hits_in` and `replace_in` work on one file's
//! LF-normalised text; `search_worktree` lists the files, reads them (an open,
//! unsaved tab's text wins over the disk) and stops at a cap or a cancel.
//! Every range is a byte range in the LF text, so a CRLF file is matched as LF
//! and `è😀漢` is never split.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use ely_gpui_component::editor::{FindOptions, Finder, GlobFilter, Hit};

use sirio_markdown::write_atomically;

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

/// The match of `query` in `lf_text` that starts at `start`, as the text reads
/// now: where a result found earlier still stands. `None` when the text moved
/// on (or the query does not compile), and the caller must not select bytes.
pub fn rematch(lf_text: &str, query: &SearchQuery, start: usize) -> Option<Range<usize>> {
    compile(query).ok()?.find_all(lf_text).into_iter().find(|range| range.start == start)
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

/// Why Replace all left a file as it was.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReplaceSkip {
    /// Its tab holds text that is not on disk.
    Unsaved,
    /// The file is a symbolic link, or a link in its path leads out of the worktree.
    Symlink,
    Missing,
    Unreadable,
}

impl ReplaceSkip {
    /// The words the report puts after the file's name.
    pub fn reason(self) -> &'static str {
        match self {
            Self::Unsaved => "unsaved changes",
            Self::Symlink => "symbolic link",
            Self::Missing => "missing",
            Self::Unreadable => "unreadable",
        }
    }
}

/// What Replace all did: how many matches it replaced, in how many files, which
/// files it wrote (relative to the worktree) and which it left as they were.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ReplaceReport {
    pub replaced: usize,
    pub files: usize,
    /// The files whose bytes changed on disk.
    pub written: Vec<PathBuf>,
    pub skipped: Vec<(PathBuf, ReplaceSkip)>,
}

/// Replaces every match of `query` by `with`, literally, in each of `files`
/// (relative to `worktree`), as the files read now: the stored results' positions
/// are never used, so a file that changed since the search is matched afresh and
/// one with no match left is not written.
///
/// A file is skipped, and reported, when its tab is in `dirty`, when it or a
/// directory above it is a symbolic link (the file's own canonical path must lie
/// inside the worktree's), when it is gone or when it cannot be read. Every byte
/// outside a replaced span stays as it was: the matches are found in the LF
/// text, each is mapped back to the raw bytes (a `\r\n` before it is one more
/// byte), and the replacement's line breaks are written in the file's ending.
pub fn replace_files(
    worktree: &Path,
    files: &[PathBuf],
    query: &SearchQuery,
    with: &str,
    dirty: &HashSet<PathBuf>,
) -> ReplaceReport {
    let mut report = ReplaceReport::default();
    let finder = compile(query);
    let root = worktree.canonicalize();
    // The replacement is text typed in one field: its breaks are LF, as the
    // in-file Replace takes them, and each is written in the file's ending.
    let with_lf = with.replace("\r\n", "\n").replace('\r', "\n");
    for relative in files {
        let outcome = match (&finder, &root) {
            (Ok(finder), Ok(root)) => {
                replace_one(worktree, root, relative, finder, &with_lf, dirty)
            }
            _ => Err(ReplaceSkip::Unreadable),
        };
        match outcome {
            Ok(None) => {}
            Ok(Some((count, wrote))) => {
                report.replaced += count;
                report.files += 1;
                if wrote {
                    report.written.push(relative.clone());
                }
            }
            Err(reason) => report.skipped.push((relative.clone(), reason)),
        }
    }
    report
}

/// One file of `replace_files`: `Ok(None)` when nothing matches any more, else
/// the number replaced and whether the bytes changed.
fn replace_one(
    worktree: &Path,
    root: &Path,
    relative: &Path,
    finder: &Finder,
    with_lf: &str,
    dirty: &HashSet<PathBuf>,
) -> Result<Option<(usize, bool)>, ReplaceSkip> {
    use std::io::ErrorKind::NotFound;

    if dirty.contains(relative) {
        return Err(ReplaceSkip::Unsaved);
    }
    let path = worktree.join(relative);
    match std::fs::symlink_metadata(&path) {
        Err(error) if error.kind() == NotFound => return Err(ReplaceSkip::Missing),
        Err(_) => return Err(ReplaceSkip::Unreadable),
        Ok(metadata) if metadata.file_type().is_symlink() => return Err(ReplaceSkip::Symlink),
        // A directory (a submodule) is no file to replace in.
        Ok(metadata) if !metadata.is_file() => return Err(ReplaceSkip::Missing),
        Ok(_) => {}
    }
    // The file is no link, so its canonical path is its directory's canonical
    // path and its name: a link among the directories above it shows there.
    match path.canonicalize() {
        Ok(canonical) if canonical.starts_with(root) => {}
        Ok(_) => return Err(ReplaceSkip::Symlink),
        Err(error) if error.kind() == NotFound => return Err(ReplaceSkip::Missing),
        Err(_) => return Err(ReplaceSkip::Unreadable),
    }
    let raw = match read_raw(&path) {
        Read::Text(raw) => raw,
        Read::Gone => return Err(ReplaceSkip::Missing),
        Read::Skipped(_) => return Err(ReplaceSkip::Unreadable),
    };
    let lf = LineEnding::normalize(&raw);
    let matches = finder.find_all(&lf);
    if matches.is_empty() {
        return Ok(None);
    }
    let replacement = LineEnding::detect(&raw).apply(with_lf);
    let breaks = crlf_breaks(&raw);
    // An offset in the LF text names the start of a `\r\n` pair as it names its `\n`.
    let raw_at = |at: usize| at + breaks.partition_point(|&lf_at| lf_at < at);
    let mut out = String::with_capacity(raw.len());
    let mut from = 0;
    for range in &matches {
        let (start, end) = (raw_at(range.start), raw_at(range.end));
        out.push_str(&raw[from..start]);
        out.push_str(&replacement);
        from = end;
    }
    out.push_str(&raw[from..]);
    if out == raw {
        return Ok(Some((matches.len(), false)));
    }
    write_atomically(&path, out.as_bytes()).map_err(|_| ReplaceSkip::Unreadable)?;
    Ok(Some((matches.len(), true)))
}

/// The offsets, in the LF text `LineEnding::normalize` makes of `raw`, of each
/// `\n` that ended a `\r\n` pair: ascending.
fn crlf_breaks(raw: &str) -> Vec<usize> {
    let bytes = raw.as_bytes();
    let mut breaks = Vec::new();
    let mut lf_at = 0;
    for (at, byte) in bytes.iter().enumerate() {
        // The `\r` of a pair is dropped by `normalize`: it has no LF offset.
        if *byte == b'\r' && bytes.get(at + 1) == Some(&b'\n') {
            continue;
        }
        if *byte == b'\n' && at > 0 && bytes[at - 1] == b'\r' {
            breaks.push(lf_at);
        }
        lf_at += 1;
    }
    breaks
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
    match read_raw(path) {
        Read::Text(raw) => Read::Text(LineEnding::normalize(&raw)),
        other => other,
    }
}

/// `read_listed` with the file's own line endings kept: what Replace all splices.
fn read_raw(path: &Path) -> Read {
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
        Ok(text) => Read::Text(text),
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
    //!   `replacing_in_text_from_a_crlf_file_gives_lf_text`);
    //!
    //! `replace_files`, each a way Replace all could damage a worktree:
    //! - a file with unsaved changes in its tab is overwritten
    //!   (`an_unsaved_file_is_skipped_and_untouched`);
    //! - a symbolic link is written through, or replaced by a regular file while
    //!   its target keeps the old text (`a_symlink_is_skipped_and_stays_a_link`);
    //! - a directory above the file is a link that leads out of the worktree and
    //!   the file outside is written (`a_file_behind_a_linked_directory_is_skipped`);
    //! - a file that vanished, or cannot be read, aborts the run or is counted as
    //!   replaced (`a_missing_and_an_unreadable_file_are_skipped_and_named`);
    //! - the stored positions are used, so a file whose text moved since the
    //!   search is cut at the wrong bytes (`the_file_is_matched_as_it_reads_now`);
    //! - a file with no match left is rewritten, or counted
    //!   (`a_file_with_no_match_left_is_not_written`);
    //! - a file of mixed endings is rewritten with one ending, or a span over a
    //!   CRLF and an LF break is cut off by a byte
    //!   (`a_mixed_file_keeps_every_ending_outside_the_span`,
    //!   `a_span_over_a_crlf_and_an_lf_break_is_replaced_whole`);
    //! - the replacement's line breaks reach a CRLF file as bare `\n`, or `$1` is
    //!   read as a group (`the_replacements_breaks_take_the_files_ending`);
    //! - the file's mode is lost to the temporary file's (`an_executable_stays_executable`);
    //! - `è😀漢` is split by the mapping back to raw bytes
    //!   (`multibyte_text_in_a_crlf_file_is_replaced_whole`).

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

    fn dirty_of(paths: &[&str]) -> HashSet<PathBuf> {
        paths.iter().map(PathBuf::from).collect()
    }

    fn replace(dir: &Path, files: &[&str], find: &SearchQuery, with: &str) -> ReplaceReport {
        let files: Vec<PathBuf> = files.iter().map(PathBuf::from).collect();
        replace_files(dir, &files, find, with, &HashSet::new())
    }

    #[test]
    fn an_unsaved_file_is_skipped_and_untouched() {
        let dir = scratch("unsaved");
        std::fs::write(dir.join("a.txt"), "needle\n").unwrap();
        std::fs::write(dir.join("b.txt"), "needle\n").unwrap();

        let files = [PathBuf::from("a.txt"), PathBuf::from("b.txt")];
        let report = replace_files(&dir, &files, &query("needle"), "x", &dirty_of(&["a.txt"]));

        assert_eq!(report.skipped, vec![(PathBuf::from("a.txt"), ReplaceSkip::Unsaved)]);
        assert_eq!((report.replaced, report.files), (1, 1));
        assert_eq!(std::fs::read_to_string(dir.join("a.txt")).unwrap(), "needle\n");
        assert_eq!(std::fs::read_to_string(dir.join("b.txt")).unwrap(), "x\n");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[cfg(unix)]
    #[test]
    fn a_symlink_is_skipped_and_stays_a_link() {
        let dir = scratch("link");
        std::fs::write(dir.join("real.txt"), "needle\n").unwrap();
        std::os::unix::fs::symlink("real.txt", dir.join("link.txt")).unwrap();

        let report = replace(&dir, &["link.txt"], &query("needle"), "x");

        assert_eq!(report.skipped, vec![(PathBuf::from("link.txt"), ReplaceSkip::Symlink)]);
        assert_eq!(report.replaced, 0);
        assert!(std::fs::symlink_metadata(dir.join("link.txt")).unwrap().is_symlink());
        assert_eq!(std::fs::read_to_string(dir.join("real.txt")).unwrap(), "needle\n");
        // Its target is replaced through its own entry.
        let report = replace(&dir, &["real.txt"], &query("needle"), "x");
        assert_eq!(report.replaced, 1);
        assert_eq!(std::fs::read_to_string(dir.join("link.txt")).unwrap(), "x\n");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[cfg(unix)]
    #[test]
    fn a_file_behind_a_linked_directory_is_skipped() {
        let outside = scratch("outside-dir");
        std::fs::write(outside.join("secret.txt"), "needle\n").unwrap();
        let dir = scratch("inside-dir");
        std::os::unix::fs::symlink(&outside, dir.join("sub")).unwrap();

        let report = replace(&dir, &["sub/secret.txt"], &query("needle"), "x");

        assert_eq!(report.skipped, vec![(PathBuf::from("sub/secret.txt"), ReplaceSkip::Symlink)]);
        assert_eq!(std::fs::read_to_string(outside.join("secret.txt")).unwrap(), "needle\n");
        let _ = std::fs::remove_dir_all(dir);
        let _ = std::fs::remove_dir_all(outside);
    }

    #[cfg(unix)]
    #[test]
    fn a_missing_and_an_unreadable_file_are_skipped_and_named() {
        use std::os::unix::fs::PermissionsExt;

        let dir = scratch("gone");
        std::fs::write(dir.join("shut.txt"), "needle\n").unwrap();
        std::fs::set_permissions(dir.join("shut.txt"), std::fs::Permissions::from_mode(0o000))
            .unwrap();
        std::fs::write(dir.join("fine.txt"), "needle\n").unwrap();
        let binary = dir.join("blob.bin");
        std::fs::write(&binary, b"needle\0\xff").unwrap();

        let files = ["gone.txt", "shut.txt", "blob.bin", "fine.txt"];
        let report = replace(&dir, &files, &query("needle"), "x");

        // A process that ignores permissions (root) reads shut.txt: nothing to say
        // about it then.
        let shut_read = std::fs::read(dir.join("shut.txt")).is_ok_and(|bytes| bytes == b"x\n");
        let mut want = vec![(PathBuf::from("gone.txt"), ReplaceSkip::Missing)];
        if !shut_read {
            want.push((PathBuf::from("shut.txt"), ReplaceSkip::Unreadable));
        }
        want.push((PathBuf::from("blob.bin"), ReplaceSkip::Unreadable));
        assert_eq!(report.skipped, want);
        assert_eq!(std::fs::read_to_string(dir.join("fine.txt")).unwrap(), "x\n");
        assert_eq!(std::fs::read(&binary).unwrap(), b"needle\0\xff");
        let open = std::fs::Permissions::from_mode(0o644);
        let _ = std::fs::set_permissions(dir.join("shut.txt"), open);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_file_is_matched_as_it_reads_now() {
        let dir = scratch("moved");
        // The search saw the needle at byte 0; it is at byte 13 now.
        std::fs::write(dir.join("a.txt"), "moved ahead; needle tail\n").unwrap();

        let report = replace(&dir, &["a.txt"], &query("needle"), "pin");

        assert_eq!((report.replaced, report.files), (1, 1));
        assert_eq!(report.written, vec![PathBuf::from("a.txt")]);
        let text = std::fs::read_to_string(dir.join("a.txt")).unwrap();
        assert_eq!(text, "moved ahead; pin tail\n");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_file_with_no_match_left_is_not_written() {
        let dir = scratch("nothing");
        std::fs::write(dir.join("a.txt"), "changed since\n").unwrap();
        let before = std::fs::metadata(dir.join("a.txt")).unwrap().modified().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));

        let report = replace(&dir, &["a.txt"], &query("needle"), "x");

        assert_eq!(report, ReplaceReport::default());
        let after = std::fs::metadata(dir.join("a.txt")).unwrap().modified().unwrap();
        assert_eq!(before, after);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_mixed_file_keeps_every_ending_outside_the_span() {
        let dir = scratch("mixed");
        // Two CRLF breaks and two LF: the file reads as LF by tie, and must not be
        // rewritten as one ending.
        std::fs::write(dir.join("a.txt"), b"one\r\nneedle\ntwo\r\nthree\n").unwrap();

        let report = replace(&dir, &["a.txt"], &query("needle"), "pin");

        assert_eq!(report.replaced, 1);
        let bytes = std::fs::read(dir.join("a.txt")).unwrap();
        assert_eq!(bytes, b"one\r\npin\ntwo\r\nthree\n");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_span_over_a_crlf_and_an_lf_break_is_replaced_whole() {
        let dir = scratch("span");
        let regex = FindOptions { regex: true, ..Default::default() };
        std::fs::write(dir.join("a.txt"), b"k1\r\nra\r\nrb\nrc\r\nk2\r\n").unwrap();

        let find = with(r"ra\nrb\nrc", regex);
        let report = replace(&dir, &["a.txt"], &find, "ZZ");

        assert_eq!(report.replaced, 1);
        let bytes = std::fs::read(dir.join("a.txt")).unwrap();
        assert_eq!(bytes, b"k1\r\nZZ\r\nk2\r\n");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_replacements_breaks_take_the_files_ending() {
        let dir = scratch("breaks");
        std::fs::write(dir.join("crlf.txt"), b"a\r\nneedle\r\nb\r\n").unwrap();
        std::fs::write(dir.join("lf.txt"), b"a\nneedle\nb\n").unwrap();

        // The field's own line breaks, and `$1`, which is text and not a group.
        let regex = FindOptions { regex: true, ..Default::default() };
        let find = with("(need)le", regex);
        replace(&dir, &["crlf.txt", "lf.txt"], &find, "x\r\ny\rz$1");

        assert_eq!(std::fs::read(dir.join("crlf.txt")).unwrap(), b"a\r\nx\r\ny\r\nz$1\r\nb\r\n");
        assert_eq!(std::fs::read(dir.join("lf.txt")).unwrap(), b"a\nx\ny\nz$1\nb\n");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn multibyte_text_in_a_crlf_file_is_replaced_whole() {
        let dir = scratch("wide");
        std::fs::write(dir.join("a.txt"), "è😀漢\r\nè😀漢 x\r\n".as_bytes()).unwrap();

        let report = replace(&dir, &["a.txt"], &query("😀漢"), "é");

        assert_eq!(report.replaced, 2);
        let bytes = std::fs::read(dir.join("a.txt")).unwrap();
        assert_eq!(bytes, "èé\r\nèé x\r\n".as_bytes());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[cfg(unix)]
    #[test]
    fn an_executable_stays_executable() {
        use std::os::unix::fs::PermissionsExt;

        let dir = scratch("mode");
        std::fs::write(dir.join("run.sh"), "#!/bin/sh\necho needle\n").unwrap();
        std::fs::set_permissions(dir.join("run.sh"), std::fs::Permissions::from_mode(0o755))
            .unwrap();

        let report = replace(&dir, &["run.sh"], &query("needle"), "done");

        assert_eq!(report.replaced, 1);
        let mode = std::fs::metadata(dir.join("run.sh")).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o755);
        assert_eq!(std::fs::read_to_string(dir.join("run.sh")).unwrap(), "#!/bin/sh\necho done\n");
        let _ = std::fs::remove_dir_all(dir);
    }
}
