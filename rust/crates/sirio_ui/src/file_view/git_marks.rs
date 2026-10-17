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
//!   `hunk_at_a_line_outside_every_hunk_is_none`
//! - The removed lines of a hunk come back reordered or with some dropped.
//!   `hunk_at_returns_the_removed_lines_in_order`
//! - A replacement with more deletions than additions marks a deleted line, or
//!   loses removed rows.
//!   `a_replacement_with_more_deletions_than_additions_keeps_every_removed_line`
//! - A deletion after the last line is marked on a line that does not exist,
//!   or not at all. `a_deletion_at_the_end_of_the_file_marks_the_last_line`
//! - A gap past the last line is handed to Ely as it is, which draws none of
//!   its removed rows. `a_gap_past_the_last_line_is_drawn_above_the_last_line`
//! - A deletion opens on the line Ely draws its removed rows above, not on
//!   its mark, so the line a click names does not open it.
//!   `a_deletion_opens_on_its_mark_not_on_the_line_below_the_gap`
//! - Two hunks overwrite each other's marks, or the marks come out unsorted.
//!   `adjacent_hunks_mark_and_open_their_own_lines`

use ely_gpui_component::editor::{DiffHunk, GitMark};
use gpui::SharedString;
use sirio_git::{DiffOrigin, FileDiff, Hunk};

/// The gutter marks of `diff`, sorted by line.
///
/// A block that only adds is `Added` on each added line, one that also removes
/// is `Modified` on each added line, and one that only removes is `Deleted` on
/// the line above its gap (line 0 when the gap is at the top).
pub fn marks(diff: &FileDiff) -> Vec<(usize, GitMark)> {
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
    marks
}

/// The hunk a click on the gutter at `line` opens, or `None` when that line
/// opens nothing. `line` is the mark's line: a hunk with additions opens on
/// any of its added lines, and a pure deletion opens on its mark, the line
/// above the gap. The result's `line` is where Ely draws the removed rows,
/// above that row, so for a deletion it is the line below the gap, which may
/// lie past the last line (see `fit`).
pub fn hunk_at(diff: &FileDiff, line: usize) -> Option<DiffHunk> {
    diff.hunks.iter().find_map(|hunk| {
        let added = added_lines(hunk);
        let opens = if added.is_empty() {
            gap_line(hunk) == line
        } else {
            added.contains(&line)
        };
        opens.then(|| draw(hunk, &added))
    })
}

/// The gutter line that opens `hunk`, the one `hunk_at` answers for it: its
/// first added line, or the line above its gap for a deletion.
pub fn opening_line(hunk: &DiffHunk) -> usize {
    if hunk.added > 0 {
        hunk.line
    } else {
        hunk.line.saturating_sub(1)
    }
}

/// `hunk` as Ely can draw it in a buffer of `lines` lines. Ely draws removed
/// rows above a line and never draws a row past the last, so a gap after the
/// last line is drawn above that line, where its removed rows show.
pub fn fit(mut hunk: DiffHunk, lines: usize) -> DiffHunk {
    hunk.line = hunk.line.min(lines.saturating_sub(1));
    hunk
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

    /// The last of five lines deleted; the new file has four (0-based line 3
    /// is the last one, and the gap is after it).
    const END_OF_FILE: &str = "@@ -5 +4,0 @@\n-five\n";

    /// An insertion after line 1, and a deletion of line 6 further down.
    const ADJACENT: &str = "@@ -1,0 +2 @@\n+X\n@@ -6 +6,0 @@\n-f\n";

    fn removed(lines: &[&str]) -> Vec<SharedString> {
        lines.iter().map(|line| SharedString::from(*line)).collect()
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
        assert_eq!(hunk_at(&diff(TOP), 0).map(|hunk| hunk.line), Some(0));
    }

    #[test]
    fn hunk_at_a_line_outside_every_hunk_is_none() {
        let added = diff(ADDED);
        assert!(hunk_at(&added, 2).is_some(), "an added line opens its hunk");
        assert_eq!(hunk_at(&added, 0), None);
        assert_eq!(hunk_at(&added, 4), None);
    }

    #[test]
    fn hunk_at_returns_the_removed_lines_in_order() {
        let hunk = hunk_at(&diff(MORE_DELETIONS), 9).expect("the added line opens the hunk");
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
        let hunk = hunk_at(&replaced, 9).expect("the added line opens the hunk");
        assert_eq!((hunk.line, hunk.added, hunk.removed.len()), (9, 1, 3));
    }

    #[test]
    fn a_deletion_at_the_end_of_the_file_marks_the_last_line() {
        assert_eq!(marks(&diff(END_OF_FILE)), vec![(3, GitMark::Deleted)]);
    }

    #[test]
    fn a_gap_past_the_last_line_is_drawn_above_the_last_line() {
        // "one\ntwo\nthree\nfour" has four lines. The gap after "four" names
        // line 4, which Ely does not draw.
        let hunk = hunk_at(&diff(END_OF_FILE), 3).expect("the deletion opens on its mark");
        let fitted = fit(hunk.clone(), 4);
        assert_eq!(fitted.line, 3);
        assert_eq!(fitted.removed, removed(&["five"]));
        assert_eq!(fit(hunk, 5).line, 4, "a gap Ely draws stays where it is");
    }

    #[test]
    fn a_deletion_opens_on_its_mark_not_on_the_line_below_the_gap() {
        let deletion = diff(PURE_DELETION);
        let hunk = hunk_at(&deletion, 4).expect("the mark opens the deletion");
        assert_eq!(hunk.line, 5, "the removed rows sit above the line after the gap");
        assert_eq!(opening_line(&hunk), 4);
        assert_eq!(hunk_at(&deletion, 5), None, "the line below the gap has no mark");
    }

    #[test]
    fn adjacent_hunks_mark_and_open_their_own_lines() {
        let both = diff(ADJACENT);
        assert_eq!(marks(&both), vec![(1, GitMark::Added), (5, GitMark::Deleted)]);
        let added = hunk_at(&both, 1).expect("the insertion opens its hunk");
        assert_eq!((added.line, added.added), (1, 1));
        assert!(added.removed.is_empty());
        let deleted = hunk_at(&both, 5).expect("the deletion opens its hunk");
        assert_eq!((deleted.line, deleted.added), (6, 0));
        assert_eq!(deleted.removed, removed(&["f"]));
    }
}
