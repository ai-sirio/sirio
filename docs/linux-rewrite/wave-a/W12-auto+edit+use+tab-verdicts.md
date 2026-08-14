# Critic verdicts — W12-auto+edit+use+tab (F-AUTO, F-EDIT, F-USE, F-TAB)

Adjudicated by a critic that neither drove nor built this slice. Driver return:
`docs/linux-rewrite/wave-a/W12-auto+edit+use+tab-evidence.md`, captures under
`reference/linux-progress/wavea-W12-auto+edit+use+tab/`. HEAD under test: `4073297`.

## F-AUTO-06 (ledger line 280) — verdict: `PASSED` (changed from `FAILED — defective`)

Opened `f-auto-06-dbus-monitor.txt` directly. It contains a genuine
`org.freedesktop.Notifications.Notify` method call with arguments `"Tiller"`, `"W12test"`,
`"hello"` — an exact match for the `notification.create {title:W12test, body:hello}` call the
driver sent immediately beforehand. `notification.list` showing the entry and `notification.clear`
emptying it are also on record. Independently re-read `record_notification`
(`rust/crates/tiller/src/main.rs:717-744`): it pushes to the in-memory `Vec` **and** calls
`(self.notification_poster)(...)` unconditionally — confirms the code the driver cites, not just
its description. Confirmed both `a5d09d7` (the cited fix) and `988d9e9` are ancestors of both HEAD
`4073297` and the current tip. The ledger's "empty dbus-monitor" defect basis is genuinely stale;
this is a live, discriminating, positive round trip.

## F-AUTO-09 (ledger line 283) — verdict: `PASSED` (changed from `FAILED — defective`)

Opened `f-auto-09-browser-probe.txt`: `browser.open` returns `ok:true` with a real
`{"surface":"surface:2","url":"https://example.com"}` result; the other seven calls each return
`ok:false` with a distinct, method-named `"<method> is unsupported on Linux: ..."` error — not the
old bare `{"ok":true,"result":{"queued":"true"}}` the stale ledger evidence cited. Independently
read `browser_request_error` and its call site (`main.rs:201-236`, `:1552-1564`): confirms
`BROWSER_CAPABILITIES = [open, navigate, act]` get real handling, the other 7 of 10
`BROWSER_METHODS` fall through to the generic unsupported-method error unconditionally, and `act`
additionally requires a `driving`/`agentDriving` param — exactly the shape the transcript shows.
Both fix commits (`988d9e9`, `a5d09d7`) confirmed ancestors of `4073297`. This satisfies the VERIFY
clause's "browser changes/results **or** explicit unsupported errors" for every method the driver
tried. Minor completeness gap, not verdict-changing: `browser.navigate` and `browser.errors`
(2 of the 10 methods) were not individually probed this pass, but both are code-identical in
shape to a sibling that was (`navigate` shares `open`'s capability branch; `errors` shares the
generic-fallthrough branch with `get`/`screenshot`/`snapshot`/`wait`/`eval`/`console`).

## F-EDIT-08 (ledger line 226) — verdict: `NOT EXERCISED` (changed from `FAILED — defective`)

Own `discriminating: false`; agree. The two captures are genuinely empty of
`org.freedesktop.portal.FileChooser` traffic **as committed** (`git show 4fb4642:...` for both
files matches the evidence prose). This does not distinguish "ctrl-o doesn't reach
`prompt_for_paths`" from "this lane's `wtype -M ctrl -k o -m ctrl` never delivered the chord to the
app at all" — `WAYLAND-LANE.md` explicitly lists modifier chords as **not yet exercised** on this
tooling, distinct from proven named-key delivery. The prior `FAILED — defective` verdict rested on
a *visual* absence (P104, four focus contexts, no picker rendered) that the triage itself already
cast doubt on, citing a sibling flow (Add Project) whose portal call is proven to fire without ever
rendering visibly — so a visual-only negative was never solid ground here either. Net: neither this
pass nor the one it's revising actually isolated the row's real question with a trustworthy
instrument. `NOT EXERCISED` is the honest state, not a defect finding.

**Evidentiary contamination found, not attributable to this driver:** the working tree currently
has `f-edit-08-portal-monitor-attempt1.txt` **modified** (uncommitted) relative to what the driver
committed at `4fb4642`. The current on-disk content is a real
`org.freedesktop.portal.FileChooser.OpenFile` call with `accept_label: "Add Project"` at a
timestamp ~7 minutes after the driver's own window — almost certainly a concurrent sibling agent
(a different row's "Add Project" flow) whose `dbus-monitor` output happened to redirect to the same
file path and clobbered it after the fact. Judged against the driver's actual committed evidence
(`git show 4fb4642:...`, reproduced above), not the now-contaminated working-tree file. Flagged for
the orchestrator below — this is a shared-capture-path collision risk across the fleet, not a
finding about F-EDIT-08 itself.

## F-EDIT-12 (ledger line 230) — verdict: `NOT EXERCISED` (unchanged)

Own `discriminating: false`; no new drive attempted, none possible. Independently re-confirmed:
`Scripts/wayland-virtual-pointer.c` and `wayland-drive.sh`'s `pointer_command`/`click`/`move`
expose only an absolute `move` and a hard-coded `move+press+release` `click` — no
button-down-only or motion-while-held primitive exists anywhere in this lane's input surface, so a
drag cannot be composed here by construction. Matches `WAYLAND-LANE.md`'s documented limitation and
the manifest's own note that this needs `DISPLAY=:1`/X11. Verdict carries forward unchanged.

## F-USE-03 (ledger line 266) — verdict: `half-proven` (unchanged)

Opened `02-use-immediate.png` directly. It is a real live capture (Browser tab active, sidebar and
Files panel rendered, a WebView-adapter error banner visible) taken moments after launch, but it
does not clearly show the usage-bar region large enough or legibly enough to read "Loading" vs.
"Signed in" vs. a stale/dimmed render of either — agree with the driver's own assessment that no
text-reading instrument (no `tesseract` on this lane) exists to discriminate between those states
from pixel statistics alone, and that `Stale` additionally needs a >=15-25s timed-out refresh with a
prior `Loaded` value in hand, unverified for the same reason. The already-proven half (loaded +
logged-out states) was not re-driven this pass and is untouched. Owed half (Loading, Stale) is
unchanged. Verdict carries forward unchanged.

## F-USE-06 (ledger line 269) — verdict: `half-proven` (changed from `FAILED — defective`)

Independently re-traced every step of the driver's code citations against the current tree and
they all check out exactly:
- `surface.chat.open` (`main.rs:1037-1042`) takes only a `worktree` param — no way to specify an
  agent — confirmed by reading the handler directly.
- `restored_chat_spec(None)` (`main.rs:1618-1633`) returns `agent_id: None` for any tab without a
  persisted agent id, by construction.
- `register_restored_agent` (`main.rs:7567-7574`) only calls `activity.register_agent_id(...)`
  when `agent_id` is `Some` — confirmed both of its call sites (`:7608` in `restore_tabs`, `:7734`
  in `restore_tabs_in_workspace`) genuinely exist and pass through the restored spec's agent id.
- `post_activity_notification` (`main.rs:3738-3741`) early-returns at its first line when
  `self.activity.agent_id(&transition.pane_id)` is `None`.
- Traced the `notify` control method itself (`main.rs:1469-1508`) end to end: it queues
  `ControlAction::Notify`, which the main loop (`main.rs:2365-2368`) turns into exactly
  `workspace.activity.notify(...)` → `workspace.post_activity_notification(&transition)` — the
  same function above, not a different one. The driver's live `notify session=pane-0
  status=running` / `status=needs-input` calls therefore genuinely exercised this real code path,
  not a mock.

This confirms two things at once: (1) the ledger's specific cited defect — "`register_agent_id`
has zero production callers for restored panes" — is now **stale**, refuting `FAILED — defective`
as currently justified; `a5d09d7` wired it into both restore paths, matching the triage's premise.
(2) The driver's own drive still does not positively demonstrate delivery: `surface.chat.open`
creates a Chat tab with no agent identity, so `post_activity_notification`'s early return firing
here is a **correct no-op for an unidentified pane**, not evidence the fix works for the row's real
case (an actual restored **agent** chat). Reaching that case needs the in-app agent picker (no
control-socket equivalent exists), which this pass could not blindly target. Net: one half proven
(the previously-cited root cause is fixed, source-verified against both call sites), one half owed
(a live positive fire — `notify-send`/D-Bus `Notify` actually firing for a pane transitioning while
backgrounded, with `tab.agent_id: Some(...)` either freshly spawned or genuinely restored).
`half-proven` is the correct verdict, not `FAILED — defective` (the cited cause no longer holds)
and not `PASSED` (delivery was never actually observed to fire for an identified pane).

## F-TAB-25 (ledger line 141) — verdict: `NOT EXERCISED` (changed from `FAILED — absent`)

**Disagreement with the ledger's prior basis, confirmed by direct execution, not just reading.**
The current `FAILED — absent` cell ("still no attach-to-terminal code... pass 14") is flatly wrong
against this tree. Read `main.rs:5823-6030` and independently ran both cited tests rather than
trusting their presence:

```
cargo test --package tiller attaches_an_eligible_terminal
  test tests::drawn_terminal_menu_attaches_an_eligible_terminal_to_the_current_tab ... ok
cargo test --package tiller drawn_terminal_attach_command_is_disabled_for_the_current_terminal
  test tests::drawn_terminal_attach_command_is_disabled_for_the_current_terminal ... ok
```

Both pass. The first is a real simulated interaction, not a state-only check: it
`simulate_mouse_down(..., MouseButton::Right, ...)` on a real tab, locates the drawn
`tab-command-attach-to-current-terminal` element, `simulate_click`s it, and asserts the source
pane genuinely joined the current tab's split tree. This is strong evidence the underlying logic
is correct — but per the fixed verdict vocabulary, "source plus a green test is `NOT EXERCISED`,
never `PASSED`": a GPUI-harness simulated click is not a live drive of the running binary in a real
compositor. The driver's `could-not-reach` for the *live* right-click gesture is independently
correct: `WAYLAND-LANE.md` lists right-click among gestures this lane's tooling cannot send (only
`move` and a hard-coded left-button `click` exist), and the manifest's separately-noted
tab-bar-popover-paints-behind-surface defect would block visual confirmation of any resulting menu
even on a lane that could send the gesture. Net: `FAILED — absent` is affirmatively false (code and
a passing simulated test exist); `PASSED` is not earned (no live gesture drive exists or was
possible this pass). `NOT EXERCISED` is the accurate verdict.

## Notes for the orchestrator

- **Two verdicts move away from stale `FAILED` bases onto positive ground (`F-AUTO-06`,
  `F-AUTO-09`), matching the driver's `exercised-working` claim exactly** — independently
  confirmed by opening the raw capture files and re-reading the cited source, not by trusting the
  driver's prose.
- **`F-TAB-25` is the sharpest correction in this slice.** `FAILED — absent` was carried forward
  unchanged since "pass 14" and is demonstrably false today: the code exists and I ran both cited
  tests myself (not just grepped for their names) and both pass. The right replacement is
  `NOT EXERCISED`, not `PASSED` — a green simulated test is not a live gesture drive, and the live
  gesture (right-click) is out of reach on this lane for two independent reasons (no right-click
  primitive; known popover z-order defect).
- **`F-USE-06` moves off its cited defect for a real reason, but only to `half-proven`, not
  `PASSED`.** The driver's own evidence is honest about this — its top-level claim was
  `partially-exercised`, correctly not `exercised-working` — and the code trace here independently
  confirms both halves of that assessment: the previously-cited cause is fixed, and delivery for a
  genuinely agent-identified pane was never actually observed.
- **Evidentiary contamination found in `F-EDIT-08`'s capture directory**, not caused by this
  driver: `reference/linux-progress/wavea-W12-auto+edit+use+tab/f-edit-08-portal-monitor-attempt1.txt`
  is modified in the working tree relative to what the driver committed (`4fb4642`), now containing
  a real FileChooser `OpenFile` call (`accept_label: "Add Project"`) from what looks like a
  concurrent sibling agent's capture landing on the same file path roughly 7 minutes later. I judged
  F-EDIT-08 against the driver's actually-committed evidence (`git show 4fb4642:...`), not the
  contaminated working-tree copy, but this is worth the orchestrator's attention: shared,
  non-unique capture filenames (`portal-monitor-attempt1.txt`) across parallel Wayland-lane agents
  can silently overwrite a sibling's committed-then-later-clobbered artifact in the working tree.
  Left the file as found — reverting it is out of scope for a critic pass and risks clobbering
  whatever the other agent is mid-way through.
- `F-EDIT-12` and `F-USE-03` are honest, non-discriminating re-confirmations; every source citation
  checked out and both verdicts carry forward unchanged.
