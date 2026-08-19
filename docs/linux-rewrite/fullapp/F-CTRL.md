# F-CTRL — control socket (34 rows) — independent finish-line critic pass

Fresh critic, no prior context trusted. Driven live against the warm build
(`/dev/shm/tt/debug/tiller` + `/dev/shm/tt/debug/tillerctl`) over ten separate
`Scripts/wayland-drive.sh` invocations (labels `ctrlA` through `ctrlI`, plus one
direct headless launch for the no-socket-override half of WIRE-04), all under
`/dev/shm/sweep-23-F-CTRL`. Fixtures at `/dev/shm/ctrlA-fixture{,2}` and
`/dev/shm/ctrlA-nongit` (throwaway git repos). Every previous verdict in
`INVENTORY-LEDGER.md` for this section was PASSED; this pass re-drove every
row rather than trusting that, and found five real, reproducible defects the
prior pass's evidence did not surface (they were tested only against
control-socket-created panes, which happen to be the one class of pane this
bug does not affect) plus two contract-mismatch findings (CLI-01's browser
subcommands, BROWSER-01's capabilities set) and two genuine
harness/environment limitations (desktop-notification delivery, the browser
webview's GPU child) that I did not let masquerade as product defects.

## Table

| id | verdict | evidence |
|---|---|---|
| `F-CTRL-WIRE-01` | PASSED | ~150+ live NDJSON round trips across 10 separate app launches today, all correctly framed/id-echoed (`ctl system.ping` etc., e.g. run1.log lines 4,12,75-79). |
| `F-CTRL-WIRE-02` | PASSED | Socket confirmed `srw-------` (0600) at `/tmp/ctrlA.sock`. Malformed JSON → `{"ok":false,"error":"malformed request: key must be a string at line 1 column 2"}`. Refined oversize test (run7.log): server replies `{"ok":false,"error":"request line too large"}` before the connection drops. 10/10 concurrent clients served correctly (run2.log: `CONCURRENT-OK-COUNT-FIXED: 10 of 10`; my first attempt showed 0/10 in run1.log purely from my own harness string-match bug — `"pong":true` vs the real `"pong":"true"` — not an app defect, confirmed by rerunning the identical scenario). Split-across-reads assembled correctly (`SPLIT-RESP` in both run1 and the earlier check). |
| `F-CTRL-WIRE-03` | PASSED | Every one of the ~150+ calls above opens a fresh connection (the `ctl` helper and my own python helpers both `connect()` once per call) and all succeed; concurrent clients (10/10) prove one connection is never blocked by another; split-across-reads assembled (see WIRE-02). |
| `F-CTRL-WIRE-04` | PASSED | With `TILLER_SOCKET`: every lane above. Without: direct headless launch (`env -u TILLER_SOCKET -u WAYLAND_DISPLAY -u DISPLAY XDG_RUNTIME_DIR=/dev/shm/ctrlwire4-xdg timeout 4 /dev/shm/tt/debug/tiller`) created `/dev/shm/ctrlwire4-xdg/TillerRust/control.sock` on its own, and `tillerctl --socket` against exactly that resolved path replied `pong` — a genuine, display-independent live round trip. |
| `F-CTRL-PANEL-01` | PASSED | `panel.create` with `cmd=` and without both returned live ids (run1.log lines 36-38: `pane-3070238-1`/`pane-3070238-2`); no-cmd pane titled `"Panel"` (default). `grep -rn legacy` across `tiller_control`/`tiller/src/main.rs` is empty — no legacy-mode concept exists, so that half is correctly N/A. |
| `F-CTRL-PANEL-02` | PASSED | `panel.split from=<ctrl-owned id> direction=right` returned a new id, appeared in `panel.list` (run1.log lines 39-41). |
| `F-CTRL-PANEL-03` | PASSED | `panel.list` called 15+ times across create/split/close/write-attempt sequences over 10 lanes; rows matched live state every time (new panes appear, closed panes vanish, `active` flag tracked). |
| `F-CTRL-PANEL-04` | **FAILED — defective** | `panel.write id=<UI-created pane> input=...` returns `{"ok":false,"error":"unknown pane: pane-0"}` — reproduced 4 times across 2 separate app processes (run1.log lines 28,32; run2.log line 22) against a pane that `panel.read` on the *identical id*, moments before and after, reads correctly (run1.log lines 26,30,34; run2.log line 20). Root cause confirmed in `crates/tiller_control/src/panel.rs`: `PaneRegistry::write()` calls the private `get()`, which only ever looks in the `panes` map (panes the control socket itself created via `panel.create`/`panel.split`) — never in `external` (the renderer/UI-owned panes a human actually opens). `read()`/`list()`/`state()`/`scrollback()` all correctly check both maps; `read()` even carries a code comment (panel.rs:392-397) describing and fixing this exact class of bug for itself — the twin fix was simply never applied to `write()`/`key()`. `panel.write` against a pane the control socket created itself (e.g. `pane-3070238-*`) works fine, which is exactly what the prior PASSED evidence tested. |
| `F-CTRL-PANEL-05` | **FAILED — defective** | Same root cause (`key()` calls `write()` internally, panel.rs:386-389). `panel.key id=<UI pane> key=enter` → `unknown pane: pane-0` (run1.log line 29; run2.log line 24). Confirmed visually too: `05-04-after-write-key-attempts.png` shows the prompt still empty after the write+key attempts — nothing landed. All 9 symbolic keys *do* work correctly (byte-exact against `terminal_key_bytes()`, run1.log line 54: `cat` pane echoed the right escape sequences) — but only against a control-created pane, again the one case the prior pass tested. |
| `F-CTRL-PANEL-06` | PASSED | `panel.read` on a pane primed with `printf 'CTRL-MARKER-1-ctrlA'` decoded exactly (base64 payload in run1.log line 30 contains the pfetch banner + the marker, byte-verified). |
| `F-CTRL-PANEL-07` | **FAILED — defective** | Correct and well-behaved against control-owned panes: quick-exit (`exit 7`) wait returned `exitCode:7` in 0.030s; a `sleep 999` pane with a 1000ms budget timed out at 1.038s with `"pane timed out"`; an unknown id gave a distinct error (run1.log lines 56-66) — this much matches the prior PASSED evidence closely. But against the *same* UI-created pane the other four rows below fail on, `panel.wait id=pane-0 timeoutMs=500` also returns `unknown pane: pane-0` (run2.log line 26) — same root cause as PANEL-04/05 (`wait()` also calls the control-only `get()`, panel.rs:471-472). A real user's own terminal can never be waited on through this method. |
| `F-CTRL-PANEL-08` | **FAILED — defective** | `panel.focus id=<UI pane>` → `unknown pane: pane-0`, reproduced in both app processes (run1.log line 68; run2.log line 28). Root cause: `focus()` also calls the control-only `get()` (panel.rs:530-531). |
| `F-CTRL-PANEL-09` | **FAILED — defective** | `panel.close id=<UI pane>` → `unknown pane: pane-0` (run2.log line 30), and the pane is confirmed still present in the next `panel.list` (run2.log line 32) — the close silently no-ops rather than closing the tab. Root cause: `close()` (panel.rs:504-510) does a bare `self.panes.lock().remove(pane_id)` with **no** `external` fallback at all — not even the buggy indirection `get()` uses. `panel.close` against a control-created pane (what the prior pass tested — `CATID`/`SLOWID` in run1.log lines 71-73) does work correctly. |
| `F-CTRL-NOTIFY-01` | half-proven | Agent mode: `running`/`needs-input`/`done` → `queued:true`; `bogus-status-xyz` → `{"ok":false,"error":"notify has an unknown status"}` (run1.log lines 88-91). User mode (`ctl notify title=... body=...`) → `ok:true`, and the message lands in the same internal store `notification.list` reads (run1.log line 98 shows the DBUS-TEST title/body alongside two `notification.create` rows — confirming user-mode `notify` and `notification.create` share one store). Also independently exercised through a **real** Claude Code agent: clicking the tab-bar "+" → "Claude Code" (run4.log) launched a genuine `claude` process in a real UI pane and wrote `~/.local/share/TillerRust/bin/tillerctl`-backed hooks for `SessionStart`/`Notification`/`Stop`/`UserPromptSubmit`/`SessionEnd`, all using `--stdin-json` (see CLI-02). What I could **not** verify: the native desktop notification actually reaching `org.freedesktop.Notifications` over D-Bus. `dbus-monitor` on the lane's own private session bus captured nothing (`NO-DBUS-MATCH`, run1.log lines 93-94) — but I proved this is a harness limitation, not an app bug: run in isolation, `notify-send` itself (the exact binary `post_desktop_notification` in `main.rs:2287` shells out to) fails identically with `ServiceUnknown` on the same bus recipe, because the private `dbus-daemon` this lane spins up has no notification-daemon service registered at all — the method call never gets past `GetServerInformation`. This is exactly what `TILLER_WL_PORTAL`/`TILLER_WL_HOST_DBUS` exist to work around, and using the latter to reach a real notification daemon risks a real toast popping on the operator's live desktop, which I judged out of scope for this pass. |
| `F-CTRL-NOTIFY-02` | half-proven | `--stdin-json` proven twice: directly (`echo '{"session_id":"..."}' | tillerctl notify --session probe-pane --status running --stdin-json` → rc=0, run7.log) and via the real Claude Code hook command line generated in CLI-02 (which uses exactly `--stdin-json`). Ambiguous mode (`--session/--status` together with `--title/--body`) correctly rejected client-side: `tillerctl: ambiguous notify modes: choose either agent status (--session/--status) or user notification (--title/--body)`. No mode at all also errors. Not independently exercised: session-id extraction from a rollout-filename argument (the alternate path to `--stdin-json`) — my own attempt was malformed (omitted `--session`, which the CLI requires before it even reaches that extraction logic), so this sub-clause is genuinely unverified rather than failing. |
| `F-CTRL-SESSION-01` | PASSED | `session.ref session=pane-0 ref=real-ref-ctrlA-001` round-tripped, then overwritten via the CLI to `cli-ref-ctrlA`; independently confirmed durable by reading `/tmp/ctrlA.sqlite`'s `session_ref` table directly: `[('pane-0', 'cli-ref-ctrlA')]` present after the app process had already exited. |
| `F-CTRL-WORK-01` | PASSED | Comment set by UUID selector (`worktree.set worktree=<id> comment=...`), echoed correctly. Restart-persistence proven with a genuine full app-process kill+relaunch (same label = same sqlite DB, confirmed no stray process before relaunch): the fresh process's `workspace.list` showed `"comment":"CLI-COMMENT-ctrlA"` still on `p-8f4e7e1c070ed103-wt-0` (run2.log line 7), matching the durable value read directly from sqlite beforehand. |
| `F-CTRL-SYS-01` | PASSED | `ping` → `pong`; `capabilities` returned the full real method roster (63 methods today, e.g. run1.log line 13 — a few more than the ledger's "62", consistent with new methods like `tray.jump`/`git.branches`/`settings.account.*` having been added since, not a regression) plus `socketEnabled:true` and the real `socketPath`. |
| `F-CTRL-SYS-02` | PASSED | All four resolution branches driven live and separately: explicit `worktree=<id>` param (run10.log, resolved correctly with zero prior selection); `TILLER_WORKTREE_ID` env var (run1.log line 16, identical result to the selected-worktree case); no-context-with-current-selection fallback (run1.log line 15); true no-context on an empty instance → `{"ok":false,"error":"no current workspace"}` (run9.log). |
| `F-CTRL-WORK-02` | PASSED | `workspace.list` stayed correctly ordered and `selected`-flagged across many live project/worktree additions and selections throughout all 10 lanes (e.g. run1.log lines 115,118 show two projects' worktrees in stable, correct order with the right one flagged selected). |
| `F-CTRL-WORK-03` | PASSED | Explicit-branch create: real `git worktree add -b ctrla-explicit-branch` on disk, returned path under `/tmp/tiller-worktrees/...` (run1.log line 116). No-branch create: auto-generated `wt-1787156535`-style branch (line 117). Rejection against a genuine non-git directory: `{"ok":false,"error":"project ctrlA-nongit is not a Git repository"}` (line 120). |
| `F-CTRL-WORK-04` | PASSED | Selected by UUID and by exact path, `workspace.current` reflected each correctly (run1.log lines 122-123). No-selection error reproduced live on a fresh empty instance: `{"ok":false,"error":"no current workspace"}` (run9.log). |
| `F-CTRL-WORK-05` | PASSED | `workspace.close` on a worktree with a live real pane → `closed:true`; the next `panel.list` on that worktree errors `unknown worktree` (unmounted); `workspace.list` still lists the row, available to reopen (run1.log lines 105-107; reproduced again in run6b.log). |
| `F-CTRL-NOTIFY-03` | half-proven | `notification.create` (with and without `subtitle`), `notification.list`, `notification.clear` fully round-tripped live: 3 stored notifications with correct title/body/subtitle/date fields, then cleared to `[]` (run1.log lines 96-100). Same D-Bus-delivery caveat as NOTIFY-01 — could not verify the native toast for the reason documented there (harness bus has no notification daemon registered; independently confirmed `notify-send` alone fails identically). |
| `F-CTRL-SESSION-02` | PASSED | Clean reproduction with a genuine restart (run6b.log): fresh process auto-remounted the worktree's Terminal tab from the launch snapshot; `workspace.close` unmounted it (`closed:true`), `panel.list` then errored `unknown worktree`; `session.restore` → `{"restoredCount":"1"}` and a **new** pane (`pane-1`) appeared under the same tab; a second `session.restore` → `{"restoredCount":"0"}`, still exactly one panel (no duplication). Note: on the very first-ever launch of a brand-new DB (run1.log lines 108-109), `session.restore` instead errored `unknown worktree: /home/enzopalmisano/Scrivania/Progetti/tiller-linux` twice — this traces to the harness's own auto-registration of a `tiller` project (every sibling git worktree of this very checkout, a documented harness quirk, not something I created) apparently being part of that very first launch snapshot; it did not reproduce on the clean restart test above, so I record it as an open, unresolved oddity rather than a scored defect against this row. |
| `F-CTRL-BROWSER-01` | half-proven | The specific discrepancy the row describes (`browser.errors` accepted by the dispatcher but absent from `system.capabilities`) is confirmed live: `capabilities` lists 9 `browser.*` methods, `browser.errors` is not among them, and calling it directly returns `{"ok":false,"error":"browser.errors is unsupported on Linux: browser automation is not implemented"}` — dispatched, not turned away as unknown (run1.log lines 200,213). But the advertised 9 are **not** the same 9 the row's own contract text documents: source (`main.rs:236-246`, `BROWSER_CAPABILITIES`) swaps `browser.screenshot` out for a new `browser.permission` (added for F-PER-08, outside this row's original scope) — so `browser.screenshot` is dispatchable (confirmed, returns the correct "unsupported" response) but silently unadvertised, a second, different discrepancy than the one the row names. The ledger's "lists exactly the 9 documented browser.* methods" overstates this: it lists 9, but a different 9. |
| `F-CTRL-BROWSER-02` | PASSED | Fresh instance, zero `project.add` calls: `workspace.current` and `browser.open url=...` both return the identical `{"ok":false,"error":"no current workspace"}` (run8.log) — the row's inconsistency check closes cleanly. |
| `F-CTRL-BROWSER-03` | half-proven | `browser.open`/`browser.get` proven live and repeatedly: real `BrowserState` fields (`canGoBack`/`canGoForward`/`loading`/`title`/`url`/`error`) accurately reflect actual (persistently-loading) page state across three separate attempts (run1, run2, run3 logs). `browser.navigate`'s URL-navigation form also proven (run1.log line 215). Not exercised: the `action=back/forward/reload` form of `browser.navigate` — I only ever passed a bare `url=`. |
| `F-CTRL-BROWSER-04` | half-proven | `browser.screenshot` correctly returns the by-design "unsupported" error (run1.log line 212) — that half is solid. `browser.snapshot` failed every time I tried it (`"browser.snapshot failed: Browser child is unavailable"`, run1/run2/run3 logs) rather than returning real structural data as the ledger claims. See the environment note under BROWSER-06 — I believe this is GPU/EGL-related on this host today, not proof the feature regressed, but I could not reproduce the prior pass's success. |
| `F-CTRL-BROWSER-05` | half-proven | `browser.act verb=click selector=a` failed every attempt with `"Browser child is unavailable"` (could not reproduce the ledger's real click-to-iana.org). `browser.wait` does correctly report `timedOut` state for reachable timeouts — but a genuinely useful separate finding: a `timeoutMs` above the app's own `CONTROL_ACTION_TIMEOUT` (hardcoded 5s, `main.rs:208`) can never be honored over the socket — `browser.wait timeoutMs=25000` (run3.log) never got to return its own timeout semantics at all; the app's *dispatcher* killed the request at 5.0s first with a generic `"control action dispatch bound (5.0s) fired before the worker replied"`. Since `browser.wait`'s handler blocks the calling thread for its full requested duration (`main.rs:7067-7091`), any caller-supplied timeout over ~5s is silently unreachable — worth a row of its own if this section is revisited. |
| `F-CTRL-BROWSER-06` | half-proven | `browser.eval`/`browser.console` failed every attempt (3 separate app processes, `"Browser child is unavailable"`) — could not reproduce the ledger's real `document.title` evaluation. Environment evidence, not a guess: every one of these processes' own stderr shows the identical signature — `libEGL warning: failed to get driver name for fd -1`, `MESA: error: ZINK: failed to choose pdev`, `egl: failed to create dri2 screen` (checked in `/tmp/ctrlA.log`, `/tmp/ctrlC.log`) — i.e. no GPU/DRI device reachable from this nested headless compositor at all, which is exactly the resource WebKitGTK's renderer child needs to come up. Independently ruled out network/DNS as the cause: `curl https://example.com` from the same shell returned `200` in 0.15s. I disagree with treating this as a confirmed live regression — it reads as "could not verify, because this host's nested-Wayland harness has no GPU device available to the browser child today," not as proof the feature is broken. |
| `F-CTRL-CLI-01` | half-proven | Nearly the whole documented surface exercised live and matches exactly: `ping`, `capabilities --json`, `identify --json`, `project list/add`, `list-workspaces`, `current-workspace`, `panel create/write/read/wait/focus/close`, `list-notifications`, `session-ref`, `worktree-set`, all via the real `tillerctl` binary (run1.log lines 124-146). But the row's contract explicitly includes "...and browser subcommands" — `grep -n browser crates/tiller_control/src/bin/tillerctl.rs` returns **zero** matches, and live, `tillerctl browser` → `tillerctl: unknown command 'browser'` (run1.log line 147). The entire browser subcommand family the row promises does not exist in the CLI at all; every `browser.*` method is reachable only through the raw control socket. I disagree with the ledger's "Source matches documented subcommand list exactly" — it does not, on this one clause. |
| `F-CTRL-CLI-02` | PASSED | Shim confirmed installed and functional: `~/.local/share/TillerRust/bin/tillerctl` present (run4.log). Live UI action (tab-bar "+" → "Claude Code", correct coordinates confirmed via screenshot `06-05-new-tab-menu-open.png`) created a real agent pane (`pane-1`, `tab:"Claude Code"`, `agent:"claude"`) and wrote a genuine `settings.local.json` with exactly 5 hooks — `Stop`/`Notification`/`SessionStart`/`UserPromptSubmit`/`SessionEnd` — every one pointing at the exact shim path with `--stdin-json`. Screenshot `04-03-claude-code-clicked.png` shows a real `claude` process's own trust-folder prompt rendered in the pane (not a stub). I stopped short of clicking through that prompt to confirm the `SessionStart` hook actually *fires* end-to-end (time budget) — the generated command line and the live running process are both genuine, but I did not observe the hook's own execution, so that last sub-step rests on the correctly-generated config rather than an observed firing. |
| `F-CTRL-PLAT-01` | PASSED | `grep -rn "Darwin\|darwin" crates/tiller_control/src/*.rs` → zero real Darwin imports (the one `protocol.rs` hit is `cfg(target_os = "macos")`/`cfg(not(...))` gates, i.e. correct cross-platform branching, not a Darwin dependency). `cfg(unix)`/`std::os::unix` present in `client.rs`/`panel.rs`/`server.rs`. Binaries are real native Linux ELF (`file` confirms). Identical NDJSON contract carried all ~150+ live calls across 10 separate lane invocations today. |

## Defects (with reproductions)

### 1. `panel.write`/`panel.key`/`panel.wait`/`panel.focus`/`panel.close` don't recognize UI-created panes (F-CTRL-PANEL-04, 05, 07, 08, 09)

One root cause, five affected control methods. `crates/tiller_control/src/panel.rs`'s
`PaneRegistry` keeps two separate maps: `panes` (created by `panel.create`/`panel.split`
themselves) and `external` (the renderer's real, UI-visible panes, pushed in via
`set_external`/`set_external_state`). `read()`, `list()`/`list_for()`, and
`state()`/`scrollback()` all correctly consult both maps — `read()`'s own code comment
(panel.rs:392-397) documents having been fixed for exactly this reason. But the private
`get()` helper (panel.rs:603-611), used by `write()`, `key()` (via `write()`), `wait()`,
and `focus()`, only ever looks in `panes`; and `close()` (panel.rs:504-510) does a bare
`self.panes.lock().remove(...)` with no `external` branch at all. The practical effect:
none of these five methods work on a pane a human actually opened through the UI — only
on a pane the control socket created itself via `panel.create`/`panel.split`, which
(per the scout's own trap note) never even appears as a visible tab.

Reproduction (either lane's log reproduces this identically):
```
$ ctl panel.list worktree=$WSID
{"panels":[{"active":true,"agent":"","id":"pane-0","tab":"Terminal","title":"Terminal"}]}
$ ctl panel.read id=pane-0
{"ok":true,"result":{"data":"<real base64 scrollback>"}}
$ ctl panel.write id=pane-0 input=/dev/shm/ctrlA-scripts/marker1.sh
{"ok":false,"error":"unknown pane: pane-0"}
$ ctl panel.key id=pane-0 key=enter
{"ok":false,"error":"unknown pane: pane-0"}
$ ctl panel.wait id=pane-0 timeoutMs=500
{"ok":false,"error":"unknown pane: pane-0"}
$ ctl panel.focus id=pane-0
{"ok":false,"error":"unknown pane: pane-0"}
$ ctl panel.close id=pane-0
{"ok":false,"error":"unknown pane: pane-0"}
$ ctl panel.list worktree=$WSID   # pane-0 is still there — close silently no-op'd
{"panels":[{"active":true,...,"id":"pane-0",...}]}
```
Full transcripts: `/dev/shm/sweep-23-F-CTRL/run1.log` (lines 27-30, 68), `run2.log`
(lines 19-32), captured across two independent app processes so it is not a
one-off race. Visual corroboration: `05-04-after-write-key-attempts.png` shows
the prompt still empty — the attempted write never landed.

Impact: any external driver (this very `tiller`/`herdr`-style skill included) that
tries to read *and* write to, wait on, focus, or close a pane a user already has
open — the overwhelmingly common real case — silently cannot, and gets a generic
"unknown pane" that looks identical to a genuinely bad id.

### 2. `tillerctl` has no `browser` subcommands at all (F-CTRL-CLI-01)

The row's contract (and the ledger's own PASSED evidence text) both list "browser
subcommands" as part of what `tillerctl` exposes. `crates/tiller_control/src/bin/tillerctl.rs`
contains zero references to `browser` anywhere in its command dispatch. Live:
`tillerctl browser` → `tillerctl: unknown command 'browser'`. Every `browser.*`
method is reachable only by hand-building a raw socket request.

### 3. `system.capabilities`'s advertised `browser.*` set silently diverges from the row's documented set (F-CTRL-BROWSER-01)

`BROWSER_CAPABILITIES` (`main.rs:236-246`, what `system.capabilities` actually
advertises) is `open, navigate, act, get, wait, eval, console, snapshot, permission`.
The row's own contract text is `open, navigate, get, screenshot, snapshot, act, wait,
eval, console`. The difference is `screenshot` (documented, but now unadvertised —
though still dispatchable and correctly returns "unsupported") swapped for
`permission` (new, added under F-PER-08, not part of this row's original scope at
all). Functionally harmless today, but the ledger's "lists exactly the 9 documented
browser.* methods" is not accurate — it is 9 methods, a different 9.

### 4. `browser.wait`'s caller-supplied `timeoutMs` above ~5s is unreachable over the socket (F-CTRL-BROWSER-05, secondary finding)

`browser.wait`'s handler blocks the calling GPUI thread for up to its own
`timeoutMs` (`main.rs:7067-7091`), but every control action is separately wrapped
in a hardcoded `CONTROL_ACTION_TIMEOUT` of 5.0s (`main.rs:208`). A `timeoutMs`
request above ~5000 will have the *dispatcher* itself time the whole action out
first, replacing `browser.wait`'s intended `{"timedOut":true/false,...}` result
with a generic `"control action dispatch bound (5.0s) fired before the worker
replied"`. Reproduced live: `ctl browser.wait timeoutMs=25000` → the generic
dispatch-bound error at ~5s, not a `timedOut` result (`run3.log`).

## Could not verify (harness/environment limitations, not recorded as defects)

- **Native desktop-notification delivery** (F-CTRL-NOTIFY-01, F-CTRL-NOTIFY-03):
  `dbus-monitor` on the lane's own private session bus never saw a `Notify` call.
  Root cause confirmed independent of the app: the private `dbus-daemon` this
  harness spins up has no notification-daemon service registered at all, so
  `notify-send` (the literal binary `main.rs:2287` shells out to) fails the same
  way in isolation with `ServiceUnknown` before it ever reaches `Notify`. The
  socket/store side of both rows (queued/created/listed/cleared) is fully proven
  live; only the D-Bus hop is unverifiable in this harness's default bus recipe.
- **`browser.eval`/`browser.console`/`browser.snapshot`/`browser.act`**
  (F-CTRL-BROWSER-04/05/06): every attempt failed with `"Browser child is
  unavailable"`, correlated 100% with an identical, reproducible EGL/DRI failure
  in the app's own stderr on every one of 3 separate launches (`libEGL warning:
  failed to get driver name for fd -1`; `MESA: error: ZINK: failed to choose
  pdev`). Network was independently confirmed fine (`curl` to the same URL,
  200 in 0.15s). This looks like "no GPU device reachable from this nested
  headless compositor today," which WebKitGTK's renderer child needs to come up,
  not a demonstrated code regression — but I could not reproduce the prior
  pass's specific success (a real click landing on iana.org, a real evaluated
  `document.title`) and am recording that honestly rather than re-asserting the
  old PASSED.
- **F-CTRL-CLI-02's SessionStart hook actually firing**: the generated hook
  config and the live `claude` process are both genuine and confirmed, but I did
  not click through the real Claude Code trust-folder prompt to observe the hook
  execute (time budget) — a real gap in my own coverage, not a defect.
- **F-CTRL-NOTIFY-02's rollout-filename session-id extraction**: my own attempt
  to exercise this sub-path was malformed (omitted `--session`, which the CLI
  requires before reaching that logic); genuinely unverified rather than failing.
- **F-CTRL-BROWSER-03's `action=back/forward/reload` form of `browser.navigate`**:
  never exercised — I only ever drove the URL-navigation form.

## Notes

- The app auto-registers a `tiller` project (every sibling git worktree of this
  checkout) into any fresh DB the first time a project is added, purely because
  this harness launches from inside a real git worktree — a documented harness
  quirk (see the scout notes), not a fixture leak or cross-instance contamination.
  It is the likely source of a one-off `session.restore` error
  (`unknown worktree: /home/enzopalmisano/.../tiller-linux`) seen only on the
  very first-ever launch of a brand-new DB in `run1.log`; it did not reproduce
  on a clean restart-test in `run6b.log`, so it's recorded as an open oddity, not
  scored against F-CTRL-SESSION-02.
