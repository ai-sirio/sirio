# F-CORE-WSP-05 — the divider steals focus: diagnosis and the exact fix

Written by the orchestrator at 07:10 on 2026-08-19, with the disk ~10 minutes from ENOSPC and no
build possible. Everything below is established by reading and by the existing regression test; the
patch is **not** applied, because committing code that cannot be compiled is below the standard
this project holds its agents to. A successor should be able to apply it in one sitting.

## What is wrong

The row requires that *nonstructural* commands — activation, fraction, view-state, rename — report
nonstructural transitions and leave focus alone. The fraction class is the pane divider, and it does
not leave focus alone: a mouse-down on the divider handle empties `window.focus`.

The existing test pins it down exactly:

```
cargo test -p tiller --bin tiller -- drawn_divider_drag_blurs_focus_to_workspace_root
```

It asserts `focus_new` and `focus_old` both read false while `tab.focused_pane` is unchanged. That
combination is the signature: the workspace's *own* idea of the focused pane is intact, so nothing
called `select_pane` — what changed is GPUI's window-level focus, which was dropped entirely. Only
F-SID-19's "nothing has focus" fallback (the workspace root reclaiming focus) keeps the window
taking keyboard input at all.

## Why it happens

`main.rs:8099-8131` builds the divider. The handle is a bare `div()`:

```rust
div()
    .id(format!("pane-divider-h-handle-{path:?}"))
    .absolute()
    .left(px(-1.5))
    .w(px(9.))
    .h_full()
    .cursor_col_resize()
    .on_drag(divider_drag.clone(), |_, _, _, cx| cx.new(|_| gpui::Empty)),
```

It is interactive (`.id()` plus `.on_drag()`) but it is **not focus-tracked** — there is no
`track_focus`, and no focus handle is associated with it. In GPUI, pressing the mouse on an
interactive element that carries no focus handle moves window focus away from whatever held it and
leaves nothing focused. `update_divider` (`main.rs:7952`) is innocent: it takes `cx` but no
`window`, and never touches focus. The blur happens at mouse-down, before any drag movement, which
is also why an attempted live drive could not separate it from "the mousedown landed on the already
focused pane" — the pane never got focus at all.

## The fix

Re-assert focus on the handle's own mouse-down, so the divider never becomes the focus target.
Add to **both** handles (horizontal at ~8106 and vertical at ~8121), before `.on_drag(...)`:

```rust
.on_mouse_down(
    MouseButton::Left,
    cx.listener(move |workspace, _, window, cx| {
        // The divider is a control, not a surface: grabbing it must not move
        // keyboard focus. Without this, GPUI blurs the focused pane on
        // mouse-down because this handle carries no focus handle of its own,
        // and only F-SID-19's root-focus fallback keeps the window taking
        // input at all.
        workspace.focus_active_pane(window, cx);
    }),
)
```

`focus_active_pane` is shorthand for whatever the workspace already uses to focus
`tab.focused_pane`'s handle — reuse the existing helper rather than adding one; the split path in
`close_terminal_at`/`confirm_pending_pane_close` already threads `window` for exactly this purpose
and is the precedent to copy.

Note the closure needs `cx.listener`, so this must be built where a `Context<TillerWorkspace>` is in
scope. At `8099` the surrounding code already has `drag_entity` (an entity handle for the
workspace); if `cx` is not available at that exact point, use `drag_entity.update(cx, ...)` inside a
plain `.on_mouse_down(MouseButton::Left, move |_, window, cx| { ... })` the same way
`.on_drag_move` at `8143` already does.

## How to verify it, in order

1. Invert the existing test. `drawn_divider_drag_blurs_focus_to_workspace_root` currently asserts
   the defect; rename it to `drawn_divider_drag_leaves_pane_focus_untouched` and assert that the
   originally focused pane still reads `focus_old == true` after the mouse-down. It must be red
   before the patch and green after — capture both transcripts.
2. Keep a negative control: clicking the *pane body* must still move focus, or the fix has simply
   frozen focus everywhere.
3. Drive it live in the nested lane with a zero-click discriminator, the way Split and Close were
   proven for this row: focus a pane, type a probe string, grab the divider and drag it, then type a
   second probe and read `panel.scrollback` — both probes must land in the **same** pane. That is
   the evidence the row still owes.

## Do not forget the rest of the row

Even with this fixed, `F-CORE-WSP-05` still has an open leg recorded in the ledger: **Insert and
Move have no identified real-app analog.** Settle whether they genuinely have none — in which case
say so as a scoped port decision with the argument written out — or find the counterpart. Do not
let the divider fix alone promote the row.
