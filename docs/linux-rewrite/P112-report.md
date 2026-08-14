# P112 — can the Wayland lane take synthetic input?

## Verdict

**Yes.** The lock-free nested-sway lane can deliver synthetic left-clicks, text, and named keys to Tiller. `Scripts/wayland-drive.sh` now provides `move`, `click`, `type`, and `key` actions. A socket-unreachable end-to-end proof populated the Projects filter with `P112_SCRIPT_E2E`; a second proof typed `P112_NAMED_KEY_X` then used `key BackSpace`, leaving `P112_NAMED_KEY_` in a forced-repaint screenshot.

The helper checks `WAYLAND_DISPLAY` and uses `swaymsg -s "$SWAYSOCK" -t get_version` to require the per-run nested sway config before every injection. It never sends to the inherited operator desktop display.

## What was actually dropping events

The initial hypothesis was partly right about seat capability zero, but wrong that GPUI failed to react later:

1. A bare `WLR_BACKENDS=headless` sway seat reported `capabilities: 0, devices: []`.
2. After a one-shot `wtype`, `WAYLAND_DEBUG=1` recorded `wl_seat.capabilities(2)`, followed by GPUI's `wl_seat.get_keyboard`, `wl_keyboard.enter`, and all key events. GPUI dynamically binds a keyboard; ordering is not required for that bind.
3. One-shot `wlrctl pointer move` and `wlrctl pointer click` each separately caused `capabilities(1)` and GPUI's `get_pointer`, but no `wl_pointer.enter`, `motion`, or `button`. The commands had sent their only event before the app had an input object. Exit 0 meant sway accepted the virtual-device request; it did not establish end-to-end delivery.
4. A persistent virtual pointer created **before** Tiller connected made GPUI bind `wl_pointer` at startup. Its later action produced `wl_pointer.enter`, `motion`, and left-button press/release in the trace. This is the dropped layer: the transient device's event raced ahead of the client's bind.
5. A long-lived keyboard keeper created before app startup avoids the analogous first-text race. It presses and releases Shift before Tiller connects, then remains connected without a modifier held; subsequent short-lived `wtype` calls deliver complete strings through the already-advertised seat.

## Implementation

- `Scripts/wayland-virtual-pointer.c` is a tiny Wayland client. It binds `wl_seat` and `zwlr_virtual_pointer_manager_v1`, keeps its virtual pointer alive, and receives absolute `move`/left-`click` commands through a unique FIFO.
- `Scripts/wlr-virtual-pointer-unstable-v1.xml` is the wlroots virtual-pointer protocol definition used by `wayland-scanner` at run time.
- When an action block contains `click` or `move`, `wayland-drive.sh` builds and starts the pointer before launching Tiller. When it contains `type` or `key`, it starts the quiescent long-lived `wtype` keyboard before launching Tiller.
- The script continues to use no `DISPLAY`, takes no `DISPLAY=:1` drive lock, and leaves no Rust changes.

Example:

```bash
Scripts/wayland-drive.sh /tmp/p112-shots '
  click 100 80
  type P112_SCRIPT_E2E
  key BackSpace
  shot filter-input
'
```

The initial `shot` forces the lane to 1400×900, which is why the proof coordinate is `100 80`; coordinates remain layout-specific.

## Evidence

| Probe | Result |
|---|---|
| Baseline seat | `capabilities: 0`, no devices |
| One-shot `wtype` | GPUI received `wl_keyboard` enter and every generated key event, but a focusable target still needs a real pointer click |
| One-shot `wlrctl` pointer | GPUI bound then immediately released `wl_pointer`; no pointer event arrived |
| Persistent pointer | trace contained `enter`, `motion`, and button down/up; cursor appeared at the requested coordinate |
| End-to-end text | `/tmp/p112-keyboard-green-shots/02-filter-input.png` visibly shows `P112_SCRIPT_E2E` |
| Named key | `/tmp/p112-key-green-shots/02-named-key.png` visibly shows `P112_NAMED_KEY_` after BackSpace |

## Scope still owed

P112 exercised only absolute pointer move, left click, ASCII text, and a named key. It did **not** exercise or implement right-click, button-held drag, modifiers/chords (including `Shift+Tab`), non-ASCII text, or IME. Keep those rows on `DISPLAY=:1` until separately proven. The current pointer service can be extended for button-down/up only with a new focused proof; do not infer drag support from a working click.
