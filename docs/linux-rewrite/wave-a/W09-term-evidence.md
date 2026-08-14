# W09-term evidence

## `F-TERM-04` — ledger line 322

**Claim: could-not-reach.** Reconfirmed from source that `open_context_menu` is bound only to
`on_mouse_down(MouseButton::Right, ...)` at `rust/crates/tiller_terminal/src/lib.rs:1446,1504`,
with no keyboard fallback anywhere in the file. `Scripts/wayland-drive.sh`'s `pointer_command`
(lines 256-273) implements only `move` and `click`, both of which drive the virtual pointer's
plain left-button path (see its FIFO protocol at lines 261-270) — there is no right-click, no
button parameter, and no chord support. `docs/linux-rewrite/WAYLAND-LANE.md` states explicitly
that "right-click, button-held drag, modifier chords ... still require `DISPLAY=:1`", which this
lane is instructed never to use for this slice. The gesture this row needs (select text in the
terminal, right-click, click Copy, right-click again, click Paste) cannot be driven by any
primitive this instrument exposes. No new capture was taken since none of the row's gesture is
reachable; this is a re-confirmation of the existing verdict via source + tooling inspection, not
a new drive.

## `F-TERM-06` — ledger line 324

**Claim: could-not-reach.** Same root cause as `F-TERM-04`: `open_context_menu` is right-click-only
(`rust/crates/tiller_terminal/src/lib.rs:1446,1504`), and `wayland-drive.sh` has no right-click
primitive to open the menu in the first place, so the "Copy Pane ID" / "Copy Terminal ID" items
(`rust/crates/tiller_terminal/src/context_menu.rs:54-66`) can never be clicked from this lane.
Checked the full `"..." =>` dispatch table in `rust/crates/tiller/src/main.rs` for a control-socket
bypass that reaches `TerminalContextAction::CopyPaneId`/`CopyTerminalId` directly — none exists;
`pane.*`/`panel.*` cover split/close/focus/list but not clipboard actions. Row remains unreachable
from this lane.

## `F-TERM-08` — ledger line 326

**Claim: exercised-working** (of the reclassified clause — see below).

Triage's reclassify claim was verified against source before driving: `requires_close_confirmation`
(`rust/crates/tiller_activity/src/activity.rs:30`) has exactly the two references `grep` finds —
its own definition and its own test — and is never called from `rust/crates/tiller/src/main.rs`.
There is no confirmation dialog gated on it anywhere in the close path. `close_terminal_at`
(`main.rs:5149`) guards only on `tab.panes.leaf_ids().len() <= 1 || !tab.panes.contains(focused_pane)`
and silently `return`s on either — no fallback to close the tab, unlike `CloseTab`. Both claims in
the triage note are accurate.

Drove the actual behavior over the socket (own capture, this session, not reused from any prior
attempt):

1. `pane.split direction=right` on the Terminal tab creates a second leaf (`pane-2`).
   `panel.list` confirms 3 panels: `pane-0` (Chat), `pane-1` (Terminal), `pane-2` (Terminal,
   active). Screenshot: `reference/linux-progress/wavea-W09-term/f-term-08/02-f-term-08-two-panes.png`.
2. `pane.close` (same `ControlAction::ClosePane` the right-click "Close Terminal…" menu item
   delegates to) removes `pane-2` — no confirmation prompt appears or is required, and the
   subsequent `panel.list` shows exactly `pane-0`/`pane-1` again. Screenshot:
   `reference/linux-progress/wavea-W09-term/f-term-08b/02-f-term-08-single-pane.png`.
3. Positive/negative control for the single-leaf guard: with only `pane-1` as the sole Terminal
   leaf, `pane.close` still returns `{"ok":true}` (the request is queued and processed) but
   `panel.list` immediately after is byte-identical to before — `pane-1` survives. This is the
   "silent no-op" triage described: the guard fires, nothing closes, and nothing tells the caller
   it was blocked. Screenshot: `reference/linux-progress/wavea-W09-term/f-term-08c/02-f-term-08-guard-noop.png`.

This resolves the row's core clause exactly as triage framed it: the close mechanism itself works
(kills the pane / collapses the split) and reaches it through the same code the menu item would
use, but there is no confirmation dialog anywhere in that path, and the single-pane guard is a
silent no-op rather than falling back to closing the tab. The verdict "half-proven" undersold what
was missing — the row's confirmation-dialog clause is not gesture-unproven, it is absent from the
build. Reclassify stands; this is a build gap, not a lane limitation.

Captures: `reference/linux-progress/wavea-W09-term/f-term-08/`,
`reference/linux-progress/wavea-W09-term/f-term-08b/`,
`reference/linux-progress/wavea-W09-term/f-term-08c/`.

## `F-TERM-UI-01` — ledger line 535

**Claim: could-not-reach** (gesture half; state half already covered by F-TERM-08 above).

Same right-click-only binding as `F-TERM-04`/`F-TERM-06` blocks opening the menu itself from this
lane, so none of the 12 items (`rust/crates/tiller_terminal/src/context_menu.rs`) can be clicked
here, including "Close Terminal…". `F-TERM-08` above exercises `ClosePane`'s delegation target
directly over the socket and confirms it works and has no confirmation dialog — but that is the
delegation target, not the menu-render-plus-click path this row's clause actually asks for, so it
does not close this row. The z-order defect noted in triage (Files panel painting over an open
context menu) could not be re-checked either, since the menu cannot be opened from this lane to
photograph it under the Files panel. Row remains unreachable from this lane for its gesture half.

## `F-TERM-UI-02` — ledger line 536

**Claim: could-not-reach.**

`opens_terminal_link(event.modifiers.platform)` (`rust/crates/tiller_terminal/src/lib.rs:988`)
requires a click carrying the platform modifier (Super on Linux/COSMIC) held down. Checked
`wayland-drive.sh`'s pointer path end to end: `pointer_command`/`move`/`click`
(lines 256-273) send only `move`/`click` operations over the virtual-pointer FIFO with no modifier
field, and the keyboard keeper (`start_virtual_keyboard`) is documented as pressing and releasing
Shift once at startup then staying unmodified — there is no facility in this script to hold a
modifier during a click. `WAYLAND-LANE.md` lists "modifier chords" explicitly among the gestures
this lane does not yet exercise. Per the task instructions this slice must not use
`Scripts/linux-drive.sh` or `DISPLAY=:1` to attempt the X11 lane fallback the manifest's approach
note suggests trying, so the row cannot be driven from any lane available to this agent. No capture
taken — there is no gesture this instrument can produce to attempt it. Distinct from "compositor
intercepts Super": the block here is the instrument's missing modifier-click primitive, observed
before any click was attempted.
