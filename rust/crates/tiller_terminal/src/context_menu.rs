//! The terminal context-menu contract.
//!
//! The terminal owns actions that only need its emulator or the platform
//! clipboard. Pane-management actions are represented as typed events so the
//! workspace can apply them to the pane that was actually right-clicked.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TerminalContextAction {
    Copy,
    Paste,
    CopyContext,
    SetTitle,
    CopyPaneId,
    CopyTerminalId,
    SplitLeft,
    SplitRight,
    SplitAbove,
    SplitDown,
    ClearTerminal,
    CloseTerminal,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TerminalContextRoute {
    Terminal,
    App,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TerminalContextItem {
    pub label: &'static str,
    pub action: TerminalContextAction,
    pub route: TerminalContextRoute,
    /// `Some(reason)` when this item cannot currently be actuated. The
    /// render site must show the reason (not just grey the row out) and
    /// must not attach a click handler for it — see F-TAB-11.
    pub disabled_reason: Option<String>,
}

const ITEMS: [TerminalContextItem; 12] = [
    TerminalContextItem {
        label: "Copy",
        action: TerminalContextAction::Copy,
        route: TerminalContextRoute::Terminal,
        disabled_reason: None,
    },
    TerminalContextItem {
        label: "Paste",
        action: TerminalContextAction::Paste,
        route: TerminalContextRoute::Terminal,
        disabled_reason: None,
    },
    TerminalContextItem {
        label: "Copy Context",
        action: TerminalContextAction::CopyContext,
        route: TerminalContextRoute::Terminal,
        disabled_reason: None,
    },
    TerminalContextItem {
        label: "Set Title",
        action: TerminalContextAction::SetTitle,
        route: TerminalContextRoute::App,
        disabled_reason: None,
    },
    TerminalContextItem {
        label: "Copy Pane ID",
        action: TerminalContextAction::CopyPaneId,
        route: TerminalContextRoute::Terminal,
        disabled_reason: None,
    },
    TerminalContextItem {
        label: "Copy Terminal ID",
        action: TerminalContextAction::CopyTerminalId,
        route: TerminalContextRoute::Terminal,
        disabled_reason: None,
    },
    TerminalContextItem {
        label: "Split Left",
        action: TerminalContextAction::SplitLeft,
        route: TerminalContextRoute::App,
        disabled_reason: None,
    },
    TerminalContextItem {
        label: "Split Right",
        action: TerminalContextAction::SplitRight,
        route: TerminalContextRoute::App,
        disabled_reason: None,
    },
    TerminalContextItem {
        label: "Split Above",
        action: TerminalContextAction::SplitAbove,
        route: TerminalContextRoute::App,
        disabled_reason: None,
    },
    TerminalContextItem {
        label: "Split Down",
        action: TerminalContextAction::SplitDown,
        route: TerminalContextRoute::App,
        disabled_reason: None,
    },
    TerminalContextItem {
        label: "Clear Terminal",
        action: TerminalContextAction::ClearTerminal,
        route: TerminalContextRoute::Terminal,
        disabled_reason: None,
    },
    TerminalContextItem {
        label: "Close Terminal…",
        action: TerminalContextAction::CloseTerminal,
        route: TerminalContextRoute::App,
        disabled_reason: None,
    },
];

pub fn items() -> &'static [TerminalContextItem] {
    &ITEMS
}

/// Mirrors the real, enforced split-pane minimum applied by `tiller`'s own
/// pane-tree layout (`MIN_SPLIT_PANE_SIZE` in `tiller/src/main.rs`, applied
/// via `.min_w()/.min_h()` on every split child). `tiller_terminal` cannot
/// import that constant directly -- `tiller` depends on `tiller_terminal`,
/// not the reverse -- so the value is intentionally duplicated here rather
/// than reused from `tiller::panes::split_disabled_reason`'s own, different,
/// never-wired 240x160 pair (F-TAB-11): that function's minimums were never
/// cross-checked against the layout that actually runs, so mirroring
/// `main.rs`'s real minimum describes the split that will actually happen.
const MIN_SPLIT_PANE_SIZE: f32 = 160.0;
/// Matches `main.rs`'s own `SPLIT_DIVIDER_SIZE`, reserved from the pane's
/// length before the remaining space is split in half.
const SPLIT_DIVIDER_SIZE: f32 = 6.0;

/// Returns the reason a directional split cannot currently be offered, given
/// this terminal pane's own live pixel size. `horizontal` selects Split
/// Left/Right (checks width); `false` selects Split Above/Down (checks
/// height).
fn split_disabled_reason(horizontal: bool, pane_width: f32, pane_height: f32) -> Option<String> {
    let length = if horizontal { pane_width } else { pane_height };
    let available = ((length - SPLIT_DIVIDER_SIZE).max(0.0) / 2.0).floor();
    (available < MIN_SPLIT_PANE_SIZE).then(|| {
        format!(
            "pane is too {} to split: {available:.0}pt available, {MIN_SPLIT_PANE_SIZE:.0}pt required",
            if horizontal { "narrow" } else { "short" }
        )
    })
}

/// The reason text for `panes::SplitDisabledReason::SoleTabInGroup`,
/// duplicated verbatim (see this module's own size-minimum duplication note
/// above for why `tiller_terminal` cannot import `tiller::panes` directly)
/// so both halves of the same disabled state read identically wherever they
/// surface.
const SOLE_TAB_IN_GROUP_REASON: &str = "cannot split the sole tab in its pane group";

/// Returns [`items`] with each Split action's `disabled_reason` populated
/// from this terminal pane's own current pixel size (F-TAB-11) and, when
/// `sole_tab_in_group` is set, from the pane's tab-group membership (the
/// other half of F-TAB-11 / `panes::SplitDisabledReason::SoleTabInGroup`):
/// a lone tab in a pane group cannot be split regardless of how much room
/// its pane has, so that reason takes priority over the size check for all
/// four split actions. All other items are always enabled.
pub fn items_with_split_availability(
    pane_width: f32,
    pane_height: f32,
    sole_tab_in_group: bool,
) -> Vec<TerminalContextItem> {
    let sole_tab_reason = sole_tab_in_group.then(|| SOLE_TAB_IN_GROUP_REASON.to_owned());
    let horizontal_reason = sole_tab_reason
        .clone()
        .or_else(|| split_disabled_reason(true, pane_width, pane_height));
    let vertical_reason =
        sole_tab_reason.or_else(|| split_disabled_reason(false, pane_width, pane_height));
    items()
        .iter()
        .cloned()
        .map(|mut item| {
            item.disabled_reason = match item.action {
                TerminalContextAction::SplitLeft | TerminalContextAction::SplitRight => {
                    horizontal_reason.clone()
                }
                TerminalContextAction::SplitAbove | TerminalContextAction::SplitDown => {
                    vertical_reason.clone()
                }
                _ => None,
            };
            item
        })
        .collect()
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TerminalIdentity {
    pane_id: String,
    terminal_id: String,
}

impl TerminalIdentity {
    pub fn new(pane_id: impl Into<String>, terminal_id: impl Into<String>) -> Self {
        Self {
            pane_id: pane_id.into(),
            terminal_id: terminal_id.into(),
        }
    }

    pub fn pane_id(&self) -> &str {
        &self.pane_id
    }

    pub fn terminal_id(&self) -> &str {
        &self.terminal_id
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TerminalContextEvent {
    pub target: TerminalIdentity,
    pub action: TerminalContextAction,
}

#[cfg(test)]
mod tests {
    use super::{
        TerminalContextAction, TerminalContextRoute, items, items_with_split_availability,
    };

    #[test]
    fn menu_contains_every_terminal_and_app_action_in_stable_order() {
        assert_eq!(items().len(), 12);
        assert_eq!(
            items().iter().map(|item| item.label).collect::<Vec<_>>(),
            vec![
                "Copy",
                "Paste",
                "Copy Context",
                "Set Title",
                "Copy Pane ID",
                "Copy Terminal ID",
                "Split Left",
                "Split Right",
                "Split Above",
                "Split Down",
                "Clear Terminal",
                "Close Terminal…",
            ]
        );
    }

    #[test]
    fn terminal_actions_are_local_and_pane_actions_are_delegated() {
        let local = [
            TerminalContextAction::Copy,
            TerminalContextAction::Paste,
            TerminalContextAction::CopyContext,
            TerminalContextAction::CopyPaneId,
            TerminalContextAction::CopyTerminalId,
            TerminalContextAction::ClearTerminal,
        ];
        for action in local {
            assert_eq!(
                items()
                    .iter()
                    .find(|item| item.action == action)
                    .unwrap()
                    .route,
                TerminalContextRoute::Terminal
            );
        }

        let delegated = [
            TerminalContextAction::SetTitle,
            TerminalContextAction::SplitLeft,
            TerminalContextAction::SplitRight,
            TerminalContextAction::SplitAbove,
            TerminalContextAction::SplitDown,
            TerminalContextAction::CloseTerminal,
        ];
        for action in delegated {
            assert_eq!(
                items()
                    .iter()
                    .find(|item| item.action == action)
                    .unwrap()
                    .route,
                TerminalContextRoute::App
            );
        }
    }

    /// F-TAB-11: a pane too narrow to produce two >=160pt children disables
    /// only the two horizontal split actions, with a reason naming the
    /// actual available/required pixel amounts -- not the whole menu, and
    /// not silently.
    #[test]
    fn a_too_narrow_pane_disables_only_the_horizontal_splits() {
        let disabled: Vec<_> = items_with_split_availability(200.0, 900.0, false)
            .into_iter()
            .filter(|item| item.disabled_reason.is_some())
            .collect();
        assert_eq!(
            disabled.iter().map(|item| item.action).collect::<Vec<_>>(),
            vec![
                TerminalContextAction::SplitLeft,
                TerminalContextAction::SplitRight
            ]
        );
        for item in &disabled {
            let reason = item.disabled_reason.as_deref().unwrap();
            assert!(reason.contains("narrow"), "reason was {reason:?}");
            assert!(reason.contains("97pt available"), "reason was {reason:?}");
            assert!(reason.contains("160pt required"), "reason was {reason:?}");
        }
    }

    /// The vertical counterpart: a pane too short disables only Split
    /// Above/Down, and the reason says "short", not "narrow".
    #[test]
    fn a_too_short_pane_disables_only_the_vertical_splits() {
        let disabled: Vec<_> = items_with_split_availability(900.0, 200.0, false)
            .into_iter()
            .filter(|item| item.disabled_reason.is_some())
            .collect();
        assert_eq!(
            disabled.iter().map(|item| item.action).collect::<Vec<_>>(),
            vec![
                TerminalContextAction::SplitAbove,
                TerminalContextAction::SplitDown
            ]
        );
        for item in &disabled {
            assert!(item.disabled_reason.as_deref().unwrap().contains("short"));
        }
    }

    /// A comfortably large pane disables nothing.
    #[test]
    fn a_large_pane_disables_no_split() {
        assert!(
            items_with_split_availability(1200.0, 900.0, false)
                .iter()
                .all(|item| item.disabled_reason.is_none())
        );
    }

    /// F-TAB-11's other half: a pane that is the sole tab in its group
    /// disables every split direction, with the sole-tab reason, even when
    /// the pane is comfortably large -- mirroring
    /// `panes::split_disabled_reason`'s own SoleTabInGroup priority over the
    /// size check.
    #[test]
    fn a_sole_tab_in_group_disables_every_split_regardless_of_size() {
        let items = items_with_split_availability(1200.0, 900.0, true);
        let split_actions = [
            TerminalContextAction::SplitLeft,
            TerminalContextAction::SplitRight,
            TerminalContextAction::SplitAbove,
            TerminalContextAction::SplitDown,
        ];
        for action in split_actions {
            let reason = items
                .iter()
                .find(|item| item.action == action)
                .unwrap()
                .disabled_reason
                .as_deref();
            assert_eq!(reason, Some("cannot split the sole tab in its pane group"));
        }
        for item in &items {
            if !split_actions.contains(&item.action) {
                assert!(item.disabled_reason.is_none());
            }
        }
    }
}
