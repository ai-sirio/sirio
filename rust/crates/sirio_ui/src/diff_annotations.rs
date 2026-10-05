//! Things a diff's owner pins to its lines (spec §4, B3 review threads):
//! which line an annotation follows, and which context lines must stay out
//! of a collapsed band because something is pinned to them.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use sirio_forge::{AnchorLine, LineKind};
use sirio_git::{DiffOrigin, FileDiff, Hunk};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AnnotationSide {
    Old,
    New,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AnnotationKind {
    /// A thread under its line; `open` counts toward the file's badge.
    Thread { open: bool },
    /// The one new comment being written, under its line (B3b).
    Composer,
    /// The folded "N outdated threads" section of one file, right under its
    /// header. `side` and `line` are unused.
    Outdated { count: usize },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Annotation {
    /// Stable across reloads: the owner's identity for the view drawn here.
    pub key: u64,
    pub path: PathBuf,
    pub side: AnnotationSide,
    pub line: Option<u32>,
    pub start_line: Option<u32>,
    pub kind: AnnotationKind,
    /// Bumped by the owner whenever the view's height changes (a card
    /// folded or opened), so the list measures the row again.
    pub revision: u64,
}

/// A run of `[start, end)` context lines: collapsed into a band, or drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Piece {
    pub start: usize,
    pub end: usize,
    pub band: bool,
}

/// Splits a run of `len` context lines around the `anchored` positions
/// (indices into the run, any order): every anchored line is drawn, and a
/// stretch between them becomes a band only if it is at least `min` long.
pub(crate) fn band_pieces(len: usize, anchored: &[usize], min: usize) -> Vec<Piece> {
    let mut marks: Vec<usize> = anchored.iter().copied().filter(|index| *index < len).collect();
    marks.sort_unstable();
    marks.dedup();
    let mut pieces = Vec::new();
    let push_stretch = |pieces: &mut Vec<Piece>, start: usize, end: usize| {
        if end > start {
            pieces.push(Piece { start, end, band: end - start >= min });
        }
    };
    let mut cursor = 0;
    for mark in marks {
        push_stretch(&mut pieces, cursor, mark);
        pieces.push(Piece { start: mark, end: mark + 1, band: false });
        cursor = mark + 1;
    }
    push_stretch(&mut pieces, cursor, len);
    pieces
}

pub(crate) fn anchored_lines(annotations: &[Annotation], path: &Path) -> (HashSet<u32>, HashSet<u32>) {
    let (mut old, mut new) = (HashSet::new(), HashSet::new());
    for annotation in annotations.iter().filter(|annotation| annotation.path == path) {
        let (AnnotationKind::Thread { .. } | AnnotationKind::Composer, Some(line)) = (annotation.kind, annotation.line) else {
            continue;
        };
        let first = annotation.start_line.unwrap_or(line).min(line);
        let side = match annotation.side {
            AnnotationSide::Old => &mut old,
            AnnotationSide::New => &mut new,
        };
        side.extend(first..=line);
    }
    (old, new)
}

/// Whether `annotation` follows the diff row showing old line `old` and new
/// line `new` (a split row may show both).
pub(crate) fn matches_row(annotation: &Annotation, old: Option<u32>, new: Option<u32>) -> bool {
    if !matches!(annotation.kind, AnnotationKind::Thread { .. } | AnnotationKind::Composer) {
        return false;
    }
    let Some(line) = annotation.line else {
        return false;
    };
    match annotation.side {
        AnnotationSide::Old => old == Some(line),
        AnnotationSide::New => new == Some(line),
    }
}

/// GitHub takes a comment only on a line of its own hunks, which carry three
/// lines of context (spec §7.4).
pub(crate) const COMMENT_CONTEXT: usize = 3;

/// Which lines of one hunk take a comment: the changed lines, and the
/// context lines at most `context` lines from one.
pub(crate) fn commentable(origins: &[DiffOrigin], context: usize) -> Vec<bool> {
    let changed: Vec<usize> = origins
        .iter()
        .enumerate()
        .filter(|(_, origin)| **origin != DiffOrigin::Context)
        .map(|(index, _)| index)
        .collect();
    (0..origins.len())
        .map(|index| changed.iter().any(|change| index.abs_diff(*change) <= context))
        .collect()
}

fn line_number(number: usize) -> u32 {
    u32::try_from(number).unwrap_or(u32::MAX)
}

/// Each line of a hunk in GitLab's diff counters: a line one side lacks
/// takes the number that side's next line would have.
pub(crate) fn positions(hunk: &Hunk) -> Vec<AnchorLine> {
    let (mut old, mut new) = (hunk.old_start, hunk.new_start);
    hunk.lines
        .iter()
        .map(|line| {
            let here = AnchorLine {
                kind: match line.origin {
                    DiffOrigin::Addition => LineKind::Added,
                    DiffOrigin::Deletion => LineKind::Removed,
                    DiffOrigin::Context => LineKind::Context,
                },
                old: line_number(line.old_line_number.unwrap_or(old)),
                new: line_number(line.new_line_number.unwrap_or(new)),
            };
            if let Some(number) = line.old_line_number {
                old = number + 1;
            }
            if let Some(number) = line.new_line_number {
                new = number + 1;
            }
            here
        })
        .collect()
}

/// Where `line` on `side` is drawn: (hunk, line in the hunk).
pub(crate) fn locate(diff: &FileDiff, side: AnnotationSide, line: u32) -> Option<(usize, usize)> {
    diff.hunks.iter().enumerate().find_map(|(hunk_index, hunk)| {
        hunk.lines
            .iter()
            .position(|candidate| {
                let number = match side {
                    AnnotationSide::Old => candidate.old_line_number,
                    AnnotationSide::New => candidate.new_line_number,
                };
                number.map(line_number) == Some(line)
            })
            .map(|index| (hunk_index, index))
    })
}

/// Where a new comment goes, as the diff drew it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommentAnchor {
    pub path: PathBuf,
    pub side: AnnotationSide,
    pub line: AnchorLine,
    pub start: Option<AnchorLine>,
}

impl CommentAnchor {
    fn number(&self, line: AnchorLine) -> u32 {
        match self.side {
            AnnotationSide::Old => line.old,
            AnnotationSide::New => line.new,
        }
    }

    pub fn last(&self) -> u32 {
        self.number(self.line)
    }

    pub fn first(&self) -> u32 {
        self.start.map_or(self.last(), |start| self.number(start))
    }

    /// `path:side:line` or `path:side:first-last`, as the socket names it.
    pub fn spec(&self) -> String {
        let side = match self.side {
            AnnotationSide::Old => "old",
            AnnotationSide::New => "new",
        };
        match self.start {
            None => format!("{}:{side}:{}", self.path.display(), self.last()),
            Some(_) => format!("{}:{side}:{}-{}", self.path.display(), self.first(), self.last()),
        }
    }
}

/// The anchor from line `first` to line `last` of one hunk (either order),
/// on `side`, or why there is none.
pub(crate) fn anchor_in(
    path: &Path,
    side: AnnotationSide,
    hunk: &Hunk,
    first: usize,
    last: usize,
) -> Result<CommentAnchor, &'static str> {
    let (first, last) = (first.min(last), first.max(last));
    let origins: Vec<DiffOrigin> = hunk.lines.iter().map(|line| line.origin).collect();
    let open = commentable(&origins, COMMENT_CONTEXT);
    let at = positions(hunk);
    let on_side = |index: usize| {
        let wrong = match side {
            AnnotationSide::Old => DiffOrigin::Addition,
            AnnotationSide::New => DiffOrigin::Deletion,
        };
        origins.get(index).is_some_and(|origin| *origin != wrong)
    };
    if !open.get(first).copied().unwrap_or(false) || !open.get(last).copied().unwrap_or(false) {
        return Err("That line cannot take a comment.");
    }
    if !open[first..=last].iter().all(|open| *open) {
        return Err("That line cannot take a comment.");
    }
    if !on_side(first) || !on_side(last) {
        return Err("A range stays on one side of the diff.");
    }
    Ok(CommentAnchor {
        path: path.to_path_buf(),
        side,
        line: at[last],
        start: (first != last).then(|| at[first]),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use sirio_forge::{AnchorLine, LineKind};
    use sirio_git::{DiffLine, DiffOrigin, FileDiff, Hunk};

    fn thread(side: AnnotationSide, line: u32, start: Option<u32>) -> Annotation {
        Annotation {
            key: 1,
            path: PathBuf::from("a.rs"),
            side,
            line: Some(line),
            start_line: start,
            kind: AnnotationKind::Thread { open: true },
            revision: 0,
        }
    }

    #[test]
    fn a_run_with_nothing_anchored_is_one_band_or_plain_lines() {
        assert_eq!(band_pieces(10, &[], 4), vec![Piece { start: 0, end: 10, band: true }]);
        assert_eq!(band_pieces(3, &[], 4), vec![Piece { start: 0, end: 3, band: false }]);
        assert_eq!(band_pieces(0, &[], 4), vec![]);
    }

    #[test]
    fn a_band_splits_around_an_anchored_line() {
        assert_eq!(
            band_pieces(10, &[5], 4),
            vec![
                Piece { start: 0, end: 5, band: true },
                Piece { start: 5, end: 6, band: false },
                Piece { start: 6, end: 10, band: true },
            ]
        );
    }

    #[test]
    fn a_short_stretch_beside_an_anchor_is_drawn_not_banded() {
        assert_eq!(
            band_pieces(10, &[2, 8], 4),
            vec![
                Piece { start: 0, end: 2, band: false },
                Piece { start: 2, end: 3, band: false },
                Piece { start: 3, end: 8, band: true },
                Piece { start: 8, end: 9, band: false },
                Piece { start: 9, end: 10, band: false },
            ]
        );
    }

    #[test]
    fn anchors_at_the_edges_and_duplicates_leave_no_empty_piece() {
        assert_eq!(
            band_pieces(6, &[5, 0, 0], 4),
            vec![
                Piece { start: 0, end: 1, band: false },
                Piece { start: 1, end: 5, band: true },
                Piece { start: 5, end: 6, band: false },
            ]
        );
        // An index past the run is ignored.
        assert_eq!(band_pieces(4, &[9], 4), vec![Piece { start: 0, end: 4, band: true }]);
    }

    #[test]
    fn a_range_keeps_every_line_it_covers_visible_on_its_side() {
        let annotations = vec![
            thread(AnnotationSide::New, 45, Some(44)),
            thread(AnnotationSide::Old, 42, None),
            Annotation { kind: AnnotationKind::Outdated { count: 2 }, line: None, ..thread(AnnotationSide::New, 1, None) },
            Annotation { path: PathBuf::from("b.rs"), ..thread(AnnotationSide::New, 7, None) },
        ];
        let (old, new) = anchored_lines(&annotations, Path::new("a.rs"));
        assert_eq!(old, HashSet::from([42]));
        assert_eq!(new, HashSet::from([44, 45]));
    }

    #[test]
    fn a_thread_follows_the_row_showing_its_line_on_its_side() {
        let new = thread(AnnotationSide::New, 42, None);
        let old = thread(AnnotationSide::Old, 42, None);
        assert!(matches_row(&new, None, Some(42)));
        assert!(matches_row(&new, Some(40), Some(42)));
        assert!(!matches_row(&new, Some(42), None));
        assert!(matches_row(&old, Some(42), None));
        assert!(!matches_row(&old, None, Some(42)));
        // A range follows its last line only.
        let range = thread(AnnotationSide::New, 45, Some(44));
        assert!(!matches_row(&range, None, Some(44)));
        assert!(matches_row(&range, None, Some(45)));
        // An outdated section follows no row.
        let section = Annotation { kind: AnnotationKind::Outdated { count: 1 }, ..new };
        assert!(!matches_row(&section, None, Some(42)));
    }

    fn diff_line(origin: DiffOrigin, old: Option<usize>, new: Option<usize>) -> DiffLine {
        DiffLine { origin, old_line_number: old, new_line_number: new, content: String::new() }
    }

    /// Context 38..=41, old 42 removed, new 42 added, context 43..=46: one
    /// line edited in the middle, as `test-forge-diff-e2e.sh` edits line 42.
    fn edited_hunk() -> Hunk {
        let mut lines: Vec<DiffLine> = (38..=41).map(|n| diff_line(DiffOrigin::Context, Some(n), Some(n))).collect();
        lines.push(diff_line(DiffOrigin::Deletion, Some(42), None));
        lines.push(diff_line(DiffOrigin::Addition, None, Some(42)));
        lines.extend((43..=46).map(|n| diff_line(DiffOrigin::Context, Some(n), Some(n))));
        Hunk { header: "@@ -38,9 +38,9 @@".into(), old_start: 38, old_lines: 9, new_start: 38, new_lines: 9, lines }
    }

    #[test]
    fn a_change_and_three_lines_either_side_take_a_comment() {
        let origins: Vec<DiffOrigin> = edited_hunk().lines.iter().map(|line| line.origin).collect();
        let open = commentable(&origins, COMMENT_CONTEXT);
        assert_eq!(open, vec![false, true, true, true, true, true, true, true, true, false]);
        assert!(commentable(&[DiffOrigin::Context; 5], COMMENT_CONTEXT).iter().all(|open| !open), "no change, nothing to comment on");
    }

    #[test]
    fn a_line_a_side_lacks_takes_that_side_s_next_number() {
        let at = positions(&edited_hunk());
        assert_eq!(at[4], AnchorLine { kind: LineKind::Removed, old: 42, new: 42 });
        assert_eq!(at[5], AnchorLine { kind: LineKind::Added, old: 43, new: 42 });
        assert_eq!(at[6], AnchorLine { kind: LineKind::Context, old: 43, new: 43 });
        let new_file = Hunk {
            header: "@@ -0,0 +1,2 @@".into(), old_start: 0, old_lines: 0, new_start: 1, new_lines: 2,
            lines: vec![diff_line(DiffOrigin::Addition, None, Some(1)), diff_line(DiffOrigin::Addition, None, Some(2))],
        };
        assert_eq!(positions(&new_file)[1], AnchorLine { kind: LineKind::Added, old: 0, new: 2 });
    }

    #[test]
    fn a_line_is_found_on_its_side_in_its_hunk() {
        let second = Hunk { header: "@@ -90 +90 @@".into(), old_start: 90, old_lines: 1, new_start: 90, new_lines: 1,
            lines: vec![diff_line(DiffOrigin::Deletion, Some(90), None), diff_line(DiffOrigin::Addition, None, Some(90))] };
        let diff = FileDiff { path: "src/login.rs".into(), hunks: vec![edited_hunk(), second], additions: 2, deletions: 2, is_binary: false, is_submodule: false };
        assert_eq!(locate(&diff, AnnotationSide::Old, 42), Some((0, 4)));
        assert_eq!(locate(&diff, AnnotationSide::New, 42), Some((0, 5)));
        assert_eq!(locate(&diff, AnnotationSide::New, 90), Some((1, 1)));
        assert_eq!(locate(&diff, AnnotationSide::New, 60), None);
    }

    #[test]
    fn an_anchor_is_one_line_or_a_downward_range_on_one_side() {
        let hunk = edited_hunk();
        let path = Path::new("src/login.rs");
        let one = anchor_in(path, AnnotationSide::New, &hunk, 6, 6).expect("a context line near the change");
        assert_eq!((one.line, one.start, one.spec()), (AnchorLine { kind: LineKind::Context, old: 43, new: 43 }, None, "src/login.rs:new:43".to_string()));
        let range = anchor_in(path, AnnotationSide::New, &hunk, 7, 3).expect("either order");
        assert_eq!((range.first(), range.last(), range.spec()), (41, 44, "src/login.rs:new:41-44".to_string()));
        assert_eq!(anchor_in(path, AnnotationSide::New, &hunk, 0, 0), Err("That line cannot take a comment."));
        assert_eq!(anchor_in(path, AnnotationSide::New, &hunk, 4, 6), Err("A range stays on one side of the diff."));
        assert_eq!(anchor_in(path, AnnotationSide::Old, &hunk, 5, 5), Err("A range stays on one side of the diff."));
        let old = anchor_in(path, AnnotationSide::Old, &hunk, 4, 4).expect("the removed line, on the old side");
        assert_eq!(old.spec(), "src/login.rs:old:42");
    }

    #[test]
    fn a_range_refuses_lines_outside_the_forges_commentable_hunk() {
        let mut lines = vec![diff_line(DiffOrigin::Addition, None, Some(1))];
        lines.extend((1..=19).map(|n| diff_line(DiffOrigin::Context, Some(n), Some(n + 1))));
        lines.push(diff_line(DiffOrigin::Addition, None, Some(21)));
        let hunk = Hunk {
            header: "@@ -1,19 +1,21 @@".into(), old_start: 1, old_lines: 19, new_start: 1, new_lines: 21,
            lines,
        };
        let open = commentable(&hunk.lines.iter().map(|line| line.origin).collect::<Vec<_>>(), COMMENT_CONTEXT);
        assert!(open[3], "the first endpoint is within three lines of a change");
        assert!(!open[10], "the middle is outside both changes' three-line context");
        assert!(open[17], "the last endpoint is within three lines of a change");
        assert_eq!(
            anchor_in(Path::new("src/login.rs"), AnnotationSide::New, &hunk, 3, 17),
            Err("That line cannot take a comment.")
        );
    }

    #[test]
    fn a_composer_follows_its_line_and_keeps_it_out_of_a_band() {
        let composer = Annotation {
            key: 9, path: PathBuf::from("a.rs"), side: AnnotationSide::New, line: Some(22), start_line: Some(21),
            kind: AnnotationKind::Composer, revision: 0,
        };
        assert!(matches_row(&composer, Some(22), Some(22)));
        assert!(!matches_row(&composer, Some(21), Some(21)), "a range follows its last line");
        let (_, new) = anchored_lines(&[composer], Path::new("a.rs"));
        assert_eq!(new, [21, 22].into_iter().collect());
    }
}
