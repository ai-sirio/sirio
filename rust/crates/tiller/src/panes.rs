//! The persistent pane tree for one open tab.
//!
//! This module deliberately contains no GPUI state. A leaf owns the live
//! surface entity, while a split owns only geometry and its two children. The
//! workspace can therefore redraw the tree without recreating a terminal or a
//! chat transcript.

use gpui::{App, Entity, KeyBinding, actions};
use tiller_terminal::TerminalView;
use tiller_ui::{changes::ChangesTab, chat::Chat, file_view::FileView};

actions!(
    pane_commands,
    [
        SplitPaneRight,
        SplitPaneDown,
        FocusPaneLeft,
        FocusPaneRight,
        FocusPaneAbove,
        FocusPaneBelow,
        ClosePane,
        CycleTabForward,
        CycleTabBackward,
        JumpToTab1,
        JumpToTab2,
        JumpToTab3,
        JumpToTab4,
        JumpToTab5,
        JumpToTab6,
        JumpToTab7,
        JumpToTab8,
        JumpToTab9,
        OpenAllTabs,
        OpenTabMenu,
        CloseTab,
        CloseOtherTabs,
        CloseTabsToRight,
        MoveTabEarlier,
        MoveTabLater,
        MoveTabToCurrentPane,
        MoveTabToOtherPane,
        ResumeChat,
    ]
);

/// Install the pane and tab commands at the application level. The shell
/// remains responsible for attaching handlers to its workspace root.
pub(crate) fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("ctrl-alt-left", FocusPaneLeft, None),
        KeyBinding::new("ctrl-alt-right", FocusPaneRight, None),
        KeyBinding::new("ctrl-alt-up", FocusPaneAbove, None),
        KeyBinding::new("ctrl-alt-down", FocusPaneBelow, None),
        KeyBinding::new("ctrl-alt-shift-right", SplitPaneRight, None),
        KeyBinding::new("ctrl-alt-shift-down", SplitPaneDown, None),
        KeyBinding::new("ctrl-alt-w", ClosePane, None),
        KeyBinding::new("ctrl-tab", CycleTabForward, None),
        KeyBinding::new("ctrl-shift-tab", CycleTabBackward, None),
        KeyBinding::new("ctrl-1", JumpToTab1, None),
        KeyBinding::new("ctrl-2", JumpToTab2, None),
        KeyBinding::new("ctrl-3", JumpToTab3, None),
        KeyBinding::new("ctrl-4", JumpToTab4, None),
        KeyBinding::new("ctrl-5", JumpToTab5, None),
        KeyBinding::new("ctrl-6", JumpToTab6, None),
        KeyBinding::new("ctrl-7", JumpToTab7, None),
        KeyBinding::new("ctrl-8", JumpToTab8, None),
        KeyBinding::new("ctrl-9", JumpToTab9, None),
        KeyBinding::new("ctrl-w", CloseTab, None),
    ]);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SplitDirection {
    Horizontal,
    Vertical,
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PaneSize {
    width: f32,
    height: f32,
}

#[allow(dead_code)]
impl PaneSize {
    pub(crate) const fn new(width: f32, height: f32) -> Self {
        Self { width, height }
    }
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum SplitDisabledReason {
    SoleTabInGroup,
    PaneTooSmall {
        direction: SplitDirection,
        available: f32,
        required: f32,
    },
}

impl std::fmt::Display for SplitDisabledReason {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SoleTabInGroup => {
                formatter.write_str("cannot split the sole tab in its pane group")
            }
            Self::PaneTooSmall {
                direction: SplitDirection::Horizontal,
                available,
                required,
            } => write!(
                formatter,
                "pane is too narrow: {available:.0}pt available, {required:.0}pt required"
            ),
            Self::PaneTooSmall {
                direction: SplitDirection::Vertical,
                available,
                required,
            } => write!(
                formatter,
                "pane is too short: {available:.0}pt available, {required:.0}pt required"
            ),
        }
    }
}

#[allow(dead_code)]
const SPLIT_DIVIDER_SIZE: f32 = 6.0;
#[allow(dead_code)]
const MIN_SPLIT_PANE_WIDTH: f32 = 240.0;
#[allow(dead_code)]
const MIN_SPLIT_PANE_HEIGHT: f32 = 160.0;

/// Return the reason a directional split cannot be offered, if any.
///
/// The available dimension is the size each child would receive after the
/// divider is reserved. Keeping this check pure lets menus, keyboard handlers,
/// accessibility callers, and tests use the same explanation.
#[allow(dead_code)]
pub(crate) fn split_disabled_reason(
    direction: SplitDirection,
    size: PaneSize,
    tab_count: usize,
) -> Option<SplitDisabledReason> {
    if tab_count == 1 {
        return Some(SplitDisabledReason::SoleTabInGroup);
    }

    let (length, required) = match direction {
        SplitDirection::Horizontal => (size.width, MIN_SPLIT_PANE_WIDTH),
        SplitDirection::Vertical => (size.height, MIN_SPLIT_PANE_HEIGHT),
    };
    let available = ((length - SPLIT_DIVIDER_SIZE).max(0.0) / 2.0).floor();
    (available < required).then_some(SplitDisabledReason::PaneTooSmall {
        direction,
        available,
        required,
    })
}

/// Pure selection state for the tabs in one pane group.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TabSelection {
    active: usize,
    tab_count: usize,
}

impl TabSelection {
    pub(crate) fn new(tab_count: usize, active: usize) -> Option<Self> {
        (tab_count > 0).then(|| Self {
            active: active.min(tab_count - 1),
            tab_count,
        })
    }

    pub(crate) fn active(self) -> usize {
        self.active
    }

    pub(crate) fn cycle(self, forward: bool) -> Self {
        let active = if forward {
            (self.active + 1) % self.tab_count
        } else if self.active == 0 {
            self.tab_count - 1
        } else {
            self.active - 1
        };
        Self { active, ..self }
    }

    /// Select a 1-based tab position. Positions beyond the group select the
    /// last tab, matching the documented Ctrl-9 behavior.
    pub(crate) fn jump(self, position: usize) -> Self {
        Self {
            active: position.saturating_sub(1).min(self.tab_count - 1),
            ..self
        }
    }
}

pub(crate) enum PaneNode<T> {
    Leaf {
        id: usize,
        // `None` exists only while an in-place tree operation temporarily
        // owns the node. A rendered tree always contains `Some`.
        content: Option<T>,
    },
    Split {
        direction: SplitDirection,
        /// The fraction of the available axis given to the first child.
        ratio: f32,
        first: Box<Self>,
        second: Box<Self>,
    },
}

impl<T> PaneNode<T> {
    pub(crate) fn leaf(id: usize, content: T) -> Self {
        Self::Leaf {
            id,
            content: Some(content),
        }
    }

    pub(crate) fn split_focused(
        &mut self,
        focused: usize,
        new_id: usize,
        direction: SplitDirection,
        content: T,
    ) -> bool {
        let mut content = Some(content);
        self.split_focused_inner(focused, new_id, direction, &mut content)
    }

    fn split_focused_inner(
        &mut self,
        focused: usize,
        new_id: usize,
        direction: SplitDirection,
        content: &mut Option<T>,
    ) -> bool {
        match self {
            Self::Leaf { id, content: old } if *id == focused => {
                let old_id = *id;
                let old_content = old.take().expect("rendered pane has content");
                let new_content = content.take().expect("new pane content is available");
                *self = Self::Split {
                    direction,
                    ratio: 0.5,
                    first: Box::new(Self::Leaf {
                        id: old_id,
                        content: Some(old_content),
                    }),
                    second: Box::new(Self::Leaf {
                        id: new_id,
                        content: Some(new_content),
                    }),
                };
                true
            }
            Self::Leaf { .. } => false,
            Self::Split { first, second, .. } => {
                first.split_focused_inner(focused, new_id, direction, content)
                    || second.split_focused_inner(focused, new_id, direction, content)
            }
        }
    }

    pub(crate) fn contains(&self, id: usize) -> bool {
        match self {
            Self::Leaf { id: leaf_id, .. } => *leaf_id == id,
            Self::Split { first, second, .. } => first.contains(id) || second.contains(id),
        }
    }

    pub(crate) fn first_id(&self) -> Option<usize> {
        match self {
            Self::Leaf { id, .. } => Some(*id),
            Self::Split { first, .. } => first.first_id(),
        }
    }

    pub(crate) fn last_id(&self) -> Option<usize> {
        match self {
            Self::Leaf { id, .. } => Some(*id),
            Self::Split { second, .. } => second.last_id(),
        }
    }

    pub(crate) fn for_each(&self, f: &mut impl FnMut(usize, &T)) {
        match self {
            Self::Leaf {
                id,
                content: Some(content),
            } => f(*id, content),
            Self::Leaf { content: None, .. } => {}
            Self::Split { first, second, .. } => {
                first.for_each(f);
                second.for_each(f);
            }
        }
    }

    pub(crate) fn leaf_ids(&self) -> Vec<usize> {
        let mut result = Vec::new();
        self.push_leaf_ids(&mut result);
        result
    }

    fn push_leaf_ids(&self, result: &mut Vec<usize>) {
        match self {
            Self::Leaf { id, .. } => result.push(*id),
            Self::Split { first, second, .. } => {
                first.push_leaf_ids(result);
                second.push_leaf_ids(result);
            }
        }
    }

    /// Selects the nearest leaf in the stable visual order. For a two-pane
    /// split this is exactly left/right or above/below; for nested splits it
    /// also gives keyboard navigation a deterministic fallback when panes do
    /// not share an edge.
    pub(crate) fn neighbor(
        &self,
        focused: usize,
        direction: SplitDirection,
        forward: bool,
    ) -> Option<usize> {
        match self {
            Self::Leaf { .. } => None,
            Self::Split {
                direction: split_direction,
                first,
                second,
                ..
            } => {
                if first.contains(focused) {
                    if *split_direction == direction {
                        if forward {
                            second.first_id()
                        } else {
                            first
                                .neighbor(focused, direction, forward)
                                .or_else(|| second.last_id())
                        }
                    } else {
                        first.neighbor(focused, direction, forward)
                    }
                } else if second.contains(focused) {
                    if *split_direction == direction {
                        if forward {
                            second
                                .neighbor(focused, direction, forward)
                                .or_else(|| first.first_id())
                        } else {
                            first.last_id()
                        }
                    } else {
                        second.neighbor(focused, direction, forward)
                    }
                } else {
                    None
                }
            }
        }
    }

    pub(crate) fn set_ratio(&mut self, path: &[bool], ratio: f32) -> bool {
        if path.is_empty() {
            if let Self::Split { ratio: current, .. } = self {
                *current = ratio.clamp(0.1, 0.9);
                return true;
            }
            return false;
        }

        match self {
            Self::Split { first, second, .. } => {
                if path[0] {
                    second.set_ratio(&path[1..], ratio)
                } else {
                    first.set_ratio(&path[1..], ratio)
                }
            }
            Self::Leaf { .. } => false,
        }
    }

    /// Removes a leaf and collapses its parent. The remaining sibling is
    /// returned as the replacement node, so it naturally fills the old split.
    pub(crate) fn remove(&mut self, target: usize) -> Option<T> {
        if matches!(self, Self::Leaf { .. }) {
            return None;
        }
        let current = std::mem::replace(self, Self::empty());
        let (replacement, removed) = remove_node(current, target);
        if let Some(replacement) = replacement {
            *self = replacement;
        }
        removed
    }

    fn empty() -> Self {
        Self::Leaf {
            id: usize::MAX,
            content: None,
        }
    }
}

pub(crate) enum PaneContent {
    Chat(Entity<Chat>),
    Terminal { view: Entity<TerminalView> },
    File { view: Entity<FileView> },
    Changes(Entity<ChangesTab>),
}

impl PaneContent {
    pub(crate) fn terminal(&self) -> Option<Entity<TerminalView>> {
        match self {
            Self::Terminal { view } => Some(view.clone()),
            Self::Chat(_) | Self::File { .. } | Self::Changes(_) => None,
        }
    }
}

fn remove_node<T>(node: PaneNode<T>, target: usize) -> (Option<PaneNode<T>>, Option<T>) {
    match node {
        PaneNode::Leaf { id, content } => {
            if id == target {
                (None, content)
            } else {
                (Some(PaneNode::Leaf { id, content }), None)
            }
        }
        PaneNode::Split {
            direction,
            ratio,
            first,
            second,
        } => {
            let first_node = *first;
            let second_node = *second;
            let (new_first, removed) = remove_node(first_node, target);
            if let Some(removed) = removed {
                return (
                    Some(match new_first {
                        Some(first) => PaneNode::Split {
                            direction,
                            ratio,
                            first: Box::new(first),
                            second: Box::new(second_node),
                        },
                        None => second_node,
                    }),
                    Some(removed),
                );
            }

            let first_node = new_first.expect("a non-matching first pane remains");
            let (new_second, removed) = remove_node(second_node, target);
            if let Some(removed) = removed {
                return (
                    Some(match new_second {
                        Some(second) => PaneNode::Split {
                            direction,
                            ratio,
                            first: Box::new(first_node),
                            second: Box::new(second),
                        },
                        None => first_node,
                    }),
                    Some(removed),
                );
            }

            (
                Some(PaneNode::Split {
                    direction,
                    ratio,
                    first: Box::new(first_node),
                    second: Box::new(new_second.expect("a non-matching second pane remains")),
                }),
                None,
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        PaneNode, PaneSize, SplitDirection, SplitDisabledReason, TabSelection,
        split_disabled_reason,
    };

    fn tree() -> PaneNode<&'static str> {
        PaneNode::leaf(0, "root")
    }

    #[test]
    fn splitting_preserves_the_original_leaf_and_adds_a_focusable_leaf() {
        let mut tree = tree();
        assert!(tree.split_focused(0, 1, SplitDirection::Horizontal, "right"));
        assert_eq!(tree.leaf_ids(), vec![0, 1]);
        assert!(tree.contains(0));
        assert!(tree.contains(1));
    }

    #[test]
    fn splitting_the_focused_pane_down_preserves_both_panes() {
        let mut tree = tree();
        assert!(tree.split_focused(0, 1, SplitDirection::Vertical, "down"));
        assert_eq!(tree.leaf_ids(), vec![0, 1]);
        assert_eq!(tree.neighbor(0, SplitDirection::Vertical, true), Some(1));
        assert_eq!(tree.neighbor(1, SplitDirection::Vertical, false), Some(0));
    }

    #[test]
    fn focus_movement_returns_the_neighbour_and_leaves_focus_unchanged_when_absent() {
        let mut tree = tree();
        assert!(tree.split_focused(0, 1, SplitDirection::Horizontal, "right"));

        assert_eq!(tree.neighbor(0, SplitDirection::Horizontal, true), Some(1));
        assert_eq!(tree.neighbor(1, SplitDirection::Horizontal, false), Some(0));
        assert_eq!(tree.neighbor(0, SplitDirection::Vertical, false), None);
        assert_eq!(tree.neighbor(1, SplitDirection::Vertical, true), None);
        assert_eq!(tree.neighbor(99, SplitDirection::Horizontal, true), None);
    }

    #[test]
    fn removing_a_leaf_collapses_the_parent() {
        let mut tree = tree();
        assert!(tree.split_focused(0, 1, SplitDirection::Horizontal, "right"));
        let removed = tree.remove(0);
        assert_eq!(removed, Some("root"));
        assert_eq!(tree.leaf_ids(), vec![1]);
        assert_eq!(tree.first_id(), Some(1));
    }

    #[test]
    fn ratios_are_clamped_and_survive_nested_splits() {
        let mut tree = tree();
        assert!(tree.split_focused(0, 1, SplitDirection::Horizontal, "right"));
        assert!(tree.split_focused(1, 2, SplitDirection::Vertical, "down"));
        assert!(tree.set_ratio(&[true], 0.75));
        assert!(tree.set_ratio(&[], 0.95));
        assert!(!tree.set_ratio(&[true, true], 0.5));
    }

    #[test]
    fn nested_splits_offer_directional_neighbors_and_prune_after_close() {
        let mut tree = tree();
        assert!(tree.split_focused(0, 1, SplitDirection::Horizontal, "right"));
        assert!(tree.split_focused(1, 2, SplitDirection::Vertical, "down"));

        assert_eq!(tree.neighbor(0, SplitDirection::Horizontal, true), Some(1));
        assert_eq!(tree.neighbor(1, SplitDirection::Vertical, true), Some(2));
        assert_eq!(tree.neighbor(2, SplitDirection::Vertical, false), Some(1));

        assert_eq!(tree.remove(2), Some("down"));
        assert_eq!(tree.leaf_ids(), vec![0, 1]);
        assert_eq!(tree.remove(1), Some("right"));
        assert_eq!(tree.leaf_ids(), vec![0]);
    }

    #[test]
    fn tab_cycle_wraps_forward_and_backward() {
        let selection = TabSelection::new(3, 2).expect("a non-empty tab group");
        assert_eq!(selection.cycle(true).active(), 0);

        let selection = TabSelection::new(3, 0).expect("a non-empty tab group");
        assert_eq!(selection.cycle(false).active(), 2);
    }

    #[test]
    fn jumping_to_a_tab_uses_one_based_positions_and_clamps_to_the_last_tab() {
        let selection = TabSelection::new(5, 0).expect("a non-empty tab group");
        assert_eq!(selection.jump(1).active(), 0);
        assert_eq!(selection.jump(5).active(), 4);
        assert_eq!(selection.jump(9).active(), 4);
    }

    #[test]
    fn empty_tab_groups_have_no_selection() {
        assert!(TabSelection::new(0, 0).is_none());
    }

    #[test]
    fn split_disabled_reason_explains_a_too_small_pane() {
        let reason =
            split_disabled_reason(SplitDirection::Horizontal, PaneSize::new(400.0, 500.0), 2)
                .expect("a 400 point pane cannot produce two 240 point panes");

        assert_eq!(
            reason,
            SplitDisabledReason::PaneTooSmall {
                direction: SplitDirection::Horizontal,
                available: 197.0,
                required: 240.0,
            }
        );
        assert_eq!(
            reason.to_string(),
            "pane is too narrow: 197pt available, 240pt required"
        );
    }

    #[test]
    fn split_disabled_reason_explains_a_sole_tab() {
        let reason =
            split_disabled_reason(SplitDirection::Vertical, PaneSize::new(600.0, 500.0), 1)
                .expect("the sole tab cannot create a pane group split");

        assert_eq!(reason, SplitDisabledReason::SoleTabInGroup);
        assert_eq!(
            reason.to_string(),
            "cannot split the sole tab in its pane group"
        );
    }

    #[test]
    fn split_disabled_reason_is_absent_when_both_constraints_are_satisfied() {
        assert_eq!(
            split_disabled_reason(SplitDirection::Vertical, PaneSize::new(600.0, 500.0), 2,),
            None
        );
    }
}
