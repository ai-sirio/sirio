# P43 tab command-layer contract

This is the shell contract for the existing typed actions in
`rust/crates/tiller/src/panes.rs`. The tab UI must invoke these actions; it
must not duplicate tab mutation logic or maintain a second selection model.
For actions that originate from a row, the shell records that row's stable id
in its command context before dispatching the unit GPUI action.

## Actions

| Affordance | GPUI action | Selector / requirement |
| --- | --- | --- |
| All Tabs overflow button | `OpenAllTabs` | `tab-overflow-button`; only draw when the current group’s tab widths plus the overflow control exceed the strip width |
| Tab context / Tab menu | `OpenTabMenu` | `workspace-tab-menu-<tab-id>`; menu is scoped to the clicked tab |
| Close | `CloseTab` | `workspace-tab-close-<tab-id>`; the shell performs the dirty prompt before removal |
| Close Others | `CloseOtherTabs` | `tab-command-close-others`; keep the selected tab and remove every sibling |
| Close Tabs to the Right | `CloseTabsToRight` | `tab-command-close-right`; keep the selected tab and every tab before it |
| Move Earlier / Later | `MoveTabEarlier` / `MoveTabLater` | disable at the corresponding edge and explain “already the first/last tab” |
| Move to This Pane | `MoveTabToCurrentPane` | disable with “no other tab is available” or “no eligible tab is available” when the candidate list is empty |
| Move to Other Pane | `MoveTabToOtherPane` | list pane groups in layout order; explain “no other pane is available” when none exists |
| Resume Chat | `ResumeChat` | `tab-command-resume-chat`; list retained sessions, or show “No retained chat sessions” |

## All Tabs menu

The menu lists every tab in the active pane group, in strip order. Each row
uses `workspace-overflow-tab-<tab-id>`, selects that tab on click, and renders
the active tab marker `✓`. The marker is state, not a decoration copied from
the clicked row.

## Move menu

The Move Existing Tab menu is a two-level command surface. The first level is
`This Pane` and `Other Panes`. `This Pane` lists tabs from other groups that
are eligible for the current group; `Other Panes` lists destination groups in
layout order and then eligible tabs from the current group. An empty branch
must contain a visible disabled explanatory row, not a blank submenu:

- no tab outside the target group: `No other tab is available`;
- tabs exist but all fail the eligibility predicate: `No eligible tab is available`;
- no destination group: `No other pane is available`.

The click callback supplies the tab id and destination group to the shell
action. The shell applies one `TabMachinery` transition, then redraws every
group from that state.

## Dirty close

Close, Close Others, Close Tabs to the Right, and the `ctrl-w` command all use
the same shell close door. A clean tab closes immediately. A terminal with a
live process or a dirty document opens a warning prompt with `Close` and
`Cancel`; Cancel leaves the tab and its entity untouched.

## Resume Chat

Closing a chat retains its title and plain transcript in the shell’s retained
session list. The Resume Chat submenu renders one row per retained session;
selecting a row dispatches `ResumeChat`, creates a fresh live ACP surface, and
restores the retained transcript before the surface is shown.

## P46 window and sidebar command layer

The Linux shell has no macOS application menu bar. The capability contract is
therefore keyboard-first: Linux primary/secondary chords use `Ctrl` while the
title strip supplies compact visibility controls. The title strip emits typed
`TitlebarEvent` values; it does not own workspace visibility state.

| Inventory entry | Transition | Linux affordance | Eligibility / evidence selector |
| --- | --- | --- | --- |
| F-WIN-02 New Tab | `NewTerminalTab` → `NewTabAction::NewTerminal` | `Ctrl+T` and the existing tab-bar New Tab surface | `linux_shell_commands_use_linux_primary_and_secondary_chords` |
| F-WIN-03 Open / Save | `OpenFile`, `SaveFile` | `Ctrl+O`, `Ctrl+S`; file picker and live `FileView::save` | Save is typed `Disabled(NoActiveFile)` without an editor; `save_command_is_disabled_without_an_active_file_and_explains_why` |
| F-WIN-04 Sidebar | `TitlebarEvent::ToggleSidebar` | title-strip `titlebar-sidebar` control and `Ctrl+Shift+S` | shell conditionally mounts the sidebar; `titlebar_controls_emit_shell_visibility_events` |
| F-WIN-05 Right panel | `TitlebarEvent::ToggleRightPanel` | title-strip `titlebar-right-panel` control and `Ctrl+Shift+I` | shell conditionally mounts the right panel; `titlebar_controls_emit_shell_visibility_events` |
| F-SID-07 Project Settings | `SidebarEvent::OpenProjectSettings` | project gear `project-settings-<row-id>` or context item `sidebar-context-item-project-settings` | project target must resolve to a catalog row; the sheet is drawn as `project-settings-sheet` |
| F-SID-08 Initialize Git | `SidebarContextAction::InitializeGit` → `git init` | project context item `sidebar-context-item-initialize-git` | enabled only for non-Git projects; Git projects retain a disabled item with typed `AlreadyGitProject` / “Git is already initialized” |
| F-SID-09 Reveal | `SidebarContextAction::RevealInFileManager` → `xdg-open` | project context item `sidebar-context-item-reveal-in-file-manager` | project path must exist in the target; launch errors are surfaced in `sidebar-notice` |
| F-SID-12 Primary | `SidebarContextAction::SetPrimary` / `UnsetPrimary` | worktree context item `sidebar-context-item-set-primary` or `sidebar-context-item-unset-primary` | target carries typed `is_primary`; one catalog worktree is marked primary and the row draws `sidebar-primary-pill-*` |
| F-SID-14 New surfaces | `SidebarContextAction::NewTab(NewTabAction)` | worktree context items `new-terminal`, `claude-code`, `codex`, `opencode`, `pi`, `oh-my-pi`, `new-chat` | target must be a worktree; click emits the typed action and the shell selects that worktree before opening it |

P48 wires the four window commands in `tiller/src/main.rs` through one Linux
keyboard-first binding table: `Ctrl+T` dispatches `NewTerminalTab`, `Ctrl+O`
dispatches the file picker, `Ctrl+S` dispatches `SaveFile`, and the two
`Ctrl+Shift` chords dispatch the typed visibility actions. The shell's
`WindowCommandAvailability::Disabled(NoActiveFile)` state keeps Save inert
until an editor tab is active. The command-dispatch fixture
`linux_window_command_chords_dispatch_typed_shell_actions` sends all five
chords through GPUI's focused key path; `titlebar_controls_emit_shell_visibility_events`
proves the two compact Linux title-strip controls emit the same typed shell
events. This is the Linux equivalent of the macOS application-menu capability:
the operations remain identical while the platform-specific chrome is
keyboard-first rather than a transliterated macOS menu bar.

Sidebar context items are generated by `Sidebar::context_menu_items`, so the
drawn menu and the action contract share one eligibility table. The visual
proof `right_click_context_menu_dispatches_a_typed_worktree_action` performs a
real right-button press/release, waits with `run_until_parked()`, clicks the
drawn New Terminal item, and asserts the typed target/action event. This is the
menu contract handed to pi rather than a second ad-hoc channel.
