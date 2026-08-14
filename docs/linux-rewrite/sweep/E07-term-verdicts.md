# Critic verdicts — E07-term (F-TERM)

Adjudicated by a critic that neither drove nor built this slice. Driver return:
`docs/linux-rewrite/sweep/E07-term-evidence.md`, captures under
`reference/linux-progress/drive-E07-term/`. HEAD under test: `4073297`.

## F-TERM-04 (ledger line 322) — verdict: `half-proven` (unchanged)

Driver performed a source-only check, no new drive, no new capture. Verified independently:
`open_context_menu` is wired only to `.on_mouse_down(MouseButton::Right, ...)` at
`rust/crates/tiller_terminal/src/lib.rs:1446` and `:1504`; grepped for a keymap/keybinding
alternative (`ShowContextMenu`, `ContextMenu`, any `Menu`/`Shift+F10`-style action) and found
none — no keyboard path exists. `Scripts/wayland-drive.sh`'s action vocabulary (`ctl`, `click`,
`move`, `type`, `key`, `shot`) confirmed to have no right-click primitive (only left-click).
Also checked `ControlAction`'s full variant list in `rust/crates/tiller/src/main.rs` for a
socket bypass (as F-TERM-08 found for close) — no `Copy`/`Paste`/clipboard-related variant
exists. So the row is genuinely unreachable from this lane, not merely under-tried. This
confirms, but does not extend, the existing half-proven state (live menu with Copy/Paste
already observed in `p17-rclick-term.png`; invoking the items and reading the clipboard back is
still the owed half). Evidence does not discriminate — verdict carries forward unchanged.

## F-TERM-06 (ledger line 324) — verdict: `half-proven` (unchanged)

Same source-only check as F-TERM-04, same confirmed blocker (`open_context_menu` bound only to
`MouseButton::Right`, no keyboard alternative, no socket bypass for `CopyPaneId`/
`CopyTerminalId`). No new drive, no new capture, evidence does not discriminate. Verdict
carries forward unchanged; owed half is still clicking each item and reading the clipboard
string back.

## F-TERM-08 (ledger line 326) — verdict: `half-proven` (unchanged, evidence strengthened)

New, discriminating, live evidence. Verified against source and captures:

- `rust/crates/tiller/src/main.rs:2450` — control socket's `ClosePane` handler calls
  `workspace.close_focused_pane(None, cx)`.
- `close_focused_pane` (`:5138`) calls `close_terminal_at(tab_id, focused_pane, ...)`.
- The right-click "Close Terminal…" item's delegation path
  (`TerminalContextAction::CloseTerminal` → `TerminalContextCommand::Close`, `:2207`,
  `:2820`) calls the *same* `workspace.close_terminal_at(tab_id, pane_id, None, cx)`.
  Confirmed identical function, not a parallel path.
- `close_terminal_at` (`:5149`) still guards `tab.panes.leaf_ids().len() <= 1` — confirms the
  pre-existing single-pane no-op defect is untouched by this drive.
- On the multi-pane branch it takes, it removes the pane and calls
  `terminal.input([3, 4])` (Ctrl-C then Ctrl-D) into the closed pane's PTY, and the terminal's
  `Drop` impl (`lib.rs:1104`) calls `shutdown()` — the same SIGTERM/SIGKILL-fallback
  process-group teardown already proven for Quit/external-kill, confirmed by existing named
  tests `shutdown_sends_the_pty_child_a_termination_request` /
  `shutdown_terminates_the_entire_pty_process_group`.

Captures checked and match the driver's narrative: `f-term-08-sleep-running.png` shows a
2-pane Terminal tab, right pane running `sleep 999` (visible in title bar and body);
`f-term-08-after-close.png` shows the same tab collapsed back to a single bash pane after
`pane.close` was sent over the socket. This corroborates the claimed close (screenshots alone
don't prove the OS-level process death — that rests on the driver's `ps --forest`
transcript, which is asserted prose, not a saved log file — but it lines up exactly with what
the source says the code does, so it is credible, not merely assumed).

**What this adds**: the previously-unknown question of whether Route B's kill logic works at
all once its guard doesn't block it is now answered — it does, on a multi-pane tab, via the
identical function the real menu item would call. **What is still owed**: (1) the single-pane
no-op — reproduced by a prior pass, not retested here — stands as a real, standing defect on
the common case (right-click "Close Terminal…" on a lone pane does nothing, orphaning
nothing only because it never reaches the kill code, not because it's safe); (2) the literal
right-click gesture (open menu, click "Close Terminal…") has still never been driven end to
end on any lane available to this critic or the driver. Verdict stays `half-proven`; the
"half" now owed is narrower (gesture only) but a genuine defect (guard no-op) remains
unresolved within that same half-proven umbrella.

## F-TERM-UI-01 (ledger line 535) — verdict: `half-proven` (unchanged)

Same structural blocker as F-TERM-04/06 (`open_context_menu` right-click-only, no keyboard
path, no socket bypass for menu rendering itself). Driver correctly notes that `pane.close`
reaching `close_terminal_at` (see F-TERM-08) proves one item's *delegation target*, not the
menu's rendering or its delegation from a real right-click event, which is this row's actual
clause (10 items, opened by a real right-click). No new drive, no new capture, evidence does
not discriminate. Verdict carries forward unchanged.

## Notes for the orchestrator (not row findings)

- **Process hygiene risk, not a row defect**: the driver reports running
  `pkill -f "target/debug/tiller"` after finishing its own row work, which is not
  instance-scoped and likely killed sibling agents' in-flight `tiller` processes on this
  shared host. This happened after the driver's own row evidence was captured, so it should
  not have corrupted the F-TERM rows above, but any other slice's drive that was mid-flight at
  that moment should be treated as a candidate false "app died" reading rather than a real
  product defect, and re-driven if its evidence looks like an unexplained crash.
- No disagreement with the driver's own self-assessed claims (`could-not-reach` /
  `partially-exercised`) — independently re-derived from source and confirmed accurate on all
  four rows, including confirming there is genuinely no socket-level bypass for the three
  still-blocked rows (checked the full `ControlAction` enum, not just taken on faith).
