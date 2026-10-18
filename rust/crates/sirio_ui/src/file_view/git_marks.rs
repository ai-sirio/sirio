//! The file's changes against HEAD, as Ely draws them: the gutter's marks and
//! the hunk that opens in place under a changed line.
//!
//! The diff is `sirio_git::diff_entry(.., 0)`: each change group whole, with no
//! context lines. A pure deletion therefore has no new-side line of its own,
//! and git names the line before its gap in the header (`+P,0`, where 0 means
//! the gap is at the top). Lines here are 0-based, as Ely counts them.
//!
//! How this module can fail, and the test that catches each:
//! - An added block marks only its first line, or marks the line after it.
//!   `an_added_block_marks_each_added_line`
//! - A block that both adds and removes is marked `Added`, which hides that
//!   lines were removed there. `a_replaced_block_marks_modified`
//! - A pure deletion is marked on the line below its gap, or on the wrong
//!   side of it. `a_pure_deletion_marks_the_line_above_the_gap`
//! - A deletion at the top of the file marks line -1, which underflows.
//!   `a_deletion_at_the_top_marks_line_zero`
//! - A line outside every hunk opens some hunk anyway.
//!   `hunks_at_a_line_outside_every_hunk_is_empty`
//! - The removed lines of a hunk come back reordered or with some dropped.
//!   `hunks_at_returns_the_removed_lines_in_order`
//! - A replacement with more deletions than additions marks a deleted line, or
//!   loses removed rows.
//!   `a_replacement_with_more_deletions_than_additions_keeps_every_removed_line`
//! - A deletion after the last line is marked on a line that does not exist,
//!   or not at all. `a_deletion_at_the_end_of_the_file_marks_the_last_line`
//! - A gap past the last line of a buffer edited since the diff was pushed is
//!   handed to Ely as it is, which draws none of its removed rows.
//!   `a_gap_past_a_buffer_edited_since_the_diff_is_drawn_above_its_last_line`
//! - A deletion opens on the line Ely draws its removed rows above, not on
//!   its mark, so the line a click names does not open it.
//!   `a_deletion_opens_on_its_mark_not_on_the_line_below_the_gap`
//! - Two deletions that both mark line 0 (one above the first row, one below
//!   it) show one mark, and a click on it opens only one of them.
//!   `two_deletions_on_line_zero_mark_it_once` and
//!   `two_deletions_on_line_zero_both_open`

use ely_gpui_component::editor::{DiffHunk, GitMark};
use gpui::SharedString;
use sirio_git::{DiffOrigin, FileDiff, Hunk};

/// The gutter marks of `diff`, sorted by line, one per line.
///
/// A block that only adds is `Added` on each added line, one that also removes
/// is `Modified` on each added line, and one that only removes is `Deleted` on
/// the line above its gap (line 0 when the gap is at the top). Two deletions
/// can share a line, line 0 when one is above the first row and one below it;
/// both are `Deleted`, so the line keeps one mark, and `hunks_at` opens both.
/// Two different marks never share a line with zero context: git puts a
/// deletion and an addition that touch in one hunk, so a deletion's mark is
/// always an unchanged line. The line therefore keeps the first mark it gets.
pub fn marks(diff: &FileDiff) -> Vec<(usize, GitMark)> {
    debug_assert_zero_context(diff);
    let mut marks = Vec::new();
    for hunk in &diff.hunks {
        let added = added_lines(hunk);
        if added.is_empty() {
            marks.push((gap_line(hunk), GitMark::Deleted));
            continue;
        }
        let mark = if has_removed(hunk) {
            GitMark::Modified
        } else {
            GitMark::Added
        };
        marks.extend(added.into_iter().map(|line| (line, mark)));
    }
    marks.sort_by_key(|(line, _)| *line);
    marks.dedup_by_key(|(line, _)| *line);
    marks
}

/// The hunks a click on the gutter at `line` opens, in diff order; empty when
/// that line opens nothing. `line` is a mark's line: a hunk with additions opens
/// on any of its added lines, and a pure deletion on its mark, the line above
/// its gap. Two deletions on one mark (line 0, see `marks`) open together. A
/// hunk's `line` is where Ely draws its removed rows, above that row: for a
/// deletion, the line below the gap, which lies past the last line only in a
/// buffer edited since the diff was pushed (see `fit`).
pub fn hunks_at(diff: &FileDiff, line: usize) -> Vec<DiffHunk> {
    debug_assert_zero_context(diff);
    diff.hunks
        .iter()
        .filter_map(|hunk| {
            let added = added_lines(hunk);
            let opens = if added.is_empty() {
                gap_line(hunk) == line
            } else {
                added.contains(&line)
            };
            opens.then(|| draw(hunk, &added))
        })
        .collect()
}

/// The gutter line that opens `hunk`, the one `hunks_at` answers for it: its
/// first added line, or the line above its gap for a deletion.
pub fn opening_line(hunk: &DiffHunk) -> usize {
    if hunk.added > 0 {
        hunk.line
    } else {
        hunk.line.saturating_sub(1)
    }
}

/// `hunk` as Ely can draw it in a buffer of `lines` lines. Ely draws removed
/// rows above a line and never draws a row past the last. A diff of the text
/// as it is never has a gap past the last row: a trailing newline makes the
/// last row empty, and the gap lands on it. So the clamp serves only a buffer
/// edited since the diff was pushed, and its gap is drawn above the last line,
/// where its removed rows show.
pub fn fit(mut hunk: DiffHunk, lines: usize) -> DiffHunk {
    hunk.line = hunk.line.min(lines.saturating_sub(1));
    hunk
}

/// `marks` and `hunks_at` read a zero-context diff. A context line inside a
/// hunk would put its added rows out of the run `DiffHunk` draws from `line`
/// on, and a context line between two hunks merges them under `-U0`. A user's
/// `diff.interHunkContext` can add those, and this assert catches it in debug
/// builds.
fn debug_assert_zero_context(diff: &FileDiff) {
    debug_assert!(
        diff.hunks
            .iter()
            .flat_map(|hunk| &hunk.lines)
            .all(|line| line.origin != DiffOrigin::Context),
        "git_marks needs a diff with no context lines (diff_entry(.., 0))"
    );
}

/// The 0-based lines `hunk` adds to the current file, in order. Git numbers
/// an added line in the new file, so each is its new number less one.
fn added_lines(hunk: &Hunk) -> Vec<usize> {
    hunk.lines
        .iter()
        .filter(|line| line.origin == DiffOrigin::Addition)
        .filter_map(|line| line.new_line_number)
        .map(|number| number.saturating_sub(1))
        .collect()
}

fn has_removed(hunk: &Hunk) -> bool {
    hunk.lines
        .iter()
        .any(|line| line.origin == DiffOrigin::Deletion)
}

/// The line a pure deletion's mark goes on. Git's header names the line
/// before the gap (`+P,0`, 0 at the top), so the mark is that line less one,
/// or line 0 when the gap is at the top.
fn gap_line(hunk: &Hunk) -> usize {
    hunk.new_start.saturating_sub(1)
}

/// The hunk as Ely draws it: at its first added line, or, for a pure
/// deletion, at the line after the gap, which is where git's header puts `P`
/// (the 0-based index of the line after the gap is `P` itself).
fn draw(hunk: &Hunk, added: &[usize]) -> DiffHunk {
    DiffHunk {
        line: added.first().copied().unwrap_or(hunk.new_start),
        added: added.len(),
        removed: hunk
            .lines
            .iter()
            .filter(|line| line.origin == DiffOrigin::Deletion)
            .map(|line| SharedString::from(line.content.as_str()))
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sirio_git::parse_diff;
    use std::path::Path;

    /// `main.rs`'s diff with zero context lines, given its hunks, as
    /// `diff_entry(.., 0)` returns it.
    fn diff(hunks: &str) -> FileDiff {
        let header = "diff --git a/main.rs b/main.rs\n\
                      index 1111111..2222222 100644\n\
                      --- a/main.rs\n\
                      +++ b/main.rs\n";
        let text = format!("{header}{hunks}");
        parse_diff(&text, Path::new("main.rs"))
    }

    /// Two lines inserted after line 2 (0-based lines 2 and 3).
    const ADDED: &str = "@@ -2,0 +3,2 @@\n+x\n+y\n";

    /// Line 4 replaced by two lines (0-based lines 3 and 4).
    const REPLACED: &str = "@@ -4 +4,2 @@\n-four\n+FOUR\n+4.5\n";

    /// Line 6 deleted, between lines 5 and 6 of the new file.
    const PURE_DELETION: &str = "@@ -6 +5,0 @@\n-six\n";

    /// Line 1 deleted, above everything.
    const TOP: &str = "@@ -1 +0,0 @@\n-first\n";

    /// Three lines replaced by one (0-based line 9).
    const MORE_DELETIONS: &str = "@@ -10,3 +10 @@\n-a\n-b\n-c\n+A\n";

    /// The last two of five lines deleted from a file with a trailing newline,
    /// as `git diff -U0` prints it. The new file is `one` to `three`, so the gap
    /// is after `three` (0-based line 2) and its line is row 3, the empty last row.
    const END_OF_FILE: &str = "@@ -4,2 +3,0 @@ three\n-four\n-five\n";

    /// Lines 1 and 3 of `a`, `b`, `c` deleted, as `git diff -U0` prints them:
    /// the first sits above row 0 and the second below `b`, so both mark line 0.
    const TWO_DELETIONS: &str = "@@ -1 +0,0 @@\n-a\n@@ -3 +1,0 @@ b\n-c\n";

    fn removed(lines: &[&str]) -> Vec<SharedString> {
        lines.iter().map(|line| SharedString::from(*line)).collect()
    }

    /// The one hunk that `line` opens, for a line that opens exactly one.
    fn opened(diff: &FileDiff, line: usize) -> DiffHunk {
        let mut hunks = hunks_at(diff, line);
        assert_eq!(hunks.len(), 1, "line {line} opens one hunk");
        hunks.remove(0)
    }

    #[test]
    fn an_added_block_marks_each_added_line() {
        assert_eq!(marks(&diff(ADDED)), vec![(2, GitMark::Added), (3, GitMark::Added)]);
    }

    #[test]
    fn a_replaced_block_marks_modified() {
        assert_eq!(marks(&diff(REPLACED)), vec![(3, GitMark::Modified), (4, GitMark::Modified)]);
    }

    #[test]
    fn a_pure_deletion_marks_the_line_above_the_gap() {
        assert_eq!(marks(&diff(PURE_DELETION)), vec![(4, GitMark::Deleted)]);
    }

    #[test]
    fn a_deletion_at_the_top_marks_line_zero() {
        assert_eq!(marks(&diff(TOP)), vec![(0, GitMark::Deleted)]);
        assert_eq!(hunks_at(&diff(TOP), 0).first().map(|hunk| hunk.line), Some(0));
    }

    #[test]
    fn hunks_at_a_line_outside_every_hunk_is_empty() {
        let added = diff(ADDED);
        assert_eq!(hunks_at(&added, 2).len(), 1, "an added line opens its hunk");
        assert!(hunks_at(&added, 0).is_empty());
        assert!(hunks_at(&added, 4).is_empty());
    }

    #[test]
    fn hunks_at_returns_the_removed_lines_in_order() {
        let hunk = opened(&diff(MORE_DELETIONS), 9);
        assert_eq!(hunk.removed, removed(&["a", "b", "c"]));
    }

    #[test]
    fn a_replacement_with_more_deletions_than_additions_keeps_every_removed_line() {
        let replaced = diff(MORE_DELETIONS);
        assert_eq!(
            marks(&replaced),
            vec![(9, GitMark::Modified)],
            "only the added line is marked"
        );
        let hunk = opened(&replaced, 9);
        assert_eq!((hunk.line, hunk.added, hunk.removed.len()), (9, 1, 3));
    }

    #[test]
    fn a_deletion_at_the_end_of_the_file_marks_the_last_line() {
        let end = diff(END_OF_FILE);
        assert_eq!(marks(&end), vec![(2, GitMark::Deleted)]);
        let hunk = opened(&end, 2);
        assert_eq!(hunk.line, 3, "the gap is after `three`, on the empty last row");
        assert_eq!(fit(hunk, 4).line, 3, "a row Ely draws is not moved");
    }

    #[test]
    fn a_gap_past_a_buffer_edited_since_the_diff_is_drawn_above_its_last_line() {
        // The buffer was cut to "one\ntwo\nthree" after the diff was pushed:
        // three rows, where the diff's gap is on row 3.
        let fitted = fit(opened(&diff(END_OF_FILE), 2), 3);
        assert_eq!(fitted.line, 2);
        assert_eq!(fitted.removed, removed(&["four", "five"]));
    }

    #[test]
    fn a_deletion_opens_on_its_mark_not_on_the_line_below_the_gap() {
        let deletion = diff(PURE_DELETION);
        let hunk = opened(&deletion, 4);
        assert_eq!(hunk.line, 5, "the removed rows sit above the line after the gap");
        assert_eq!(opening_line(&hunk), 4);
        assert!(hunks_at(&deletion, 5).is_empty(), "the line below the gap has no mark");
    }

    #[test]
    fn two_deletions_on_line_zero_mark_it_once() {
        assert_eq!(marks(&diff(TWO_DELETIONS)), vec![(0, GitMark::Deleted)]);
    }

    #[test]
    fn two_deletions_on_line_zero_both_open() {
        let hunks = hunks_at(&diff(TWO_DELETIONS), 0);
        assert_eq!(hunks.iter().map(|hunk| hunk.line).collect::<Vec<_>>(), vec![0, 1]);
        assert_eq!(hunks[0].removed, removed(&["a"]));
        assert_eq!(hunks[1].removed, removed(&["c"]));
    }
}
