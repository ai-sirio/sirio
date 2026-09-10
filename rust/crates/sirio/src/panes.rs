//! The persistent pane tree for one open tab.
//!
//! This module deliberately contains no GPUI state. A leaf owns the live
//! surface entity, while a split owns only geometry and its two children. The
//! workspace can therefore redraw the tree without recreating a terminal or a
//! chat transcript.

use std::time::{Duration, Instant};

use gpui::{App, Entity, KeyBinding, actions};
use sirio_activity::{AgentActivityModel, Transition, detect_content_status};
use sirio_terminal::{TerminalActivityEvent, TerminalExitStatus, TerminalView};
use sirio_ui::{browser::BrowserSurface, changes::ChangesTab, chat::Chat, file_view::FileView};

/// Layer D refresh cadence. 500 ms is fast enough for the sidebar to notice
/// a native foreground agent without making `/proc` traversal a redraw-rate
/// poll; it also leaves the interval easy to tune in the one place the app's
/// subscription contract names.
pub(crate) const PROCESS_SIGNAL_INTERVAL: Duration = Duration::from_millis(500);

#[allow(dead_code)]
pub(crate) fn process_signal_interval() -> Duration {
    PROCESS_SIGNAL_INTERVAL
}

/// Routes a terminal's typed PTY evidence into the workspace's one activity
/// model. The app owns the model; this module owns the translation seam so
/// context-menu `SetTitle` events cannot accidentally be treated as OSC data.
pub(crate) fn apply_terminal_activity_event(
    activity: &mut AgentActivityModel,
    pane_id: &str,
    event: &TerminalActivityEvent,
    now: Instant,
) -> Option<Transition> {
    match event {
        TerminalActivityEvent::OscTitle(title) => activity.handle_title_change(pane_id, title, now),
        TerminalActivityEvent::OutputSettled { scrollback } => {
            let agent_id = activity.agent_id(pane_id)?;
            let status = detect_content_status(scrollback, agent_id)?;
            activity.apply_content_signal(pane_id, status, now)
        }
        TerminalActivityEvent::ChildExited { status } => {
            // Process- and title-owned identities have independent clearing
            // paths. A PTY child-exit notification is the spawn-owned path;
            // never let it clear or rewrite either of the other two kinds.
            if activity.is_process_owned(pane_id) || activity.is_title_owned(pane_id) {
                return None;
            }
            activity.apply_exit_result(pane_id, terminal_exit_code(*status), now)
        }
    }
}

fn terminal_exit_code(status: TerminalExitStatus) -> i32 {
    match status {
        TerminalExitStatus::Success => 0,
        TerminalExitStatus::Code(code) => code,
        TerminalExitStatus::Signal(signal) => 128 + signal,
        TerminalExitStatus::Unknown => 1,
    }
}

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
        // #226: ctrl-w must reach the shell (readline delete-word) when a
        // terminal is focused, so the binding is scoped to !Terminal. The
        // close chord from inside a terminal is ctrl-shift-w, one line below.
        // Mirrored in main.rs SirioWorkspace::new -- keep both in sync.
        KeyBinding::new("ctrl-w", CloseTab, Some("!Terminal")),
        // #226: the close chord that survives inside a terminal.
        KeyBinding::new("ctrl-shift-w", CloseTab, None),
    ]);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SplitDirection {
    Horizontal,
    Vertical,
}

/// Which side of the focused pane receives the newly-created pane.
///
/// Keeping placement separate from the axis preserves the existing focus and
/// navigation API while allowing left/above splits to share the same tree
/// representation and persistence format.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SplitPlacement {
    Before,
    After,
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
        self.split_focused_with_placement(
            focused,
            new_id,
            direction,
            SplitPlacement::After,
            content,
        )
    }

    pub(crate) fn split_focused_with_placement(
        &mut self,
        focused: usize,
        new_id: usize,
        direction: SplitDirection,
        placement: SplitPlacement,
        content: T,
    ) -> bool {
        // F-CORE-WSP-06: a live caller always mints `new_id` from a
        // monotonic counter (`self.next_pane_id`), so this can never fire
        // for a real split -- it exists for the one caller that does *not*
        // control `new_id`: `replay_pane_events`, replaying a persisted
        // `pane_events` blob. `SessionTabState::decode` only rejects
        // syntactically invalid JSON; a blob where two `Split` events name
        // the same `new_id` decodes cleanly and would otherwise land two
        // leaves under one id -- the same leaf id then answers to two
        // different live `TabContent`s for every id-keyed lookup
        // (`contains`, activity's `pane-{id}` status, focus). Refusing the
        // split here is the same graceful-degradation shape the `focused`
        // check below already uses for a dangling reference: the malformed
        // event is dropped, not applied.
        if self.contains(new_id) {
            return false;
        }
        let mut content = Some(content);
        self.split_focused_inner(focused, new_id, direction, placement, &mut content)
    }

    fn split_focused_inner(
        &mut self,
        focused: usize,
        new_id: usize,
        direction: SplitDirection,
        placement: SplitPlacement,
        content: &mut Option<T>,
    ) -> bool {
        match self {
            Self::Leaf { id, content: old } if *id == focused => {
                let old_id = *id;
                let old_content = old.take().expect("rendered pane has content");
                let new_content = content.take().expect("new pane content is available");
                let (first, second) = match placement {
                    SplitPlacement::Before => (
                        Self::Leaf {
                            id: new_id,
                            content: Some(new_content),
                        },
                        Self::Leaf {
                            id: old_id,
                            content: Some(old_content),
                        },
                    ),
                    SplitPlacement::After => (
                        Self::Leaf {
                            id: old_id,
                            content: Some(old_content),
                        },
                        Self::Leaf {
                            id: new_id,
                            content: Some(new_content),
                        },
                    ),
                };
                *self = Self::Split {
                    direction,
                    ratio: 0.5,
                    first: Box::new(first),
                    second: Box::new(second),
                };
                true
            }
            Self::Leaf { .. } => false,
            Self::Split { first, second, .. } => {
                first.split_focused_inner(focused, new_id, direction, placement, content)
                    || second.split_focused_inner(focused, new_id, direction, placement, content)
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
        self.take(target)
    }

    /// Removes a leaf even when it is the only pane. Callers that move a
    /// live terminal to another tab need this stronger transition; ordinary
    /// close operations keep using [`Self::remove`] so they never leave an
    /// accidental empty terminal tree behind.
    pub(crate) fn take(&mut self, target: usize) -> Option<T> {
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
    Browser(Entity<BrowserSurface>),
}

impl PaneContent {
    pub(crate) fn terminal(&self) -> Option<Entity<TerminalView>> {
        match self {
            Self::Terminal { view } => Some(view.clone()),
            Self::Chat(_) | Self::File { .. } | Self::Changes(_) | Self::Browser(_) => None,
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
        PaneNode, PaneSize, SplitDirection, SplitDisabledReason, SplitPlacement, TabSelection,
        apply_terminal_activity_event, process_signal_interval, split_disabled_reason,
    };
    use sirio_activity::{AgentActivityModel, AgentStatus};
    use sirio_terminal::{TerminalActivityEvent, TerminalExitStatus, TerminalView};
    use std::time::{Duration, Instant};

    fn tree() -> PaneNode<&'static str> {
        PaneNode::leaf(0, "root")
    }

    #[test]
    fn terminal_events_feed_title_and_settled_content_into_the_one_model() {
        let now = Instant::now();
        let mut activity = AgentActivityModel::new();

        let title = apply_terminal_activity_event(
            &mut activity,
            "pane-title",
            &TerminalActivityEvent::OscTitle("✳ idle".to_string()),
            now,
        );
        assert_eq!(
            title.map(|transition| transition.new),
            Some(AgentStatus::NeedsInput)
        );

        activity.agent_spawned("pane-content", "claude", now);
        let content = apply_terminal_activity_event(
            &mut activity,
            "pane-content",
            &TerminalActivityEvent::OutputSettled {
                scrollback: "Do you want to proceed?\n1. Yes".to_string(),
            },
            now + Duration::from_millis(1),
        );
        assert_eq!(
            content.map(|transition| transition.new),
            Some(AgentStatus::NeedsInput)
        );
    }

    #[test]
    fn process_owned_status_survives_title_and_child_exit_events() {
        let now = Instant::now();
        let mut activity = AgentActivityModel::new();
        activity.process_identified("pane-process", "codex");

        let title = apply_terminal_activity_event(
            &mut activity,
            "pane-process",
            &TerminalActivityEvent::OscTitle("zsh".to_string()),
            now,
        );
        assert_eq!(title, None);
        assert_eq!(activity.status("pane-process"), Some(AgentStatus::Running));
        assert!(activity.is_process_owned("pane-process"));

        let exit = apply_terminal_activity_event(
            &mut activity,
            "pane-process",
            &TerminalActivityEvent::ChildExited {
                status: TerminalExitStatus::Success,
            },
            now,
        );
        assert_eq!(exit, None);
        assert_eq!(activity.status("pane-process"), Some(AgentStatus::Running));

        // The real periodic caller supplies the shell PID to this helper; the
        // process model, not a title or child-exit event, owns the clearing.
        assert_eq!(process_signal_interval(), Duration::from_millis(500));
        // The production path takes one snapshot per tick and walks every
        // pane against it on the background executor (#248), then applies
        // the observations on the UI thread; the test drives that split
        // rather than a wrapper nothing calls.
        let snapshot = sirio_activity::process::take_snapshot().expect("process snapshot");
        let observation =
            sirio_activity::process::inspect_foreground_agent_in(&snapshot, std::process::id());
        let refresh = activity.apply_process_signal("pane-process", observation);
        assert!(matches!(refresh, Ok(None)));
        assert_eq!(activity.status("pane-process"), None);
        assert!(!activity.is_process_owned("pane-process"));
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
    fn splitting_before_the_focused_pane_places_the_new_pane_on_the_requested_side() {
        let mut tree = tree();
        assert!(tree.split_focused_with_placement(
            0,
            1,
            SplitDirection::Horizontal,
            SplitPlacement::Before,
            "left",
        ));
        assert_eq!(tree.leaf_ids(), vec![1, 0]);
        assert_eq!(tree.neighbor(0, SplitDirection::Horizontal, false), Some(1));
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

    #[test]
    fn layer_a_debounce_still_suppresses_two_title_events_in_order() {
        let started = Instant::now();
        let mut activity = AgentActivityModel::new();
        activity.agent_spawned("pane-debounce", "claude", started);
        activity.notify(
            "pane-debounce",
            AgentStatus::Running,
            started + Duration::from_millis(500),
        );

        assert_eq!(
            apply_terminal_activity_event(
                &mut activity,
                "pane-debounce",
                &TerminalActivityEvent::OscTitle("✳ idle".to_string()),
                started + Duration::from_secs(1),
            ),
            None,
            "the first contradictory OSC title is inside Layer A's debounce"
        );
        assert_eq!(
            apply_terminal_activity_event(
                &mut activity,
                "pane-debounce",
                &TerminalActivityEvent::OscTitle("✳ idle".to_string()),
                started + Duration::from_secs(2),
            )
            .map(|transition| transition.new),
            Some(AgentStatus::NeedsInput),
            "the second OSC title is eligible after the debounce window"
        );
    }

    /// This is the app seam, not a model-only test: a real PTY emits two
    /// different signals, and the observable activity state follows first
    /// Layer B and then settled Layer C output.
    #[gpui::test]
    async fn real_pty_activity_status_follows_osc_title_then_settled_content(
        cx: &mut gpui::TestAppContext,
    ) {
        use sirio_theme::Theme;
        use std::sync::{Arc, Mutex};

        cx.set_global(Theme::light());
        let working_directory =
            std::env::temp_dir().join(format!("sirio-pane-activity-e2e-{}", std::process::id()));
        std::fs::create_dir_all(&working_directory).expect("create PTY directory");
        // The Windows fixture writes the title and the question in one go
        // rather than with a gap between them: `print` emits once. Nothing
        // here depends on the gap — the title is parsed the moment it lands
        // and the content scan runs when output settles, by which point
        // both are on the grid, in the same order.
        let shell = crate::pty_fixture_shell(
            "sleep 0.1; printf '\\033]0;. working\\007'; sleep 0.2; printf 'Do you want to proceed?\\n'; exec sleep 1",
            &[
                "print",
                "\\033]0;. working\\007Do you want to proceed?\\n",
                "--delay",
                "0.1",
                "--sleep",
                "1",
            ],
        );
        let (terminal, cx) = cx.add_window_view(|_, cx| {
            TerminalView::with_shell(&working_directory, shell, cx).expect("spawn PTY")
        });
        let observed = Arc::new(Mutex::new((AgentActivityModel::new(), Vec::new())));
        let _subscription = cx.update(|_, app| {
            let observed = observed.clone();
            app.subscribe(&terminal, move |_, event: &TerminalActivityEvent, _| {
                let mut observed = observed.lock().expect("activity lock");
                let _ = apply_terminal_activity_event(
                    &mut observed.0,
                    "pane-real-pty",
                    event,
                    Instant::now(),
                );
                let status = observed.0.status("pane-real-pty");
                observed.1.push((event.clone(), status));
            })
        });

        let deadline = std::time::Instant::now() + Duration::from_secs(30);
        while std::time::Instant::now() < deadline {
            cx.run_until_parked();
            cx.background_executor
                .advance_clock(Duration::from_millis(5));
            cx.run_until_parked();
            if observed
                .lock()
                .expect("activity lock")
                .1
                .iter()
                .any(|(event, status)| {
                    matches!(event, TerminalActivityEvent::OutputSettled { scrollback }
                    if scrollback.contains("Do you want to proceed?"))
                        && *status == Some(AgentStatus::NeedsInput)
                })
            {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let observed = observed.lock().expect("activity lock");
        assert!(
            observed.1.iter().any(|(event, status)| {
                event == &TerminalActivityEvent::OscTitle(". working".to_string())
                    && *status == Some(AgentStatus::Running)
            }),
            "the real OSC title must identify Claude as working"
        );
        assert!(
            observed.1.iter().any(|(event, status)| {
                matches!(event, TerminalActivityEvent::OutputSettled { scrollback }
                if scrollback.contains("Do you want to proceed?"))
                    && *status == Some(AgentStatus::NeedsInput)
            }),
            "settled real PTY text must drive Layer C"
        );
        drop(observed);

        terminal.update(&mut cx.cx, |terminal, _| terminal.shutdown());
        cx.run_until_parked();
    }

    #[gpui::test]
    async fn real_pty_layer_a_debounce_suppresses_first_title_and_accepts_second(
        cx: &mut gpui::TestAppContext,
    ) {
        use sirio_theme::Theme;
        use std::sync::{Arc, Mutex};

        cx.set_global(Theme::light());
        let working_directory =
            std::env::temp_dir().join(format!("sirio-pane-debounce-e2e-{}", std::process::id()));
        std::fs::create_dir_all(&working_directory).expect("create PTY directory");
        // Both halves alternate a title with a wait for one typed line, so
        // the test can assert on each contradictory title before the next
        // one is allowed out. On unix that is `printf` and `IFS= read -r _`;
        // the fixture's `titles` mode is the same alternation, one argument
        // per `printf`.
        let shell = crate::pty_fixture_shell(
            "printf '\\033]0;. working\\007'; IFS= read -r _; printf '\\033]0;✳ idle\\007'; IFS= read -r _; printf '\\033]0;✳ idle\\007'; exec sleep 1",
            &[
                "titles",
                "\\033]0;. working\\007",
                "\\033]0;✳ idle\\007",
                "\\033]0;✳ idle\\007",
                "--sleep",
                "1",
            ],
        );
        let (terminal, cx) = cx.add_window_view(|_, cx| {
            TerminalView::with_shell(&working_directory, shell, cx).expect("spawn PTY")
        });
        // The PTY supplies the real event ordering, but the debounce assertion
        // must not depend on how long a loaded test process takes to deliver a
        // callback. Keep the logical evidence clock deterministic while still
        // exercising the real terminal boundary.
        let evidence_start = Instant::now();
        let observed = Arc::new(Mutex::new((AgentActivityModel::new(), Vec::new())));
        let state = observed.clone();
        let _subscription = cx.update(|_, app| {
            app.subscribe(&terminal, move |_, event: &TerminalActivityEvent, _| {
                let mut state = state.lock().expect("activity lock");
                if let TerminalActivityEvent::OscTitle(title) = event {
                    if title == ". working" {
                        let _ = apply_terminal_activity_event(
                            &mut state.0,
                            "pane-debounce-pty",
                            event,
                            evidence_start,
                        );
                        state
                            .0
                            .notify("pane-debounce-pty", AgentStatus::Running, evidence_start);
                    } else if title == "✳ idle" {
                        let status_before = state.0.status("pane-debounce-pty");
                        let evidence_time = evidence_start
                            + if state.1.is_empty() {
                                Duration::from_millis(100)
                            } else {
                                Duration::from_millis(1600)
                            };
                        state.1.push((evidence_time, status_before));
                        let _ = apply_terminal_activity_event(
                            &mut state.0,
                            "pane-debounce-pty",
                            event,
                            evidence_time,
                        );
                    }
                }
            })
        });

        let deadline = std::time::Instant::now() + Duration::from_secs(30);
        let mut released = false;
        while std::time::Instant::now() < deadline {
            cx.run_until_parked();
            cx.background_executor
                .advance_clock(Duration::from_millis(5));
            cx.run_until_parked();
            let (running, first_title_arrived) = {
                let state = observed.lock().expect("activity lock");
                (
                    state.0.status("pane-debounce-pty") == Some(AgentStatus::Running),
                    state.1.len() >= 1,
                )
            };
            if running && !released {
                terminal.update(&mut cx.cx, |terminal, _| terminal.input("\n"));
                released = true;
            }
            if first_title_arrived {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        {
            let state = observed.lock().expect("activity lock");
            assert_eq!(
                state.1.len(),
                1,
                "the first real contradictory title arrived"
            );
            assert_eq!(
                state.0.status("pane-debounce-pty"),
                Some(AgentStatus::Running),
                "Layer A must suppress the first real OSC idle title"
            );
        }

        // Release the fixture's second sentinel only after the first real
        // contradictory title has been observed and asserted.
        terminal.update(&mut cx.cx, |terminal, _| terminal.input("\n"));

        let deadline = std::time::Instant::now() + Duration::from_secs(30);
        while std::time::Instant::now() < deadline {
            cx.run_until_parked();
            cx.background_executor
                .advance_clock(Duration::from_millis(5));
            cx.run_until_parked();
            let state = observed.lock().expect("activity lock");
            if state.1.len() >= 2
                && state.0.status("pane-debounce-pty") == Some(AgentStatus::NeedsInput)
            {
                break;
            }
            drop(state);
            std::thread::sleep(Duration::from_millis(10));
        }
        let state = observed.lock().expect("activity lock");
        assert_eq!(
            state.1.len(),
            2,
            "the second real contradictory title arrived"
        );
        assert_eq!(
            state.0.status("pane-debounce-pty"),
            Some(AgentStatus::NeedsInput),
            "Layer B becomes eligible after 1.5 seconds"
        );
        drop(state);
        terminal.update(&mut cx.cx, |terminal, _| terminal.shutdown());
        cx.run_until_parked();
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn process_refresh_preserves_process_ownership_until_process_gone() {
        use std::os::unix::fs::symlink;
        use std::process::Command;

        let root =
            std::env::temp_dir().join(format!("sirio-pane-process-e2e-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("create process fixture");
        let agent = root.join("codex");
        symlink("/bin/sleep", &agent).expect("create matching comm alias");
        // The trailing `true` keeps the shell alive as the parent: `sh -c`
        // with a single command exec-replaces itself (bash and dash both do),
        // and the walk reads comm for descendants only, never for the shell.
        let mut child = Command::new("sh")
            .args(["-c", &format!("{} 2; true", agent.display())])
            .spawn()
            .expect("spawn shell with a real matching child");
        std::thread::sleep(Duration::from_millis(50));

        let mut activity = AgentActivityModel::new();
        let transition = activity
            .refresh_process_signal("pane-process-e2e", child.id())
            .expect("refresh process signal");
        assert_eq!(
            transition.map(|transition| transition.new),
            Some(AgentStatus::Running)
        );
        assert!(activity.is_process_owned("pane-process-e2e"));
        assert_eq!(
            apply_terminal_activity_event(
                &mut activity,
                "pane-process-e2e",
                &TerminalActivityEvent::OscTitle("zsh".to_string()),
                Instant::now(),
            ),
            None
        );
        assert_eq!(
            activity.status("pane-process-e2e"),
            Some(AgentStatus::Running),
            "an unrelated title cannot clear process-owned state"
        );

        let _ = child.kill();
        let _ = child.wait();
        activity
            .refresh_process_signal("pane-process-e2e", child.id())
            .expect("refresh after child exit");
        assert_eq!(activity.status("pane-process-e2e"), None);
        let _ = std::fs::remove_dir_all(root);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn apply_process_signal_matches_a_direct_refresh() {
        use std::os::unix::fs::symlink;
        use std::process::Command;

        let root = std::env::temp_dir().join(format!(
            "sirio-pane-process-apply-e2e-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("create process fixture");
        let agent = root.join("codex");
        symlink("/bin/sleep", &agent).expect("create matching comm alias");
        let mut child = Command::new("sh")
            .args(["-c", &format!("{} 2; true", agent.display())])
            .spawn()
            .expect("spawn shell with a real matching child");
        std::thread::sleep(Duration::from_millis(50));

        let shell_pid = child.id();
        let observation = std::thread::spawn(move || {
            let snapshot = sirio_activity::process::take_snapshot().unwrap();
            sirio_activity::process::inspect_foreground_agent_in(&snapshot, shell_pid)
        })
        .join()
        .expect("join process observation thread");
        let mut activity = AgentActivityModel::new();
        let transition = activity
            .apply_process_signal("pane-x", observation)
            .expect("apply process observation");
        assert_eq!(
            transition.map(|transition| transition.new),
            Some(AgentStatus::Running)
        );
        assert!(activity.is_process_owned("pane-x"));

        let _ = child.kill();
        let _ = child.wait();
        let shell_pid = child.id();
        let observation = std::thread::spawn(move || {
            let snapshot = sirio_activity::process::take_snapshot().unwrap();
            sirio_activity::process::inspect_foreground_agent_in(&snapshot, shell_pid)
        })
        .join()
        .expect("join process observation thread after exit");
        activity
            .apply_process_signal("pane-x", observation)
            .expect("apply process disappearance");
        assert!(!activity.is_process_owned("pane-x"));
        assert_eq!(activity.status("pane-x"), None);
        let _ = std::fs::remove_dir_all(root);
    }
}
