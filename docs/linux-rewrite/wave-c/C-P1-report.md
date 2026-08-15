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

## F-EDIT-12 — not attempted as a live drive; strengthened as a unit test

Confirmed both product-side halves already exist and already match types exactly:
`changes.rs`'s `changes-file-row` carries `.on_drag(payload, ..)` with `DiffPayload =
(PathBuf, String)`, and `tiller_terminal`'s `TerminalView` has a real
`on_drop::<(PathBuf, String)>` handler. The gap the evidence pointed at was that only the
*drop-target* half (in `tiller_terminal`) had a test driving real GPUI mouse events — the
drag-source half in `changes.rs` only had a payload-construction unit test
(`a_textual_diff_builds_the_terminal_drag_payload`), never a real drag gesture off the actual
drawn row.

Added `a_drawn_change_row_drags_its_diff_payload_to_a_drop_target` in `changes.rs`: renders the
real `ChangesTab`, locates the real `changes-file-row` (not a synthetic stand-in), and drives it
through GPUI's actual `MouseDownEvent`/`MouseMoveEvent`/`MouseUpEvent` sequence onto a drop
fixture (a stand-in for a pane's `on_drop`, since `tiller_ui` cannot depend on
`tiller_terminal` — the crate graph forbids it, per `CLAUDE.md`'s dependency-direction rule).
Asserts the fixture receives the exact dragged path and diff text.

- `howToExercise`: `cd rust && cargo test -p tiller_ui a_drawn_change_row_drags_its_diff_payload_to_a_drop_target`.
  Live: this remains genuinely unexercisable on the Wayland lane — re-confirmed against
  `WAYLAND-LANE.md`, the virtual-pointer driver has no button-down-only/motion-while-held
  primitive, so a drag cannot be composed here by construction (not a code gap). An X11/DISPLAY=:1
  lane, if available, would drag a `changes-file-row` from an open Changes tab onto a terminal
  pane and expect the terminal to render `Dropped diff: <path>`.
- Commit: `88e7476` — `test(ui): prove real changes-row drag delivers its diff payload`
- Verdict left to the critic: this remains structurally NOT EXERCISED live, but the two halves
  are now each proven against real production code independently, closing the "harness-only"
  gap noted in older evidence (P81/E09).
