# D-MAIN-3 critic verdicts

Critic pass, independent of the builder. HEAD at verify time: `4560076` (post wave-D
integration). Instruments used: `Scripts/wayland-drive.sh` (label `d3wlcheck`/`d3wlcheck2`/
`d3wlcheck3`, sockets `/tmp/d3wlcheck*.sock`), a direct binary launch with
`TILLER_SOCKET_ENABLE=off` (label `d3setcheck`/`d3setcheck2`, no wayland lane), and source
inspection of `rust/crates/tiller/src/main.rs` / `rust/crates/tiller_ui/src/browser.rs`.

## F-CTRL-BROWSER-02 — PASSED

Live on a genuinely fresh instance (`d3wlcheck`, no prior `project.add`): `ctl workspace.current`
-> `{"ok":false,"error":"no current workspace"}`, immediately followed by
`ctl browser.open url=https://example.com` -> the same `{"ok":false,"error":"no current
workspace"}`. Previously (per the ledger's C-MAIN-2 evidence) `browser.open` succeeded here with a
live surface while `workspace.current` failed — a state now closed. Discriminating: the fresh-vs.
populated instance produces different, correct outcomes for the same call.

## F-CTRL-BROWSER-03 — PASSED

Live (`d3wlcheck2`, after `project.add` + `browser.open url=https://example.com`):
`ctl browser.get` -> `{"ok":true,"result":{"canGoBack":"false","canGoForward":"false","error":"",
"loading":"true","title":"","url":"https://example.com"}}` — a real `BrowserState` snapshot, not
the previously recorded `unsupported on Linux: not implemented` error. Discriminating against the
prior absent behaviour.

## F-CTRL-BROWSER-04 — FAILED — absent

Live (`d3wlcheck3`, same session): `ctl browser.screenshot` and `ctl browser.snapshot` both still
return `{"ok":false,"error":"... is unsupported on Linux: browser automation is not
implemented"}`. Source-confirmed: `BROWSER_CAPABILITIES` in `main.rs` still omits both methods
(only `open/navigate/act/get/wait/eval/console/permission` are listed), so they're pre-rejected
before `handle_browser_action` is ever reached. Matches the builder's own "blocked, not attempted"
claim exactly.

## F-CTRL-BROWSER-05 — half-proven

`browser.wait` no longer errors "unsupported": live (`d3wlcheck2`), `ctl browser.navigate
url=https://example.org` then `ctl browser.wait timeoutMs=3000` returned a real
`{"loading":"true","timedOut":"true",...}` reply, pumping the GTK loop as the report describes —
a genuine behaviour change from the recorded `FAILED — absent` baseline. But the row's specific
discriminating claim ("loading reads false once the page settles") could not be exercised: `loading`
stayed `true`/`timedOut` on every attempt, and source inspection of `tiller_ui/src/browser.rs`
shows why — `build_webview`/`build_production_webview` fail through both the direct-XCB and
XCB->Xlib adapter paths under Wayland (visible in the capture as the red "Direct XCB build failed
... Wayland(WaylandWindowHandle...)" banner), so no native `WebView` is ever constructed and no
page-load-finished signal can ever fire to flip `loading` false — a structural lane limitation
(`WAYLAND-LANE.md`'s "no webview content"), not something this row's fix controls. Confirming the
settle-to-`false` half needs the `DISPLAY=:1` lane, which was held by another agent
(`dm4crit-browser`) for the duration of this pass, so that half stays unexercised rather than
assumed. Separately, current `main.rs` (post this row, via a later `F-CTRL-BROWSER-06` commit) now
also implements `browser.act`'s click/fill/type/press/scroll verbs via `evaluate_script` — the
"verbs remain absent" half of this row's original claim is stale, but that fix is not attributed to
this row and does not change the wait-specific verdict above.

## F-CORE-SET-01 — PASSED

Live, direct binary launches (no wayland lane needed — `wayland-drive.sh`'s own readiness gate
can't reach this path, as both the prior critic and the builder note). Negative control:
`TILLER_SOCKET_ENABLE=off TILLER_DB=/tmp/d3setcheck.sqlite TILLER_SOCKET=/tmp/d3setcheck.sock
rust/target/debug/tiller` -> stdout `[control] disabled`, no `.sock` file ever created. Positive
control, same binary without the override: stdout `[control] listening on
/tmp/d3setcheck2.sock`, and `/tmp/d3setcheck2.sock` exists as a real socket file. The two runs
diverge exactly as `tiller_project/src/settings.rs`'s env-override parsing intends, and the
positive control proves the instrument (and the socket-creation path itself) is alive, not just
that the env var was read.

## F-CORE-WSP-04 — NOT EXERCISED

Re-grepped fresh from disk (`grep -n 'LayoutCommand\|classify_layout_command' rust/crates/tiller/src/main.rs`):
zero matches. `lib.rs` still only re-exports the type; nothing in `main.rs` or `session.rs`
constructs or classifies a `LayoutCommand`. Confirms the builder's "blocked" claim; there is no
live behaviour to drive because no call site exists. Unreachable, not merely unattempted this
pass — same as the prior two critic sweeps.

## F-CORE-WSP-08 — NOT EXERCISED

Re-grepped fresh from disk: `WorkspaceTabViewState`/`WorkspaceTab` have zero callers outside
`layout.rs` and its `lib.rs` re-export. No restore path, chat/editor/terminal view-state code in
`main.rs`, `session.rs`, `chat.rs`, `editor.rs`, or `file_view.rs` references either type. Confirms
the builder's "blocked" claim; nothing exists to exercise.
