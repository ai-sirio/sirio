//! Width resolution for the two shell side panels.
//!
//! The persisted width of a panel is a *preference*, not the width that gets
//! drawn. What is drawn is that preference narrowed to the space actually
//! available. Keeping the two apart is what makes shrinking a window
//! non-destructive: the clamp is a projection applied at render time, never
//! written back, so re-widening the window restores the chosen width.
//!
//! Every rule about the two panels competing for space lives here, in a
//! function with no gpui types, no state and no window — which is the only
//! reason those rules are testable at all.

/// Which of the two side panels a gesture or a clamp is talking about.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PanelSide {
    Left,
    Right,
}

impl PanelSide {
    /// The panel's own settings range, mirroring
    /// `settings_ranges::SIDEBAR_WIDTH` / `RIGHT_PANEL_WIDTH` in
    /// `sirio_persistence`. Duplicated rather than imported because the
    /// persisted range is `i64` and layout arithmetic is `f32`; the two are
    /// pinned together by `ranges_match_the_persisted_settings_ranges`.
    pub(crate) fn range(self) -> (f32, f32) {
        match self {
            PanelSide::Left => (220.0, 480.0),
            PanelSide::Right => (220.0, 640.0),
        }
    }
}

/// Narrowest **one centre pane** may get before the side panels start
/// yielding. `MIN_SPLIT_PANE_SIZE` (160, `main.rs:3339`) is the comparable
/// floor for terminal splits; a centre pane carries a whole tab strip above a
/// terminal, so it gets twice that.
///
/// #321: this was the floor for the whole centre column, and the sentence
/// above already described a single pane. With the strip now inside each
/// pane it says what it always meant, and what the *column* needs is derived
/// from it by [`min_center_width`].
pub(crate) const MIN_CENTER_PANE_WIDTH: f32 = 320.0;

/// What the whole centre column needs: one pane's floor, or two of them with
/// the divider between, depending on whether the Secondary pane is open.
pub(crate) fn min_center_width(secondary_open: bool, divider: f32) -> f32 {
    if secondary_open {
        MIN_CENTER_PANE_WIDTH * 2.0 + divider
    } else {
        MIN_CENTER_PANE_WIDTH
    }
}

/// Splits the centre column between the two panes.
///
/// The ratio is a *preference*, exactly like a panel's width above: the clamp
/// happens here, at render time, and is never written back, so narrowing the
/// window and widening it again restores the split the user chose.
///
/// `None` for the Secondary width means the pane is not open — not that it
/// was squeezed to nothing.
pub(crate) fn resolve_center_split(
    center_width: f32,
    ratio_millis: i64,
    secondary_open: bool,
    divider: f32,
) -> (f32, Option<f32>) {
    if !secondary_open {
        return (center_width.max(0.0), None);
    }
    let usable = (center_width - divider).max(0.0);
    let ratio = (ratio_millis as f32 / 1000.0).clamp(0.0, 1.0);
    // Too narrow for two floors: both go under together, keeping their ratio.
    // The side panels never go under theirs: when the window is too small they
    // sit at their floors and the centre absorbs the rest, which is how it can
    // arrive here too narrow. The Secondary pane is never auto-hidden to make
    // room — that would take the user's tabs off screen without being asked.
    if usable < MIN_CENTER_PANE_WIDTH * 2.0 {
        let primary = usable * ratio;
        return (primary, Some(usable - primary));
    }
    let primary = (usable * ratio).clamp(MIN_CENTER_PANE_WIDTH, usable - MIN_CENTER_PANE_WIDTH);
    (primary, Some(usable - primary))
}

/// How wide the Primary half is while collapsed to its rail of tab icons
/// (spec 2026-11-01-primary-rail §3). Fixed, with no pane floor.
pub(crate) const PRIMARY_RAIL_WIDTH: f32 = 40.0;

/// What the whole centre column needs while the rail is drawn: the rail and
/// one pane's floor, with no divider between them.
pub(crate) const MIN_COLLAPSED_CENTER_WIDTH: f32 = PRIMARY_RAIL_WIDTH + MIN_CENTER_PANE_WIDTH;

/// The centre while the Primary half is collapsed to the rail.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct CollapsedCenter {
    /// Always [`PRIMARY_RAIL_WIDTH`].
    pub rail: f32,
    /// Everything right of the rail.
    pub secondary: f32,
    /// The peek's width: what the Primary half would get expanded (§5), so
    /// a peeked terminal keeps its columns. Never wider than the space right
    /// of the rail.
    pub peek: f32,
}

/// Splits the centre while the Primary half is collapsed. The divider is not
/// drawn, so the Secondary gets everything right of the rail; the stored
/// ratio is only read, for the peek, and expanding returns to it unchanged.
pub(crate) fn resolve_collapsed_center(
    center_width: f32,
    ratio_millis: i64,
    divider: f32,
) -> CollapsedCenter {
    let room = (center_width - PRIMARY_RAIL_WIDTH).max(0.0);
    let expanded = resolve_center_split(center_width, ratio_millis, true, divider).0;
    CollapsedCenter {
        rail: PRIMARY_RAIL_WIDTH,
        secondary: room,
        peek: expanded.min(room),
    }
}

/// Narrows each panel's preferred width to what the viewport can actually
/// give it. `None` means the panel is hidden, on the way in and on the way
/// out.
pub(crate) fn resolve_panel_widths(
    viewport_width: f32,
    left_pref: Option<f32>,
    right_pref: Option<f32>,
    dragging: Option<PanelSide>,
    outer_inset: f32,
    gap: f32,
    // #321: passed in rather than read off a constant, because what the
    // centre needs now depends on whether the Secondary pane is open.
    min_center: f32,
) -> (Option<f32>, Option<f32>) {
    let visible = usize::from(left_pref.is_some()) + usize::from(right_pref.is_some());
    let budget = viewport_width - (2.0 * outer_inset) - (gap * visible as f32) - min_center;

    let left = left_pref.unwrap_or(0.0);
    let right = right_pref.unwrap_or(0.0);
    // A hidden panel holds no floor: it is not in the competition at all.
    let left_floor = left_pref.map_or(0.0, |_| PanelSide::Left.range().0);
    let right_floor = right_pref.map_or(0.0, |_| PanelSide::Right.range().0);

    if left + right <= budget {
        return (left_pref, right_pref);
    }

    let (left, right) = match dragging {
        // The dragged panel is authoritative; the other absorbs the rest,
        // down to its own floor.
        Some(PanelSide::Left) => {
            let held = left.clamp(left_floor, (budget - right_floor).max(left_floor));
            (held, (budget - held).max(right_floor))
        }
        Some(PanelSide::Right) => {
            let held = right.clamp(right_floor, (budget - left_floor).max(right_floor));
            ((budget - held).max(left_floor), held)
        }
        // No drag: the window shrank under them. Neither is "the one being
        // touched", so they scale together and keep their ratio. A panel
        // lifted to its floor takes more than its share, so the other one
        // gives that back (down to its own floor, never above its
        // preference); otherwise the pair would overrun the budget and
        // squeeze the centre under its floor.
        None => {
            let requested = left + right;
            let scale = if requested > 0.0 {
                budget / requested
            } else {
                0.0
            };
            let (scaled_left, scaled_right) = (left * scale, right * scale);
            let (mut left_out, mut right_out) =
                (scaled_left.max(left_floor), scaled_right.max(right_floor));
            if left_out + right_out > budget {
                if scaled_right < right_floor {
                    left_out = (budget - right_out).clamp(left_floor, left.max(left_floor));
                } else if scaled_left < left_floor {
                    right_out = (budget - left_out).clamp(right_floor, right.max(right_floor));
                }
            }
            (left_out, right_out)
        }
    };

    (left_pref.map(|_| left), right_pref.map(|_| right))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The panel tests below all predate the split and ask the same question:
    /// how the two side panels share what is left once the centre has its
    /// floor. With the Secondary pane closed that floor is the 320 they were
    /// written against, so they keep asserting exactly what they always did.
    const PANEL_FLOOR: f32 = MIN_CENTER_PANE_WIDTH;

    /// 1470 wide, both panels asking for their defaults: everything fits, so
    /// nothing is touched.
    #[test]
    fn preferences_survive_untouched_when_they_fit() {
        let (left, right) = resolve_panel_widths(
            1470.0,
            Some(325.0),
            Some(405.0),
            None,
            4.0,
            4.0,
            PANEL_FLOOR,
        );

        assert_eq!(left, Some(325.0));
        assert_eq!(right, Some(405.0));
    }

    /// The panel under the hand keeps exactly what the drag asked for; the
    /// other one gives up the difference. Moving the panel the user is *not*
    /// touching reads as a bug, so this is the rule that matters most.
    #[test]
    fn the_dragged_panel_keeps_its_width_and_the_other_yields() {
        let (left, right) = resolve_panel_widths(
            1000.0,
            Some(400.0),
            Some(400.0),
            Some(PanelSide::Left),
            4.0,
            4.0,
            PANEL_FLOOR,
        );

        assert_eq!(left, Some(400.0), "the dragged panel is authoritative");
        // budget = 1000 - 8 - 8 - 320 = 664; the right panel takes 664 - 400.
        assert_eq!(right, Some(264.0));
    }

    /// No drag means the window shrank. Neither panel is "the one being
    /// touched", so they shrink together and their ratio survives.
    #[test]
    fn without_a_drag_both_shrink_proportionally() {
        let (left, right) = resolve_panel_widths(
            1000.0,
            Some(400.0),
            Some(400.0),
            None,
            4.0,
            4.0,
            PANEL_FLOOR,
        );

        assert_eq!(left, Some(332.0));
        assert_eq!(right, Some(332.0));
    }

    /// When the scaled right panel falls under its floor and is lifted to
    /// it, the left one must not keep its scaled width: together they would
    /// overrun the budget and squeeze the centre under its floor.
    #[test]
    fn without_a_drag_a_floored_panel_leaves_the_rest_to_the_other() {
        let (left, right) = resolve_panel_widths(
            1100.0,
            Some(300.0),
            Some(240.0),
            None,
            4.0,
            4.0,
            MIN_CENTER_PANE_WIDTH * 2.0 + 1.0,
        );

        // budget = 1100 - 8 - 8 - 641 = 443; the right panel floors at 220.
        assert_eq!(left, Some(223.0));
        assert_eq!(right, Some(220.0));
    }

    /// The mirror: the scaled left panel falls under its floor, and the right
    /// one gives back what the lift cost.
    #[test]
    fn without_a_drag_a_floored_left_panel_leaves_the_rest_to_the_right() {
        let (left, right) = resolve_panel_widths(
            1100.0,
            Some(300.0),
            Some(640.0),
            None,
            4.0,
            4.0,
            MIN_CENTER_PANE_WIDTH * 2.0 + 1.0,
        );

        // budget = 443; the left panel floors at 220, the right takes 443 - 220.
        assert_eq!(left, Some(220.0));
        assert_eq!(right, Some(223.0));
    }

    /// The yielding panel stops at the low end of its own settings range
    /// rather than collapsing to nothing.
    #[test]
    fn the_yielding_panel_stops_at_its_own_floor() {
        let (left, right) = resolve_panel_widths(
            800.0,
            Some(480.0),
            Some(400.0),
            Some(PanelSide::Left),
            4.0,
            4.0,
            PANEL_FLOOR,
        );

        // budget = 800 - 8 - 8 - 320 = 464. The right panel's floor is 220,
        // so the left panel cannot hold more than 464 - 220 = 244.
        assert_eq!(left, Some(244.0));
        assert_eq!(right, Some(220.0));
    }

    /// A hidden panel is not competing: it contributes neither width nor a
    /// floor, and it stays hidden in the output.
    #[test]
    fn a_hidden_panel_neither_takes_space_nor_holds_a_floor() {
        let (left, right) =
            resolve_panel_widths(700.0, None, Some(640.0), None, 4.0, 4.0, PANEL_FLOOR);

        assert_eq!(left, None, "a hidden panel stays hidden");
        // budget = 700 - 8 - 4 - 320 = 368
        assert_eq!(right, Some(368.0));
    }

    /// A window too small to honour both floors still has to draw something.
    /// The centre column is allowed below its minimum rather than the panels
    /// going to zero or negative.
    #[test]
    fn a_window_too_small_for_both_floors_still_returns_the_floors() {
        let (left, right) =
            resolve_panel_widths(400.0, Some(325.0), Some(405.0), None, 4.0, 4.0, PANEL_FLOOR);

        assert_eq!(left, Some(220.0));
        assert_eq!(right, Some(220.0));
    }

    /// The ratio is thousandths, and the divider comes out of the middle
    /// before either pane is measured.
    #[test]
    fn the_ratio_divides_what_is_left_after_the_divider() {
        // 1200 usable, not 1000: at 1000 a 700 ratio would leave the other
        // pane 300 and the floor would bind, which is the *next* test's
        // question, not this one's.
        let (primary, secondary) = resolve_center_split(1201.0, 700, true, 1.0);

        assert_eq!(primary, 840.0);
        assert_eq!(secondary, Some(360.0));
    }

    /// A ratio that would starve one pane is clamped at that pane's floor —
    /// and clamped *here*, at render time. Nothing is written back, so
    /// widening the window again restores the ratio the user chose.
    #[test]
    fn a_lopsided_ratio_stops_at_the_pane_floor() {
        let (primary, secondary) = resolve_center_split(1001.0, 950, true, 1.0);

        assert_eq!(
            primary, 680.0,
            "the primary pane stops short of starving the other"
        );
        assert_eq!(secondary, Some(320.0), "the secondary pane keeps its floor");
    }

    /// Too narrow for two floors: both go under together and keep their
    /// ratio. The side panels never go under their own floors, so this is
    /// the centre absorbing a window too small for everything. Neither pane
    /// is hidden to make the other fit.
    #[test]
    fn a_centre_too_small_for_both_floors_shrinks_both_and_keeps_the_ratio() {
        let (primary, secondary) = resolve_center_split(501.0, 600, true, 1.0);

        assert_eq!(primary, 300.0);
        assert_eq!(secondary, Some(200.0));
    }

    /// Two numbers, two crates, no compiler tying them together. This test is
    /// the only thing that notices when one moves without the other.
    #[test]
    fn panel_ranges_match_the_persisted_settings_ranges() {
        use sirio_persistence::settings_ranges;

        let (left_floor, left_ceiling) = PanelSide::Left.range();
        assert_eq!(left_floor, *settings_ranges::SIDEBAR_WIDTH.start() as f32);
        assert_eq!(left_ceiling, *settings_ranges::SIDEBAR_WIDTH.end() as f32);

        let (right_floor, right_ceiling) = PanelSide::Right.range();
        assert_eq!(
            right_floor,
            *settings_ranges::RIGHT_PANEL_WIDTH.start() as f32
        );
        assert_eq!(
            right_ceiling,
            *settings_ranges::RIGHT_PANEL_WIDTH.end() as f32
        );
    }

    // Spec 2026-11-01-primary-rail §3/§5, written from their failure modes
    // before the code: the rail and the Secondary leave a gap or overlap; the
    // peek is not the expanded Primary at a normal width; the peek runs past
    // the space right of the rail; the peek drops under the expanded half's
    // floor although there is room; a tiny centre goes negative; an
    // out-of-range ratio escapes the clamp.

    #[test]
    fn collapsed_center_secondary_fills_the_rest() {
        let c = resolve_collapsed_center(1400.0, 500, 1.0);
        assert_eq!(c.rail, PRIMARY_RAIL_WIDTH);
        assert_eq!(c.rail + c.secondary, 1400.0);
    }

    #[test]
    fn collapsed_center_peek_matches_the_expanded_primary() {
        let c = resolve_collapsed_center(1400.0, 400, 1.0);
        assert_eq!(c.peek, resolve_center_split(1400.0, 400, true, 1.0).0);
    }

    #[test]
    fn collapsed_center_peek_fits_beside_the_rail() {
        let c = resolve_collapsed_center(500.0, 900, 1.0);
        assert!(c.peek <= 500.0 - PRIMARY_RAIL_WIDTH);
    }

    #[test]
    fn collapsed_center_peek_keeps_its_floor_when_there_is_room() {
        let c = resolve_collapsed_center(700.0, 50, 1.0);
        assert!(c.peek >= MIN_CENTER_PANE_WIDTH);
    }

    #[test]
    fn collapsed_center_never_goes_negative() {
        let c = resolve_collapsed_center(10.0, 500, 1.0);
        assert!(c.secondary >= 0.0 && c.peek >= 0.0);
    }

    #[test]
    fn collapsed_center_clamps_the_ratio() {
        let c = resolve_collapsed_center(1400.0, 5000, 1.0);
        assert!(c.peek <= 1400.0 - PRIMARY_RAIL_WIDTH);
    }
}
