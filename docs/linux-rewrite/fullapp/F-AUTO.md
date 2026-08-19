# F-AUTO — Control socket automation, app tier (9 rows)

Fresh, independent critic pass. Live-drove the warm binary (`/dev/shm/tt/debug/tiller`) via
`Scripts/wayland-drive.sh` under label `critauto10`, across nine sequential invocations (the label
is reused deliberately between my own runs — each redrive is documented as killing/relaunching my
own prior instance, never another agent's). Fixtures: `/dev/shm/critauto10-fixture` (throwaway git
repo, primary worktree used for most rows) and `/dev/shm/critauto10-fixture2` (a second throwaway
repo, added mid-pass to get a clean, single-tab baseline for the session-restore row once the first
fixture had accumulated tabs from earlier rows). A local `python3 -m http.server` on
`127.0.0.1:8971` served a real, curl-verified page for the browser row. Screenshots referenced below
live under `/dev/shm/sweep-10-F-AUTO/` (not committed — filenames are reused across invocations by
design, so only the frames named uniquely below were re-inspected fresh; the ones I actually opened
and read are called out as such).

I did not replay the existing ledger verdicts. Two of them I disagree with, loudly, below.

## Verdict table

| row id | verdict | evidence |
|---|---|---|
| F-AUTO-01 | PASSED | Live click-driven round trip. `ctl surface.settings.read` baseline: `controlSocketEnabled:true`, `socketPath:/tmp/critauto10.sock`. `03-02-settings-general.png`: real General screen, "Control socket" toggle ON, "Socket path: /tmp/critauto10.sock" displayed. Clicked the toggle (1288,843) → `10-09-settings-toggled-off.png`: knob OFF, path label **still displayed**. Immediately after, `ctl system.ping` → `CTL-FAIL system.ping: [Errno 2] No such file or directory` (the socket file itself was unlinked, not just refused). Clicked the toggle again → `12-11-settings-toggled-back-on.png`: knob ON; `ctl system.ping` → `{"pong":"true"}`. |
| F-AUTO-02 | half-proven | `ctl panel.create worktree=<wt> cmd=/bin/sh` returns a real id (`pane-569928-1`) and the pane is real and live — confirmed via `panel.list` (shows it, `tab:"control"`) and, in the F-AUTO-03 exercise, via `panel.read` returning genuine shell output. But it produces **zero UI trace**: `04-03-new-terminal-clicked.png` (before) and `05-04-after-control-panel-create.png` (after) are pixel-identical — same single "Terminal" tab, no new tab, no sidebar change, nothing a person looking at the screen would ever see. The row's own VERIFY text offers two outcomes ("a terminal pane/tab is created, or a visible protocol error is returned") — neither literally happened: a real *pane* was created (satisfying the control-plane half), no *tab* appeared, and no error was returned either. See "Where I disagree" below. |
| F-AUTO-03 | PASSED | Full verb set exercised against a `panel.create`-made pane (`pane-610776-1`/`pane-652310-1`), which is what this row's own VERIFY text scopes to ("With a control-created pane…"). `panel.split from=<id> direction=right` → new pane `pane-610776-2`, tab `"control"`, title `"Split (right)"`, confirmed via `panel.list`. `panel.write id=<id> input=AUTOMARK_nospace321` + `panel.key id=<id> key=enter` → `panel.read` returned (base64-decoded) `"$ AUTOMARK_nospace321\r\n/bin/sh: 1: AUTOMARK_nospace321: not found\r\n$ "` — the exact marker, round-tripped cleanly. `panel.state` → `exitStatus:running`. `panel.focus` → ok. Wrote `exit` + Enter, `panel.wait id=<id> timeoutMs=4000` → `exitCode:0` (a real process exit, not a timeout). `panel.close` → ok, confirmed gone from a follow-up `panel.list`. |
| F-AUTO-04 | PASSED | All three sub-requests confirmed with real UI and on-disk state, not just `ok:true`. `ctl notify session=<UI-pane> status=needs-input agentSession=critauto10-sess-1` → `08-07-notify-needs-input.png`: the tab title gained a `?` glyph and the sidebar's `master` row grew a status dot — a real, unmistakable UI change. `ctl worktree.set worktree=<wt> comment=AUTOMARK_comment_9d1` → `09-08-worktree-comment.png`: the sidebar row now shows an `AUTOMARK_c…` badge next to "Primary". `ctl session.ref session=explicit-probe ref=AUTOMARK_explicit_ref_88a`, independently re-read straight from the live SQLite file afterward: `python3 -c "sqlite3... select session,reference from session_ref"` → `[('explicit-probe', 'AUTOMARK_explicit_ref_88a'), ('pane-0','critauto10-sess-1'), ...]` — both the explicit `session.ref` call and `notify`'s `agentSession` param landed in persisted state, not just memory. |
| F-AUTO-05 | PASSED | List/select/create/close all exercised with real, visible effect. `ctl workspace.create project=<id> branch=auto-critic-branch2` → real new git worktree; selecting it (`04-03-second-workspace-selected.png`) shows the sidebar highlight moved, the Files panel and status bar switched to the new path, center pane shows a clean "No Terminals" state for the new worktree. `ctl workspace.close workspace=<that-id>` while it was genuinely mounted → `05-04-second-workspace-closed.png`: center pane and Files panel both revert to "No worktree selected"; `workspace.list` afterward shows `mounted:false` for it. (My first attempt at this row closed a worktree that had never been mounted, which is correctly a no-op — that is a flaw in my first test, not the app; the mounted→closed transition above is the real exercise.) `system.identify` with no context, before any workspace existed, returned the documented explicit error `"no current workspace"`. |
| F-AUTO-06 | PASSED | `notification.list` → `[]` → `notification.create title=AUTOMARK_notif_title body=AUTOMARK_notif_body` → `notification.list` → one entry with the exact title/body → `notification.clear` → `notification.list` → `[]` again. Full round trip, exact content match, reproduced with distinct markers across two invocations. I attempted to also independently capture the real D-Bus `Notify` call via `dbus-monitor` on the harness's private session bus and could not get a clean capture in the time available (a backgrounding/timing issue in my own script, not something I'm recording as a defect) — the store-level delivery/list/clear evidence above stands on its own and is what the row's VERIFY text actually asks for ("confirm a notification is delivered, list it, clear…"). |
| F-AUTO-07 | PASSED | `system.ping` → `pong` (repeatedly, across every invocation). `system.identify` with no context before any workspace existed → explicit `"no current workspace"` error. `system.identify worktree=<wt>` → real `branch`/`path`/`workspaceId`. `system.identify pane=<UI-pane>` → real `branch`/`path`/`workspaceId`/`surfaceId`/`sessionRef`. `system.capabilities` → live table of 60+ methods (including all `browser.*` capability entries) plus `socketEnabled`/`socketPath`. |
| F-AUTO-08 | PASSED | Clean reproduction, after two of my own attempts were invalidated by test-setup mistakes (documented below so the next critic doesn't repeat them). Final run: activated the sole "Terminal" tab, clicked its close-x (447,51) → real "Close dirty tab?" dialog. Clicked Close (856,499) → `06-05-empty-state.png`: Terminal gone, only the two pre-existing Browser tabs remain. `ctl session.restore` → `{"restoredCount":"1", "path":"/dev/shm/critauto10-fixture"}` → `07-06-after-restore.png`: Terminal tab back, in both the tab strip and the sidebar. |
| F-AUTO-09 | half-proven | The row references an "enable the universal workspace" precondition that does not exist anywhere in this codebase (grepped the whole tree; nothing named `universal_workspace`/`UniversalWorkspace` exists) — `browser.*` is simply always present, gated only by an internal capability allowlist, not a setting. Treating the row as testing the `browser.*` socket surface directly: `browser.open`/`browser.navigate`/`browser.act(driving=true)` all produce real, correct, live UI changes — a genuine "Browser" tab with sidebar entry (`10-09-browser-open.png`), a real connection-refused error surfaced live in the tab when navigating to an unreachable address, and a real "Agent driving" pill when `driving=true` (`11-10-browser-act-driving.png`). `browser.screenshot`/`browser.errors` return explicit `"…unsupported on Linux"` errors; `browser.eval`/`console`/`snapshot` return explicit `"Browser child is unavailable"` errors — never a silent `ok:true`. **But**: for every URL I tried, reachable or not — including a `curl`-verified-working local `http.server` page — the Browser tab's content area shows nothing but a permanent red banner, "Direct XCB build failed: the window handle kind is not supported; XCB→Xlib adapter failed: GPUI returned unsupported handle: Wayland(WaylandWindowHandle{...})" (`08-07-browser-reachable-open.png`). Real page content never renders, ever, even though `browser.open`/`browser.navigate` report success. See "Where I disagree" below — this is the same, already-source-confirmed defect as `F-WIN-06`, reached here through the control socket instead of the `+` menu. |

## Where I disagree with the recorded ledger

- **F-AUTO-02** was **PASSED** in the ledger ("tillerctl panel create … returned a real pane id,
  confirmed present in a follow-up panel list"). That's true as far as it goes, but this section's
  own header frames F-AUTO as specifically "app-visible effects … as seen from the UI side,
  distinct from package-tier F-CTRL wire-protocol rows." A `panel.create`'d pane is invisible: no
  tab, no sidebar entry, nothing — a real live process a user cannot see or reach, running
  alongside whatever they actually have open. That's architecturally deliberate (see
  `rust/crates/tiller_control/src/panel.rs`'s own module doc: "The GPUI terminal renderer
  intentionally owns its alacritty terminal object privately. The control socket therefore keeps a
  small, independent PTY registry… without reaching into the renderer") — this is not a bug, it's
  a designed seam. But a wire-protocol-level PASSED is exactly what F-CTRL's copy of this row
  should say; for an *app-tier* row specifically about what a user watching the screen would see, I
  am downgrading it to **half-proven**. Reproduction: `04-03-new-terminal-clicked.png` vs
  `05-04-after-control-panel-create.png` — identical tab strips, before and after a `panel.create`
  call that itself reported `ok:true` with a real id.

- **F-AUTO-09** was **PASSED** in the ledger, on the strength of the control methods themselves
  behaving correctly (real changes or explicit typed errors, never silent `ok:true`) — which is
  still true, and which is why I did not fail this row outright. But a sibling critic's `F-WIN.md`
  (this same pass, different section) independently traced the *same* underlying defect to source:
  `rust/crates/tiller_ui/src/browser.rs`'s WebView embedding only ever handles `RawWindowHandle::Xcb`
  or adapts to `RawWindowHandle::Xlib`; every other handle kind — including
  `RawWindowHandle::Wayland`, which is exactly what GPUI hands back on a native Wayland session (the
  only mode this whole harness runs Tiller in) — falls straight into an unconditional
  `Err(format!("GPUI returned unsupported handle: {raw:?}"))`. There is no Wayland embedding path
  in that module at all, gated by `#[cfg(target_os = "linux")]` rather than by windowing system. So
  this is not a nested-compositor artifact and not something my harness manufactured: it is a
  permanent, by-construction gap that affects every native-Wayland Tiller session, reached here via
  `browser.open`/`browser.navigate` exactly as it is reached via the `+ → New Browser` menu item in
  F-WIN-06. I'm recording F-AUTO-09 as half-proven rather than mirroring F-WIN-06's
  "FAILED — defective" only because this row's own text is narrowly about the *control-socket
  surface's* behavior (which is honest and correct), not about page-render fidelity (which is
  F-BRW's and F-WIN's job) — but the gap is real, user-visible, and already confirmed at the source
  level, so a bare PASSED understates it.

## Notable behavior worth flagging (not a defect, but a real gotcha)

`session.restore` (F-AUTO-08) does not mean "undo the tab I just closed." Its `launch_snapshot` is
captured exactly once, at the moment the Tiller *process* starts, and `ControlAction::RestoreSession`
always re-selects whatever worktree was current *then* — even if the caller has since switched to a
completely different, unrelated worktree via `workspace.select`. I burned three invocations on this
before realizing it: switching to a freshly-added second project, closing a tab there, and calling
`session.restore` silently jumps the UI back to the *original* boot-time worktree and reports
`restoredCount:0` for the workspace you actually closed something in — not an error, just a report
of "nothing was missing from the snapshot I actually track," which is the *other* worktree. Once I
stopped switching worktrees before the close+restore sequence and drove it entirely within whatever
worktree was already current at boot, it worked exactly as documented (`restoredCount:1`, tab back).
An automation author scripting "close then restore" against a worktree they picked mid-session would
be surprised by this; worth a doc note near `session.restore` if one doesn't already exist.

## Harness notes (so the next critic doesn't re-spend the same effort)

- `ctl`'s own params parser (`Scripts/wayland-drive.sh`'s `ctl()` function) splits `CTL_ARGS` on
  **all** whitespace before splitting each token on `=`. A value containing a space (e.g.
  `input="echo AUTOMARK_x"`) silently loses everything after the first space — no error, just a
  truncated param. I hit this directly: an early `panel.write input="echo AUTOMARK_..."` only ever
  wrote `echo`. Not an app bug; use single-token values (or a separate `panel.key key=enter` call)
  when driving `panel.write` through this harness.
- A tab's close-x at a fixed pixel position (e.g. `(447,51)` for the first tab slot) is only
  reliably clickable when that tab is the **active** one. Clicking the same coordinate on an
  inactive tab (verified directly: a Browser tab was active, Terminal was first-but-inactive)
  just activates that tab instead of hitting a close affordance — no dialog, no error, easy to
  mistake for "the close silently did nothing." Click the tab body first to activate it, then its
  close-x.
- `ctl workspace.list`/`project.list`/`system.capabilities` responses can approach the `ctl`
  helper's 3000-character response truncation; none of my parses were actually cut short this pass,
  but it is close enough (the `system.capabilities` method table is ~2900 characters) that a future
  row needing more fields appended could be silently truncated. Worth knowing if a parse ever comes
  back with an unterminated JSON string.

## Not reached

Nothing in this section was left completely undriven — all nine rows were exercised live at least
once, several multiple times to correct my own test-setup mistakes. The one thing I did not manage
to independently reconfirm is the real D-Bus `org.freedesktop.Notifications.Notify` call underlying
`notification.create` (F-AUTO-06) — my `dbus-monitor` capture attempt raced its own backgrounding
and produced no output before the private bus connection dropped. The store-level evidence
(create → list → clear, exact content match) is solid and is what I've based the PASSED verdict on;
the D-Bus hop itself is simply unconfirmed by me this pass, not contradicted.
