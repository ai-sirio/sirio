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
    // That is the side panels' own no-drag rule. The Secondary pane is never
    // auto-hidden to make room — that would take the user's tabs off screen
    // without being asked.
    if usable < MIN_CENTER_PANE_WIDTH * 2.0 {
        let primary = usable * ratio;
        return (primary, Some(usable - primary));
    }
    let primary = (usable * ratio).clamp(MIN_CENTER_PANE_WIDTH, usable - MIN_CENTER_PANE_WIDTH);
    (primary, Some(usable - primary))
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
        // touched", so they scale together and keep their ratio.
        None => {
            let requested = left + right;
            let scale = if requested > 0.0 {
                budget / requested
            } else {
                0.0
            };
            (
                (left * scale).max(left_floor),
                (right * scale).max(right_floor),
            )
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

    /// With no Secondary pane there is nothing to split: the whole centre is
    /// the Primary pane, and `None` says the pane is absent rather than
    /// squeezed to zero.
    #[test]
    fn a_closed_secondary_pane_leaves_the_whole_centre_to_the_primary() {
        let (primary, secondary) = resolve_center_split(900.0, 500, false, 1.0);

        assert_eq!(primary, 900.0);
        assert_eq!(secondary, None);
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
    /// ratio, the same rule the side panels follow when the window shrinks
    /// under them. Neither pane is hidden to make the other fit.
    #[test]
    fn a_centre_too_small_for_both_floors_shrinks_both_and_keeps_the_ratio() {
        let (primary, secondary) = resolve_center_split(501.0, 600, true, 1.0);

        assert_eq!(primary, 300.0);
        assert_eq!(secondary, Some(200.0));
    }

    /// The column's floor is what the side panels must leave it, and it
    /// depends on how many panes are in it.
    #[test]
    fn the_column_floor_counts_the_panes_and_the_divider() {
        assert_eq!(min_center_width(false, 1.0), 320.0);
        assert_eq!(min_center_width(true, 1.0), 641.0);
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
}
