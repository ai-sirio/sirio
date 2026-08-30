use sirio_project::PaneRole;

use crate::OpenTab;

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

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CenterSplit {
    primary_active: Option<usize>,
    secondary_active: Option<usize>,
    focused: PaneRole,
}

impl CenterSplit {
    pub(crate) fn new(tabs: &[OpenTab]) -> Self {
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

    pub(crate) fn rebuild(&mut self, tabs: &[OpenTab]) {
        let primary_ids: Vec<usize> = tabs
            .iter()
            .filter(|tab| tab.kind.pane_role() == PaneRole::Primary)
            .map(|tab| tab.id)
            .collect();
        let secondary_ids: Vec<usize> = tabs
            .iter()
            .filter(|tab| tab.kind.pane_role() == PaneRole::Secondary)
            .map(|tab| tab.id)
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

    pub(crate) fn select_tab(&mut self, tab_id: usize, tabs: &[OpenTab]) -> bool {
        let Some(tab) = tabs.iter().find(|tab| tab.id == tab_id) else {
            return false;
        };
        let role = tab.kind.pane_role();
        self.focused = role;
        self.set_active(role, Some(tab_id));
        true
    }

    pub(crate) fn tabs_for(&self, role: PaneRole, tabs: &[OpenTab]) -> Vec<usize> {
        tabs.iter()
            .filter(|tab| tab.kind.pane_role() == role)
            .map(|tab| tab.id)
            .collect()
    }

    pub(crate) fn tabs_for_focused(&self, tabs: &[OpenTab]) -> Vec<usize> {
        self.tabs_for(self.focused, tabs)
    }

    pub(crate) fn close_others(&self, tabs: &[OpenTab]) -> Option<Vec<usize>> {
        let active = self.active_for_focused()?;
        let role = self.focused;
        let ids = self.tabs_for(role, tabs);
        let removed = ids.into_iter().filter(|id| *id != active).collect::<Vec<_>>();
        Some(removed)
    }

    pub(crate) fn close_tabs_to_right(&self, tabs: &[OpenTab]) -> Option<Vec<usize>> {
        let active = self.active_for_focused()?;
        let role = self.focused;
        let ids = self.tabs_for(role, tabs);
        let index = ids.iter().position(|id| *id == active)?;
        let removed = ids.into_iter().skip(index + 1).collect::<Vec<_>>();
        Some(removed)
    }

    pub(crate) fn move_active_tab(
        &mut self,
        direction: MoveDirection,
        tabs: &mut Vec<OpenTab>,
    ) -> bool {
        let active = match self.active_for_focused() {
            Some(id) => id,
            None => return false,
        };
        let role = self.focused;
        let filtered_ids: Vec<usize> = tabs
            .iter()
            .filter(|tab| tab.kind.pane_role() == role)
            .map(|tab| tab.id)
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
        let from_idx = tabs.iter().position(|tab| tab.id == active).expect("active exists");
        let to_idx = tabs.iter().position(|tab| tab.id == target_id).expect("target exists");
        tabs.swap(from_idx, to_idx);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::{CenterSplit, MoveDirection};
    use sirio_project::{PaneRole, TabKind};

    use crate::session::SessionTabState;
    use crate::{OpenTab, PaneNode, TabContent};

    fn make_tab(id: usize, kind: TabKind) -> OpenTab {
        OpenTab {
            id,
            persistence_id: format!("test-{id}"),
            title: format!("Tab {id}"),
            kind,
            agent_icon: None,
            agent_id: None,
            session_state: SessionTabState::default(),
            panes: PaneNode::leaf(id, TabContent::Terminal { view: unsafe { std::mem::zeroed() } }),
            focused_pane: id,
            title_is_auto_named: true,
        }
    }

    fn tabs_primary_secondary() -> Vec<OpenTab> {
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
        let remaining: Vec<OpenTab> = tabs.into_iter().filter(|t| t.id != 2).collect();
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
