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

## F-TERM-PTY-06 — already correct, no code change

Verified the evidence's own description was accurate and current: `receive_file_drop` takes
`Vec<PathBuf>`, `on_drop::<gpui::ExternalPaths>` is wired on the running-terminal branch
(`lib.rs` ~1567), and `receive_file_drop` funnels both the in-app typed-drag path (always one
`PathBuf`) and the multi-file XDND path through the same quoted-and-space-joined insertion
(`tiller_project::terminal_file_drop`). The test
`a_drawn_terminal_accepts_a_real_external_paths_drop_with_several_files` drives the real
`on_drop::<ExternalPaths>` handler with GPUI's actual XDND payload type carrying several files,
and passes (`cargo test -p tiller_terminal a_drawn_terminal_accepts_a_real_external_paths_drop_with_several_files`).
Nothing in the code needed fixing; the row's own evidence already correctly states no lane can
drive a real XDND drop end-to-end (ENVIRONMENT.md). Left as `already-correct`, not reclassified,
since I am not the verdict-writer.

## F-TERM-UI-02 — strengthened as a unit test

`opens_terminal_link(event.modifiers.platform)` at `lib.rs`'s `on_left_mouse_down` had only
`link_router.rs`'s pure-function unit test (`linux_link_gesture_is_platform_modifier_only`)
covering the boolean logic — no test drove the real mouse-dispatch path on a live, drawn
terminal. Added `platform_modifier_click_opens_a_terminal_link`: spawns a real PTY that prints
an actual URL, waits for it to land in the alacritty grid (`terminal.link_at(0, 0)`), then
drives the real `on_left_mouse_down` via `cx.simulate_mouse_down` twice at the same screen
point — once with `Modifiers::none()` (asserts no `TerminalLinkEvent`), once with
`Modifiers { platform: true, .. }` (asserts the event carries the exact URL
`https://example.test/docs`).

- `howToExercise`: `cd rust && cargo test -p tiller_terminal platform_modifier_click_opens_a_terminal_link`.
  Live: re-confirmed the lane genuinely cannot drive this — `wayland-drive.sh`'s pointer command
  has no modifier-held-click primitive and the lane's keyboard-keeper stays unmodified after
  startup (per prior evidence). Instrument-blocked, not platform-impossible.
- Commit: `b177b53` — `test(terminal): prove platform-modifier click opens a terminal link`

## F-GIT-REMOTE-01 — partial fix; needs one foreign-file line to fully land

`github_owner` (the owner-parsing half of remote parsing) had zero callers outside
`tiller_git` — only a re-export, an internal wrapper, and its own tests. `project_name` (the
project-naming half) was already genuinely reachable at `project_forms.rs`'s `destination_for`
(used to compute the clone destination folder), so only the owner half was the actual gap.

Added `ProjectIconPicker::with_value_and_repo(value, repo, cx)` in `project_identity.rs`: same
as `with_value`, but pre-fills the Avatar tab's GitHub field from `tiller_git::github_owner(repo)`
when the checkout has a GitHub `origin` remote. Purely additive — `value`/`mode` are untouched,
so it behaves exactly like `with_value` for a non-GitHub or remote-less checkout, and an
already-chosen icon survives unchanged. Covered by `a_github_origin_remote_prefills_the_avatar_field`,
a `gpui::test` against a real `git init` + `git remote add origin git@github.com:...` checkout,
asserting "Use GitHub Avatar" commits the pre-filled owner with zero typing.

This makes `github_owner` reachable from `tiller_ui`, but not yet from the running app: the
picker is actually mounted in `sidebar.rs`'s `open_project_settings` (not an owned file for this
slice), which currently calls `ProjectIconPicker::with_value(icon.borrow().clone(), cx)`. That
same function already resolves `row.path` — the exact project's checkout path — a few lines
earlier, so the one-line fix is:

```rust
ProjectIconPicker::with_value_and_repo(icon.borrow().clone(), &path, cx)
```

in place of the current `with_value(...)` call (`sidebar.rs` around line 976).

- `wantedForeignFiles`: `rust/crates/tiller_ui/src/sidebar.rs` — apply the one-line
  `with_value` → `with_value_and_repo` swap described above at `open_project_settings`
  (~line 976) to make `github_owner` reachable end-to-end from the running app's Project
  Settings gear.
- `howToExercise` (once the sidebar.rs line lands): open a project whose checkout has a
  GitHub `origin` remote, click its sidebar gear → Project Settings, switch the icon picker to
  the Avatar tab — the GitHub field should already show the parsed owner instead of being
  empty; clicking "Use GitHub Avatar" with no typing commits it.
- Commit: `1f9f860` — `feat(ui): pre-fill GitHub avatar field from the repo's origin remote`
