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
}

impl PaletteDisabledReason {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Window(WindowCommandDisabledReason::NoActiveFile) => "No active file",
            Self::Window(WindowCommandDisabledReason::NoActiveBrowser) => "No active browser",
            Self::Sidebar(SidebarDisabledReason::AlreadyGitProject) => "Git is already initialized",
            Self::Sidebar(SidebarDisabledReason::PrimaryWorktree) => {
                "The primary worktree cannot be removed"
            }
            Self::Sidebar(SidebarDisabledReason::ResolvingUpstream) => "Checking the remote…",
            Self::Sidebar(SidebarDisabledReason::NoUpstreamBranch) => "No remote branch to delete",
            Self::NoActiveTab => "No active tab",
            Self::NoRetainedChat => "No retained chat sessions",
            Self::NoSelectedProject => "No selected project",
            Self::NoSelectedWorktree => "No selected worktree",
            Self::AlreadyPrimary => "Already the primary worktree",
            Self::NotPrimary => "Worktree is not primary",
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
        // #319: "Move Tab to Other Pane" used to sit here, gated on
        // `has_other_pane`; F-TAB-12 had already deleted its "Move Tab to This
        // Pane" sibling. The center split ends the family rather than fixing
        // it: a tab's half is derived from its `TabKind`, so the only way to
        // move a tab across the divider is to change what the tab is. There is
        // no destination left to name.
        //
        // Swift's richer gesture (`SplitContentMenu.swift:286`, a "Move
        // Existing Tab" submenu listing tabs by name under "This Pane" /
        // "Other Panes") is not unported work waiting to be finished — it is
        // work the two fixed roles make meaningless. Do not re-add it.
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
    fn catalog_contains_window_tab_sidebar_and_chat_commands() {
        let commands = entries(&context());
        assert!(commands.iter().any(|entry| {
            entry.label == "Toggle Sidebar"
                && entry.command == PaletteCommand::Window(WindowCommand::ToggleSidebar)
        }));
        assert!(commands.iter().any(|entry| entry.label == "Close Tab"));
        assert!(
            commands
                .iter()
                .any(|entry| entry.label == "Project Settings")
        );
        assert!(commands.iter().any(|entry| entry.label == "New Chat"));
    }

    /// #319: the palette must not offer to move a tab between the two center
    /// panes, under either of the two labels the command has worn. It is
    /// pinned here rather than left to the absence of code because both
    /// spellings were deleted for *different* reasons — "This Pane" was inert
    /// and enabled (F-TAB-12), "Other Pane" was real and worked — and only the
    /// second removal is load-bearing for the center split. A re-addition
    /// would compile and behave, so nothing but a test catches it.
    #[test]
    fn the_palette_does_not_offer_to_move_a_tab_between_panes() {
        let commands = entries(&context());

        for label in ["Move Tab to This Pane", "Move Tab to Other Pane"] {
            assert!(
                !commands.iter().any(|entry| entry.label == label),
                "{label} names a gesture the center split does not have: a \
                 tab's half is derived from its kind"
            );
        }

        // The neighbours it sat between must survive, so this test fails on a
        // re-addition rather than on the whole tab family going missing.
        assert!(
            commands
                .iter()
                .any(|entry| entry.label == "Move Tab Earlier"),
            "tab reordering within a pane is still offered"
        );
        assert!(
            commands.iter().any(|entry| entry.label == "Move Tab Later"),
            "tab reordering within a pane is still offered"
        );
    }

    /// The palette mirrors the "+" menu: agents are reached through New Chat,
    /// never by opening a terminal that runs one agent's CLI. Eleven labels
    /// are pinned because the family was spelled twice -- six global entries
    /// and five "Here" variants targeting the selected worktree -- and a
    /// partial re-addition (one spelling, not the other) is exactly the kind
    /// of drift a label-level assertion catches and the type system does not.
    #[test]
    fn catalog_offers_no_agent_terminal_entries() {
        let commands = entries(&context());

        for label in [
            "Claude Code",
            "Codex",
            "OpenCode",
            "Pi",
            "Oh-My-Pi",
            "Split Claude Code",
            "Claude Code Here",
            "Codex Here",
            "OpenCode Here",
            "Pi Here",
            "Oh-My-Pi Here",
        ] {
            assert!(
                !commands.iter().any(|entry| entry.label == label),
                "{label} names an agent-specialized terminal the palette no \
                 longer offers"
            );
        }

        // The generic commands they sat among must survive, so this test
        // fails on a re-addition rather than on the whole family going
        // missing.
        for label in [
            "New Terminal",
            "New Terminal Here",
            "New Chat",
            "New Chat Here",
        ] {
            assert!(
                commands.iter().any(|entry| entry.label == label),
                "{label} is still offered"
            );
        }
    }

    #[test]
    fn catalog_contains_an_enabled_new_browser_command() {
        let browser = entries(&context())
            .into_iter()
            .find(|entry| entry.command == PaletteCommand::NewTab(NewTabAction::NewBrowser))
            .expect("New Browser command");
        assert!(browser.is_enabled());
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

    /// F-WIN-06: New Browser Tab is always available (it creates a tab, the
    /// same as New Terminal Tab), while Focus Address Bar needs an active
    /// browser tab to focus into -- the same shape SaveFile already has for
    /// "needs an active editor".
    #[test]
    fn new_browser_tab_is_always_enabled_and_focus_address_bar_needs_a_browser_tab() {
        let new_browser = entries(&context())
            .into_iter()
            .find(|entry| entry.command == PaletteCommand::Window(WindowCommand::NewBrowser))
            .expect("New Browser Tab is always listed");
        assert!(new_browser.is_enabled());
        assert_eq!(new_browser.shortcut, Some("Ctrl+Shift+L"));

        let mut no_browser = context();
        no_browser.active_tab_kind = Some(TabKind::Terminal);
        let disabled = entries(&no_browser)
            .into_iter()
            .find(|entry| entry.command == PaletteCommand::Window(WindowCommand::FocusAddressBar))
            .expect("Focus Address Bar is always listed");
        assert_eq!(
            disabled.disabled_reason,
            Some(PaletteDisabledReason::Window(
                WindowCommandDisabledReason::NoActiveBrowser
            ))
        );

        let mut with_browser = context();
        with_browser.active_tab_kind = Some(TabKind::Browser);
        let enabled = entries(&with_browser)
            .into_iter()
            .find(|entry| entry.command == PaletteCommand::Window(WindowCommand::FocusAddressBar))
            .expect("Focus Address Bar is always listed");
        assert!(enabled.is_enabled());
        assert_eq!(enabled.shortcut, Some("Ctrl+L"));
    }

    /// F-WIN-06: the '+' menu's mouse-driven "New Browser" row
    /// (`NewTabAction::NewBrowser`) and the keyboard-bindable "New Browser
    /// Tab" window command must coexist without colliding on a label --
    /// the same two-routes shape "New Terminal"/"New Terminal Tab" already
    /// has.
    #[test]
    fn new_browser_tab_and_the_plus_menus_new_browser_do_not_collide() {
        let commands = entries(&context());
        assert!(commands.iter().any(|entry| entry.label == "New Browser Tab"
            && entry.command == PaletteCommand::Window(WindowCommand::NewBrowser)));
        assert!(commands.iter().any(|entry| entry.label == "New Browser"
            && entry.command == PaletteCommand::NewTab(NewTabAction::NewBrowser)));
    }

    /// #374: the three retargeted Windows chords must surface in the
    /// palette rows themselves, or the documented shortcut and the working
    /// one disagree again.
    #[test]
    fn layout_toggle_rows_show_the_platform_chords() {
        let commands = entries(&context());
        let shortcut = |command| {
            commands
                .iter()
                .find(|entry| entry.command == PaletteCommand::Window(command))
                .expect("window command is always listed")
                .shortcut
        };
        if cfg!(target_os = "windows") {
            assert_eq!(shortcut(WindowCommand::ToggleSidebar), Some("Ctrl+Shift+D"));
            assert_eq!(
                shortcut(WindowCommand::ToggleRightPanel),
                Some("Ctrl+Shift+R")
            );
            assert_eq!(
                shortcut(WindowCommand::RestoreLaunchSnapshot),
                Some("Ctrl+Shift+H")
            );
        } else {
            assert_eq!(shortcut(WindowCommand::ToggleSidebar), Some("Ctrl+Shift+S"));
            assert_eq!(
                shortcut(WindowCommand::ToggleRightPanel),
                Some("Ctrl+Shift+I")
            );
            assert_eq!(
                shortcut(WindowCommand::RestoreLaunchSnapshot),
                Some("Ctrl+Shift+O")
            );
        }
    }
}
