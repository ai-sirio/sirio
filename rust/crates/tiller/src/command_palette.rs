//! Shell-owned command palette catalog and filtering seam.
//!
//! Rendering and dispatch stay with `TillerWorkspace`; this module owns the
//! value-level command inventory so the overlay cannot grow a second command
//! model that drifts from the shell's typed routes.

use std::path::PathBuf;

use tiller_project::TabKind;
use tiller_ui::{sidebar::SidebarDisabledReason, tab_bar::NewTabAction};

use crate::{WindowCommand, WindowCommandDisabledReason, panes::SplitDirection};

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
    MoveTabToOtherPane,
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
    pub has_other_pane: bool,
    pub sidebar_target: Option<SidebarPaletteTarget>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PaletteDisabledReason {
    Window(WindowCommandDisabledReason),
    Sidebar(SidebarDisabledReason),
    NoActiveTab,
    NoRetainedChat,
    NoOtherPane,
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
            Self::NoActiveTab => "No active tab",
            Self::NoRetainedChat => "No retained chat sessions",
            Self::NoOtherPane => "No other pane is available",
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
            Some("Ctrl+T"),
            context,
        ),
        window_entry(
            WindowCommand::OpenFile,
            "Open File",
            Some("Ctrl+O"),
            context,
        ),
        window_entry(
            WindowCommand::SaveFile,
            "Save File",
            Some("Ctrl+S"),
            context,
        ),
        window_entry(
            WindowCommand::ToggleSidebar,
            "Toggle Sidebar",
            Some("Ctrl+Shift+S"),
            context,
        ),
        window_entry(
            WindowCommand::ToggleRightPanel,
            "Toggle Right Panel",
            Some("Ctrl+Shift+I"),
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
            Some("Ctrl+Shift+O"),
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
            Some("Ctrl+Shift+L"),
            context,
        ),
        window_entry(
            WindowCommand::FocusAddressBar,
            "Focus Address Bar",
            Some("Ctrl+L"),
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
        // F-TAB-12: "Move Tab to This Pane" used to sit here, gated on
        // `has_other_pane`. It is deleted for the same reason the tab context
        // menu's copy was, and the gate is why it was worth catching: the gate
        // asks whether *another pane exists*, but the command moves
        // `move_selected_tab(MoveTarget::CurrentPane)`, and from the palette
        // `tab_menu_tab` is always `None`, so the tab it moves is the active
        // one -- already in the active group by definition. The entry was
        // therefore *enabled* and inert whenever a second pane existed, which
        // is worse than the context menu's copy: that one at least drew
        // visibly disabled.
        //
        // Note this is not the feature Swift has. `SplitContentMenu.swift:286`
        // offers a "Move Existing Tab" submenu listing individual tabs *by
        // name* under "This Pane" / "Other Panes", so you choose which tab to
        // bring into the pane you right-clicked. Collapsing that per-tab
        // picker into one command that moves the active tab is what made it
        // degenerate. Re-porting it means the submenu, not re-adding this.
        //
        // How much of that submenu already exists, checked rather than guessed
        // (2026-08-20), because "unported" would overstate the work:
        //   - `TabMachinery::move_tab` SHIPS -- the action that moves a tab
        //     between groups is real code in the binary.
        //   - `MoveCandidates` (tab_machinery.rs:32) and `move_candidates()`
        //     (:235) are each marked `#[cfg(test)]`, so the eligibility model
        //     is written and pinned by a test, and then deliberately compiled
        //     out. Its two empty states, `NoOtherTab` and `NoEligibleTab`, are
        //     exactly Swift's two texts ("No other tabs in this pane." / "No
        //     other panes in this layout.").
        //   - Nothing renders a submenu. That is the whole of what is absent.
        // So this is machinery that exists and that the app cannot call --
        // the same shape as F-CORE-FILE-01's first failed fix, except here
        // `#[cfg(test)]` makes it unreachable by construction rather than by
        // oversight. Whether to finish it is a scope decision for the user;
        // do not quietly un-gate it as a side effect of other work.
        if context.has_other_pane {
            PaletteEntry::enabled(
                PaletteCommand::Tab(TabCommand::MoveTabToOtherPane),
                "Move Tab to Other Pane",
                None,
            )
        } else {
            PaletteEntry::disabled(
                PaletteCommand::Tab(TabCommand::MoveTabToOtherPane),
                "Move Tab to Other Pane",
                None,
                PaletteDisabledReason::NoOtherPane,
            )
        },
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
        new_tab_entry(NewTabAction::ClaudeCode, "Claude Code"),
        new_tab_entry(NewTabAction::Codex, "Codex"),
        new_tab_entry(NewTabAction::OpenCode, "OpenCode"),
        new_tab_entry(NewTabAction::Pi, "Pi"),
        new_tab_entry(NewTabAction::OhMyPi, "Oh-My-Pi"),
        new_tab_entry(NewTabAction::SplitClaudeCode, "Split Claude Code"),
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
        (NewTabAction::ClaudeCode, "Claude Code Here"),
        (NewTabAction::Codex, "Codex Here"),
        (NewTabAction::OpenCode, "OpenCode Here"),
        (NewTabAction::Pi, "Pi Here"),
        (NewTabAction::OhMyPi, "Oh-My-Pi Here"),
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
pub(crate) fn filter_entries<'a>(entries: &'a [PaletteEntry], query: &str) -> Vec<PaletteEntry> {
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
    use std::path::PathBuf;
    use tiller_project::TabKind;

    fn context() -> PaletteContext {
        PaletteContext {
            active_tab_kind: Some(TabKind::Terminal),
            has_retained_chat: true,
            has_other_pane: true,
            sidebar_target: Some(SidebarPaletteTarget {
                project_id: "tiller".into(),
                project_path: PathBuf::from("/tmp/tiller"),
                project_is_git: true,
                worktree_path: PathBuf::from("/tmp/tiller"),
                worktree_is_primary: false,
            }),
        }
    }

    #[test]
    fn catalog_contains_window_tab_sidebar_and_agent_commands() {
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
        assert!(commands.iter().any(|entry| entry.label == "Codex"));
        assert!(commands.iter().any(|entry| entry.label == "Oh-My-Pi"));
    }

    /// F-TAB-12: the palette must not offer "Move Tab to This Pane". It is
    /// pinned here rather than left to the absence of code because the entry
    /// was **enabled** whenever a second pane existed, and did nothing — an
    /// inert enabled command is harder to notice than a disabled one, and the
    /// context menu's identical copy had already been deleted once.
    ///
    /// The gate is asserted alongside the label deliberately: `has_other_pane`
    /// is true in `context()`, which is exactly the state that used to enable
    /// it. A test built on the false branch would pass on an accidental
    /// re-addition.
    #[test]
    fn the_palette_does_not_offer_move_tab_to_this_pane() {
        let context = context();
        assert!(
            context.has_other_pane,
            "the fixture must have a second pane, or this asserts nothing — \
             that is the state the deleted entry was enabled in"
        );

        let commands = entries(&context);
        assert!(
            !commands
                .iter()
                .any(|entry| entry.label == "Move Tab to This Pane"),
            "the entry moved the active tab into the group it is already in"
        );

        // The neighbours it sat between must survive, so this test fails on a
        // re-addition rather than on the whole tab family going missing.
        assert!(
            commands
                .iter()
                .any(|entry| entry.label == "Move Tab to Other Pane"
                    && entry.is_enabled()),
            "the genuinely meaningful move command is still offered"
        );
        assert!(
            commands.iter().any(|entry| entry.label == "Move Tab Earlier"),
            "tab reordering is still offered"
        );
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
}
