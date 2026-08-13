#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TabGroup {
    pub(crate) id: usize,
    pub(crate) tabs: Vec<usize>,
    pub(crate) active_tab: Option<usize>,
}

impl TabGroup {
    pub(crate) fn new(id: usize, tabs: Vec<usize>, active_tab: Option<usize>) -> Self {
        Self {
            id,
            tabs,
            active_tab,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MoveDirection {
    Earlier,
    Later,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MoveTarget {
    CurrentPane,
    Group(usize),
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum MoveCandidates {
    NoOtherTab,
    NoEligibleTab,
    Available(Vec<usize>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TabMachineryError {
    NoGroups,
    UnknownGroup(usize),
    UnknownTab(usize),
    DuplicateGroup(usize),
    DuplicateTab(usize),
    ActiveTabMissing(usize),
}

pub(crate) fn strip_overflows(
    tab_widths: &[f32],
    available_width: f32,
    overflow_width: f32,
) -> bool {
    tab_widths.iter().copied().sum::<f32>() + overflow_width > available_width
}

/// Return how many leading tabs fit while reserving room for the overflow
/// control. The active group owns the strip order, so keeping a prefix here
/// makes the hidden suffix deterministic and lets the overflow menu list the
/// complete group without relying on layout side effects.
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

/// Pure placement and ordering state for tabs in pane groups.
///
/// The shell owns the actual tab entities. This value type owns only their
/// stable ids and the transitions that menus and keyboard commands must
/// share, so a menu cannot drift from the mutation it advertises.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TabMachinery {
    groups: Vec<TabGroup>,
    active_group: usize,
}

impl TabMachinery {
    pub(crate) fn new(
        groups: Vec<TabGroup>,
        active_group: usize,
    ) -> Result<Self, TabMachineryError> {
        if groups.is_empty() {
            return Err(TabMachineryError::NoGroups);
        }
        if !groups.iter().any(|group| group.id == active_group) {
            return Err(TabMachineryError::UnknownGroup(active_group));
        }

        let mut group_ids = Vec::new();
        let mut tab_ids = Vec::new();
        for group in &groups {
            if group_ids.contains(&group.id) {
                return Err(TabMachineryError::DuplicateGroup(group.id));
            }
            group_ids.push(group.id);
            for tab in &group.tabs {
                if tab_ids.contains(tab) {
                    return Err(TabMachineryError::DuplicateTab(*tab));
                }
                tab_ids.push(*tab);
            }
            if let Some(active_tab) = group.active_tab
                && !group.tabs.contains(&active_tab)
            {
                return Err(TabMachineryError::ActiveTabMissing(active_tab));
            }
        }

        Ok(Self {
            groups,
            active_group,
        })
    }

    pub(crate) fn groups(&self) -> &[TabGroup] {
        &self.groups
    }

    pub(crate) fn active_group(&self) -> usize {
        self.active_group
    }

    pub(crate) fn active_tab(&self) -> Option<usize> {
        self.group(self.active_group)
            .and_then(|group| group.active_tab)
    }

    pub(crate) fn group_tabs(&self, group_id: usize) -> Option<&[usize]> {
        self.group(group_id).map(|group| group.tabs.as_slice())
    }

    #[cfg(test)]
    pub(crate) fn add_group(&mut self, group_id: usize) -> bool {
        if self.group(group_id).is_some() {
            return false;
        }
        self.groups.push(TabGroup::new(group_id, Vec::new(), None));
        true
    }

    pub(crate) fn select_tab(&mut self, group_id: usize, tab_id: usize) -> bool {
        let Some(group) = self.group_mut(group_id) else {
            return false;
        };
        if !group.tabs.contains(&tab_id) {
            return false;
        }
        group.active_tab = Some(tab_id);
        self.active_group = group_id;
        true
    }

    pub(crate) fn close_others(&mut self) -> Option<Vec<usize>> {
        let group = self.group_mut(self.active_group)?;
        let active = group.active_tab?;
        let removed = group
            .tabs
            .iter()
            .copied()
            .filter(|tab| *tab != active)
            .collect::<Vec<_>>();
        group.tabs.retain(|tab| *tab == active);
        Some(removed)
    }

    pub(crate) fn close_tabs_to_right(&mut self) -> Option<Vec<usize>> {
        let group = self.group_mut(self.active_group)?;
        let active_index = group
            .active_tab
            .and_then(|active| group.tabs.iter().position(|tab| *tab == active))?;
        let removed = group.tabs.split_off(active_index + 1);
        Some(removed)
    }

    pub(crate) fn remove_tab(&mut self, tab_id: usize) -> Option<usize> {
        let group = self
            .groups
            .iter_mut()
            .find(|group| group.tabs.contains(&tab_id))?;
        let index = group.tabs.iter().position(|tab| *tab == tab_id)?;
        group.tabs.remove(index);
        if group.active_tab == Some(tab_id) {
            group.active_tab = group
                .tabs
                .get(
                    index
                        .saturating_sub(1)
                        .min(group.tabs.len().saturating_sub(1)),
                )
                .copied()
                .or_else(|| group.tabs.first().copied());
        }
        Some(group.id)
    }

    pub(crate) fn move_active_tab(&mut self, direction: MoveDirection) -> bool {
        let Some(group) = self.group_mut(self.active_group) else {
            return false;
        };
        let Some(active) = group.active_tab else {
            return false;
        };
        let Some(index) = group.tabs.iter().position(|tab| *tab == active) else {
            return false;
        };
        let target = match direction {
            MoveDirection::Earlier if index > 0 => index - 1,
            MoveDirection::Later if index + 1 < group.tabs.len() => index + 1,
            _ => return false,
        };
        group.tabs.swap(index, target);
        true
    }

    /// Returns the ids that the Move Existing Tab menu may offer for a target
    /// pane. A target with no tabs outside it gets a distinct explanation from
    /// a target with tabs that all fail the caller's eligibility rule.
    #[cfg(test)]
    pub(crate) fn move_candidates(
        &self,
        target_group: usize,
        mut eligible: impl FnMut(usize) -> bool,
    ) -> MoveCandidates {
        if self.group(target_group).is_none() {
            return MoveCandidates::NoOtherTab;
        }
        let outside = self
            .groups
            .iter()
            .filter(|group| group.id != target_group)
            .flat_map(|group| group.tabs.iter().copied())
            .collect::<Vec<_>>();
        if outside.is_empty() {
            return MoveCandidates::NoOtherTab;
        }
        let available = outside
            .into_iter()
            .filter(|tab| eligible(*tab))
            .collect::<Vec<_>>();
        if available.is_empty() {
            MoveCandidates::NoEligibleTab
        } else {
            MoveCandidates::Available(available)
        }
    }

    /// Moves a tab to the end of a target group and activates it there.
    /// Moving within the same group is intentionally supported: it is the
    /// same transition used by the This Pane menu item, and keeps the menu's
    /// action semantics identical to moving across panes.
    pub(crate) fn move_tab(
        &mut self,
        tab_id: usize,
        target: MoveTarget,
    ) -> Result<(usize, usize), TabMachineryError> {
        let source_group = self
            .groups
            .iter()
            .find(|group| group.tabs.contains(&tab_id))
            .map(|group| group.id)
            .ok_or(TabMachineryError::UnknownTab(tab_id))?;
        let target_group = match target {
            MoveTarget::CurrentPane => self.active_group,
            MoveTarget::Group(id) => id,
        };
        if self.group(target_group).is_none() {
            return Err(TabMachineryError::UnknownGroup(target_group));
        }

        let source = self
            .group_mut(source_group)
            .expect("source group was found above");
        let source_index = source
            .tabs
            .iter()
            .position(|tab| *tab == tab_id)
            .expect("source tab was found above");
        source.tabs.remove(source_index);
        if source.active_tab == Some(tab_id) {
            source.active_tab = source.tabs.first().copied();
        }

        let destination = self
            .group_mut(target_group)
            .expect("target group was validated above");
        destination.tabs.push(tab_id);
        destination.active_tab = Some(tab_id);
        self.active_group = target_group;
        Ok((source_group, target_group))
    }

    fn group(&self, id: usize) -> Option<&TabGroup> {
        self.groups.iter().find(|group| group.id == id)
    }

    fn group_mut(&mut self, id: usize) -> Option<&mut TabGroup> {
        self.groups.iter_mut().find(|group| group.id == id)
    }
}

#[cfg(test)]
mod tests {
    use super::{MoveDirection, MoveTarget, TabGroup, TabMachinery};

    fn groups() -> TabMachinery {
        TabMachinery::new(
            vec![
                TabGroup::new(10, vec![1, 2, 3], Some(2)),
                TabGroup::new(20, vec![4, 5], Some(4)),
            ],
            10,
        )
        .expect("valid tab groups")
    }

    #[test]
    fn close_others_keeps_the_active_tab_and_reports_removed_ids() {
        let mut machinery = groups();

        let removed = machinery.close_others().expect("active group");

        assert_eq!(removed, vec![1, 3]);
        assert_eq!(machinery.group_tabs(10), Some(&[2][..]));
        assert_eq!(machinery.active_tab(), Some(2));
    }

    #[test]
    fn close_to_right_keeps_the_active_tab_and_tabs_before_it() {
        let mut machinery = groups();

        let removed = machinery.close_tabs_to_right().expect("active group");

        assert_eq!(removed, vec![3]);
        assert_eq!(machinery.group_tabs(10), Some(&[1, 2][..]));
        assert_eq!(machinery.active_tab(), Some(2));
    }

    #[test]
    fn active_tab_moves_earlier_and_later_without_wrapping() {
        let mut machinery = groups();

        assert!(machinery.move_active_tab(MoveDirection::Earlier));
        assert_eq!(machinery.group_tabs(10), Some(&[2, 1, 3][..]));
        assert!(machinery.move_active_tab(MoveDirection::Later));
        assert_eq!(machinery.group_tabs(10), Some(&[1, 2, 3][..]));
        assert!(machinery.move_active_tab(MoveDirection::Later));
        assert_eq!(machinery.group_tabs(10), Some(&[1, 3, 2][..]));
        assert!(!machinery.move_active_tab(MoveDirection::Later));
    }

    #[test]
    fn moving_an_existing_tab_to_this_or_another_group_preserves_identity() {
        let mut machinery = groups();

        let moved = machinery
            .move_tab(5, MoveTarget::Group(10))
            .expect("tab can move into another group");
        assert_eq!(moved, (20, 10));
        assert_eq!(machinery.group_tabs(10), Some(&[1, 2, 3, 5][..]));
        assert_eq!(machinery.group_tabs(20), Some(&[4][..]));
        assert_eq!(machinery.active_group(), 10);
        assert_eq!(machinery.active_tab(), Some(5));

        let moved = machinery
            .move_tab(2, MoveTarget::CurrentPane)
            .expect("the current pane accepts the tab");
        assert_eq!(moved, (10, 10));
        assert_eq!(machinery.group_tabs(10), Some(&[1, 3, 5, 2][..]));
    }

    #[test]
    fn move_candidates_explain_no_other_tab_and_no_eligible_tab() {
        let one_group = TabMachinery::new(vec![TabGroup::new(10, vec![1], Some(1))], 10)
            .expect("valid single group");
        assert_eq!(
            one_group.move_candidates(10, |_| true),
            super::MoveCandidates::NoOtherTab
        );

        let machinery = groups();
        assert_eq!(
            machinery.move_candidates(10, |_| false),
            super::MoveCandidates::NoEligibleTab
        );
    }

    #[test]
    fn removing_a_tab_repairs_the_group_active_tab() {
        let mut machinery = groups();

        assert_eq!(machinery.remove_tab(2), Some(10));
        assert_eq!(machinery.group_tabs(10), Some(&[1, 3][..]));
        assert_eq!(machinery.active_tab(), Some(1));
        assert_eq!(machinery.remove_tab(5), Some(20));
        assert_eq!(machinery.group_tabs(20), Some(&[4][..]));
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

    #[test]
    fn an_empty_pane_group_can_be_added_as_a_move_destination() {
        let mut machinery = groups();

        assert!(machinery.add_group(30));
        assert!(!machinery.add_group(30));
        assert_eq!(machinery.group_tabs(30), Some(&[][..]));
    }
}
