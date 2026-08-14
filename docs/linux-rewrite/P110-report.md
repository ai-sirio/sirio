# P110 builder report — codex12

## F-SID-18 — selected worktree with no terminals

- Built a COSMIC-styled central empty state for a selected worktree whose tab
  group is empty: terminal glyph, **No Terminals**, explanatory copy, and a
  primary **New Terminal** action. The action creates a terminal tab through
  the existing shell-owned creation path.
- Drawn test: `drawn_selected_worktree_without_tabs_offers_a_new_terminal`
  asserts the empty state and clicks the real drawn action, then verifies that
  a terminal tab replaced it.
- Capture: `reference/linux-progress/p110-f-sid-18/02-no-terminals.png`, made
  with `Scripts/wayland-drive.sh` and `TILLER_WL_LABEL=codex12` after closing
  and reselecting the worktree. It visibly shows both required text and the
  New Terminal control.
- Not exercised in the Wayland lane: the physical click on New Terminal. That
  lane has no input devices; the action itself is covered by the drawn click
  test. Owed gesture: click **New Terminal** in the no-terminal empty state.

## F-TERM-11 — no selected worktree in the terminal surface

- Built a central no-worktree state that covers retained tabs/PTYs whenever
  the control selection is empty, preventing old terminal output from being
  presented as current.
- Drawn test: `drawn_deselected_worktree_replaces_terminals_with_an_empty_state`.
- Capture: `reference/linux-progress/p110-f-term-11/02-no-worktree.png`,
  driven by `workspace.close`; it visibly shows **No worktree selected** and
  its recovery explanation instead of a terminal.
- All VERIFY conjuncts are state/render checks; no pointer or keyboard
  conjunct remains owed.

## F-CHG-02 — no selected worktree in Files/Changes

- Fixed the stale-data correctness bug. Closing the selected workspace now
  clears the right panel's worktree binding and cached rows. `surface.changes.open`
  now returns `no current workspace` instead of constructing a Changes tab for
  the closed path.
- Drawn test: `drawn_closed_worktree_clears_the_right_panel_and_blocks_changes_open`.
- Capture: `reference/linux-progress/p110-f-chg-02/02-no-worktree-changes.png`.
  The drive log records `workspace.close` followed by the expected
  `surface.changes.open` error, while the pixels show the right-panel
  **No worktree selected** explanation and no stale tree.
- All VERIFY conjuncts are state/render checks; no pointer or keyboard
  conjunct remains owed.

## F-TAB-25 — attach to current terminal

- Built **Attach to Current Terminal** in the terminal tab context menu. It is
  enabled only for another terminal-bearing tab while a terminal is active;
  otherwise it is disabled with an explanation. The transition moves the
  existing terminal pane/PTY into a horizontal split beside the active
  terminal, preserving its pane identity and removing an emptied source tab
  without terminating that process.
- Drawn tests:
  `drawn_terminal_menu_attaches_an_eligible_terminal_to_the_current_tab` and
  `drawn_terminal_attach_command_is_disabled_for_the_current_terminal`.
- Capture: `reference/linux-progress/p110-f-tab-25/02-terminal-tabs.png`,
  produced through the required Wayland driver after selecting the worktree.
  It records the live app frame for this row; it cannot show the context menu
  because this lane has no input devices.
- Owed gesture: right-click an eligible source terminal tab, choose **Attach
  to Current Terminal**, then right-click the current terminal and confirm
  the action is disabled. The state transition and both enabled/disabled
  drawn surfaces are covered by the two UI tests above.

## Verification

- Fresh drawn-test runs passed for every P110 state, including both F-TAB-25
  enabled and disabled cases.
- `cargo check -p tiller` passed.
- `Scripts/ci.sh` was invoked but cannot begin in this Linux checkout because
  its first command requires the macOS-only `xcodegen` executable, which is
  not installed here. The command stopped with `xcodegen: command not found`.
