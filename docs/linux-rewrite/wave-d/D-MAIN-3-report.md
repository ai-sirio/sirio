# D-MAIN-3 report

Link 3 of 8 in the `D-MAIN` chain. Files owned: `rust/crates/tiller/src/main.rs` (plus a
one-line `rust/crates/tiller/Cargo.toml` dependency addition needed to implement one row).

## F-CTRL-BROWSER-02 — fixed

`browser.open` was the only surface-opening control method that did **not** require a current
workspace, so a genuinely fresh instance could get `workspace.current -> 'no current workspace'`
immediately followed by `browser.open -> ok:true` with a live surface — an inconsistent state
none of the other `surface.*.open` methods allow (they all check `has_current_worktree()` first).

Fix: `handle_browser_action` now returns `Err("no current workspace")` for `browser.open` before
creating the tab when `!self.has_current_worktree()`, matching `control_open_changes` /
`ChatControlAction::Open`.

`howToExercise`: on a fresh instance (no `project.add` yet), call `ctl browser.open url=https://example.com`
— it must now fail with `no current workspace`, the same error `workspace.current` already gives.
Then `project.add` + select a worktree, retry `browser.open` — it should now succeed.

Commit: `b88cad1`

## F-CTRL-BROWSER-03 — implemented (`browser.get`)

`browser.get` was rejected pre-dispatch by `browser_request_error`'s `BROWSER_CAPABILITIES` allow-list
(only open/navigate/act were in it), so it always produced a "not implemented" error and never reached
`handle_browser_action`.

Implemented a real `browser.get` arm returning `url`, `title`, `loading`, `canGoBack`, `canGoForward`,
`error` read straight off `BrowserState` (no navigation side effect) and added `"browser.get"` to
`BROWSER_CAPABILITIES` so it's no longer pre-rejected. Updated `system.capabilities`'s advertised
`browser.*` list and the `browser_methods_are_explicit_and_capabilities_are_truthful` unit test to match
(it now asserts `browser_request_error` returns `None` for `browser.get`/`browser.wait` instead of
asserting they're rejected).

`howToExercise`: `ctl browser.open url=https://example.com` then `ctl browser.get` — reply should carry
`url`/`title`/`loading=false` reflecting the live surface, not an `unsupported` error.

Commit: `4bb4ede`

## F-CTRL-BROWSER-05 (half) — implemented (`browser.wait`); `browser.act` verbs still absent

Same pre-dispatch rejection problem as `browser.get`. `browser.wait` now pumps the process-global
GTK main loop directly (`gtk::events_pending()` / `gtk::main_iteration_do(false)` in a bounded loop,
default 5s or `timeoutMs` param) so WebKit's async page-load callbacks — which `BrowserSurface`'s own
16ms timer task normally drives — actually get a chance to run instead of the call just idling until
timeout with a frozen `loading` flag. Returns `url`/`title`/`loading`/`timedOut`. Required adding
`gtk = "0.18.2"` to `crates/tiller/Cargo.toml` (already a dependency of `tiller_ui`, same version).

`browser.act`'s missing verbs (click/fill/type/press/scroll — everything but the `driving` flag) are
**not implemented**. Doing so needs a way to run JS/synthesize input against the real WebKit view,
which only `BrowserSurface`/`browser.rs` has access to (the `SharedWebView`/`wry::WebView` is private,
`main.rs` only sees `BrowserState` via `surface.state()`).

**wantedForeignFiles**: `rust/crates/tiller_ui/src/browser.rs` — add a public method on
`BrowserSurface`, e.g. `pub fn act(&self, verb: &str, selector: Option<&str>, text: Option<&str>) ->
Result<(), BrowserError>`, that builds and runs a small JS snippet via `webview.evaluate_script` (click:
`document.querySelector(sel).click()`; fill/type: set `.value`+dispatch `input` event; press: dispatch a
`KeyboardEvent`; scroll: `window.scrollBy`/`element.scrollIntoView`). `main.rs`'s `browser.act` arm can
then call it directly once that lands.

`howToExercise` (wait only): `ctl browser.navigate url=<slow-loading-or-any-url>` then
`ctl browser.wait timeoutMs=3000` — reply's `loading` should read `false` once the page settles
(vs. previously always erroring `unsupported`).

Commit: `4bb4ede` (same commit as BROWSER-03, both landed together since both routes through the
same `BROWSER_CAPABILITIES` gate).

## F-CTRL-BROWSER-04 — blocked (not attempted)

`browser.screenshot` and `browser.snapshot` need pixel/DOM capture from the live WebKitGTK view.
`wry` 0.56.1's safe public API (checked via `cargo doc -p wry`) exposes no screenshot/snapshot method;
capturing a frame would need the underlying `WebKitWebView*` (via `webkit2gtk`'s
`webkit_web_view_get_snapshot`) or an X11/GTK-level surface grab of the child window `BrowserSurface`
creates — either way, code that only `browser.rs` can reach (webview/window handles are private to
`BrowserSurface`). Genuinely out of scope for a `main.rs`-only row at this budget; not attempted rather
than faked.

**wantedForeignFiles**: `rust/crates/tiller_ui/src/browser.rs` — add
`pub fn screenshot_png(&self) -> Result<Vec<u8>, String>` (webkit2gtk `get_snapshot` async call bridged
to a blocking result, or a GTK `gdk::Window` pixbuf grab of the child Xlib window) and a
`pub fn dom_snapshot(&self) -> Result<String, String>` (an `evaluate_script` call returning
`document.documentElement.outerHTML` or an accessibility-tree walk). `main.rs` can then map both onto
`browser.screenshot`/`browser.snapshot` the same way `browser.get` maps onto `BrowserState`.

## F-CORE-SET-01 — already-correct, live-verified

No code change; wave-C's own note says 5 of the settings/clamp fields were already proven and code
was untouched since. The one specifically flagged as unreachable through `wayland-drive.sh`'s own
readiness gate is `TILLER_SOCKET_ENABLE=off`: the gate waits for the control socket to appear before
declaring the app ready, which can never happen when the override disables the socket — so the lane
itself can't prove this path without script changes I don't own (`Scripts/wayland-drive.sh`).

Verified live anyway, independent of the lane's readiness gate: ran the built binary directly
(`TILLER_SOCKET_ENABLE=off TILLER_WL_LABEL=setcheck1 Scripts/wayland-drive.sh /tmp/setcheck1-out '' 1`),
which fails the script's own gate with `FAIL: no control socket ... in 30s` (expected — no socket ever
appears) but its captured `$APP_LOG` shows `[control] disabled` — the app read the env override and
skipped starting the socket, exactly as `crates/tiller_project/src/settings.rs`'s
`"0" | "false" | "no" | "off" => self.control_socket_enabled = false` intends. No regression, no code
change; genuinely exercised this pass rather than left unchanged.

`howToExercise`: not driveable through `wayland-drive.sh`'s ctl loop (by design — no socket exists to
`ctl` against). Instead: `TILLER_SOCKET_ENABLE=off <binary or wayland-drive.sh>` and grep the app's
stdout/stderr log for `[control] disabled`, or confirm no `.sock` file appears where one otherwise would.

## F-CORE-WSP-04 / F-CORE-WSP-08 — blocked (not attempted)

Both concern `rust/crates/tiller_project/src/layout.rs`'s `LayoutCommand`/`classify_layout_command`
and `WorkspaceTabViewState`/`WorkspaceTab` types, confirmed still having zero callers outside
`layout.rs`/`lib.rs`'s re-export (re-grepped fresh from disk today, unchanged from the wave-C note).

This is not a small wiring gap: `layout.rs` is a complete, independently-tested (`F-CORE-WSP-05/06/07`
all PASSED) alternate domain model for workspace layout — groups/tabs/panes keyed by string ids, with
a `WorkspaceTabViewState` that carries editor caret/selection/scroll/folds, chat draft/attachments/
transcript/follows-tail, and terminal viewport per tab. `main.rs`'s actual tab/pane machinery
(`TabContent`, `PaneNode`, integer ids, ad hoc `schedule_save` calls scattered through split/close/
rename/activate handlers) is a separate, already-shipped implementation that doesn't share types with
it. Giving `classify_layout_command`/`WorkspaceTabViewState` a real caller honestly means either:

1. Routing every structural tab operation (split/close/move/rename/activate/divider-drag) through
   `LayoutCommand` construction + `classify_layout_command` to decide save/focus timing, replacing
   the ad hoc logic at each of those call sites, or
2. Adopting `WorkspaceTab`/`WorkspaceTabViewState` as the actual session-restore persistence format
   in place of whatever `tiller_persistence` currently persists (editor caret/scroll/chat draft
   restoration is not obviously covered by the existing restore tests I found —
   `restoring_launch_snapshot_adds_missing_tabs`, `restore_tabs_registers_restored_agent_identity` —
   which suggests this may be the intended destination, not a redundant parallel system).

Either is a real, multi-call-site refactor across a 12.8k-line file I don't have the budget to do
safely (and verify live) this pass. Reporting `blocked` rather than bolting on a token call site
that would make the grep pass without giving the critic anything real to exercise.

**wantedForeignFiles**: none — both files (`main.rs`, `layout.rs`) are already in this row's owned
list; the gap is scope/time, not access.
