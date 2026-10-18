use std::ops::Range;

use super::state::Marks;

/// Where a list of ranges can be looked up by the lines they touch, without keeping the list in start order: each range with its place in the list, sorted by start, and how far the ranges up to each reach.
#[derive(Clone, Debug, Default)]
pub(crate) struct Reach {
    /// (start, end, place in the list), by start.
    sorted: Vec<(usize, usize, usize)>,
    /// The furthest end among the ranges of `sorted` up to and including each.
    furthest: Vec<usize>,
}

impl Reach {
    pub(crate) fn new(ranges: &[Range<usize>]) -> Self {
        let mut sorted: Vec<(usize, usize, usize)> = ranges
            .iter()
            .enumerate()
            .map(|(at, range)| (range.start, range.end, at))
            .collect();
        sorted.sort_by_key(|(start, ..)| *start);
        let mut reach = 0;
        let furthest = sorted
            .iter()
            .map(|(_, end, _)| {
                reach = reach.max(*end);
                reach
            })
            .collect();
        Self { sorted, furthest }
    }

    /// The places, in list order, of the ranges that begin at or before `hi` and end at or after `lo`: every range a line from `lo` to `hi` can touch, for the caller's own test to narrow.
    pub(crate) fn candidates(&self, lo: usize, hi: usize) -> Vec<usize> {
        // Past `hi` none begins; before `first` none has yet reached `lo`.
        let last = self.sorted.partition_point(|(start, ..)| *start <= hi);
        let first = self.furthest[..last].partition_point(|end| *end < lo);
        let mut found: Vec<usize> = self.sorted[first..last]
            .iter()
            .filter(|(_, end, _)| *end >= lo)
            .map(|(.., at)| *at)
            .collect();
        found.sort_unstable();
        found
    }
}

/// Where `offset` lands after `edits`, sorted and apart, each a replaced range and the length written; none when an edit replaced it.
fn moved(offset: usize, edits: &[(Range<usize>, usize)]) -> Option<usize> {
    let mut at = offset as isize;
    for (range, written) in edits {
        if offset <= range.start {
            break;
        }
        if offset < range.end {
            return None;
        }
        at += *written as isize - range.len() as isize;
    }
    Some(at as usize)
}

/// A range through `edits`; none when an edit cut into it.
fn moved_range(range: &Range<usize>, edits: &[(Range<usize>, usize)]) -> Option<Range<usize>> {
    Some(moved(range.start, edits)?..moved(range.end, edits)?)
}

impl Marks {
    /// Looks the diagnostics and backgrounds up again after either list changed.
    pub(crate) fn reindex(&mut self) {
        let ranges: Vec<Range<usize>> = self
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.range.clone())
            .collect();
        self.diagnostic_reach = Reach::new(&ranges);
        let ranges: Vec<Range<usize>> = self
            .backgrounds
            .iter()
            .map(|(range, _)| range.clone())
            .collect();
        self.background_reach = Reach::new(&ranges);
    }

    /// The diagnostics a line from `lo` to `hi` can touch (they begin at or before `hi` and end at or after `lo`), in the owner's order.
    pub(crate) fn diagnostics_touching(
        &self,
        lo: usize,
        hi: usize,
    ) -> Vec<&crate::editor::Diagnostic> {
        self.diagnostic_reach
            .candidates(lo, hi)
            .into_iter()
            .map(|at| &self.diagnostics[at])
            .collect()
    }

    /// The backgrounds a line from `lo` to `hi` can touch, in the owner's order.
    pub(crate) fn backgrounds_touching(
        &self,
        lo: usize,
        hi: usize,
    ) -> Vec<&(Range<usize>, gpui::Hsla)> {
        self.background_reach
            .candidates(lo, hi)
            .into_iter()
            .map(|at| &self.backgrounds[at])
            .collect()
    }
}

/// Carries the owner's offsets through an edit of the text, dropping what it replaced.
pub(crate) fn shift(marks: &mut Marks, edits: &[(Range<usize>, usize)]) {
    marks.diagnostics.retain_mut(|diagnostic| {
        moved_range(&diagnostic.range, edits)
            .map(|range| diagnostic.range = range)
            .is_some()
    });
    marks.backgrounds.retain_mut(|(range, _)| {
        moved_range(range, edits)
            .map(|moved| *range = moved)
            .is_some()
    });
    marks.hints.retain_mut(|hint| {
        moved(hint.offset, edits)
            .map(|at| hint.offset = at)
            .is_some()
    });
    marks.ghost = marks.ghost.take().and_then(|mut ghost| {
        ghost.offset = moved(ghost.offset, edits)?;
        Some(ghost)
    });
    marks.reindex();
}

/// Drops the owner's offsets when the whole text changes; they named the old one.
pub(crate) fn clear(marks: &mut Marks) {
    marks.diagnostics.clear();
    marks.backgrounds.clear();
    marks.hints.clear();
    marks.ghost = None;
    marks.reindex();
}

/// Whether `range` runs forward between whole characters of `text`.
pub(crate) fn fits(text: &str, range: &Range<usize>) -> bool {
    range.start <= range.end
        && text.is_char_boundary(range.start)
        && text.is_char_boundary(range.end)
}

#[cfg(test)]
mod tests {
    use super::{Reach, fits, moved, shift};
    use crate::editor::{Diagnostic, state::Marks};
    use crate::primitives::Severity;
    use std::ops::Range;

    // Task 12b fix 2: a row's washes and diagnostics are looked up by range, not
    // by testing every one of them (a find on `e` washes ~117k places, tested for
    // each of ~45 drawn rows a frame). The lists keep the owner's order, which
    // is the order they draw in (a later wash lies over an earlier one) and the
    // order the status card reads diagnostics in. The ways a lookup can fail,
    // written before the code:
    //   1. a range that began on an earlier line and runs through this one
    //      (a multi-line diagnostic) missed because a search looked only at starts;
    //   2. an empty range at a line's first or last byte missed;
    //   3. a range ending exactly where the line starts, or beginning exactly
    //      where it ends, left out (the callers' tests differ on whether they
    //      count, so the lookup must hand over both);
    //   4. ranges out of start order (diagnostics, worst first) not found, or
    //      found in start order and not the owner's;
    //   5. equal starts, equal ranges, an empty list;
    //   6. after the owner's offsets ride an edit, or are cleared, the lookup
    //      still answering for the old offsets.

    fn scattered_ranges(seed: u64, count: usize) -> Vec<Range<usize>> {
        let mut state = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        let mut next = move |modulo: usize| {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (state >> 33) as usize % modulo
        };
        (0..count)
            .map(|_| {
                let start = next(400);
                let len = match next(6) {
                    0 => 0,
                    1 => 1 + next(300),
                    _ => 1 + next(6),
                };
                start..start + len
            })
            .collect()
    }

    #[test]
    fn a_lookup_by_range_finds_what_a_scan_of_every_range_finds() {
        for seed in 0..200 {
            let ranges = scattered_ranges(seed, seed as usize % 60);
            let reach = Reach::new(&ranges);
            for lo in (0..420).step_by(7) {
                for width in [0, 1, 9, 40] {
                    let hi = lo + width;
                    let scan: Vec<usize> = ranges
                        .iter()
                        .enumerate()
                        .filter(|(_, range)| range.start <= hi && range.end >= lo)
                        .map(|(at, _)| at)
                        .collect();
                    assert_eq!(
                        reach.candidates(lo, hi),
                        scan,
                        "seed {seed}, lines {lo}..={hi}"
                    );
                }
            }
        }
        assert!(Reach::new(&[]).candidates(0, 10).is_empty());
    }

    fn diagnostic(range: Range<usize>, severity: Severity) -> Diagnostic {
        Diagnostic {
            range,
            severity,
            message: "m".into(),
        }
    }

    #[test]
    fn marks_look_their_lists_up_in_the_owners_order_and_follow_edits() {
        let mut marks = Marks {
            diagnostics: vec![
                diagnostic(50..60, Severity::Danger),
                diagnostic(0..300, Severity::Info),
                diagnostic(10..12, Severity::Warning),
                diagnostic(10..12, Severity::Danger),
                diagnostic(120..120, Severity::Success),
            ],
            backgrounds: vec![
                (30..35, gpui::hsla(0.1, 1.0, 0.5, 1.0)),
                (5..9, gpui::hsla(0.2, 1.0, 0.5, 1.0)),
                (31..33, gpui::hsla(0.3, 1.0, 0.5, 1.0)),
            ],
            ..Marks::default()
        };
        marks.reindex();
        let diagnostics = |marks: &Marks, lo, hi| -> Vec<Range<usize>> {
            marks
                .diagnostics_touching(lo, hi)
                .into_iter()
                .map(|diagnostic| diagnostic.range.clone())
                .collect()
        };
        let backgrounds = |marks: &Marks, lo, hi| -> Vec<Range<usize>> {
            marks
                .backgrounds_touching(lo, hi)
                .into_iter()
                .map(|(range, _)| range.clone())
                .collect()
        };
        assert_eq!(diagnostics(&marks, 55, 58), [50..60, 0..300], "a long one runs through");
        assert_eq!(diagnostics(&marks, 0, 11), [0..300, 10..12, 10..12], "the owner's order, not start order");
        assert_eq!(diagnostics(&marks, 60, 70), [50..60, 0..300], "one ending where the line starts");
        assert_eq!(diagnostics(&marks, 120, 125), [0..300, 120..120], "an empty one at the line's first byte");
        assert_eq!(backgrounds(&marks, 31, 34), [30..35, 31..33]);
        assert_eq!(backgrounds(&marks, 0, 5), [5..9], "one beginning where the line ends");

        // An edit that writes 4 bytes in place of 2..4 carries what lies after
        // it and drops what it replaced.
        shift(&mut marks, &[(2..4, 4)]);
        assert_eq!(diagnostics(&marks, 55, 58), [52..62, 0..302], "after the edit");
        assert_eq!(diagnostics(&marks, 52, 53), [52..62, 0..302]);
        assert_eq!(backgrounds(&marks, 31, 34), [32..37, 33..35]);
        assert_eq!(backgrounds(&marks, 5, 9), [7..11], "the old place holds nothing now");

        super::clear(&mut marks);
        assert!(marks.diagnostics_touching(0, 1000).is_empty());
        assert!(marks.backgrounds_touching(0, 1000).is_empty());
    }

    #[test]
    fn offsets_ride_their_edits() {
        let edits = [(2..4, 1), (6..6, 3)];
        let cut = 0..2;
        assert_eq!(moved(1, &edits), Some(1), "before every edit");
        assert_eq!(moved(2, &edits), Some(2), "at an edit's start");
        assert_eq!(moved(3, &edits), None, "inside what it replaced");
        assert_eq!(moved(5, &edits), Some(4), "past it, by its change");
        assert_eq!(
            moved(6, &edits),
            Some(5),
            "at an insertion, before what it adds"
        );
        assert_eq!(moved(8, &edits), Some(10));
        assert!(fits("héllo", &(1..3)) && !fits("héllo", &cut) && !fits("ab", &(1..9)));
    }
}
