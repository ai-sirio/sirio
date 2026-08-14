# T8-ctrl-sid — build-fleet plan (F-CTRL, F-SID)

Read-only triage. No code changed, no verdicts set. Each section names the row's current
ledger verdict, what it actually needs, the files a fix would touch, and the approach.

All line numbers below were re-checked against the current `linux/gpui-waku` HEAD
(`rust/crates/tiller/src/main.rs` is 12298 lines; it has moved substantially since several
ledger rows' evidence was recorded — cite the numbers here, not the ones in
`INVENTORY-LEDGER.md`, if the two disagree).

---

## Shared cause — the browser cluster's evidence predates its own fix

`F-CTRL-BROWSER-02` through `F-CTRL-BROWSER-06` (5 of my 14 rows) all carry evidence
timestamped **03:02:56** on 2026-08-14, captured against the pre-`P90` binary. Commit
`988d9e9` ("fix: make browser control responses honest", landed **04:29:51**, same day)
rewrote the entire `browser.*` dispatch — `ControlAction::Browser` gained a `reply` channel it
previously lacked (`rust/crates/tiller/src/main.rs:317-321`), `browser.open` now validates and
returns `surface`/`url`/`title` (`main.rs:4476-4491`), and every method the app does not
implement now fails **immediately and honestly** at `browser_request_error`
(`main.rs:201-239`, gated by `BROWSER_CAPABILITIES = ["browser.open", "browser.navigate",
"browser.act"]` at `main.rs:197`) rather than returning `queued:true` for a no-op. Only
`F-CTRL-BROWSER-01` was re-exercised after the fix (ledger, "orchestrator headless-lane probe,
2026-08-14 05:05") and flipped to PASSED. Rows 02-06 were never re-run — their evidence
literally describes code that commit `988d9e9` already deleted (the `main.rs:4638-4643`
seven-way no-op arm the ledger quotes does not exist anymore; I grepped for it and read the
current `handle_browser_action`, `main.rs:4469-4528` — the no-op arm is gone, replaced by the
pre-dispatch honest-error gate).

There is also a **standing product-policy answer**, not just a bugfix, that changes how these
rows should be read: `docs/linux-rewrite/tasks/P90-the-socket-that-says-yes.md`'s orchestrator
reply (2026-08-14 10:55) rules explicitly: *"the target state is not 'three methods forever'
... three methods that work and are advertised, seven that are visibly unimplemented."* That
is a deliberate scope ruling for `browser.get`/`screenshot`/`snapshot`/`wait`/`eval`/`console`
and for `browser.navigate`'s back/forward/reload and `browser.act`'s click/fill/type/press/
scroll — not an oversight. Each row below says which half of its clause that ruling now covers
and which half is still a real gap under the clause's literal wording.

**Fleet implication:** whoever re-verifies this cluster should re-run the exact `dbus-monitor`-
style socket probe fresh (all ten methods, current binary) rather than trusting any of 02-06's
recorded evidence, and should read the P90 policy answer before deciding whether the surviving
gaps are `FAILED` or `N/A — platform` under the new ruling.

---

## F-CTRL-BROWSER-02 — `browser.open`

**Ledger verdict:** FAILED — defective (evidence timestamp 03:02:56, pre-`P90`).

**needs: reclassify**

Both defects the row cites are gone in current code:
- "returns no surface identifier, URL or title" — false now. `main.rs:4482-4490` returns
  `surface`, `url`, `title` from the created `BrowserSurface`.
- "`ControlAction::Browser` ... no `reply` field, so no result can ever be returned" — false
  now. `main.rs:317-321` shows `Browser { method, params, reply }`, and the dispatch arm at
  `main.rs:2462-2472` sends `reply.send(result)`.
- "does not reject a missing URL — silently defaults to `https://example.com`" — false now.
  Rejected twice: pre-dispatch at `main.rs:209-216` (`browser_request_error`) and again inline
  at `main.rs:4477-4481`.

What the clause still asks that current code does not do: reject on **missing workspace
context** or an **unavailable adapter**. `add_browser_tab` (`main.rs:4419-4452`) creates the
tab unconditionally — there is no workspace-context check, and there is no "adapter"
concept for a native-WebKit browser surface on this platform (no `browser.rs` equivalent of
an agent-adapter registry to be "unavailable"). Whether that half of the clause even
translates to this platform is a judgment call for whoever reclassifies this, not something I
should resolve read-only.

**files:** `rust/crates/tiller/src/main.rs` (`browser_request_error` :201-239,
`handle_browser_action` :4469-4528, `add_browser_tab` :4419-4452, `ControlAction::Browser`
:317-321, dispatch arm :2462-2472)

**approach:** re-run the live socket probe (missing URL, present URL, — there is no
"workspace" or "adapter" parameter in the current wire protocol to even send) against the
current binary; if the fleet decides workspace-context validation is in scope, that is a
`needs: build` addition to `browser_request_error`/`handle_browser_action`, not a reclassify.

**size:** S (verification); the possible workspace-context addition, if wanted, is S-M.

---

## F-CTRL-BROWSER-03 — `browser.navigate` / `browser.get`

**Ledger verdict:** FAILED — defective (evidence timestamp 03:02:56, pre-`P90`).

**needs: both** (reclassify the stale half, build the real gap)

`browser.navigate` with a URL works today (`main.rs:4497-4510`, via `submit_address`) and
returns `url`/`title` — that part of the ledger's "implements none of the clause's three
operations" is stale. But back/forward/reload genuinely are not implemented: any `action`
param is rejected up front with an honest error (`main.rs:217-219`,
`"{method} action is unsupported on Linux: only URL navigation is implemented"`) — there is no
route to them at all. `browser.get` is entirely unimplemented — it fails at the
`BROWSER_CAPABILITIES` gate (`main.rs:197`, `main.rs:202-206`) before ever reaching a handler;
no selector/format/text/HTML extraction code exists anywhere in `BrowserSurface`.

**files:**
- `rust/crates/tiller/src/main.rs` (`browser_request_error` :201-239, `handle_browser_action`
  :4496-4527, `BROWSER_CAPABILITIES` :197)
- the `BrowserSurface` type itself (search `struct BrowserSurface` — not in `main.rs`; it's the
  browser surface crate/module `add_browser_tab` constructs at `main.rs:4428`) would need new
  methods for back/forward/reload and for reading page text/HTML if this is built for real.

**approach:** decide against the P90 policy ruling first — "visibly unimplemented" was an
explicit, accepted target state for `browser.get`. If the fleet wants literal clause
compliance instead, back/forward/reload is the smaller half (wraps existing WebKit
navigation-history calls, if the surface exposes them) and `browser.get`'s text/HTML
extraction is the larger half (needs a real DOM/text-extraction hook into the WebKit view).

**size:** navigate back/forward/reload: S-M. `browser.get`: M-L (depends on what the WebKit
binding already exposes — I did not chase that into the browser surface's own crate).

---

## F-CTRL-BROWSER-04 — `browser.screenshot` / `browser.snapshot`

**Ledger verdict:** FAILED — defective (evidence timestamp 03:02:56, pre-`P90`).

**needs: both**

Both methods are honestly refused today (`BROWSER_CAPABILITIES` gate, `main.rs:197`,
`202-206`) rather than the old silent `queued:true`/no file/no data — the ledger's specific
"wrote no file" / "no generation and no nodes" complaint described the old no-op arm, which no
longer exists. Under the P90 policy these two are explicitly in the "seven visibly
unimplemented" bucket. Under the clause's literal wording they are still not built at all —
zero screenshot-to-disk code, zero accessibility-node/generation code anywhere reachable from
`handle_browser_action`.

**files:** `rust/crates/tiller/src/main.rs` (`browser_request_error` :201-239,
`handle_browser_action` :4469-4528) plus the `BrowserSurface` module for the actual
screenshot/accessibility-tree capture, which does not exist yet.

**approach:** re-verify honestly-refused behavior live first (cheap, closes the stale-evidence
half). Building screenshot support needs a WebKit surface-to-PNG capture call; snapshot needs
an accessibility-tree walk — both are new capability surface on `BrowserSurface`, not just
socket plumbing.

**size:** L. Neither is "wire it up" — both need new capture machinery in the browser surface
that nothing in this codebase does yet.

---

## F-CTRL-BROWSER-05 — `browser.act` / `browser.wait`

**Ledger verdict:** FAILED — defective (evidence timestamp 03:02:56, pre-`P90`).

**needs: both**

`browser.act` today supports exactly one thing — a `driving`/`agentDriving` boolean flag
toggling `set_agent_driving` (`main.rs:4512-4523`) — and is honest about the limit: missing the
flag returns `"browser.act is unsupported on Linux: only the driving flag is implemented"`
(`main.rs:229-236`, `4516-4519`). Click/fill/type/press/scroll are not implemented at all —
the P90 orchestrator's own reply flags this ambiguity and asks the builder to say plainly
whether "driving" belongs with the honest three or the honest seven; as shipped it is
advertised (`BROWSER_CAPABILITIES` includes `browser.act`, `main.rs:197`) but only does the one
thing. `browser.wait` is entirely unimplemented (fails the capabilities gate, same as
browser.get/screenshot/snapshot) — no selector/text/URL/load-state/function condition is
parsed anywhere.

**files:** `rust/crates/tiller/src/main.rs` (`browser_request_error` :201-239,
`handle_browser_action` :4512-4527), plus `BrowserSurface` for real click/fill/type/press/
scroll input-injection and for any wait-condition polling loop — neither exists today.

**approach:** the "driving flag" ambiguity is worth flagging to whoever reclassifies this row
even before new code — P90's own author never definitively settled it. Building the five
click/fill/type/press/scroll actions needs synthetic-input injection into the WebKit view
(the same category of primitive `Scripts/wayland-virtual-pointer.c` provides for real OS input,
but this would be a programmatic/DOM-level injection, not an OS-level one). `browser.wait`
needs a poll loop against whichever of those becomes available.

**size:** L.

---

## F-CTRL-BROWSER-06 — `browser.eval` / `browser.console`

**Ledger verdict:** FAILED — defective (evidence timestamp 03:02:56, pre-`P90`).

**needs: both**

Same shape as -04: both honestly refused today via the capabilities gate rather than the old
silent no-op; the ledger's "the script is never read from params" / "the cursor is ignored"
complaints describe code (`main.rs:4638-4643` in the old evidence) that no longer exists.
Neither is built — no JS-evaluation bridge into the WebKit view, no console-message buffer
with a cursor.

**files:** `rust/crates/tiller/src/main.rs` (`browser_request_error` :201-239,
`handle_browser_action` :4469-4528) plus `BrowserSurface` for a `evaluate_javascript`-style
call and a console-message ring buffer with cursor support — neither exists.

**approach:** re-verify the honest-refusal half live first. Building `eval` needs whatever
`webkit_web_view_evaluate_javascript`-equivalent binding this project's WebKit wrapper exposes
(I did not chase into the browser surface's dependency to confirm it's already linked).
`console` needs a `console-message` signal handler feeding a bounded ring buffer, keyed by
cursor.

**size:** L.

---

## F-CTRL-CLI-02 — installed `tillerctl` symlink + real agent hook

**Ledger verdict:** NOT EXERCISED.

**needs: exercise**

The code this row asks about is real and already wired into the agent-spawn path, not stubbed.
`resolve_tillerctl_for_process` / `resolve_tillerctl_path` (`main.rs:7868-7911`) installs a
symlink at `$XDG_DATA_HOME/TillerRust/bin/tillerctl` (`TILLERCTL_INSTALL_SUBPATH`,
`main.rs:7858`) pointing at the running app's sibling `tillerctl` binary, falling back to a
`PATH` search, and is called from both agent-tab creation sites
(`main.rs:4601` and `main.rs:5061` — i.e. `add_agent_tab` and the second agent-spawn call site)
before a hook is ever written, so every agent adapter's hook config genuinely receives this
resolved absolute path. `install_tillerctl` (`main.rs:7948-7975`) does the actual
`symlink_metadata`/`std::os::unix::fs::symlink` work. Three tests already exercise
`resolve_tillerctl_path` directly (`main.rs:10225`, `:10264`, `:10280`), which is exactly why
the row can't be more than NOT EXERCISED off tests alone.

What's actually missing is the live proof P116 didn't do: (1) launch the real app, open an
agent tab, and inspect `$XDG_DATA_HOME/TillerRust/bin/tillerctl` on disk — confirm it's a
symlink and that it resolves and executes; (2) let a real agent (not a manual socket client)
invoke it from inside its own hook — e.g. drive a Claude Code pane to a real status
transition and confirm the Layer-A `tillerctl notify` call the hook fires lands over the
socket, the same way `F-CTRL-SESSION-01`/`F-CTRL-SYS-02` were already proven live per the
ledger.

**files:** `rust/crates/tiller/src/main.rs` (`resolve_tillerctl_path`/`resolve_tillerctl_for_process`
:7868-7911, `install_tillerctl` :7948-7975, call sites :4601, :5061) — no changes anticipated;
listed for the fleet's reference only.

**approach:** open an agent tab in the running app, `ls -la $XDG_DATA_HOME/TillerRust/bin/tillerctl`
to confirm the symlink and its target, then drive that agent to a real status transition and
confirm the resulting `tillerctl notify` call (fired by the agent's own hook, not a manual CLI
invocation) reaches the control socket.

**size:** S.

---

## F-CTRL-NOTIFY-03 — `notification.create` posts a real notification

**Ledger verdict:** FAILED — defective ("orchestrator, live D-Bus capture + code trace,
2026-08-14" — timestamp-less, but the evidence text is the pre-fix diagnosis).

**needs: reclassify**

**Shared cause with two sibling rows already re-verified after the fix — this is the strongest
lead in the whole group.** The ledger's own `F-CTRL-NOTIFY-03`/`F-AUTO-06`/`F-USE-06` rows were
all scoped together as "six ledger entries for one piece" in
`docs/linux-rewrite/tasks/P100-the-notification-that-never-arrives.md`. Commit `a5d09d7`
("fix: wire notification delivery and restored agents", 2026-08-14 13:42:52) fixed both
defects that doc names: (1) `record_notification` now calls `(self.notification_poster)(...)`
(`main.rs:717-744`, the call is at `:737-742`), where `notification_poster` defaults to the
real `post_desktop_notification` → `notify-send` path (`main.rs:693`); (2) both restore paths
(`restore_tabs` and `restore_tabs_in_workspace`, `main.rs:7581` and `:7703`) now call
`register_restored_agent` → `activity.register_agent_id` (`main.rs:7567-7575`, call sites at
`:7608` and the equivalent line in the second function) instead of leaving `pane_agents`
unpopulated after a relaunch.

**Two of the six sibling rows already got fresh post-fix evidence and it confirms the fix
works live**: the ledger's current `F-CORE-ACT-19` and `F-CORE-ACT-20` entries both cite
"live D-Bus Notify" captures from **P120** (after `a5d09d7`) — ACT-19 shows a real `Notify`
call reaching the session bus (it now fails only on title *wording*, agent+status vs.
agent+worktree-label — a content bug, not a missing-delivery bug), and ACT-20 shows the
visible/hidden suppression gate working both directions live. Both use the exact same
`post_desktop_notification`/`notify-send` pipe `record_notification` now also calls. Nobody
went back and re-ran the `dbus-monitor` capture across `notification.create` specifically
(the technique P120 already used for the sibling rows) — `F-CTRL-NOTIFY-03`, `F-AUTO-06`, and
`F-USE-06` still carry pre-fix evidence.

**files:** `rust/crates/tiller/src/main.rs` (`record_notification` :717-744, `notification.create`
handler :1378-1386, `post_desktop_notification` :1604-1613) — no changes anticipated.

**approach:** repeat P120's exact technique — `dbus-monitor --session
"interface='org.freedesktop.Notifications',member='Notify'"` on the headless lane, call
`notification.create` with a title/body, and confirm a `Notify` call lands with that title/
body. Given the identical code path already proved live for `F-CORE-ACT-19/20`, this is very
likely a quick flip to PASSED rather than more building — but it needs the same live capture,
not an inference from a sibling row.

**size:** S (verification only, on current evidence).

---
