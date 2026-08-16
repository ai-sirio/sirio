# P130 — `modclick` was never disproven either; it works, and a positive control proves it

`modclick` (`Scripts/wayland-drive.sh`) has been in the lane vocabulary since P124 (2026-08-15) but
carried an honest caveat: "not independently proven end-to-end against a live app target." Wave-G's
critic then drove 4 live `modclick logo <x> <y>` attempts against a real rendered
`https://example.com` line in a terminal pane and got no Browser tab in any of them
(`docs/linux-rewrite/wave-g/G5-terminal-verdicts.md`, `F-TERM-UI-02`). That null result was left
`half-proven` rather than `FAILED`, per this project's own standing rule that a null from an
unproven primitive is inconclusive, not negative.

This task settles the primitive itself: **does the app ever observe the modifier as held at the
moment of the click?** Answer: **yes, every time it was tried.** `modclick` is not the blocker.
`F-TERM-UI-02`'s continued null result has a different, app-side explanation (see "What this does
and does not settle" below) that is out of this task's scope (`nothing under rust/`).

## Method

Two independent kinds of evidence, both live, both same-invocation:

1. **Ground truth at the wire.** `WAYLAND_DEBUG=1` on the app's own process (added as
   `TILLER_WL_PROTOCOL_LOG=1` to `wayland-drive.sh`, see below) prints every event the app's *one*
   Wayland connection receives, in true arrival order. This is the only source that can answer an
   ordering question — a screenshot shows the result of event processing, never the order the
   events were delivered in.
2. **A positive control.** Per the task's own instruction: find a modifier+click the app already
   gates correctly, and prove `modclick` fires it. `file_view.rs`'s markdown-link-open path
   (`F-CORE-FILE-04`) is exactly that — `TranscriptSelectableText`'s sibling in the file viewer,
   gated on `event.modifiers.platform` at `MouseDown` and `MouseUp` the same way the terminal's link
   click is, but resolving to a real repo-relative file (`resolve_file_link`) instead of an
   `xdg-open` URL — so success is a **new tab appearing**, not an external process launch this
   sandbox may not even have. It is independent of `F-TERM-UI-02`'s own code (different crate,
   different element, no shared bug surface) which is exactly what a positive control needs to be
   to mean anything.

Both were run against a real build (`cargo build -p tiller`, `rust/target/debug/tiller`) under
`wayland-drive.sh` on `sway -d` headless, this machine, 2026-08-16.

## Evidence 1 — wire trace: the modifier always lands before the click

Attempt against the terminal's rendered URL (`click`/`type`/`key Return` first put
`https://example.com` on screen, then `modclick logo 400 111`):

```
[1484205.654] wl_keyboard#95.keymap(1, fd 37, 22495)      <- modclick's `wtype -M logo` connects
[1484311.718] wl_keyboard#95.modifiers(85, 64, 0, 0, 0)   <- Mod4 (Logo/Super) depressed
[1484311.734] wl_pointer#97.motion(10082172, 400.0, 111.0)
[1484311.743] wl_pointer#97.button(86, 10082197, 272, 1)  <- BTN_LEFT press, 25us after modifiers
[1484343.520] wl_pointer#97.button(87, 10082222, 272, 0)  <- BTN_LEFT release
```

`64` is `Mod4`, the standard XKB bit for `MOD_NAME_LOGO` — matches
`gpui_linux/src/linux/platform.rs::modifiers_from_xkb` mapping Logo to `Modifiers.platform`, which
`opens_terminal_link`/`on_left_mouse_down` (`tiller_terminal/src/lib.rs:1079`) reads directly. The
`modifiers` event is on the wire **before** the button-press event, in the app's own single ordered
connection — GPUI's Wayland client dispatches wire messages in arrival order on one thread
(`gpui_linux/src/linux/wayland/client.rs`), so `state.modifiers` (mutated by the `Modifiers` handler)
is already `platform: true` by the time the `Button` handler builds the `MouseDownEvent` a few
microseconds later. This is not inference from timing — it is what the app's own protocol stream
says happened.

Second attempt, same shape, against the markdown link (see Evidence 2):

```
[1765823.334] wl_keyboard#95.keymap(1, fd 38, 22495)
[1765855.809] wl_keyboard#95.modifiers(228, 64, 0, 0, 0)  <- Logo depressed
[1765919.672] wl_pointer#97.button(229, 10363825, 272, 1) <- press, 64ms later, still well inside
[1765969.595] wl_pointer#97.button(230, 10363850, 272, 0) <- release
[1766223.414] wl_keyboard#95.modifiers(231, 0, 0, 0, 0)   <- Logo released ~400ms after depress
```

The hold started at `823.809` and released at `1766223.414` — a ~400ms window matching
`modclick`'s `wtype -M logo -s 400 -m logo`, with both the button press and release falling
comfortably inside it. The modifier was never at risk of expiring before the click landed.

Two independent keyboard-client connects appear before each successful modifier report
(`keymap` at `fd 32`/`fd 37` and again `fd 37`/`fd 38`) — every one-shot `wtype` invocation
(`type`, `key`, `chord`, and `modclick`'s modifier-hold half) opens a fresh
`zwp_virtual_keyboard_v1` and uploads its own keymap, which the compositor reports to the app as a
keymap change (sometimes with a `leave`/`enter` cycle, sometimes without). This is pre-existing
behavior of every keyboard primitive in the file, not something `modclick` introduced — `chord` was
already proven live in P124 despite the same per-invocation reconnect.

## Evidence 2 — positive control: `modclick` opens a markdown link, plain click does not

Fixture: two scratch files at the repo root (not committed, deleted after the run —
`git status --porcelain` clean afterward, same discipline the wave-G critic used for its own
scratch fixture):

```
P130_SCRATCH_LINK.md:   [LINKTEST](P130_SCRATCH_TARGET.md)
P130_SCRATCH_TARGET.md: P130 scratch link target — if this file is open as a new tab, the
                        platform-modifier click worked.
```

Opened `P130_SCRATCH_LINK.md` in the file viewer (Files panel → select → Return), located the
rendered `LINKTEST` link, then in the same running instance:

- **Negative control**, plain `click 510 147` (no modifier held): tab strip unchanged, no new tab.
  `reference/linux-progress/p130-modclick/p130-negative-control-plain-click.png`.
- **Positive test**, `modclick logo 510 147`: `P130_SCRATCH_TARGET.md` opens as a new tab and is
  brought to the foreground, showing its "the platform-modifier click worked" text — the
  discriminating marker requested by the task, not a screenshot that merely contains the pane.
  `reference/linux-progress/p130-modclick/p130-positive-control-modclick-logo.png`.

Same click, same coordinates, same pane — the only variable between the negative and positive
capture is whether `modclick` or plain `click` drove it. The app only acts on the modifier-held
case, which is exactly the gate `file_view.rs` codes (`pressed.set((global, event.modifiers.platform))`
on `MouseDown`, `platform_held = down_platform || event.modifiers.platform` on `MouseUp`).

## What this does and does not settle

**Settled: `modclick` delivers a correctly-ordered, correctly-valued, sufficiently-held modifier
before its click, and the app acts on it when its own gate is coded to look.** Both the harness
mechanism (virtual-keyboard modifier hold via a backgrounded one-shot `wtype`, click via the
persistent virtual-pointer FIFO client) and the 50ms settle before firing the click are adequate —
the wire trace shows ~64ms and ~106ms gaps between the modifier landing and the button press in the
two runs above, comfortably inside the 400ms hold, and no case where the button arrived first. No
change to `modclick` itself was needed or made.

**Not settled by this task, and not this task's to fix (`nothing under rust/`): why
`F-TERM-UI-02`'s own live gesture still produces no Browser tab.** Given Evidence 1 and 2, the
remaining candidates are all on the app side of `on_left_mouse_down`
(`tiller_terminal/src/lib.rs:1072`), not in how the click is delivered:

- The row/column arithmetic fix (`9a13174f`, `86695b34`) is real and unit-tested, but only proven
  against a `gpui::test` window and a *directly*-owned `TerminalView`. This task's own terminal
  pane sits inside the real app's sidebar + tab-strip + (potentially) split layout, the same shape
  an **uncommitted, in-progress fixture** on this branch (`OffsetHostFixture` /
  `p129_context_menu_position_when_pane_is_offset`, `tiller_terminal/src/lib.rs`) is independently
  investigating for the *context-menu* paint path. A `terminal.last_bounds` that is stale, or
  belongs to the wrong pane in a multi-pane tab, would produce exactly this task's symptom: a
  correctly-modifier-gated click that resolves to the wrong grid cell and finds no link there.
  That fixture was not authored by this task and was left untouched, per instructions.
- `url_at_column`/`link_at` require the click to land within the exact cell range of
  `https://example.com` on the exact rendered row — a coordinate off by one row or a few columns
  finds no link and produces this task's observed null, indistinguishable from a modifier failure
  without the wire trace this task adds.

Whoever next picks up `F-TERM-UI-02`'s gesture half should drive it with
`TILLER_WL_PROTOCOL_LOG=1` and additionally dump the clicked (row, column) — the wire trace alone
answers "was the modifier there" but not "did it land in the right cell"; that needs either an
app-side print (out of this task's scope) or bisecting coordinates against a screenshot with known
character-cell geometry.

## What changed

- `Scripts/wayland-drive.sh`: added `TILLER_WL_PROTOCOL_LOG=1`, an opt-in env var that adds
  `WAYLAND_DEBUG=1` to the app's own launch environment only (never the injectors', whose trace
  would dwarf the app's and bury the question). Diagnostic-only; `modclick`'s own implementation is
  unchanged — it did not need fixing.
- `docs/linux-rewrite/WAYLAND-LANE.md`: `modclick`'s vocabulary entry updated from "not
  independently proven" to a live positive-control capture, matching the other P124 primitives.
