//! Things a diff's owner pins to its lines (spec §4, B3 review threads):
//! which line an annotation follows, and which context lines must stay out
//! of a collapsed band because something is pinned to them.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AnnotationSide {
    Old,
    New,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AnnotationKind {
    /// A thread under its line; `open` counts toward the file's badge.
    Thread { open: bool },
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
        let (AnnotationKind::Thread { .. }, Some(line)) = (annotation.kind, annotation.line) else {
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
    if !matches!(annotation.kind, AnnotationKind::Thread { .. }) {
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
