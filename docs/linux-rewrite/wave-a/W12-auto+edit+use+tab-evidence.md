# Wave A slice W12-auto+edit+use+tab — evidence

Instance: `TILLER_WL_LABEL=wavea-W12-auto+edit+use+tab`, Wayland lane only.

## `F-AUTO-06` — Create, list, and clear delivered notifications through the control socket

**Claim: exercised-working.**

**Drove:** Started `dbus-monitor --session "interface='org.freedesktop.Notifications'"`
against the running instance's session bus, then over the control socket:
`notification.create {title:W12test, body:hello}` → `notification.list` →
`notification.clear` → `notification.list`.

**Observed:** `notification.create` returned `ok:true`; the subsequent `notification.list`
showed the entry; `notification.clear` returned `ok:true` and the following `list` was empty
— the documented round trip. Critically, the concurrent `dbus-monitor` capture recorded a real
`org.freedesktop.Notifications.Notify` method call carrying `"Tiller"`, `"W12test"`, `"hello"`
— i.e. `notification.create` now reaches the desktop notifier, which is exactly what the
triage's reclassify approach predicted (commit `a5d09d7` wiring `record_notification` to
`post_desktop_notification`). The ledger's "empty dbus-monitor" evidence is confirmed stale
against this build.

**Captures:** `reference/linux-progress/wavea-W12-auto+edit+use+tab/f-auto-06-dbus-monitor.txt`
(full dbus-monitor transcript, includes the `Notify` call body).

**Discriminating:** yes — a fresh dbus-monitor capture during a live `notification.create` call
is exactly the instrument the ledger's stale defect cited as empty; this run shows it non-empty.

## `F-AUTO-09` — Drive browser surfaces through the control socket

**Claim: exercised-working.**

**Drove:** Over the control socket: `browser.open url=https://example.com`, then
`browser.get`, `browser.screenshot`, `browser.snapshot`, `browser.wait`, `browser.eval`,
`browser.console`, `browser.act`.

**Observed:** `browser.open` returned `ok:true` with a real surface/url result (a browser
change). The other seven methods each returned `ok:false` with an explicit
`"<method> is unsupported on Linux: ..."` error — no bare `ok:true` with an empty/queued
result, which is what the ledger's stale defect cited. This matches the VERIFY clause's
"browser changes/results **or** explicit unsupported errors" exactly.

**Captures:** `reference/linux-progress/wavea-W12-auto+edit+use+tab/f-auto-09-browser-probe.txt`.

**Discriminating:** yes — the old defect evidence was a bare `{"ok":true,"result":{"queued":"true"}}`
for every method; this run shows a real result for `open` and typed errors for the rest.

## `F-EDIT-08` — Open the same document without creating duplicate visible editor state

**Claim: could-not-reach.**

**Drove:** Confirmed via `swaymsg -t get_tree` that the GPUI window held `focused: true`.
Started `dbus-monitor --session "interface='org.freedesktop.portal.FileChooser'"` against the
instance's session bus, then sent the documented shortcut (`main.rs:118`, `ctrl-o` →
`WindowCommand::OpenFile` → `handle_open_file` → `cx.prompt_for_paths`) directly via
`wtype -M ctrl -k o -m ctrl` (bypassing the lane script's single-key `key` helper, which has no
modifier syntax).

**Observed:** Zero `org.freedesktop.portal.FileChooser` D-Bus traffic in two separate attempts
(with and without the project already loaded), despite the binary linking `ashpd` (confirmed in
`Cargo.lock`) and the window reporting keyboard focus. This is consistent with either (a) the
picker genuinely not firing, matching the ledger's original defect, or (b) this lane's virtual
keyboard failing to deliver a modifier chord at all — `WAYLAND-LANE.md` documents "modifier
chords (including Shift+Tab)" as **not yet exercised** on this tooling, i.e. unproven in either
direction, distinct from named single keys which are proven (P112). I could not budget a
positive-control chord test (e.g. Ctrl+A in a focused text field) within a few minutes without
requiring the virtual pointer setup this run didn't start.

**Captures:**
`reference/linux-progress/wavea-W12-auto+edit+use+tab/f-edit-08-portal-monitor-attempt1.txt`,
`reference/linux-progress/wavea-W12-auto+edit+use+tab/f-edit-08-portal-monitor2.txt` (both empty
of FileChooser activity).

**Reason for could-not-reach:** Cannot distinguish "picker still doesn't fire" from "this lane
cannot deliver a Ctrl-chord at all" without a modifier-chord positive control, which needs the
DISPLAY=:1/X11 lane's real keyboard (as WAYLAND-LANE.md recommends for chords generally) or a
longer within-lane setup than this pass budgeted. The dedup *logic* itself
(`add_file_tab`, cited by triage as already correct) was not separately re-verified this pass.
