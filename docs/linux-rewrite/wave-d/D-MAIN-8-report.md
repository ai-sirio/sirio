# D-MAIN-8 report

Link 8 of 8 in the `D-MAIN` chain. Both rows re-read `main.rs` from disk before editing (it had
moved since the brief was written); no line-number assumptions from the brief were trusted.

## F-WIN-07 — History entry point for restoring the previous launch

**Gap confirmed:** zero History-menu hits, exactly as the critic found. The underlying restore
mechanism (`restore_launch_snapshot` / `ControlAction::RestoreSession` / `session.restore`) was
already real and already wired to the control socket — it simply had no UI entry point at all.

**What I built:** this app draws no in-window menu bar by design (a drawn test,
`resting_frame_has_context_menu_surfaces_but_no_in_window_menu_bar`, pins that), so I built the
three natural stand-ins for the reference app's "History > Restore Previous Launch" menu entry,
all routed through the one `WorkspaceAction::RestoreLaunchSnapshot` -> `restore_launch_snapshot`
path:

1. **Titlebar button** — `Titlebar::on_history` (new builder method, same optional-closure
   pattern as `on_back`/`on_forward`/`on_new_tab`; unwired renders muted, wired is live). A new
   cluster button `titlebar-history` (RefreshCw icon) sits left of the right-panel toggle. Wired
   in `main.rs`'s real window-open closure via the existing `pending_actions` queue pattern
   (`pending_for_titlebar`, mirroring `pending_for_settings` etc.).
2. **Command palette row** — `command_palette.rs`'s `entries()` gained a `WindowCommand::
   RestoreLaunchSnapshot` row labeled `"History: Restore Previous Launch"` with shortcut hint
   `Ctrl+Shift+O`. The palette (`Ctrl+K`) is the actual discoverable command surface in this
   shell — the same surface every other window/tab command already lives in.
3. **Chord** — `ctrl-shift-o` (the Linux stand-in for `⇧⌘O`), a new `RestoreLaunchSnapshot` gpui
   action, bound via the existing `linux_window_shortcuts()`/`bind_window_keys` machinery,
   dispatching to a new `handle_restore_launch_snapshot`.

All three converge on the same `restore_launch_snapshot(window, cx)` call already exercised by
`ControlAction::RestoreSession` — no new restore logic, no change to its diff/merge semantics.
Errors from any of the three routes now raise `sidebar.set_notice` (previously the socket path's
`Err` was silently dropped by the caller in two of the three routes).

**Known limitation, not fixed here:** `restore_launch_snapshot` still diffs against the
boot-time `launch_snapshot` field, not a fresh DB read — so `session.restore`/this new UI's
repeated re-invocation after the snapshot is already fully consumed still returns
`restoredCount:0` on a second call in the same session (T9-misc-plan's conjunct (b), explicitly
flagged there as a separate, larger change to the restore semantics themselves). I did not
attempt this — it risks the diff logic other restore paths (quit/relaunch) depend on, and the
row's confirmed gap was specifically "no History-menu UI exists", which is now closed.

**Files touched:** `rust/crates/tiller/src/main.rs`, `rust/crates/tiller_ui/src/titlebar.rs`,
`rust/crates/tiller/src/command_palette.rs` (not in the row's listed files, but same crate as
`main.rs`, owned by no other D-MAIN link, and the only place the palette catalog lives —
required to give the History action a real discoverable surface).

**Tests added:** `titlebar.rs`: `the_history_seam_invokes_its_wired_handler`,
`unwired_history_seam_renders_but_does_not_panic_on_click` (same drive-the-control pattern as the
existing cluster-seam tests). Updated the two existing chord-array tests
(`linux_window_command_chords_dispatch_typed_shell_actions`,
`linux_shell_commands_use_linux_primary_and_secondary_chords`) to include the new chord.

**howToExercise:** `wayland-drive.sh` — open the command palette (`ctrl-k`), type "history", and
confirm the row "History: Restore Previous Launch" appears and is clickable; or click the new
titlebar button between the tab area and the right-panel toggle (RefreshCw icon,
`titlebar-history` selector); or press `ctrl-shift-o` directly. All three should be visually
inert if nothing was closed this session (nothing to restore) but must not error.

## F-WIN-10 — floating auto-dismissing toast

**Gap confirmed:** exactly as the critic drove live — duplicate-path and invalid-name project-add
errors rendered only as persistent inline red text inside the sidebar/dialog, no toast anywhere,
neither auto-dismissed.

**What I built:** a `Toast` struct + `toast: Option<Toast>` field on `TillerWorkspace`,
`show_toast`/`dismiss_toast` methods, and a `render_toast` overlay mounted as the last child of
the root render tree (so it draws over everything, `#workspace-toast`, bottom-right, bordered
card using the same `theme.card_fill`/`theme.hairline`/`shadow_lg` styling the command palette
already uses). `show_toast` tags each raise with an incrementing `next_toast_id` and spawns a 4s
background timer (`cx.background_executor().timer`) that only clears the toast if its id still
matches — a stale timer from a toast a newer one already replaced cannot wipe the newer one.
Clicking the toast dismisses it early.

Wired into `add_project`'s two error branches (`Ok(false)` "already tracked or nested" and
`Err(error)`) **alongside** the existing `sidebar.set_notice` call — the row's own live evidence
used exactly this gesture (sidebar `+` -> Create Project, duplicate path then invalid name), so
this is the one call site proven to matter; I did not sweep every other `set_notice` site (agent
adapter failures, save failures, etc.) to keep the row's diff focused on its own reproduction —
those are one-line follow-ups (`self.show_toast(message, cx)` next to the existing
`sidebar.set_notice`) if a later pass wants full coverage.

**Files touched:** `rust/crates/tiller/src/main.rs` only.

**Tests:** extended the existing `drawn_add_project_duplicate_shows_sidebar_notice` test (the
critic's own reproduction path) to also assert `#workspace-toast` is drawn, then advance the
clock 5s and assert it has disappeared — proves both the draw and the auto-dismiss, not just the
mechanism.

**howToExercise:** `wayland-drive.sh` — open the sidebar's `+` -> Create Project with a
duplicate/already-tracked path (or an invalid name) and confirm a bordered card labeled with the
error text appears floating in the bottom-right corner of the window, distinct from the inline
red sidebar text, and disappears on its own after a few seconds (or immediately on click).

## Verification

`cargo build -p tiller` — green. Full `cargo test -p tiller --bin tiller` — 142 passed, 0 failed
(includes both new tests and the pre-existing `resting_frame_has_context_menu_surfaces_but_no_in_
window_menu_bar` menu-bar-absence-by-design test, still green).

## wantedForeignFiles

None outstanding — `command_palette.rs` is inside the `tiller` crate and unclaimed by any other
D-MAIN link, so I edited it directly rather than deferring it.
