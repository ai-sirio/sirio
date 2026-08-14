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

## `F-EDIT-12` — Drag a file from the file explorer or changes list into a pane

**Claim: could-not-reach.**

**Drove:** Re-confirmed the tooling limitation cited by triage rather than re-driving:
inspected `Scripts/wayland-virtual-pointer.c` (already source-verified per the manifest) and
`Scripts/wayland-drive.sh`'s `pointer_command`/`click`/`move` helpers, which only expose an
absolute `move` and a hard-coded `move+press+release` `click` — no button-down-only or
motion-while-held primitive exists anywhere in this lane's input surface, so a drag (press,
move while held, release) cannot be composed here.

**Observed:** No new drive attempted; this row is out of reach by construction on this lane,
matching `WAYLAND-LANE.md`'s explicit "Pointer drags ... are not yet exercised" statement and
the manifest's own note that this needs `DISPLAY=:1`/X11.

**Captures:** none — no drive was possible.

**Reason for could-not-reach:** Wayland lane's virtual-pointer tooling has no button-down-only
or motion-while-held primitive; this is an X11-lane row per this assignment's explicit
Wayland-only scope.

## `F-USE-03` — See provider usage values, loading, stale, logged-out, and error displays

**Claim: could-not-reach** (missing half only — Loading/Stale; Loaded/logged-out already
confirmed per the ledger and not re-driven this pass).

**Drove:** Relaunched a fresh instance and captured the status bar within ~1-3s of the socket
appearing (`02-use-immediate.png`, 1715x972), aiming to catch the transient `Loading` state
before the first provider refresh settles.

**Observed:** A frame was captured, but this text-only lane has no OCR (`tesseract` is not
installed) and the measurement instrument available (`convert ... -format
"%[fx:standard_deviation*255] %[fx:mean*255]"`) reports region variance/brightness, not text
content — it cannot distinguish "Loading…" from "Signed in" from a stale/dimmed render of the
same string, since all three are non-empty text in the same region. Region stddev alone is not
a valid instrument for this row's discriminating question (which *word* is shown), so no claim
about Loading is defensible from this capture alone.

**Reason for could-not-reach:** No text-reading instrument is available to distinguish the
documented states from each other (only presence/absence of drawing, which was never in
question). Additionally, `Stale` requires a refresh to run past its timeout
(`claude.rs::TIMEOUT` = 25s, `codex.rs::TIMEOUT` = 15s) with a prior `Loaded` value already in
hand — a multi-tens-of-seconds live sequence with no way to verify its result once captured, for
the same OCR reason. Both are out of this pass's budget and this lane's instrumentation.

**Captures:**
`reference/linux-progress/wavea-W12-auto+edit+use+tab/02-use-immediate.png` (uninterpretable
without OCR — kept for a future pass with a vision-capable reader).

## `F-USE-06` — Receive a user notification when an agent status changes while its pane is not visible

**Claim: partially-exercised.**

**Drove:** Opened a Chat tab (`surface.chat.open`) in a running instance, cleanly quit
(`system.quit`, socket dropped), relaunched the app against the same `TILLER_DB` on the same
compositor, confirmed via `panel.list` the Chat pane (`pane-0`) restored non-active (Browser
was active — a genuine "backgrounded" pane), started
`dbus-monitor --session "interface='org.freedesktop.Notifications'"`, then sent
`notify session=pane-0 status=running` followed by `notify session=pane-0 status=needs-input`
over the control socket — exactly the transition the row's approach names (`should_notify`
rejects `new == Running`, so the second call, `Running -> NeedsInput` on an invisible pane, is
the one that should fire per `NotificationPolicy::should_notify` in
`tiller_activity/src/notification.rs:19-27`).

**Observed:** Both `notify` calls returned `{"queued":"true"}`. Zero
`org.freedesktop.Notifications.Notify` traffic followed. Reading `restore_tabs` /
`restored_chat_spec` (`main.rs:1618-1635`) explains why without needing to guess: the default
Chat tab this drive created has `tab.agent_id == None`, and `restored_chat_spec(None)` returns
`agent_id: None` — so `register_restored_agent` (`main.rs:7566-7574`) is called with `None` and
skips `activity.register_agent_id` entirely. `post_activity_notification` then early-returns at
its very first line (`self.activity.agent_id(&transition.pane_id)` is `None`). This is a correct,
uninteresting no-op for a pane that was never agent-identified in the first place — it does not
test the row's real claim, which is about a pane that **was** a real agent chat before restore.

**Not reached:** producing a restored pane with `tab.agent_id: Some(<agent>)` requires opening
an actual agent chat (e.g. via the in-app agent picker's "+" control), which has no control-socket
equivalent (`surface.chat.open` takes no agent parameter) and so needs a blind pixel-coordinate
GUI click sequence this pass did not budget time to locate without a vision-capable reader.

**Captures:**
`reference/linux-progress/wavea-W12-auto+edit+use+tab/f-use-06-dbus-monitor.txt` (empty of
Notify traffic, as predicted by the code trace above — not by itself proof of a defect).

**Discriminating:** the restore-and-background state was real (confirmed via `panel.list`
before/after quit+relaunch, not assumed), so this is a genuine negative result for the specific
pane tested, not a null instrument; it just tested a pane the code was never going to notify for.
