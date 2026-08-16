# H4-window report

Read `docs/linux-rewrite/tasks/P128-platform-exemptions-overstated.md` first, as instructed. It put
`F-WIN-09` and `F-WIN-11` back in scope after they were wrongly marked `N/A — platform`, and recorded
that `F-WIN-11` is further along than that verdict implied: the state machine already exists and is
unit-tested, just unconsumed.

## `F-WIN-09` — follow the system title-bar double-click preference — **implemented**

Confirmed the diagnosis before building: `gsettings get org.gnome.desktop.wm.preferences
action-double-click-titlebar` resolves on this box (`'toggle-maximize'`, verified live, `XDG_CURRENT_DESKTOP=COSMIC`
notwithstanding — the schema is installed independent of which compositor owns the session). Also
confirmed the other half of the diagnosis by reading the vendored `gpui` source directly: `Window::
titlebar_double_click`/`PlatformWindow::titlebar_double_click` exists but its own doc-comment says
*"This is macOS specific"*, and the checked-out tree carries no Linux platform implementation at all
— GPUI genuinely gives this app nothing to defer to on Linux, matching P128's finding.

`crates/tiller_ui/src/titlebar.rs`:

- New `DoubleClickAction` enum (`ToggleMaximize` / `Minimize` / `None` / `Menu`) with
  `from_gsettings_output` (parses `gsettings`' quoted stdout, e.g. `'toggle-maximize'\n`) and
  `from_system()` (shells out to `gsettings get org.gnome.desktop.wm.preferences
  action-double-click-titlebar`; any failure — missing binary, missing schema, non-zero exit —
  degrades to `ToggleMaximize`, GNOME's own documented default, not an error).
- `Titlebar` gained a `double_click_action` field, set from `DoubleClickAction::from_system()` in
  `new()`, overridable via `with_double_click_action` for deterministic tests.
- The titlebar's own drag-area `div` gets a second `on_mouse_up(MouseButton::Left, …)` listener
  (GPUI dispatches every registered listener for a hitbox, so this coexists with the existing
  should-move one) that checks `MouseUpEvent::click_count == 2` — GPUI's own double-click
  disambiguation, already timing-correct by the point this field is populated, so no bookkeeping is
  needed here — and applies the configured action: `zoom_window()`/`minimize_window()` (through the
  same `on_maximize`/`on_minimize` seams the traffic lights already use), nothing, or
  `show_window_menu(event.position)` (through a new `on_show_menu` seam, added because
  `PlatformWindow::show_window_menu` is `unimplemented!()` under `TestWindow` the same way
  minimize/zoom are — a new `with_menu_handler` builder follows the file's own established
  `with_minimize_handler`/`with_maximize_handler` convention).
- Added `.debug_selector(|| "tiller-titlebar".to_owned())` to the outer titlebar div — it had an
  `.id()` but no selector, so `debug_bounds("tiller-titlebar")` returned `None` until this was added;
  caught by the first test run, not assumed.

**Known collision, left as-is:** traffic lights and cluster buttons stop propagation of their own
`on_mouse_down` (so a button click doesn't also start a window drag) but not of `on_mouse_up`, so a
literal double-click landing squarely on e.g. the maximize dot would fire that button's own
`on_click` twice *and* this new handler once. Real desktop titlebars have the same shape of
interaction (a button already consumes the semantic action first); not fixed here as out of the
row's scope, and noted rather than silently accepted.

### Tests — `crates/tiller_ui/src/titlebar.rs`

- `double_click_action_parses_every_gsettings_value` — all four documented values plus one garbage
  value, table-driven.
- `double_click_action_from_system_resolves_without_panicking` — the row's own instruction ("check it
  resolves on this box"), asserting the live value equals `ToggleMaximize` — true both because that
  is what this box's `gsettings` actually returns *and* because it is the graceful-absence default,
  so the assertion holds regardless of which is true on whatever box runs it next.
- Four drawn `#[gpui::test]`s, each firing a real `MouseDownEvent`/`MouseUpEvent` pair with
  `click_count: 2` at the titlebar's own drawn bounds and asserting the wired seam fired (or, for
  `None`, that nothing did — deliberately left un-overridden so a regression to
  `ToggleMaximize`/`Minimize` would panic on GPUI's `unimplemented!()` test defaults rather than pass
  silently).

`cargo test -p tiller_ui titlebar::` — 17 passed, 0 failed. `cargo test -p tiller_ui` (whole crate) —
321 passed, 0 failed.

### Live verification — partial, and why

Live-drove two back-to-back synthetic clicks at the same point on the empty drag area
(`Scripts/wayland-drive.sh …  'shot; click 800 19; click 800 19; shot'`); no panic, no crash, but no
visually confirmable change either way. This is a harness ceiling, not a null result on the app:
`wayland-drive.sh`'s virtual pointer issues two ordinary clicks with no guarantee they land inside
whatever timing window the (nested) compositor uses to synthesize a double-click, and — per this
wave's own `H3-tray-report.md` finding — this same nested `sway` compositor does not honor
`xdg_toplevel::set_minimized`, and a maximize toggle is invisible in a compositor that already sizes
the window to the virtual output. Confirmed the system half is real by round-tripping the actual
system setting (`gsettings set … minimize` / `… toggle-maximize`, restored to its original value
afterward) rather than asserting it from documentation alone. The gesture is real and covered by
name-exact drawn tests above; what the harness cannot add is a pixel-diff of a maximize/minimize that
this compositor cannot visibly perform in the first place.

Commit: `3cab2ee6 feat(F-WIN-09): follow the GNOME double-click-titlebar preference`

**howToExercise**: `cargo test -p tiller_ui titlebar::double_click` runs all five deterministic
proofs directly (parser, live-resolve, and the three configured-action drawn tests). Live, on a real
(non-nested) desktop session: set the preference (`gsettings set
org.gnome.desktop.wm.preferences action-double-click-titlebar minimize`), launch `tiller`, double-click
an empty stretch of its own titlebar (not a traffic light or cluster button) — the window should
minimize; restore the setting to `toggle-maximize` and repeat — the window should maximize/restore.

## `F-WIN-11` — the update toast's five user-visible states — **implemented**

`UpdateState`/`UpdateEvent` (`crates/tiller_project/src/ui.rs`) were untouched by this row — they
already existed, transitioned correctly, and were unit-tested. The only thing missing was a consumer.

`crates/tiller/src/main.rs`:

- `TillerWorkspace` gained an `update_state: UpdateState` field, `Idle` by default.
- New control-socket method **`update.event`** (added to `system.capabilities`'s advertised list) —
  `{"event": "<kind>", …}` where `<kind>` is one of `check-started` / `available` (+ `version`) /
  `download-progress` (+ integer `percent`) / `install-started` / `finished` / `failed` (+ `message`)
  / `reset`. It queues a new `ControlAction::UpdateEvent(UpdateEvent)`, the same fire-and-forget
  pattern `notify`'s `ControlAction::Notify` already uses; the app's existing control-action drain
  loop applies it with `workspace.update_state = workspace.update_state.clone().transition(event)`.
  This is the "cheapest honest test source" the row asked for — no fake Linux update transport was
  invented to drive it.
- New `render_update_toast`, wired into the same render tree as the existing `render_toast` (a
  distinct `top-right` floating panel, `id("update-toast")`, so it never collides with the existing
  bottom-right transient notice). Renders every state but `Idle` (which is silence, matching the
  reference `UpdateToastView`'s `.idle, .checking, .upToDate: EmptyView()` — except this row's own
  contract explicitly names `UpToDate` as one of the five visible states, so unlike the Swift
  reference it *does* render here): `Checking` ("Checking for updates…"), `Available` (message +
  muted "Download" label), `Downloading` (message + a real, width-driven progress bar clamped the
  same way `UpdateState::transition` clamps it), `Installing` ("Installing update…"), `UpToDate`
  ("Tiller is up to date"), `Failed` (message + muted "Retry" label).
- `Download`/`Retry` render **muted and inert** rather than fake-live: no Linux update transport
  exists yet to back them (the same reasoning `UpdateState`'s own doc-comment states), and this file
  already has an established convention for exactly this situation — `Titlebar`'s
  back/forward/`+`/history cluster seams render muted until a host wires a real handler, rather than
  ship a live-looking dead control. `Dismiss` (the `×`) is the one action every state actually
  supports and is fully wired, to a real `UpdateEvent::Reset` transition via the new
  `dismiss_update_toast`.

### Tests — `crates/tiller/src/main.rs`

- `update_event_queues_the_matching_control_action` — every one of the seven event kinds round-trips
  through the real `AppControlHandler::handle` and is asserted to queue the exact matching
  `UpdateEvent`; also asserts the three ways to submit a bad request (`available` without `version`,
  a non-integer `percent`, an unrecognized event name) are rejected, and that no rejected request
  queues an action.
- `drawn_update_toast_renders_every_state_and_dismiss_resets_it` — drives all six non-`Idle` states
  through the real `.transition()` call, asserting the toast draws for every one of them (and draws
  nothing for `Idle`), separately confirms the progress bar itself draws under a clamped-to-100%
  `Downloading` state, then clicks the real `update-toast-dismiss` control and asserts both that the
  toast un-draws and that `update_state` is back to `Idle`.

`cargo test -p tiller update_event` / `drawn_update_toast` — both pass. Full crate:
`cargo test -p tiller -- --skip shutdown_terminates_a_job_control_child_that_detached_into_its_own_process_group`
— 158 passed, 0 failed (up from 156).

### Live verification (real COSMIC nested-Wayland session, `wayland-drive.sh`)

All in one invocation per capture (state does not survive across separate invocations):

1. `ctl update.event event=available version=1.4.0` then `shot` — toast reads **"Tiller 1.4.0 is
   available"** with a "Download" label and a dismiss `×`, top-right, exactly where the new render
   code places it. (`/tmp/h4win11/02-shot.png` from this session.)
2. A second invocation: `ctl update.event event=download-progress percent=57` then `shot` — toast
   reads **"Downloading Tiller… 57%"** with a visibly filled progress bar under it.
   (`/tmp/h4win11b/02-shot.png`.)
3. Same invocation, `click` at the drawn dismiss control's pixel position, then `shot` — the toast is
   gone from the capture. (`/tmp/h4win11b/03-shot.png`.) This is the real control-socket → real
   render → real click path, not a unit-level proxy for it.

Commit: `b1f719c9 feat(F-WIN-11): build the update-toast consumer for UpdateState`

**howToExercise**: over the control socket (`tillerctl` or `Scripts/control-probe.py`), send
`update.event event=available version=1.4.0` — a toast reading "Tiller 1.4.0 is available" appears
top-right; `update.event event=download-progress percent=57` — the toast switches to "Downloading
Tiller… 57%" with a filled progress bar; clicking the toast's `×` (drawable at `update-toast-dismiss`)
makes it disappear and resets state to `Idle`. `cargo test -p tiller drawn_update_toast_renders_every_state_and_dismiss_resets_it`
runs the same sequence as a deterministic drawn test.
