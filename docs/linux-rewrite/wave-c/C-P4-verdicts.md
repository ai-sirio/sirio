# C-P4 critic verdicts

Critic pass, independent of the builder. Builder's claims (`C-P4-report.md`) were treated as
routes only, not conclusions. All evidence below is freshly driven this session — Wayland lane
(`Scripts/wayland-drive.sh`) for F-CHG-20/F-CHG-03, X11 lane (`Scripts/linux-drive.sh`,
DISPLAY=:1) for the rest, per WAYLAND-LANE.md's routing rule ("Route every F-BRW row to the X
lane"; F-TERM-04 needs a modifier chord the Wayland lane can't send).

Disposable fixture used throughout: a private git repo at
`/tmp/claude-1000/-home-enzopalmisano-Scrivania-Progetti-tiller/6c13680a-d3bb-4d5b-8c40-6c3bc45a5e73/scratchpad/cp4-proj`
(files `note.txt`, `README.md`), added as its own Tiller project so no test touched the real
`tiller-linux` checkout or another agent's state.

## F-CHG-20 — PASSED

Reached a genuine zero-tab worktree (not the app's default, which auto-opens Chat+Terminal) by
closing both default tabs one at a time (Terminal's close required confirming a "Close dirty
tab?" dialog since its PTY was live), landing on the "No Terminals" empty state. Clicked the
Activity header to expand it: the section shows "No activity" as clearly legible text, not the
sub-pixel specks the original evidence photographed. Instrument: Wayland lane, label `cp4e`,
frame `/tmp/cp4-shots/07-s6-final.png`. This is the exact route the builder gave (worktree with
no chat/terminal activity, expand Activity) and it now holds up live.

## F-CHG-03 — PASSED

`chmod 000` on the selected worktree's root while it was loaded and idle produced "Files
unavailable: Permission denied (os error 13)" with a Retry button (frame
`/tmp/cp4-shots/03-02-permission-denied.png`). `chmod 755` it back and then issued **zero
clicks** — only `sleep 3` — before the next capture: the panel had repopulated on its own with
the real listing (`note.txt`, `README.md`), unprompted (frame
`/tmp/cp4-shots/04-03-self-healed.png`). Instrument: Wayland lane, label `cp4f`, both steps in
one continuous app session (no restart between chmods, since the fix lives in that session's
background refresh loop). Discriminates cleanly: the error frame and the healed frame are visibly
different states, and the healing frame was captured with the Retry button never touched.

## F-BRW-01 — FAILED — defective (contradicts the builder's already-correct claim)

Pixel-scanned the rendered frame on DISPLAY=:1 for a Browser tab showing https://example.com,
both a pre-existing tab and a brand-new one opened this session via `browser.open` +
`tab.select`. In both cases the page's white content box spans x=331..1059 (729px), while a scan
of the pane's own chrome (background-color transitions at y=850, well below the content) shows
the pane's true bounds are x=386..1236 (850px wide). The content therefore starts 55px to the
*left* of the pane's own left edge (bleeding into the sidebar divider) and stops 177px short of
the pane's right edge, leaving a large unpainted dark gap before the Files panel — reproducing
the ledger's original defect ("~176px unpainted before Files panel") almost to the pixel, on a
tab created fresh this session, not a stale one. Frames:
`/tmp/cp4-x11/01-browser-click.png` (pre-existing tab), `/tmp/cp4-x11/02-fresh-browser.png`
(brand-new tab, surface:15). The builder's own report states this rendered "with no shrink and no
sidebar coverage" — that claim does not survive a pixel measurement of the frame it cites having
captured.

## F-BRW-03 — PASSED (already-correct, confirmed)

Clicked the address field, `key ctrl+a`, then `type`'d `https://claude.ai/test-cp4-marker` via
`Scripts/linux-drive.sh`. The field shows exactly that string, caret at the end, zero dropped or
transposed characters. Frame: `/tmp/cp4-x11/03-addr-typed.png`. No corruption reproduced across
this drive.

## F-TERM-04 — PASSED (already-correct, confirmed)

Clicked into a focused terminal pane on DISPLAY=:1 and sent `key shift+F10` via
`Scripts/linux-drive.sh` — no right-click. The full 12-item menu opened: Copy, Paste, Copy
Context, Set Title, Copy Pane ID, Copy Terminal ID, Split Left, Split Right, Split Above, Split
Down, Clear Terminal, Close Terminal… — matching `context_menu.rs::ITEMS` exactly. Frame:
`/tmp/cp4-x11/07-ctxmenu.png`.

Separately observed, not graded here: a Browser tab's native webview kept painting on top of the
entire window even after the tab (and the whole Browser surface) was closed, clearing only on a
full app relaunch — the same stacking bug the builder flagged as out-of-scope. It sits in
`browser.rs` teardown, not this row's files (`context_menu.rs`), so it does not change this
verdict; worth its own ledger row if one does not exist yet.

## F-CORE-FILE-06 — PASSED

Used `xdotool click --repeat 2 --delay 40 1` for a genuine <100ms double-click on a Files-panel
row (well under the builder's own diagnosis of what a slower scripted double-click misses). It
opened `note.txt` in an editor tab. Typed into the buffer (dirty "edited" badge + tab dot
appeared, frame `/tmp/cp4-x11/12-dirty.png`), then overwrote the file externally with
`printf > note.txt` from the host shell. Within 1.5s, with no further clicks, the conflict banner
"This file changed on disk." with Reload/Keep rendered directly above the editor content (frame
`/tmp/cp4-x11/13-conflict.png`) — clearly the FileView editor, not the Terminal or Changes panel.

## F-CORE-FILE-03 — FAILED — absent (upgraded from NOT EXERCISED; the tooling gap is now closed)

Two independent lines of evidence, both new this session:

1. **Code reading.** `RightPanel::render_file_row` (`rust/crates/tiller_ui/src/right_panel.rs` —
   owned by this same slice) wires only `.on_mouse_down(Left)` and `.on_mouse_down(Right)` on
   file rows. Grepped the whole `tiller_ui`/`tiller_terminal`/`tiller` crates for `.on_drag(`
   outside `#[cfg(test)]`: the only production call sites are a pane-divider resize and a tab
   reorder, both unrelated to file rows. The single `.on_drag` that emits a `PathBuf` for a
   file-like payload lives in `right_panel.rs`'s own test module (`DragFixture`), explicitly
   commented as proving the *harness* can express a payload drag, not the product wiring — "the
   product half is routed to codex12 ... a file row becomes the drag source, a pane the drop
   target" (i.e. acknowledged in-repo as not yet built).
2. **Live drive.** Used the drag helper the integrator landed on this row's behalf after the
   builder's report (`Scripts/linux-drive.sh:e8f72a6`, `drag x1 y1 x2 y2 [steps]` — real
   mousedown, 12 intermediate `mousemove --sync` steps, mouseup) to drag README.md's Files-panel
   row onto a Terminal pane. Result: the terminal prompt received no text at all; the Files panel
   only registered a plain row-select (blue highlight), exactly what a mousedown with no
   `.on_drag` source produces. Frames: `/tmp/cp4-x11/14a-before-drag.png` (before),
   `/tmp/cp4-x11/14-drag-result.png` (after — terminal still empty, README.md merely selected).

`terminal_file_drop` itself (`tiller_project/src/file.rs`, this row's nominally owned file) is a
correct, well-tested pure function — the defect is that nothing in the owned Files-panel code
(`right_panel.rs`, also owned by this slice) ever calls it from a live drag. The builder's
"tooling gap" framing is no longer accurate now that the primitive exists; the feature the row
describes (drag a Files-panel row onto a terminal pane) does not work, live, today.
