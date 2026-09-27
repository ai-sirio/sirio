//! Shell-owned command palette catalog and filtering seam.
//!
//! Rendering and dispatch stay with `SirioWorkspace`; this module owns the
//! value-level command inventory so the overlay cannot grow a second command
//! model that drifts from the shell's typed routes.

use std::path::PathBuf;

use sirio_project::TabKind;
use sirio_ui::{sidebar::SidebarDisabledReason, tab_bar::NewTabAction};

use crate::{
    WindowCommand, WindowCommandDisabledReason, panes::SplitDirection, window_shortcut_hint,
};

/// A command that the shell can route through an existing typed action or
/// typed UI event. The palette stores this value; it never stores a closure
/// that could become a second implementation of the transition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PaletteCommand {
    Window(WindowCommand),
    Tab(TabCommand),
    NewTab(NewTabAction),
    Sidebar(SidebarPaletteAction),
    GoToSymbol,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TabCommand {
    FocusPane(SplitDirection, bool),
    SplitPane(SplitDirection),
    ClosePane,
    CycleTab(bool),
    JumpToTab(usize),
    OpenAllTabs,
    OpenTabMenu,
    CloseTab,
    CloseOtherTabs,
    CloseTabsToRight,
    MoveTabEarlier,
    MoveTabLater,
    ResumeChat,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SidebarPaletteAction {
    ProjectSettings,
    InitializeGit,
    RevealInFileManager,
    RemoveProject,
    SetPrimary,
    UnsetPrimary,
    NewTab(NewTabAction),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SidebarPaletteTarget {
    pub project_id: String,
    pub project_path: PathBuf,
    pub project_is_git: bool,
    pub worktree_path: PathBuf,
    pub worktree_is_primary: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct PaletteContext {
    pub active_tab_kind: Option<TabKind>,
    pub has_retained_chat: bool,
    pub sidebar_target: Option<SidebarPaletteTarget>,
    /// Whether the active tab is a file whose server offers document
    /// symbols. Answered by the workspace, which owns the supervisor.
    pub symbols_available: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PaletteDisabledReason {
    Window(WindowCommandDisabledReason),
    Sidebar(SidebarDisabledReason),
    NoActiveTab,
    NoRetainedChat,
    NoSelectedProject,
    NoSelectedWorktree,
    AlreadyPrimary,
    NotPrimary,
    NoSymbolProvider,
}

impl PaletteDisabledReason {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Window(WindowCommandDisabledReason::NoActiveFile) => "No active file",
            Self::Window(WindowCommandDisabledReason::NoActiveBrowser) => "No active browser",
            Self::Window(WindowCommandDisabledReason::NoMovableTab) => {
                "Only terminals and chats move between panes"
            }
            Self::Sidebar(SidebarDisabledReason::AlreadyGitProject) => "Git is already initialized",
            Self::Sidebar(SidebarDisabledReason::ResolvingUpstream) => "Checking the remote…",
            Self::Sidebar(SidebarDisabledReason::NoUpstreamBranch) => "No remote branch to delete",
            Self::NoActiveTab => "No active tab",
            Self::NoRetainedChat => "No retained chat sessions",
            Self::NoSelectedProject => "No selected project",
            Self::NoSelectedWorktree => "No selected worktree",
            Self::AlreadyPrimary => "Already the primary worktree",
            Self::NotPrimary => "Worktree is not primary",
            Self::NoSymbolProvider => "No language server for this file",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PaletteEntry {
    pub command: PaletteCommand,
    pub label: &'static str,
    pub shortcut: Option<&'static str>,
    pub disabled_reason: Option<PaletteDisabledReason>,
}

impl PaletteEntry {
    fn enabled(
        command: PaletteCommand,
        label: &'static str,
        shortcut: Option<&'static str>,
    ) -> Self {
        Self {
            command,
            label,
            shortcut,
            disabled_reason: None,
        }
    }

    fn disabled(
        command: PaletteCommand,
        label: &'static str,
        shortcut: Option<&'static str>,
        reason: PaletteDisabledReason,
    ) -> Self {
        Self {
            command,
            label,
            shortcut,
            disabled_reason: Some(reason),
        }
    }

    pub(crate) fn is_enabled(self) -> bool {
        self.disabled_reason.is_none()
    }
}

pub(crate) const EMPTY_RESULT_LABEL: &str = "No commands match your search";

fn window_entry(
    command: WindowCommand,
    label: &'static str,
    shortcut: Option<&'static str>,
    context: &PaletteContext,
) -> PaletteEntry {
    match crate::window_command_availability(command, context.active_tab_kind) {
        crate::WindowCommandAvailability::Enabled => {
            PaletteEntry::enabled(PaletteCommand::Window(command), label, shortcut)
        }
        crate::WindowCommandAvailability::Disabled(reason) => PaletteEntry::disabled(
            PaletteCommand::Window(command),
            label,
            shortcut,
            PaletteDisabledReason::Window(reason),
        ),
    }
}

fn sidebar_target_reason(context: &PaletteContext, project: bool) -> Option<PaletteDisabledReason> {
    match (project, context.sidebar_target.is_some()) {
        (true, false) => Some(PaletteDisabledReason::NoSelectedProject),
        (false, false) => Some(PaletteDisabledReason::NoSelectedWorktree),
        _ => None,
    }
}

fn sidebar_entry(
    action: SidebarPaletteAction,
    label: &'static str,
    context: &PaletteContext,
    reason: Option<PaletteDisabledReason>,
) -> PaletteEntry {
    let reason = reason.or_else(|| {
        let project = matches!(
            action,
            SidebarPaletteAction::ProjectSettings
                | SidebarPaletteAction::InitializeGit
                | SidebarPaletteAction::RevealInFileManager
                | SidebarPaletteAction::RemoveProject
        );
        sidebar_target_reason(context, project)
    });
    match reason {
        Some(reason) => {
            PaletteEntry::disabled(PaletteCommand::Sidebar(action), label, None, reason)
        }
        None => PaletteEntry::enabled(PaletteCommand::Sidebar(action), label, None),
    }
}

fn new_tab_entry(action: NewTabAction, label: &'static str) -> PaletteEntry {
    PaletteEntry::enabled(PaletteCommand::NewTab(action), label, None)
}

/// The complete command catalog for the current shell. Filtering is applied
/// after this eligibility pass so unavailable operations remain discoverable.
pub(crate) fn entries(context: &PaletteContext) -> Vec<PaletteEntry> {
    let mut entries = vec![
        window_entry(
            WindowCommand::NewTerminalTab,
            "New Terminal Tab",
            Some(window_shortcut_hint(WindowCommand::NewTerminalTab)),
            context,
        ),
        window_entry(
            WindowCommand::OpenFile,
            "Open File",
            Some(window_shortcut_hint(WindowCommand::OpenFile)),
            context,
        ),
        window_entry(
            WindowCommand::SaveFile,
            "Save File",
            Some(window_shortcut_hint(WindowCommand::SaveFile)),
            context,
        ),
        window_entry(
            WindowCommand::ToggleSidebar,
            "Toggle Sidebar",
            Some(window_shortcut_hint(WindowCommand::ToggleSidebar)),
            context,
        ),
        window_entry(
            WindowCommand::ToggleRightPanel,
            "Toggle Right Panel",
            Some(window_shortcut_hint(WindowCommand::ToggleRightPanel)),
            context,
        ),
        // The one way back to a Secondary pane hidden by its own `×`, short
        // of the chord: named after the tab menu's "Move to Right Pane".
        window_entry(
            WindowCommand::ToggleSecondaryPane,
            "Toggle Right Pane",
            Some(window_shortcut_hint(WindowCommand::ToggleSecondaryPane)),
            context,
        ),
        // F-WIN-07: this app draws no in-window menu bar by design, so the
        // command palette is the "History" surface -- the Linux stand-in
        // for the reference app's History > Restore Previous Launch menu
        // entry, routed through the same `WindowCommand` typed dispatch as
        // every other palette row above.
        window_entry(
            WindowCommand::RestoreLaunchSnapshot,
            "History: Restore Previous Launch",
            Some(window_shortcut_hint(WindowCommand::RestoreLaunchSnapshot)),
            context,
        ),
        // F-WIN-06: distinct label from the '+' menu's own "New Browser"
        // row (`NewTabAction::NewBrowser`, listed separately below) --
        // same two-routes-one-destination shape "New Terminal Tab" already
        // has alongside "New Terminal", so a keyboard-bindable window
        // command and a mouse-driven menu item can list side by side
        // without reading as a duplicate.
        window_entry(
            WindowCommand::NewBrowser,
            "New Browser Tab",
            Some(window_shortcut_hint(WindowCommand::NewBrowser)),
            context,
        ),
        window_entry(
            WindowCommand::FocusAddressBar,
            "Focus Address Bar",
            Some(window_shortcut_hint(WindowCommand::FocusAddressBar)),
            context,
        ),
        window_entry(
            WindowCommand::MoveTabToOtherPane,
            "Move Tab to Other Pane",
            Some(window_shortcut_hint(WindowCommand::MoveTabToOtherPane)),
            context,
        ),
        if context.symbols_available {
            PaletteEntry::enabled(PaletteCommand::GoToSymbol, "Go to Symbol in File", None)
        } else {
            PaletteEntry::disabled(
                PaletteCommand::GoToSymbol,
                "Go to Symbol in File",
                None,
                PaletteDisabledReason::NoSymbolProvider,
            )
        },
        PaletteEntry::enabled(
            PaletteCommand::Tab(TabCommand::FocusPane(SplitDirection::Horizontal, false)),
            "Focus Pane Left",
            Some("Ctrl+Alt+Left"),
        ),
        PaletteEntry::enabled(
            PaletteCommand::Tab(TabCommand::FocusPane(SplitDirection::Horizontal, true)),
            "Focus Pane Right",
            Some("Ctrl+Alt+Right"),
        ),
        PaletteEntry::enabled(
            PaletteCommand::Tab(TabCommand::FocusPane(SplitDirection::Vertical, false)),
            "Focus Pane Above",
            Some("Ctrl+Alt+Up"),
        ),
        PaletteEntry::enabled(
            PaletteCommand::Tab(TabCommand::FocusPane(SplitDirection::Vertical, true)),
            "Focus Pane Below",
            Some("Ctrl+Alt+Down"),
        ),
        PaletteEntry::enabled(
            PaletteCommand::Tab(TabCommand::SplitPane(SplitDirection::Horizontal)),
            "Split Pane Right",
            Some("Ctrl+Alt+Shift+Right"),
        ),
        PaletteEntry::enabled(
            PaletteCommand::Tab(TabCommand::SplitPane(SplitDirection::Vertical)),
            "Split Pane Down",
            Some("Ctrl+Alt+Shift+Down"),
        ),
        PaletteEntry::enabled(
            PaletteCommand::Tab(TabCommand::ClosePane),
            "Close Pane",
            Some("Ctrl+Alt+W"),
        ),
        PaletteEntry::enabled(
            PaletteCommand::Tab(TabCommand::CycleTab(true)),
            "Next Tab",
            Some("Ctrl+Tab"),
        ),
        PaletteEntry::enabled(
            PaletteCommand::Tab(TabCommand::CycleTab(false)),
            "Previous Tab",
            Some("Ctrl+Shift+Tab"),
        ),
    ];

    const JUMP_LABELS: [&str; 9] = [
        "Jump to Tab 1",
        "Jump to Tab 2",
        "Jump to Tab 3",
        "Jump to Tab 4",
        "Jump to Tab 5",
        "Jump to Tab 6",
        "Jump to Tab 7",
        "Jump to Tab 8",
        "Jump to Tab 9",
    ];
    for position in 1..=9 {
        entries.push(PaletteEntry::enabled(
            PaletteCommand::Tab(TabCommand::JumpToTab(position)),
            JUMP_LABELS[position - 1],
            Some(match position {
                1 => "Ctrl+1",
                2 => "Ctrl+2",
                3 => "Ctrl+3",
                4 => "Ctrl+4",
                5 => "Ctrl+5",
                6 => "Ctrl+6",
                7 => "Ctrl+7",
                8 => "Ctrl+8",
                _ => "Ctrl+9",
            }),
        ));
    }

    entries.extend([
        PaletteEntry::enabled(
            PaletteCommand::Tab(TabCommand::OpenAllTabs),
            "Open All Tabs",
            None,
        ),
        PaletteEntry::enabled(
            PaletteCommand::Tab(TabCommand::OpenTabMenu),
            "Open Tab Menu",
            None,
        ),
        if context.active_tab_kind.is_some() {
            PaletteEntry::enabled(
                PaletteCommand::Tab(TabCommand::CloseTab),
                "Close Tab",
                Some("Ctrl+W"),
            )
        } else {
            PaletteEntry::disabled(
                PaletteCommand::Tab(TabCommand::CloseTab),
                "Close Tab",
                Some("Ctrl+W"),
                PaletteDisabledReason::NoActiveTab,
            )
        },
        PaletteEntry::enabled(
            PaletteCommand::Tab(TabCommand::CloseOtherTabs),
            "Close Other Tabs",
            None,
        ),
        PaletteEntry::enabled(
            PaletteCommand::Tab(TabCommand::CloseTabsToRight),
            "Close Tabs to the Right",
            None,
        ),
        PaletteEntry::enabled(
            PaletteCommand::Tab(TabCommand::MoveTabEarlier),
            "Move Tab Earlier",
            None,
        ),
        PaletteEntry::enabled(
            PaletteCommand::Tab(TabCommand::MoveTabLater),
            "Move Tab Later",
            None,
        ),
        // #319: the old "Move Tab to Other Pane" tab-family row was removed
        // when a tab's half was derived from its kind; F-TAB-12 had also
        // removed the inert "Move Tab to This Pane" sibling. Terminals and
        // chats now store their half on the tab. The active-tab move lives
        // above as a `WindowCommand`; don't add a second row to this family.
        //
        // Swift's richer gesture (`SplitContentMenu.swift:286`, a "Move
        // Existing Tab" submenu listing tabs by name under "This Pane" /
        // "Other Panes") is still not a one-to-one match: this command moves
        // only the focused terminal/chat to its other half.
        if context.has_retained_chat {
            PaletteEntry::enabled(
                PaletteCommand::Tab(TabCommand::ResumeChat),
                "Resume Chat",
                None,
            )
        } else {
            PaletteEntry::disabled(
                PaletteCommand::Tab(TabCommand::ResumeChat),
                "Resume Chat",
                None,
                PaletteDisabledReason::NoRetainedChat,
            )
        },
        new_tab_entry(NewTabAction::NewTerminal, "New Terminal"),
        new_tab_entry(NewTabAction::NewChanges, "Changes"),
        new_tab_entry(NewTabAction::NewChat, "New Chat"),
        new_tab_entry(NewTabAction::NewBrowser, "New Browser"),
        sidebar_entry(
            SidebarPaletteAction::ProjectSettings,
            "Project Settings",
            context,
            None,
        ),
        sidebar_entry(
            SidebarPaletteAction::InitializeGit,
            "Initialize Git",
            context,
            context.sidebar_target.as_ref().and_then(|target| {
                target
                    .project_is_git
                    .then_some(PaletteDisabledReason::Sidebar(
                        SidebarDisabledReason::AlreadyGitProject,
                    ))
            }),
        ),
        sidebar_entry(
            SidebarPaletteAction::RevealInFileManager,
            "Reveal in File Manager",
            context,
            None,
        ),
        sidebar_entry(
            SidebarPaletteAction::RemoveProject,
            "Remove Project",
            context,
            None,
        ),
    ]);

    if let Some(target) = &context.sidebar_target {
        entries.push(sidebar_entry(
            SidebarPaletteAction::SetPrimary,
            "Set Primary Worktree",
            context,
            target
                .worktree_is_primary
                .then_some(PaletteDisabledReason::AlreadyPrimary),
        ));
        entries.push(sidebar_entry(
            SidebarPaletteAction::UnsetPrimary,
            "Unset Primary Worktree",
            context,
            (!target.worktree_is_primary).then_some(PaletteDisabledReason::NotPrimary),
        ));
    } else {
        entries.push(sidebar_entry(
            SidebarPaletteAction::SetPrimary,
            "Set Primary Worktree",
            context,
            Some(PaletteDisabledReason::NoSelectedWorktree),
        ));
        entries.push(sidebar_entry(
            SidebarPaletteAction::UnsetPrimary,
            "Unset Primary Worktree",
            context,
            Some(PaletteDisabledReason::NoSelectedWorktree),
        ));
    }

    for (action, label) in [
        (NewTabAction::NewTerminal, "New Terminal Here"),
        (NewTabAction::NewChat, "New Chat Here"),
    ] {
        entries.push(sidebar_entry(
            SidebarPaletteAction::NewTab(action),
            label,
            context,
            sidebar_target_reason(context, false),
        ));
    }

    entries
}

/// Minimum filtering promised by D2: a case-insensitive substring over the
/// visible label, with no fuzzy ranking that could make a command disappear.
pub(crate) fn filter_entries(entries: &[PaletteEntry], query: &str) -> Vec<PaletteEntry> {
    let query = query.trim().to_ascii_lowercase();
    entries
        .iter()
        .copied()
        .filter(|entry| query.is_empty() || entry.label.to_ascii_lowercase().contains(&query))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::super::{WindowCommand, WindowCommandDisabledReason};
    use super::*;
    use sirio_project::TabKind;
    use std::path::PathBuf;

    fn context() -> PaletteContext {
        PaletteContext {
            active_tab_kind: Some(TabKind::Terminal),
            has_retained_chat: true,
            symbols_available: false,
            sidebar_target: Some(SidebarPaletteTarget {
                project_id: "sirio".into(),
                project_path: PathBuf::from("/tmp/sirio"),
                project_is_git: true,
                worktree_path: PathBuf::from("/tmp/sirio"),
                worktree_is_primary: false,
            }),
        }
    }

    #[test]
    fn filter_is_case_insensitive_substring_and_reports_empty_state() {
        let commands = entries(&context());
        let filtered = filter_entries(&commands, "side");
        assert_eq!(
            filtered.iter().map(|entry| entry.label).collect::<Vec<_>>(),
            ["Toggle Sidebar",]
        );
        assert!(filter_entries(&commands, "no command has this text").is_empty());
        assert_eq!(EMPTY_RESULT_LABEL, "No commands match your search");
    }

    #[test]
    fn disabled_rows_keep_the_existing_typed_reason() {
        let mut unavailable = context();
        unavailable.active_tab_kind = None;
        let save = entries(&unavailable)
            .into_iter()
            .find(|entry| entry.command == PaletteCommand::Window(WindowCommand::SaveFile))
            .expect("Save File is always listed");
        assert_eq!(
            save.disabled_reason,
            Some(PaletteDisabledReason::Window(
                WindowCommandDisabledReason::NoActiveFile
            ))
        );
    }

    /// A chord is not discoverable on its own. Ctrl+Shift+B was bound but
    /// never listed, so a Secondary pane hidden by the strip's `×` had no
    /// visible way back: the `×` and the launcher's Hide Pane go away with
    /// the pane they hide.
    #[test]
    fn every_bound_window_command_is_listed_with_its_chord() {
        let commands = entries(&context());
        for (command, _) in crate::linux_window_shortcuts() {
            let row = commands
                .iter()
                .find(|entry| entry.command == PaletteCommand::Window(command))
                .unwrap_or_else(|| panic!("{command:?} is bound but not in the palette"));
            assert_eq!(row.shortcut, Some(window_shortcut_hint(command)));
        }
    }

    #[test]
    fn go_to_symbol_explains_itself_when_no_server_offers_symbols() {
        // Present with a reason, never absent and never enabled-and-inert:
        // the same contract the file context menu follows.
        let mut context = context();
        context.symbols_available = false;
        let entry = entries(&context)
            .into_iter()
            .find(|entry| entry.command == PaletteCommand::GoToSymbol)
            .expect("the entry is listed even when it cannot run");
        assert_eq!(entry.label, "Go to Symbol in File");
        assert_eq!(
            entry.disabled_reason,
            Some(PaletteDisabledReason::NoSymbolProvider)
        );
        assert_eq!(
            PaletteDisabledReason::NoSymbolProvider.label(),
            "No language server for this file"
        );
    }
}
