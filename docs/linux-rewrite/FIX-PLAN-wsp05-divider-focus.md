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

### The helper, written against the API this file already uses

There is no existing "focus the focused pane" helper, so add one. The body is the same shape as
`close_terminal_at`'s replacement-focus block at `main.rs:7930-7944`, which is the precedent to
copy — it is the code that already knows how to turn a pane id into a focus handle:

```rust
/// Puts keyboard focus back on `tab_index`'s currently focused pane.
///
/// The divider needs this. It is a control, not a surface, and GPUI blurs
/// whatever held focus when the mouse goes down on an interactive element
/// carrying no focus handle of its own. Without re-asserting, focus falls
/// through to F-SID-19's root reclaim and the pane quietly stops receiving
/// keystrokes.
fn refocus_focused_pane(
    &mut self,
    tab_index: usize,
    window: &mut Window,
    cx: &mut Context<Self>,
) {
    let Some(tab) = self.tabs.get(tab_index) else {
        return;
    };
    let target = tab.focused_pane;
    let mut handle = None;
    tab.panes.for_each(&mut |id, content| {
        if id == target {
            handle = match content {
                TabContent::Chat(chat) => Some(chat.focus_handle(cx)),
                TabContent::Terminal { view } => Some(view.focus_handle(cx)),
                TabContent::File { .. }
                | TabContent::Changes(_)
                | TabContent::Browser(_) => None,
            };
        }
    });
    if let Some(handle) = handle {
        window.focus(&handle, cx);
    }
}
```

Note the `None` arms: file, changes and browser panes have no focus handle here, exactly as
`close_terminal_at` treats them. For those the divider cannot restore focus to a pane, and the
root-focus fallback remains correct — say so in the test rather than treating it as a failure.

### Wiring it, at both handles

The handler runs with `&mut App`, not `Context<Self>`, so use the entity rather than `cx.listener`
— the same shape `.on_drag_move` at `8143` already uses. Add one more clone of the workspace entity
alongside `drag_entity_move`/`drag_entity_drop` (`8132-8133`), then on **each** handle, before
`.on_drag(...)`:

```rust
.on_mouse_down(gpui::MouseButton::Left, {
    let entity = drag_entity_focus.clone();
    move |_, window, cx| {
        entity.update(cx, |workspace, cx| {
            workspace.refocus_focused_pane(tab_index, window, cx);
        });
    }
})
```

One thing to confirm on contact, which could not be checked without compiling: `drag_entity` is
first bound above `8094` and the clone must be created before the `drag` div at `8099` consumes it.
If the binding turns out to sit lower, move the clone up rather than reordering the div.

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
