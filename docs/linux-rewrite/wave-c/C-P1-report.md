# C-P1 report

## F-CORE-TERM-02 — fixed

`TerminalView::on_key_down` (rust/crates/tiller_terminal/src/lib.rs) previously ignored
everything except scroll keys and PTY passthrough; `open_context_menu` was wired only to
`MouseButton::Right`. Added a keyboard path: `on_key_down` now recognizes the dedicated
"Menu"/"ContextMenu" key and Shift+F10 (the conventional keyboard-context-menu chords) and
opens the same context menu, anchored at a fixed `(20px, 20px)` offset since a keyboard event
carries no pointer position to anchor to.

New test `shift_f10_opens_the_context_menu_from_the_keyboard` (same file, `view_tests` module)
spawns a real running terminal, focuses it with a real click (`on_left_mouse_down`'s focus
path), then drives `cx.simulate_keystrokes("shift-f10")` through the actual `on_key_down`
dispatch — not a direct field mutation — and asserts both `context_menu.is_some()` and that
`terminal-context-item-0` is actually drawn.

- `howToExercise`: on the Wayland lane this is still only provable by unit test — the lane's
  `key <name>` driver action was not confirmed to have a "shift-f10" / menu-key primitive at
  the time of this row (only plain keys were exercised in prior evidence). The unit test is the
  reproducible route: `cd rust && cargo test -p tiller_terminal shift_f10_opens_the_context_menu`.
  If the lane does support a shift+F10 chord, drive a running terminal pane, `key shift-f10`,
  `shot` — the same context-menu popup that right-click draws should appear at (20,20) inside
  the pane.
- Commit: `0c5871a` — `feat(terminal): open context menu from Shift+F10 keyboard chord`
