use sirio_project::PaneRole;

use crate::OpenTab;

/// What [`CenterSplit`] needs to know about a tab: its identity, and which
/// half of the split it is stored in.
///
/// A trait rather than `&[OpenTab]` so the split's own tests can exercise it
/// without minting a `gpui` entity. `CenterSplit` is pure state — a test that
/// had to build a real `TerminalView` to check which tab is active would be
/// testing the window instead of the model, and the shortcut that avoids
/// that (`mem::zeroed()` for the entity) is undefined behaviour, not a
/// shortcut.
pub(crate) trait SplitTab {
    fn split_id(&self) -> usize;
    fn split_role(&self) -> PaneRole;
}

impl SplitTab for OpenTab {
    fn split_id(&self) -> usize {
        self.id
    }

    fn split_role(&self) -> PaneRole {
        self.pane
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MoveDirection {
    Earlier,
    Later,
}

pub(crate) fn strip_overflows(
    tab_widths: &[f32],
    available_width: f32,
    overflow_width: f32,
) -> bool {
    tab_widths.iter().copied().sum::<f32>() + overflow_width > available_width
}

pub(crate) fn visible_tab_count(
    tab_widths: &[f32],
    available_width: f32,
    overflow_width: f32,
) -> usize {
    if !strip_overflows(tab_widths, available_width, overflow_width) {
        return tab_widths.len();
    }

    let budget = (available_width - overflow_width).max(0.0);
    let mut used = 0.0;
    tab_widths
        .iter()
        .take_while(|width| {
            let fits = used + **width <= budget;
            if fits {
                used += **width;
            }
            fits
        })
        .count()
}

/// #319: the tab that takes over a half when the one it showed leaves it —
/// the tab that slid into its place, or the one before it when it was last.
/// `remaining` is the half's tab ids after the departure, `position` where
/// the departed tab stood. `close_tab` and `move_tab_to_pane` both ask this,
/// so closing a tab and moving it away cannot pick different neighbours.
pub(crate) fn nearest_remaining(remaining: &[usize], position: usize) -> Option<usize> {
    remaining
        .get(position)
        .or_else(|| remaining.last())
        .copied()
}

/// Where a tab moving into `target` is inserted in the shared tab list.
/// `tabs` is that list *without* the moving tab. With an anchor that is in
/// `target`, the tab lands just before or after it; otherwise after the last
/// tab of `target`, or at the end of the list when `target` holds none. Each
/// strip's order is the relative order of its own tabs in the one list, so
/// either answer leaves the other strip exactly as it was.
pub(crate) fn cross_pane_insertion_index<T: SplitTab>(
    tabs: &[T],
    target: PaneRole,
    anchor: Option<(usize, bool)>,
) -> usize {
    if let Some((anchor_id, before)) = anchor
        && let Some(index) = tabs
            .iter()
            .position(|tab| tab.split_id() == anchor_id && tab.split_role() == target)
    {
        return index + usize::from(!before);
    }
    tabs.iter()
        .rposition(|tab| tab.split_role() == target)
        .map_or(tabs.len(), |index| index + 1)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CenterSplit {
    primary_active: Option<usize>,
    secondary_active: Option<usize>,
    focused: PaneRole,
}

impl CenterSplit {
    pub(crate) fn new<T: SplitTab>(tabs: &[T]) -> Self {
        let mut this = Self {
            primary_active: None,
            secondary_active: None,
            focused: PaneRole::Primary,
        };
        this.rebuild(tabs);
        this
    }

    pub(crate) fn focused(&self) -> PaneRole {
        self.focused
    }

    pub(crate) fn set_focused(&mut self, role: PaneRole) {
        self.focused = role;
    }

    pub(crate) fn active(&self, role: PaneRole) -> Option<usize> {
        match role {
            PaneRole::Primary => self.primary_active,
            PaneRole::Secondary => self.secondary_active,
        }
    }

    pub(crate) fn active_for_focused(&self) -> Option<usize> {
        self.active(self.focused)
    }

    pub(crate) fn set_active(&mut self, role: PaneRole, tab_id: Option<usize>) {
        match role {
            PaneRole::Primary => self.primary_active = tab_id,
            PaneRole::Secondary => self.secondary_active = tab_id,
        }
    }

    pub(crate) fn rebuild<T: SplitTab>(&mut self, tabs: &[T]) {
        let primary_ids: Vec<usize> = tabs
            .iter()
            .filter(|tab| tab.split_role() == PaneRole::Primary)
            .map(|tab| tab.split_id())
            .collect();
        let secondary_ids: Vec<usize> = tabs
            .iter()
            .filter(|tab| tab.split_role() == PaneRole::Secondary)
            .map(|tab| tab.split_id())
            .collect();

        self.primary_active = match self.primary_active {
            Some(id) if primary_ids.contains(&id) => Some(id),
            _ => primary_ids.first().copied(),
        };
        self.secondary_active = match self.secondary_active {
            Some(id) if secondary_ids.contains(&id) => Some(id),
            _ => secondary_ids.first().copied(),
        };

        if self.focused == PaneRole::Secondary && secondary_ids.is_empty() {
            self.focused = PaneRole::Primary;
        }
    }

    pub(crate) fn select_tab<T: SplitTab>(&mut self, tab_id: usize, tabs: &[T]) -> bool {
        let Some(tab) = tabs.iter().find(|tab| tab.split_id() == tab_id) else {
            return false;
        };
        let role = tab.split_role();
        self.focused = role;
        self.set_active(role, Some(tab_id));
        true
    }

    pub(crate) fn tabs_for<T: SplitTab>(&self, role: PaneRole, tabs: &[T]) -> Vec<usize> {
        tabs.iter()
            .filter(|tab| tab.split_role() == role)
            .map(|tab| tab.split_id())
            .collect()
    }

    pub(crate) fn close_others<T: SplitTab>(&self, tabs: &[T]) -> Option<Vec<usize>> {
        let active = self.active_for_focused()?;
        let role = self.focused;
        let ids = self.tabs_for(role, tabs);
        let removed = ids
            .into_iter()
            .filter(|id| *id != active)
            .collect::<Vec<_>>();
        Some(removed)
    }

    pub(crate) fn close_tabs_to_right<T: SplitTab>(&self, tabs: &[T]) -> Option<Vec<usize>> {
        let active = self.active_for_focused()?;
        let role = self.focused;
        let ids = self.tabs_for(role, tabs);
        let index = ids.iter().position(|id| *id == active)?;
        let removed = ids.into_iter().skip(index + 1).collect::<Vec<_>>();
        Some(removed)
    }

    pub(crate) fn move_active_tab<T: SplitTab>(
        &mut self,
        direction: MoveDirection,
        tabs: &mut [T],
    ) -> bool {
        let active = match self.active_for_focused() {
            Some(id) => id,
            None => return false,
        };
        let role = self.focused;
        let filtered_ids: Vec<usize> = tabs
            .iter()
            .filter(|tab| tab.split_role() == role)
            .map(|tab| tab.split_id())
            .collect();
        let pos = match filtered_ids.iter().position(|id| *id == active) {
            Some(p) => p,
            None => return false,
        };
        let target_pos = match direction {
            MoveDirection::Earlier if pos > 0 => pos - 1,
            MoveDirection::Later if pos + 1 < filtered_ids.len() => pos + 1,
            _ => return false,
        };
        let target_id = filtered_ids[target_pos];
        let from_idx = tabs
            .iter()
            .position(|tab| tab.split_id() == active)
            .expect("active exists");
        let to_idx = tabs
            .iter()
            .position(|tab| tab.split_id() == target_id)
            .expect("target exists");
        tabs.swap(from_idx, to_idx);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CenterSplit, MoveDirection, SplitTab, cross_pane_insertion_index, nearest_remaining,
    };
    use sirio_project::{PaneRole, TabKind};

    /// The split reads a tab's id and stored half. Building those two
    /// directly keeps these tests on the model: an `OpenTab` would drag in a
    /// live `TerminalView` entity, which cannot be faked without undefined
    /// behaviour and cannot be built without a window.
    struct TestTab {
        id: usize,
        pane: PaneRole,
    }

    impl SplitTab for TestTab {
        fn split_id(&self) -> usize {
            self.id
        }

        fn split_role(&self) -> PaneRole {
            self.pane
        }
    }

    /// A tab in its kind's home half — every tab, before anything moves.
    fn make_tab(id: usize, kind: TabKind) -> TestTab {
        placed(id, kind, kind.default_pane())
    }

    /// A tab in a chosen half: what a moved terminal or chat looks like.
    fn placed(id: usize, _kind: TabKind, pane: PaneRole) -> TestTab {
        TestTab { id, pane }
    }

    fn tabs_primary_secondary() -> Vec<TestTab> {
        vec![
            make_tab(1, TabKind::Terminal),
            make_tab(2, TabKind::AgentChat),
            make_tab(3, TabKind::Editor),
            make_tab(4, TabKind::Diff),
            make_tab(5, TabKind::Browser),
        ]
    }

    #[test]
    fn new_derives_active_per_role_and_focuses_primary() {
        let tabs = tabs_primary_secondary();
        let split = CenterSplit::new(&tabs);
        assert_eq!(split.active(PaneRole::Primary), Some(1));
        assert_eq!(split.active(PaneRole::Secondary), Some(3));
        assert_eq!(split.focused(), PaneRole::Primary);
    }

    #[test]
    fn select_tab_switches_focus_and_active() {
        let tabs = tabs_primary_secondary();
        let mut split = CenterSplit::new(&tabs);
        assert!(split.select_tab(4, &tabs));
        assert_eq!(split.focused(), PaneRole::Secondary);
        assert_eq!(split.active(PaneRole::Secondary), Some(4));
        assert!(split.select_tab(2, &tabs));
        assert_eq!(split.focused(), PaneRole::Primary);
        assert_eq!(split.active(PaneRole::Primary), Some(2));
    }

    #[test]
    fn rebuild_preserves_existing_active_and_falls_back() {
        let tabs = tabs_primary_secondary();
        let mut split = CenterSplit::new(&tabs);
        split.select_tab(2, &tabs);
        let remaining: Vec<TestTab> = tabs.into_iter().filter(|t| t.id != 2).collect();
        split.rebuild(&remaining);
        assert_eq!(split.active(PaneRole::Primary), Some(1));
        assert_eq!(split.active(PaneRole::Secondary), Some(3));
    }

    #[test]
    fn close_others_keeps_active_and_reports_removed() {
        let tabs = tabs_primary_secondary();
        let mut split = CenterSplit::new(&tabs);
        split.select_tab(1, &tabs);
        let removed = split.close_others(&tabs).expect("has active");
        assert_eq!(removed, vec![2]);
        split.select_tab(4, &tabs);
        let removed = split.close_others(&tabs).expect("has active");
        assert_eq!(removed, vec![3, 5]);
    }

    #[test]
    fn close_to_right_keeps_active_and_right_tabs() {
        let tabs = tabs_primary_secondary();
        let mut split = CenterSplit::new(&tabs);
        split.select_tab(1, &tabs);
        let removed = split.close_tabs_to_right(&tabs).expect("has active");
        assert_eq!(removed, vec![2]);
        split.select_tab(3, &tabs);
        let removed = split.close_tabs_to_right(&tabs).expect("has active");
        assert_eq!(removed, vec![4, 5]);
        split.select_tab(5, &tabs);
        let removed = split.close_tabs_to_right(&tabs).expect("has active");
        assert!(removed.is_empty());
    }

    #[test]
    fn active_tab_moves_earlier_and_later_within_role() {
        let mut tabs = vec![
            make_tab(1, TabKind::Terminal),
            make_tab(2, TabKind::Terminal),
            make_tab(3, TabKind::Terminal),
            make_tab(4, TabKind::Editor),
        ];
        let mut split = CenterSplit::new(&tabs);
        split.select_tab(2, &tabs);
        assert!(split.move_active_tab(MoveDirection::Earlier, &mut tabs));
        assert_eq!(split.tabs_for(PaneRole::Primary, &tabs), vec![2, 1, 3]);
        assert!(split.move_active_tab(MoveDirection::Later, &mut tabs));
        assert_eq!(split.tabs_for(PaneRole::Primary, &tabs), vec![1, 2, 3]);
        assert!(split.move_active_tab(MoveDirection::Later, &mut tabs));
        assert_eq!(split.tabs_for(PaneRole::Primary, &tabs), vec![1, 3, 2]);
        assert!(!split.move_active_tab(MoveDirection::Later, &mut tabs));
    }

    #[test]
    fn secondary_empty_focus_returns_to_primary_on_rebuild() {
        let tabs = vec![make_tab(1, TabKind::Terminal), make_tab(2, TabKind::Editor)];
        let mut split = CenterSplit::new(&tabs);
        split.select_tab(2, &tabs);
        assert_eq!(split.focused(), PaneRole::Secondary);
        let remaining = vec![make_tab(1, TabKind::Terminal)];
        split.rebuild(&remaining);
        assert_eq!(split.focused(), PaneRole::Primary);
        assert_eq!(split.active(PaneRole::Secondary), None);
    }

    /// Spec 2026-09-24 §3: the split reads the half stored on the tab, not
    /// its kind, so a terminal placed on the right is a right-hand tab.
    #[test]
    fn a_terminal_placed_right_belongs_to_the_right_half() {
        let tabs = vec![
            make_tab(1, TabKind::Terminal),
            placed(2, TabKind::Terminal, PaneRole::Secondary),
            make_tab(3, TabKind::Editor),
        ];
        let mut split = CenterSplit::new(&tabs);
        assert_eq!(split.tabs_for(PaneRole::Primary, &tabs), vec![1]);
        assert_eq!(split.tabs_for(PaneRole::Secondary, &tabs), vec![2, 3]);

        assert!(split.select_tab(2, &tabs));
        assert_eq!(split.focused(), PaneRole::Secondary);
        assert_eq!(split.active(PaneRole::Secondary), Some(2));

        split.rebuild(&tabs);
        assert_eq!(
            split.active(PaneRole::Secondary),
            Some(2),
            "rebuild keeps it right"
        );
    }

    /// #319: the half a shown tab leaves goes to the tab that slid into its
    /// place, or the one before it when it was last.
    #[test]
    fn nearest_remaining_takes_the_tab_that_slid_in_or_the_one_before() {
        assert_eq!(nearest_remaining(&[1, 3, 4], 1), Some(3));
        assert_eq!(nearest_remaining(&[1, 3], 2), Some(3));
        assert_eq!(nearest_remaining(&[], 0), None);
    }

    /// Spec §3: a tab crossing the divider lands beside its anchor, or after
    /// the target half's last tab; the other strip's order never changes.
    #[test]
    fn a_tab_crossing_over_lands_by_its_anchor_or_after_the_targets_last_tab() {
        // The shared list without the moving tab: P1 S3 P2 S4.
        let tabs = vec![
            placed(1, TabKind::Terminal, PaneRole::Primary),
            placed(3, TabKind::Editor, PaneRole::Secondary),
            placed(2, TabKind::Terminal, PaneRole::Primary),
            placed(4, TabKind::Diff, PaneRole::Secondary),
        ];
        let at = |anchor| cross_pane_insertion_index(&tabs, PaneRole::Secondary, anchor);
        assert_eq!(at(Some((4, true))), 3, "before 4");
        assert_eq!(at(Some((3, false))), 2, "after 3");
        assert_eq!(at(None), 4, "after the right half's last tab");
        assert_eq!(
            at(Some((2, true))),
            4,
            "an anchor in the wrong half is no anchor"
        );

        let primary_only = vec![placed(1, TabKind::Terminal, PaneRole::Primary)];
        assert_eq!(
            cross_pane_insertion_index(&primary_only, PaneRole::Secondary, None),
            1,
            "an empty target half: the end of the list"
        );
    }

    #[test]
    fn overflow_is_reported_only_when_the_strip_exceeds_available_width() {
        assert!(!super::strip_overflows(&[100.0, 100.0], 230.0, 24.0));
        assert!(super::strip_overflows(&[100.0, 100.0, 100.0], 220.0, 24.0));
    }

    #[test]
    fn visible_tab_count_reserves_the_overflow_control_for_hidden_tabs() {
        assert_eq!(super::visible_tab_count(&[100.0, 100.0], 230.0, 24.0), 2);
        assert_eq!(
            super::visible_tab_count(&[100.0, 100.0, 100.0], 220.0, 24.0),
            1
        );
        assert_eq!(super::visible_tab_count(&[100.0], 20.0, 24.0), 0);
    }
}
