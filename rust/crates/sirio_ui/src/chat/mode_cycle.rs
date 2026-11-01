//! Shift+Tab in the composer: the next permission mode, in the order the
//! agent lists them in the mode menu. Pure, so the rule is proven without a
//! window.

/// Whether choosing this mode asks first. Bypass permissions is the one mode
/// in which the agent runs every tool without asking: `Chat::select_mode`
/// holds it behind a dialog and the mode pill draws it in danger. Shift+Tab
/// never lands on it — a key pressed twice in a row must neither open that
/// dialog nor reach Bypass silently — but may leave it.
pub(super) fn needs_confirmation(mode_id: &str) -> bool {
    mode_id == super::BYPASS_PERMISSIONS_MODE
}

/// The index in `ids` of the mode Shift+Tab moves to from `current`.
///
/// `ids` is the agent's own order, the order of the mode menu. The answer is
/// the first mode after `current` in that order, wrapping round, that
/// [`needs_confirmation`] does not name. `None` means there is nowhere to go,
/// and the key does nothing.
pub(super) fn next_cycle_mode(ids: &[&str], current: &str) -> Option<usize> {
    let len = ids.len();
    // The slot after `current`; an unknown `current` starts from the top.
    let start = ids
        .iter()
        .position(|id| *id == current)
        .map_or(0, |at| at + 1);
    // One lap, `current` itself last: landing back on it is going nowhere.
    (0..len)
        .map(|step| (start + step) % len)
        .find(|&at| ids[at] != current && !needs_confirmation(ids[at]))
}

#[cfg(test)]
mod tests {
    use super::next_cycle_mode;

    const BYPASS: &str = "bypassPermissions";

    #[test]
    fn an_empty_catalogue_goes_nowhere() {
        assert_eq!(next_cycle_mode(&[], "default"), None);
    }

    #[test]
    fn the_next_mode_follows_the_agents_order() {
        let ids = ["default", "acceptEdits", "plan"];
        assert_eq!(next_cycle_mode(&ids, "default"), Some(1));
        assert_eq!(next_cycle_mode(&ids, "acceptEdits"), Some(2));
    }

    #[test]
    fn the_last_mode_wraps_to_the_first() {
        let ids = ["default", "acceptEdits", "plan"];
        assert_eq!(next_cycle_mode(&ids, "plan"), Some(0));
    }

    #[test]
    fn bypass_is_never_the_destination_wherever_it_sits() {
        let last = ["default", "acceptEdits", "plan", BYPASS];
        assert_eq!(next_cycle_mode(&last, "plan"), Some(0));
        let middle = ["default", BYPASS, "plan"];
        assert_eq!(next_cycle_mode(&middle, "default"), Some(2));
        let first = [BYPASS, "default", "plan"];
        assert_eq!(next_cycle_mode(&first, "plan"), Some(1));
    }

    #[test]
    fn leaving_bypass_goes_to_the_next_safe_mode_after_it() {
        let ids = ["default", BYPASS, "plan"];
        assert_eq!(next_cycle_mode(&ids, BYPASS), Some(2));
        let at_end = ["default", "plan", BYPASS];
        assert_eq!(next_cycle_mode(&at_end, BYPASS), Some(0));
    }

    #[test]
    fn a_lone_safe_mode_goes_nowhere() {
        assert_eq!(next_cycle_mode(&["default"], "default"), None);
        assert_eq!(next_cycle_mode(&["default", BYPASS], "default"), None);
    }

    #[test]
    fn from_bypass_a_lone_safe_mode_is_still_somewhere_to_go() {
        assert_eq!(next_cycle_mode(&["default", BYPASS], BYPASS), Some(0));
    }

    #[test]
    fn a_catalogue_of_bypass_alone_goes_nowhere() {
        assert_eq!(next_cycle_mode(&[BYPASS], BYPASS), None);
        assert_eq!(next_cycle_mode(&[BYPASS], "default"), None);
    }

    #[test]
    fn an_unknown_current_mode_goes_to_the_first_safe_one() {
        let ids = [BYPASS, "default", "plan"];
        assert_eq!(next_cycle_mode(&ids, "gone"), Some(1));
    }
}
