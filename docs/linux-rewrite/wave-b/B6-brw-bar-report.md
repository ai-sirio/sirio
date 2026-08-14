# B6-brw-bar — build report

Slice: `docs/linux-rewrite/wave-b/B6-brw-bar.md`
Files owned this wave: `rust/crates/tiller_ui/src/browser.rs`, `rust/crates/tiller_ui/src/status_bar.rs`
Branch: `linux/gpui-waku`, worktree `/home/enzopalmisano/Scrivania/Progetti/tiller-linux`

All 5 rows built. No foreign files needed — everything landed inside the two owned files.

## F-BRW-01 — native child window painted outside its pane

**Root cause confirmed, and it is the direction inversion triage suspected.** Read wry
0.56.1's own `WebView::set_bounds` for the WebKitGTK/X11 backend
(`webkitgtk/mod.rs:964-966`):

```rust
pub fn set_bounds(&self, bounds: Rect) -> Result<()> {
    let scale_factor = self.webview.scale_factor() as f64;
    let (width, height) = bounds.size.to_logical::<i32>(scale_factor).into();
    ...
```

This runs unconditionally, on *every* `set_bounds` call, regardless of whether the `Rect`
handed in is already `Logical` or `Physical` — for a `Logical` value `to_logical` is a
no-op `.cast()` (see `dpi-0.1.2/src/lib.rs:699-704`), for `Physical` it divides by
`self.webview.scale_factor()`, wry's *own* GTK-reported scale, not GPUI's. `browser.rs`'s
`native_webview_rect` (formerly at :1510) was calling `bounds.to_device_pixels(window
.scale_factor())` and tagging the result `Physical` — so every live `set_bounds` call
divided GPUI's already-correct layout size by GTK's scale factor a second time, on top of
the multiplication this file had already done with GPUI's own scale factor. Whenever those
two scale factors disagree (they do on this desktop — the observed 729/850 ≈ 679/792 ≈ 6/7
shrink is not reproducible from an integer GTK scale factor alone, confirming the extra
division was the live bug, not a magnitude/rounding error), the child lands at a uniform
fraction of the intended rect around the window origin — exactly the (331,114)/(729×679)
vs (386,133)/(850×792) evidence on record.

**Fix:** `native_webview_rect` no longer converts anything. It builds the `Rect` straight
from GPUI's `Bounds<Pixels>` using `LogicalPosition`/`LogicalSize`, the same unit the
webview's *initial* bounds already use in `build_webview`/`build_production_webview` above
it in the same file — one unit for every `Rect` this file constructs, and wry's own
internal `to_logical` becomes a guaranteed no-op instead of an uncontrolled second
conversion. `window.scale_factor()` is no longer read here at all (the `prepaint` window
param is now `_window`, unused).

- Files: `rust/crates/tiller_ui/src/browser.rs` (`native_webview_rect`,
  `NativeWebViewElement::prepaint`, imports, tests)
- Tests: rewrote `webview_bounds_convert_gpui_logical_pixels_to_device_pixels` →
  `webview_bounds_pass_gpui_logical_pixels_straight_to_wry` (asserts the `Rect` equals the
  input bounds, tagged `Logical`, no scale factor involved) and added
  `webview_bounds_clamp_to_a_minimum_visible_size` (a 0×0 bounds still produces a visible
  1×1 rect, preserving the old crash-avoidance behavior that used to live in the
  `i32::from(...).max(1)` calls).
- **howToExercise:** tab-bar `+` → **New Browser**. The WebKitGTK child's on-screen rect
  should now exactly coincide with its GPUI pane region — no sliver of sidebar visible
  through the left/top edge, no gap of unpainted background at the right/bottom edge of the
  pane. A pixel-scan (or screenshot diff) of the child's rect against the pane's layout
  bounds is the same method the FAILED evidence used; they should now match 1:1 instead of
  the ~0.857× shrink on record.

## F-BRW-02 — Back button changes model but never repaints

Confirmed exactly as triage described, by direct read: `go_back`/`can_go_back` and
`browser_button`'s enablement gate are both correct, and `on_back`/`on_forward` mutated
`self.state`/`self.address_editor` through `navigate_history` but never called
`cx.notify()` — unlike `on_address_key`'s "enter" branch, which does, right after calling
`submit_address`. Without a notify, GPUI has no reason to schedule a repaint, so the model
update (confirmed correct by the unaffected `navigation_history_controls_follow_real_page_events`
unit test) never reaches the screen.

**Fix:** `on_back`/`on_forward` now take `cx: &mut Context<Self>` and call `cx.notify()`
unconditionally after `navigate_history`, mirroring `on_address_key`. Updated both
`browser_button` closures in `render_toolbar` to pass `cx` through instead of discarding it.

- Files: `rust/crates/tiller_ui/src/browser.rs` (`on_back`, `on_forward`,
  `render_toolbar`'s back/forward `browser_button` closures)
- Tests: no new automated test — the bug is purely GPUI repaint scheduling, which the
  existing plain `#[test]`s on `BrowserState` (no live window) cannot observe either way;
  they continue to pass unchanged, confirming the underlying history model was never the
  problem.
- **Not independently verified live:** the manifest also asked to check whether the native
  webview itself actually re-navigates (`self.webview` populated, `wry::load_url`
  effective) if `cx.notify()` alone doesn't fully resolve it. That requires driving the
  live app, which this build pass does not do (a dedicated integrator runs the app after
  slices land). `load_url` is called unconditionally by `navigate_history` regardless of
  the notify fix, so nothing about this change touches that path either way.
- **howToExercise:** in a Browser tab, click an in-page link so the address field and page
  title change (e.g. to an iana.org subpage as in the FAILED evidence), then click `‹`.
  The address field and page title should now flip back to the previous URL/title
  immediately — previously they stayed frozen even though the button's hover state and
  focus-on-click both proved the click landed.

## F-BRW-03 — address field can only append, never replace

Confirmed the manifest's own explanation on read: `AddressEditor` (select_all, move_to,
replace_selection, click hit-testing in `AddressTextElement`) are all correctly
implemented — not stubs. The defect is `pump_web_events`'s `WebEvent::PageLoad(Finished)`
handler, which called `self.address_editor.set_text(self.state.address())`
**unconditionally on every Finished event**. WebKit's `load-changed` signal (wire wry
forwards as `PageLoadEvent::Finished`) fires once per top-level navigation *and* once per
sub-resource/iframe load that completes while the page is still settling — `set_text`
resets both the text and the caret to end-of-text (`AddressEditor::set_text` always calls
`move_to(text.len(), false)`), so any stray Finished event landing while the user is mid-edit
silently wipes their click-placed caret or ctrl+a selection back to the end. The net effect
matches the recorded evidence exactly: an early reset restores the old address and puts the
caret at its end: from then on every keystroke of the new URL is a `replace_selection` on an
empty (collapsed) selection, i.e. a plain append — producing old-address+new-address
concatenation.

**Fix:** added a field `address_focused: bool` on `BrowserSurface`, refreshed at the top of
`render()` (the only place a `Window` is available to call
`address_focus.is_focused(window)` — `pump_web_events` runs from a background
`cx.spawn` timer task with no `Window`). The `PageLoad(Finished)` handler now skips the
`set_text` resync while `address_focused` is true. `submit_address` (Enter) still sets the
text explicitly and is unaffected — it's the user's own authoritative action, not a
background resync.

- Files: `rust/crates/tiller_ui/src/browser.rs` (`BrowserSurface` struct + `new`,
  `render`, `pump_web_events`)
- Tests: no new automated test — reproducing the race needs a live WebKit child firing
  repeated `PageLoad(Finished)` events during a focused edit, which isn't reachable from
  the existing plain-`#[test]` `AddressEditor`/`BrowserState` unit tests (no live window,
  no webview). The two existing `AddressEditor` click/selection tests still pass unchanged,
  confirming the editor's own logic was never at fault, consistent with the manifest.
- **howToExercise:** in a Browser tab, let a page finish loading and wait a couple of
  seconds (so any trailing sub-resource `Finished` events have a chance to fire), then
  click into the address field mid-URL and type a character — it should insert at the click
  position, not jump to the end. Separately: press ctrl+a — the entire address text should
  highlight (selection-filled), then typing a replacement URL should *replace* it, not
  concatenate onto it (compare against the FAILED evidence's
  `https://www.iana.org/help/example-domainshttps://www.iana.org`).

## F-USE-01 — no manual refresh control in the status bar

Confirmed absent by read: `on_refresh: Option<Rc<dyn Fn()>>` and its builder existed
(`status_bar.rs:112-113,160-162`) but nothing in `impl Render for StatusBar` ever
constructed a control that called it — only the settings gear rendered.

**Fix:** added a sibling `icon_button("status-refresh", Icon::RefreshCw)` right next to
`status-settings`, using the same `icon_button`/`.on_click` pattern already in `render()`.
Its click handler calls a new `on_refresh_clicked` method which (a) invokes
`self.on_refresh` if a host has wired one up, exactly as the manifest specified, and (b)
independently — since no call site in `rust/crates/tiller/src/main.rs` currently wires
`.on_refresh(...)` at all (only `.on_settings(...)` is called there, confirmed by grep) —
also forces every provider segment to `Loading` immediately and spawns one extra,
interval-independent fetch/apply cycle, reusing the exact fetch calls already in
`ensure_refresh_task`. This makes the control genuinely functional today without
depending on a host wire-up that is out of this slice's owned files.

- Files: `rust/crates/tiller_ui/src/status_bar.rs` (`on_refresh_clicked` method, `render`)
- wantedForeignFiles note: `rust/crates/tiller/src/main.rs` (owned by B1 this wave, not
  touched) could additionally call `.on_refresh(...)` on each `StatusBar::new(...)` alongside
  its existing `.on_settings(...)` calls, e.g. to log/toast the manual refresh at the host
  level — purely additive, the button already works without it.
- Tests: existing `status_bar::tests` all still pass unchanged (see `testsRun`); no new
  automated test added — `on_refresh_clicked` spawns a real background fetch via
  `cx.spawn`/`background_executor`, which the crate's existing tests don't drive against a
  live provider CLI either (the drawn test only exercises `apply_preferences`, not a real
  fetch cycle).
- **howToExercise:** look at the bottom status bar. A second icon (circular refresh arrow)
  now sits immediately to the right of the settings gear. Click it: the provider segments
  (Claude/Codex/…) should briefly show their loading text (`Claude …` etc.) and then
  repopulate a moment later.

## F-USE-02 — no tooltip on any usage-bar segment

Confirmed by grep: zero `.tooltip(` calls anywhere in the crate before this change, and
`provider_segment` rendered the unavailable reason (or the loaded numbers) only as
permanent label text.

**Fix:** `provider_segment`'s div gained `.id(...)` (required — `.tooltip()` is on
`StatefulInteractiveElement`, which only stateful/`id`'d elements implement) and
`.tooltip(...)`, applied uniformly to all four segments regardless of state (the FAILED
evidence explicitly found the gap on an *enabled/loaded* segment's hover, not just an
unavailable one). The tooltip repeats the segment's own already-computed text — the same
string already handed to the `text!` child — via a small new `StatusBarTooltip` entity
(`Render`, themed div). This was the "repeats the visible reason" option the manifest
offered; it stays correct for every `ProviderUsageState` variant for free, since it's the
exact same string already proven correct by `unavailable_reasons_render_distinct_text`.

- Files: `rust/crates/tiller_ui/src/status_bar.rs` (`provider_segment` closure,
  new `StatusBarTooltip` struct + `Render` impl, `AnyView` import)
- Tests: existing `status_bar::tests` all still pass unchanged; no new automated test — a
  hover-triggered tooltip's *appearance* needs a live window with real mouse-hover timing,
  which this crate's test harness (plain `#[gpui::test]` + `VisualTestContext`, used for
  `debug_bounds` presence checks) doesn't drive.
- **howToExercise:** hover the mouse over any status-bar provider segment (e.g.
  `Claude 12% 5h · 10% wk` or an unavailable `Claude not found`) and hold still for about a
  second. A small tooltip bubble should appear near the cursor repeating that segment's
  exact text. Try both an enabled/loaded segment and — once F-SET/whatever unblocks the
  PATH-strip precondition F-USE-02 shares with F-USE-03 — an unavailable one, to confirm
  both cases render a tooltip now.

## Verification

```
cargo check -p tiller_ui   → clean (0 errors, 0 warnings in browser.rs/status_bar.rs;
                              1 pre-existing unrelated dead_code warning on
                              BrowserSurface::pump_task, not touched by this slice)
cargo test -p tiller_ui --lib browser::    → 9 passed; 0 failed
cargo test -p tiller_ui --lib status_bar:: → 4 passed; 0 failed
cargo clippy -p tiller_ui --lib            → 0 new warnings in either owned file
                                              (2 pre-existing warnings in browser.rs at
                                              lines untouched by this slice: an
                                              `unneeded unit expression` and the
                                              `pump_task` dead_code lint above)
```

No commits to `rust/crates/tiller/src/main.rs` or any file outside the two owned above.
