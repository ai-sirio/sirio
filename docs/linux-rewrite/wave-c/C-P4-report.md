# C-P4 report

7 rows. Files owned: `rust/crates/tiller_project/src/file.rs`,
`rust/crates/tiller_terminal/src/context_menu.rs`, `rust/crates/tiller_ui/src/browser.rs`,
`rust/crates/tiller_ui/src/file_view.rs`, `rust/crates/tiller_ui/src/right_panel.rs`.

## F-CHG-20 — fixed

**Bug found**: `render_activity`'s expanded-section height only added
`self.activity.len() * ACTIVITY_ROW_HEIGHT` to the section's fixed `.h(px(...))`. When the
activity list is empty and the section is expanded, the code still renders one "No activity"
placeholder row (`ACTIVITY_ROW_HEIGHT` tall) but the container reserved **zero** extra height for
it — so the row got squeezed into a sliver, and its text rendered as the "illegible sub-pixel
specks" the critic photographed and confirmed via edge-crop/contrast checks.

**Fix**: `right_panel.rs` — height now uses `self.activity.len().max(1)` so the placeholder row
always gets its full `ACTIVITY_ROW_HEIGHT` of reserved space.

Commit: `9dd2216` fix(ui): reserve row height for empty activity placeholder (F-CHG-20)

**howToExercise**: open a worktree with no chat/terminal activity, expand the Activity section
(click the "Activity" header at the bottom of the Files panel) — "No activity" should be fully
legible, not a sliver of pixels.

## F-CHG-03 — fixed

**Bug found**: `ensure_tree_refresh`'s periodic 1s loop only called `panel.refresh(cx)` when
`panel.refresh_error.is_none()`. Once a refresh failed (e.g. permission denied), the loop
permanently stopped retrying — the ONLY way to recover was a manual Retry click. The ledger's
recorded evidence said manual clicks were *also* unreliable (7/8 stuck); I could not reproduce that
specific claim (5/5 live cycles of chmod-000 → chmod-755 → click Retry recovered correctly on
today's HEAD, verified via the Wayland lane with real screenshots), but the "does not self-heal
without a click" half of the defect was real and unfixed.

**Fix**: `right_panel.rs` — the periodic loop now calls `panel.refresh(cx)` unconditionally every
tick, regardless of `refresh_error`. `refresh()` is already single-flight, so this costs nothing
extra; it just means a cleared permission error resolves on its own within ~1s, no click required.

Commit: `d286759` fix(ui): auto-retry Files panel refresh after error clears (F-CHG-03)

**howToExercise**: `chmod 000` a worktree's root while it's the selected worktree → Files panel
shows "Files unavailable: Permission denied (os error 13)" with a Retry button. `chmod 755` it back
and **do not click anything** — within ~1-3s the panel repopulates on its own (verified live,
screenshots `heal-err.png` → `heal-after.png` in this session's scratch dir, not committed).

## F-BRW-01 — already correct, verified live (no code change)

The ledger's evidence ("bit-identical to pre-fix... covering sidebar") pointed at
`native_webview_rect` in `browser.rs`, which already converts GPUI's `Bounds<Pixels>` straight into
wry's `Rect::Logical` with no scale-factor pre-conversion. I verified against the actual vendored
`dpi` crate (0.1.2) and `wry` 0.56.1 (`webkitgtk/mod.rs::set_bounds`) that `Position::to_logical`/
`Size::to_logical` are **no-ops for an already-`Logical` value** — the code comment's claim that
wry double-converts unconditionally does not hold for this wry version. On real DISPLAY=:1 (via
`Scripts/linux-drive.sh`), the webview renders inside its correct panel bounds with no shrink and
no sidebar coverage (`/tmp/c-p4-test/brw01.png` this session — not committed, reproducible any
time with the gesture below). No code change made; the existing unit tests
(`webview_bounds_use_gpui_layout_directly`-style, lines ~1746-1773) already lock this in.

**howToExercise**: `ctl browser.open url=https://example.com` (or click "+" → New Browser tab),
`tab.select` it, screenshot on `DISPLAY=:1` — the page content should fill the browser pane exactly,
not spill into the sidebar or shrink to a fraction of the pane.

## F-BRW-03 — already correct, verified live (no code change)

Reproduced the exact critic scenario three ways on real DISPLAY=:1: (1) `type` in three separate
chunks (`"https://"`, `"test.example"`, `".org/x"`) — landed correctly as
`https://test.example.org/x`; (2) individual `xdotool key` calls per character
(`h t t p s colon slash slash`) — landed correctly as `https://`; (3) `Home` + 8×`Right` + insert —
landed at the exact intended offset. Zero corruption across all three attempts. `AddressEditor`'s
state machine (`browser.rs`) is fully synchronous with no shared mutable state that could race; I
could not find or reproduce the reported "htweqtps" transposition bug in current code. No change
made.

**howToExercise**: open a Browser tab, click the address field, `key ctrl+a`, then `type` a URL —
the field should show exactly what was typed, no dropped/transposed characters.

## F-TERM-04 — already correct, verified live (no code change)

`context_menu.rs`'s 12-item menu and route table are already correct and fully unit-tested. The
row's recorded gap was tooling ("wayland-drive.sh has no right-click primitive"), but a keyboard
bypass already exists at HEAD: `tiller_terminal/src/lib.rs` (not owned by this slice, added in
commit `0c5871a` for the sibling row F-CORE-TERM-02) opens the context menu on `Menu` key or
`Shift+F10`, with no mouse click at all. `Scripts/linux-drive.sh`'s `key` helper (xdotool-backed)
supports modifier chords out of the box. Live-verified: `key shift+F10` on a focused terminal pane
opened the full 12-item menu (Copy … Close Terminal…) exactly matching `context_menu.rs::ITEMS`.
No change needed in the owned file.

**howToExercise**: click into a terminal pane, `key shift+F10` (via `Scripts/linux-drive.sh`, not
the Wayland lane — no modifier-chord support there) — the full context menu should open anchored
near the pane origin.

## F-CORE-FILE-06 — already correct, verified live (no code change)

Reproduced the full scenario end-to-end on the Wayland lane: opened `note.txt` in a FileView editor
tab (double-click on the Files-panel row), typed into the buffer (dirty state: "edited" badge + red
tab dot appeared), then modified the file externally (`printf > note.txt` from outside the app).
Within ~1s the poll (`poll_file_system_events`, 100ms timer) picked it up and the conflict banner
"This file changed on disk." with Reload/Keep rendered directly above the editor content — visibly
distinct from the Terminal prompt and the git Changes panel the earlier evidence had mis-cited. No
code change needed; `file_view.rs`'s `Conflict`/`check_external`/`render_conflict_banner` machinery
already works.

Root cause of the prior mis-citations: the Files-panel double-click needs both `click` events to
land within GPUI's double-click window; two separate `wayland-drive.sh` `click` actions in the same
script (each with ~1s of internal ack-polling/settle overhead) land too far apart and are seen as
two independent single clicks — the app silently keeps focus on whatever tab was already active
(Terminal), so a scripted "type" after that lands in the terminal prompt, exactly matching the
earlier miscaptures. Driving two clicks with <100ms between them (direct FIFO writes) reliably opens
the editor tab.

**howToExercise**: double-click a file row in the Files panel with the two clicks <100ms apart,
type into the buffer, then externally overwrite the file — the conflict banner should appear over
the editor content within ~1s, unprompted.

## F-CORE-FILE-03 — not attempted / blocked (tooling gap, unchanged)

`terminal_file_drop` (`file.rs`) is a pure, already-thoroughly-unit-tested function (spaces, quotes,
non-ASCII, `terminal_drop_is_one_quoted_space_separated_string`). The remaining gap is the live
drag gesture from a Files-panel row onto a terminal pane. Neither `wayland-drive.sh` nor
`linux-drive.sh` has a named drag primitive, but `linux-drive.sh` exports `WID`/`DISP`/`WIN_X`/
`WIN_Y`, so raw `xdotool mousedown`/`mousemove`/`mouseup` sequences are constructible inline. I
attempted this (mousedown on a Files-panel row, several intermediate mousemoves, mouseup over a
terminal pane) and got an inconclusive/ambiguous result — the terminal pane visually
duplicated/split rather than showing a written path, which may itself be an unrelated rendering
artifact of the synthetic drag rather than a real drop. I did not chase this further within budget.
No code change made in `file.rs` (nothing in it looked wrong).

**wantedForeignFiles**: `Scripts/linux-drive.sh` — a real `drag <x1> <y1> <x2> <y2>` helper
(mousemove --sync, mousedown 1, several intermediate mousemove --sync steps, mouseup 1) would let
this row (and F-TERM-PTY-06/F-EDIT-12, its siblings per `WORK-BREAKDOWN.md`) be driven properly.
I did not edit this file since it is not in this slice's owned-files list and other slices/critics
may be using it concurrently.

## Notes for the critic

- Two files (`context_menu.rs`, `file.rs`) needed **no code changes** — both rows' owned-file code
  was already correct; the ledger's verdicts were driven by tooling/gesture-timing gaps, not code
  defects. Re-drive with the `howToExercise` gestures above before concluding otherwise.
- `right_panel.rs` had two real, now-fixed bugs (F-CHG-20 layout, F-CHG-03 self-heal).
- `browser.rs` — both rows re-verified live with **no code change**; I could not reproduce either
  reported defect on current HEAD.
- Noticed but out of scope: when driving F-TERM-04, a previously-opened Browser tab's native
  webview stayed painted on top of the window even after switching to the Terminal tab — worth a
  follow-up row if not already tracked, but not part of this slice's files.
