# W11-ctrl+brw+sid — evidence log

## F-CTRL-NOTIFY-03 — ledger line 442, currently FAILED — defective

**Drove:** Wayland lane (`wavea-W11-ctrl+brw+sid`). Started `dbus-monitor --session
"interface='org.freedesktop.Notifications',member='Notify'"` in the background, then sent
`ctl notification.create title=W11NOTIFYTEST body=W11NOTIFYBODY` over the control socket.

**Observed:** the control reply was `{"id":"drive","ok":true,"result":{}}` (unchanged from
prior evidence — `create` still reports success unconditionally). But this time the
`dbus-monitor` capture, running concurrently, recorded a real `Notify` method call:

```
method call ... path=/org/freedesktop/Notifications; interface=org.freedesktop.Notifications; member=Notify
   string "Tiller"
   ...
   string "W11NOTIFYTEST"
   string "W11NOTIFYBODY"
```

This is the exact conjunct the ledger recorded as false ("the posting conjunct never happens").
It now happens: `notification.create` reaches the D-Bus `Notify` call with the title/body I sent,
confirming commit a5d09d7 wired `record_notification` to `notification_poster` as triage
described.

**Claim:** exercised-working (reclassify confirmed — the defect described in the ledger no
longer reproduces; live D-Bus capture with a positive discriminator (my own title/body strings)
shows the posting conjunct now firing).

Captures: `reference/linux-progress/wavea-W11-ctrl+brw+sid/notify03/01-baseline.png`,
`02-after-create.png` (chrome only — no visible desktop-notification UI in this headless
compositor, D-Bus capture is the load-bearing evidence).

## F-CTRL-BROWSER-02 — ledger line 445, currently FAILED — defective

**Drove:** Wayland lane. `ctl browser.open url=https://example.com/w11test`, then
`ctl browser.open` (no url param) to probe validation, then a third call with a url but no
prior `project.add` in that run to probe workspace-context gating.

**Observed:**
- `ctl browser.open url=https://example.com/w11test` -> `{"ok":true,"result":{"surface":"surface:2","title":"","url":"https://example.com/w11test"}}`.
  Both of the ledger's named defects are gone: the reply now carries a real `surface`
  identifier and echoes the `url` I sent (title is empty because the page has not loaded yet —
  expected, not the defect described).
- `ctl browser.open` with no `url` -> `{"ok":false,"error":"browser.open requires a non-empty url"}`.
  The missing-url case is now rejected, not silently defaulted to example.com.
- A third call, `ctl browser.open url=https://example.com/noworkspace`, issued as the first
  request of a fresh instance with no preceding `project.add`, still succeeded
  (`{"ok":true,"result":{"surface":"surface:3",...}}`). Triage's approach note names exactly
  this as the still-genuine gap: no workspace-context/adapter-unavailable validation. Confirmed
  live — it is still absent.

**Claim:** exercised-broken. Both defects named in the current ledger text (no
surface/url/title in the reply; missing url silently defaulted) are fixed and reproduce fixed
live — that part of the verdict is stale. But the clause's workspace-context validation
requirement is still genuinely unmet: `browser.open` succeeds with no workspace/project
context at all. Reclassify the *named* defects, but the row is not clean.

Captures: `reference/linux-progress/wavea-W11-ctrl+brw+sid/brw02/{01-baseline,02-after-open}.png`,
`reference/linux-progress/wavea-W11-ctrl+brw+sid/brw02b/{01-baseline,02-noworkspace}.png`.

## F-CTRL-CLI-02 — ledger line 451, currently NOT EXERCISED

**Drove:** Inspected the on-disk installed tillerctl location directly (not the socket):
`ls -la ~/.local/share/TillerRust/bin/tillerctl` (the exact `TILLERCTL_INSTALL_SUBPATH =
"TillerRust/bin/tillerctl"` constant from `main.rs:7858`, joined to the real `XDG_DATA_HOME`
default resolution in `xdg_data_home_for`).

**Observed:**
```
lrwxrwxrwx ... tillerctl -> /home/enzopalmisano/Scrivania/Progetti/tiller-linux/rust/target/debug/tillerctl
```
A real symlink exists at the exact path `resolve_tillerctl_path` computes, pointing at the
real `tillerctl` binary built in this worktree — confirms `install_tillerctl` genuinely ran
against the shared `$HOME` this project uses (not a synthetic env) and produced a working,
resolvable symlink, satisfying the "inspect the installed XDG symlink on disk" half of VERIFY.

**Not reached:** driving a real agent CLI's own native hook (Layer A) to a status transition.
The agent-tab launch path (`NewTabAction::ClaudeCode`/`Codex`) is only reachable from the
command palette, which opens on `ctrl-shift-p` — a modifier chord. `WAYLAND-LANE.md` states
chords are not yet exercised on this lane and routes them to `DISPLAY=:1`; `panel.create`
(the one socket action that spawns an arbitrary command in a pane) does not go through
`AgentAdapter::prepare`, so a command spawned that way would not get worktree-local hook
config and would not be a genuine test of the hook path.

**Claim:** partially-exercised. The installed-symlink half is confirmed live and genuine.
The real-agent-hook half is could-not-reach on this lane: it requires a `ctrl-shift-p` chord
(or another UI path not available without a chord or right-click, both out of scope here) to
open an agent tab through the code path that wires hooks; route to the `DISPLAY=:1` lane.

No new captures for this row (filesystem inspection only).
