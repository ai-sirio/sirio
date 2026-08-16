# F3-browser — re-verification verdicts

Re-judged live, from scratch, against a freshly rebuilt binary (`rust/target/debug/tiller`,
rebuilt 2026-08-16 09:39 — the worktree's `target/` was found missing at task start, apparently
mid-recovery by shared infrastructure; rebuilt with `cargo build -p tiller` before any drive).
Instrument: `Scripts/wayland-drive.sh` (nested headless Wayland compositor, `grim` captures,
persistent virtual pointer/keyboard) unless stated otherwise.

## `F-TAB-06` — ledger line 122

**Verdict: PASSED. `heldUp: true`.**

Live re-drive, label `f3crit03`. Single continuous session (`shot base` → `click` the tab-bar `+`
at (1289,48) → `shot menu-open` confirmed the New Tab dropdown with "New Browser" (globe icon) at
row (1043,143) → `click` it → `shot browser-tab` showed a new "Browser" tab in the tab strip with
a globe icon → `click` that tab to bring it forward → `shot browser-selected`).

`/tmp/f3crit03/02-browser-selected.png` (read directly, not just described) shows: the Browser tab
selected and highlighted in both the tab bar and the sidebar tree under the `linux/gpui-waku`
worktree; an address bar reading `https://example.com`; and the browser chrome's own
"Direct XCB build failed ... Wayland ... not supported" banner, which is the documented, expected
Wayland-lane limitation (chrome renders, page content does not — WAYLAND-LANE.md), not a defect of
this row. The row's VERIFY clause is tab creation only, which this drive proves end-to-end through
a real gesture, not a socket call.

## `F-CTRL-BROWSER-01` — ledger line 444

**Verdict: PASSED. `heldUp: true`** (the row's actual narrow clause holds), **but the prior
evidence text is stale in one respect** (noted below).

Live raw-socket probe against the freshly-built binary, connected to the still-running
`f3crit03` instance's control socket (`/tmp/f3crit03.sock`) with a Python client — not
`tillerctl`, and not a re-read of the earlier probe's saved output:

```
ADVERTISED browser.* methods (9): ['browser.open', 'browser.navigate', 'browser.act',
  'browser.get', 'browser.wait', 'browser.eval', 'browser.console', 'browser.snapshot',
  'browser.permission']
browser.errors -> {"ok": false, "error": "browser.errors is unsupported on Linux: browser
  automation is not implemented"}
browser.screenshot -> {"ok": false, "error": "browser.screenshot is unsupported on Linux:
  browser automation is not implemented"}
```

`browser.errors` is accepted by the dispatcher's method routing (the error text names the method
and gives a platform reason — the router's own signature for a recognized-but-unimplemented
browser method, not the generic "unknown method" text an unrecognized RPC gets) yet is absent from
the 9 advertised methods, and a `grep -rn browser rust/crates/tiller_control/src` I ran this pass
returns zero hits, so there is still no tillerctl builder command for it. Both are exactly the
discrepancy the row's clause asks to be recorded.

**What is now stale in the ledger's evidence text:** it describes every non-`browser.open` method
as a stub "honest refusal". That is no longer accurate — `browser.get`, `browser.wait`,
`browser.eval`, `browser.console`, `browser.snapshot` and `browser.permission` are now backed by a
real WebKitGTK webview (`tiller_ui/src/browser.rs`, landed under F-CTRL-BROWSER-03/04/05/06/
F-PER-08, after this row's 05:05 timestamp) and do real work, not `ok:true` no-ops and not
refusals either. Only `browser.errors` and `browser.screenshot` still refuse. This does not change
the verdict — the row's own clause is specifically about the `browser.errors`
accepted-but-unadvertised gap, and that gap is exactly what still reproduces live today — but the
surrounding prose in the ledger cell should not be read as a current description of the other
eight methods' behaviour.

## `F-GIT-CLONE-01` — ledger line 488

**Verdict: PASSED. `heldUp: true`.**

Live UI drive, label `f3crit07`, `+` → **Clone Repository…**. Per
`docs/linux-rewrite/tasks/P123-popover-click-routing.md` the overlay draws confined to the sidebar
column, not window-centered; I clicked/typed at its actual on-screen position, read from my own
screenshot, not the visually implied center.

- Typed a deliberately unreachable URL, `https://192.0.2.1/no-such-repo-f3crit.git` (TEST-NET-1,
  RFC 5737 — guaranteed not to route anywhere, so any progress is real, not a canned string). The
  form's "Destination" line reactively derived `/home/enzopalmisano/Scrivania/Progetti/no-such-repo-f3crit`
  from the URL live as I typed, confirming the URL is really bound to `CloneFormState`, not just
  echoed (`03-url-ready.png`).
- Clicked "Clone repository". The very next capture (`04-submit1.png`) shows the button disabled
  and the status line reading **"Cloning... 0%"** — `CloneStatus::Running { progress: 0.0 }` — a
  state the app cannot reach except by actually calling `clone_repository` and spawning a real
  `git clone` subprocess against that address. This is discriminating evidence a socket call or a
  static screenshot could not fabricate: the default/idle state is "Ready to clone", and nothing
  but a real dispatched clone reaches "Cloning...".
- Confirmed by source read this pass (not from memory) that `clone_repository` is still imported
  and invoked from `tiller_ui/src/project_forms.rs`, i.e. this is the same production path, not a
  test-only stub.

Getting to this frame took several retries: the popover's click-routing defect from P123
reproduced intermittently on this lane (a click that should focus the URL field or dismiss the
menu sometimes did neither, or a session restart between separate script invocations dropped
in-progress UI state, though the underlying project/worktree data persisted via its sqlite DB) —
none of that is claimed as a defect of this row; it is noted so the frames above are read as what
they are, the result of the attempt that got through, not the only attempt. Did not wait out the
TEST-NET connect timeout to watch the clone fail (git's own OS-level TCP timeout against a
black-holed address can run past a minute) — "Cloning... 0%" is already the discriminating frame
this pass rests on. The pass-12 unit test
(`clone_from_local_repository_reports_receiving_progress`) was read as background only, per the
rule that re-running a unit test is not independent re-verification — the live UI drive above is
what actually re-earns this PASSED.
