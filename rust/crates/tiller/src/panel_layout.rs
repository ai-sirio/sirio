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
    /// `tiller_persistence`. Duplicated rather than imported because the
    /// persisted range is `i64` and layout arithmetic is `f32`; the two are
    /// pinned together by `ranges_match_the_persisted_settings_ranges`.
    pub(crate) fn range(self) -> (f32, f32) {
        match self {
            PanelSide::Left => (160.0, 480.0),
            PanelSide::Right => (220.0, 640.0),
        }
    }
}

/// Narrowest the centre column may get before the side panels start yielding.
/// `MIN_SPLIT_PANE_SIZE` (160, `main.rs:3339`) is the comparable floor for
/// terminal splits; the centre column carries a whole tab strip above a
/// terminal, so it gets twice that.
pub(crate) const MIN_CENTER_WIDTH: f32 = 320.0;

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
) -> (Option<f32>, Option<f32>) {
    let visible = usize::from(left_pref.is_some()) + usize::from(right_pref.is_some());
    let budget =
        viewport_width - (2.0 * outer_inset) - (gap * visible as f32) - MIN_CENTER_WIDTH;

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
            let scale = if requested > 0.0 { budget / requested } else { 0.0 };
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

    /// 1470 wide, both panels asking for their defaults: everything fits, so
    /// nothing is touched.
    #[test]
    fn preferences_survive_untouched_when_they_fit() {
        let (left, right) =
            resolve_panel_widths(1470.0, Some(325.0), Some(405.0), None, 4.0, 4.0);

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
        );

        assert_eq!(left, Some(400.0), "the dragged panel is authoritative");
        // budget = 1000 - 8 - 8 - 320 = 664; the right panel takes 664 - 400.
        assert_eq!(right, Some(264.0));
    }

    /// No drag means the window shrank. Neither panel is "the one being
    /// touched", so they shrink together and their ratio survives.
    #[test]
    fn without_a_drag_both_shrink_proportionally() {
        let (left, right) =
            resolve_panel_widths(1000.0, Some(400.0), Some(400.0), None, 4.0, 4.0);

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
        let (left, right) = resolve_panel_widths(700.0, None, Some(640.0), None, 4.0, 4.0);

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
            resolve_panel_widths(400.0, Some(325.0), Some(405.0), None, 4.0, 4.0);

        assert_eq!(left, Some(160.0));
        assert_eq!(right, Some(220.0));
    }

}