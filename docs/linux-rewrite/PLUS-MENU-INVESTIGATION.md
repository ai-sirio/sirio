# The "+" tab-strip menu — diagnosis, not harness superstition

Two critics independently reported the tab-strip "+" dropdown (`crates/tiller_ui/src/tab_bar.rs`,
`TabBar::toggle_menu` / the `new-tab-button` around lines 383-425 and 620-680) as undriveable:

- **Critic 1** (wave B, F-CHAT-25): splitting the click into `move` + `down` + `up` showed the menu
  closing on mousedown alone, before mouseup lands on any item.
- **Critic 2** (wave F, F-AGENT-OPENCODE-01): `New Terminal` / `Claude Code` / `OpenCode` clicks in
  that dropdown never created a tab across 7 attempts, two lane labels, low and high load.

The discriminator handed to this investigation: the **right-click context menu** (same
`tab_bar.rs`, `render_tab_context_menu`, wired up in `crates/tiller/src/main.rs`'s
`render_tab_context_menu`) works fine under the identical synthetic pointer — a critic right-clicked
a live pane, got the real 13-item menu, clicked "Restart Terminal", and the pane restarted (verified
by PID). So the fault is not "synthetic clicks can't drive menus" in general. Something is different
about *this* dropdown specifically.

## Verdict up front

**Both are true, and they compound:**

1. **Real, narrow app bug**, now fixed: the "+" dropdown's open popup does not reposition itself
   after a window resize that happens while it's open. Its anchor point is measured one render pass
   late by design (a `canvas` prepaint callback), and nothing was forcing the follow-up render that
   would let it catch up. The popup stayed glued to the button's *pre-resize* screen position
   forever.
2. **Harness practice that reliably detonates #1**: `wayland-drive.sh`'s (and
   `x11-nested-drive.sh`'s) `shot` action forces a repaint by doing a real window resize (nudge to
   W2×H2 and back) — it is not a passive snapshot. Both critics' reproduction sequences called `shot`
   either between a `down` and its matching `up`, or immediately after opening the menu and before
   clicking an item. That resize is exactly the trigger for bug #1: the popup silently detached from
   the button and the `up`/second `click` landed on dead space, which reads indistinguishably from
   "the menu closed on mousedown" or "the click did nothing."

Neither hypothesis alone explains both critics' reports as cleanly as the two together do, and the
evidence below rules out "input cannot reach this dropdown at all" specifically.

## 1. Reproducing it live

Built the current tree (`cargo build --manifest-path rust/Cargo.toml --workspace`, exit 0), pinned
to `/tmp/wg-plus-tiller`, driven with `Scripts/wayland-drive.sh` under `TILLER_WL_LABEL=wg-plus`.

**Frame 1 — before the click** (`/tmp/wg-plus-shots2/02-before-menu.png`): tab strip shows five
tabs, Files panel open on the right, no menu. The "+" sits at roughly `(1290, 48)` in this window's
1715×972 layout (its exact position is read from the Files-panel gutter width, which varies with
window size — this is precisely the value `anchor_bounds` caches).

**Frame 2 — menu open** (`/tmp/wg-plus-shots2/03-menu-open.png`): a single atomic `click 1290 48`
(down+up with no repaint forced in between) opened the real 10-item menu — New Terminal, Changes,
New Browser, Claude Code, Codex, OpenCode, Pi, Oh-My-Pi, Split Claude Code, New Chat — anchored
directly under the button. Confirms `deferred(anchored()...)` renders and is hit-testable from a
synthetic pointer exactly like the working context menu.

**Frame 3 — after clicking a menu item** (`/tmp/wg-plus-shots3/02-after-opencode-click.png`): a
second atomic `click 1330 202` (the "Codex" row) with nothing else in between produced a real new
Codex tab in both the tab strip and the sidebar's worktree tree — `add_agent_tab`'s callback fired.
**The dropdown works.** Screenshots and the exact command sequences are reproducible verbatim; see
"Commands run" below.

So an atomic `click` (GPUI pairs mousedown+mouseup into one click with no frame boundary in
between) on both the opener and an item works every time. The critics' failures both involved
splitting a gesture across a frame boundary — `down`/`up` pairs, or a `shot` between two clicks —
which is exactly where bug #1 bites.

## 2. Comparing the two menus in source

**The working context menu** (`render_tab_context_menu`, called from
`crates/tiller/src/main.rs:8333`):

```rust
let menu = div()
    .absolute()
    .top(px(TAB_BAR_HEIGHT))
    .left(px(self.tab_context_menu_left()))   // computed FRESH, synchronously, every render
    .on_mouse_down_out(...)
    .child(render_tab_context_menu(...));
deferred(menu)   // deferred only for PAINT ORDER (z-stacking above #centre-surface)
```

`tab_context_menu_left()` is a plain function that walks `self.tabs` and sums tab widths — pure,
synchronous, recomputed from current model state on every single render call. There is no
measurement lag: the position used to draw the menu this frame is always consistent with the layout
being drawn this frame, because it's derived from the same state, not from a previous frame's
prepaint of some other element.

**The "+" dropdown** (`tab_bar.rs`, `TabBar::render`):

```rust
// the button's own canvas, evaluated during PREPAINT (after render already ran):
canvas(
    move |bounds, window, _| { anchor_bounds.set(Some(bounds)); ... },
    |_, _, _, _| {},
)
...
// the menu, built during RENDER, reading whatever prepaint left behind LAST frame:
deferred(
    anchored()
        .anchor(Anchor::TopLeft)
        .position(self.anchor_bounds.get()....)   // stale by one full render pass
        .snap_to_window_with_margin(px(8.0))
        .child(menu),
).priority(1)
```

`anchor_bounds` is a `Rc<Cell<Option<Bounds<Pixels>>>>`. GPUI's frame pipeline runs `render()` for
the whole tree first, then `prepaint()` for the whole tree (bottom-up layout), then `paint()`. The
`canvas`'s prepaint closure is what updates `anchor_bounds` — but `render()` reads it *before* that
prepaint pass runs, so it always sees last frame's value. Invisible when the button's screen
position never changes frame to frame (the overwhelmingly common case — nothing else on the
`TabBar` row causes a relayout). Not invisible the moment something moves the button: a window
resize is the only thing that does, in practice, and `wayland-drive.sh`'s `shot` action performs
exactly that resize as a side effect of forcing a repaint.

Both menus use the same dismissal contract otherwise — `on_mouse_down_out` closes the menu, Escape
is bound via `DismissMenu`/`dismiss_menu` — so dismissal logic was never the differentiator. The
differentiator is **how the open menu's position is computed**: recomputed fresh every render vs.
cached from a stale prepaint. Only the cached path can go stale, and only a layout-moving event
(resize) exposes it.

## 3. Cross-checking with the second input stack

Ran the identical gesture (`click` on the "+" button) under `Scripts/x11-nested-drive.sh` — a wholly
separate input path (real `XTestFakeButtonEvent` over a private nested Xwayland, not the Wayland
virtual-pointer protocol `wayland-drive.sh` uses). The dropdown opened there too, on the first try,
with no positioning defect (no resize was interposed in that drive). This is consistent with the
diagnosis: the defect is not in either input stack, it's in `tab_bar.rs`'s own popup-positioning
code, and it only shows up when something resizes the window while the popup is open — a condition
neither input stack causes on its own, but which `shot`'s implementation does as a documented side
effect.

(The x11-nested lane's fresh database had no worktree open, which is an environment-setup
difference, not a per-input-stack difference; it doesn't change the conclusion — the "+" button was
reachable and its menu opened and rendered correctly there too, ruling out "X11 input can't reach
this widget either.")

## 4. The fix

`tab_bar.rs`'s button canvas now schedules a follow-up render when its measured bounds actually
change, using the same `Window::on_next_frame` primitive `toggle_menu` already relies on for its own
"deferred content settles one frame late" focus dance:

```rust
move |bounds, window, _| {
    if anchor_bounds.get() != Some(bounds) {
        anchor_bounds.set(Some(bounds));
        window.on_next_frame(|window, _cx| window.refresh());
    }
},
```

`Window::refresh()` is a no-op if called synchronously inside this same prepaint (prepaint runs
mid-draw, and `refresh` guards against re-triggering a redraw from inside its own frame) — which is
exactly why the naive fix of calling it directly here does nothing. Scheduling it through
`on_next_frame` calls it from *outside* any draw, where it actually marks the window dirty, so the
frame after the one that measured the new bounds renders the menu at the corrected position instead
of staying stale forever.

A regression test, `drawn_new_tab_menu_tracks_the_button_after_a_window_resize`
(`tab_bar.rs::tests`), opens the menu, resizes the test window (`cx.simulate_resize`), pumps one
manual `simulate_next_frame` (tests have no platform frame loop to deliver the scheduled refresh on
their own), and asserts the menu's drawn position has caught up with the button's new position.

**Verified the test actually exercises the bug**: reverted just the fix hunk in `tab_bar.rs` back to
the pre-fix one-liner (`move |bounds, _, _| anchor_bounds.set(Some(bounds))`), reran the test —

```
thread '...' panicked: assertion `left == right` failed: the open menu must self-heal onto the
button's post-resize position within one more delivered frame...
  left: 1222px
 right: 980px
```

— confirmed the failure, then restored the fix (`diff` against the pre-revert file: identical) and
reran the full `tab_bar` suite — 9/9 pass, the new test included.

## 5. The harness side

Even with the app fixed, the underlying trap for the *next* critic is real and worth closing at the
harness level too: `shot`'s resize is invisible in its own output (`SHOT ...png (WxH · N colours)`
gives no hint a resize happened), so a critic reading a "menu missing" frame has no local signal
that their own probe caused it. Both `wayland-drive.sh` and `x11-nested-drive.sh` usage headers now
carry an explicit warning against calling `shot` between a `down` and its matching `up`, or between
opening a menu and clicking one of its items — capture state *before* a gesture and *after* it
completes, never mid-gesture — with a cross-reference to this document. `WAYLAND-LANE.md`'s known-symptoms
table gained a matching row.

## Commands run (reproducible verbatim)

```bash
export XDG_RUNTIME_DIR=/run/user/1000 WAYLAND_DISPLAY=wayland-1 TILLER_WL_LABEL=wg-plus
cargo build --manifest-path rust/Cargo.toml --workspace
cp rust/target/debug/tiller /tmp/wg-plus-tiller
export TILLER_WL_BIN=/tmp/wg-plus-tiller

# frame 1+2: open the menu with an atomic click, no interleaved repaint
Scripts/wayland-drive.sh /tmp/wg-plus-shots2 '
  shot before-menu
  click 1290 48
  shot menu-open
' 15

# frame 3: click a real item, atomic, nothing between the two clicks
Scripts/wayland-drive.sh /tmp/wg-plus-shots3 '
  click 1290 48
  click 1330 202
  shot after-opencode-click
' 15

# cross-check on the other input stack
Scripts/x11-nested-drive.sh /tmp/wg-plus-x11-shots2 '
  click 852 48
  shot menu-open
' 15

# regression test, with and without the fix
cargo test --manifest-path rust/Cargo.toml -p tiller_ui tab_bar
```

## What a fresh critic should still confirm

This was diagnosed and fixed as the sole builder on `tab_bar.rs`, in one pass, without a second
independent pair of eyes rerunning the exact split-gesture sequences (`move`+`down`+`up`, and a
`shot` interposed between a menu's `down` and `up`) that originally produced the two false reports —
only the *fixed* binary was driven end-to-end here. A critic should:

1. Pin this same fixed binary (or rebuild) and re-run **exactly** critic 1's original split-gesture
   sequence and critic 2's original 7-attempt New Terminal/Claude Code/OpenCode sequence, and
   confirm both now succeed.
2. Deliberately reproduce the *old* failure once more by putting a `shot` between a menu-opening
   `down` and its `up` **against the fixed binary**, to confirm the fix specifically closes the
   resize-staleness window rather than merely making the common case faster.
3. Re-check F-CHAT-25 and F-AGENT-OPENCODE-01 (and any other row blocked on this dropdown) against
   the fixed binary now that the primary UI path to an agent tab is confirmed reachable.
