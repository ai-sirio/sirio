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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TerminalContextItem {
    pub label: &'static str,
    pub action: TerminalContextAction,
    pub route: TerminalContextRoute,
}

const ITEMS: [TerminalContextItem; 12] = [
    TerminalContextItem {
        label: "Copy",
        action: TerminalContextAction::Copy,
        route: TerminalContextRoute::Terminal,
    },
    TerminalContextItem {
        label: "Paste",
        action: TerminalContextAction::Paste,
        route: TerminalContextRoute::Terminal,
    },
    TerminalContextItem {
        label: "Copy Context",
        action: TerminalContextAction::CopyContext,
        route: TerminalContextRoute::Terminal,
    },
    TerminalContextItem {
        label: "Set Title",
        action: TerminalContextAction::SetTitle,
        route: TerminalContextRoute::App,
    },
    TerminalContextItem {
        label: "Copy Pane ID",
        action: TerminalContextAction::CopyPaneId,
        route: TerminalContextRoute::Terminal,
    },
    TerminalContextItem {
        label: "Copy Terminal ID",
        action: TerminalContextAction::CopyTerminalId,
        route: TerminalContextRoute::Terminal,
    },
    TerminalContextItem {
        label: "Split Left",
        action: TerminalContextAction::SplitLeft,
        route: TerminalContextRoute::App,
    },
    TerminalContextItem {
        label: "Split Right",
        action: TerminalContextAction::SplitRight,
        route: TerminalContextRoute::App,
    },
    TerminalContextItem {
        label: "Split Above",
        action: TerminalContextAction::SplitAbove,
        route: TerminalContextRoute::App,
    },
    TerminalContextItem {
        label: "Split Down",
        action: TerminalContextAction::SplitDown,
        route: TerminalContextRoute::App,
    },
    TerminalContextItem {
        label: "Clear Terminal",
        action: TerminalContextAction::ClearTerminal,
        route: TerminalContextRoute::Terminal,
    },
    TerminalContextItem {
        label: "Close Terminal…",
        action: TerminalContextAction::CloseTerminal,
        route: TerminalContextRoute::App,
    },
];

pub fn items() -> &'static [TerminalContextItem] {
    &ITEMS
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
    use super::{TerminalContextAction, TerminalContextRoute, items};

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
}
