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

## F-BRW-07 — ledger line 256, currently half-proven

**Drove:** `grep -rn "request_permission" rust/crates/tiller_ui/src/browser.rs` to re-confirm
the blocker triage names before spending a live drive on it.

**Observed:**
```
588:    pub fn request_permission(&mut self, origin: &str) {
880:    pub fn request_permission(&mut self, origin: &str) {
881:        self.state.request_permission(origin);
1666:        browser.request_permission("https://agent.example");   <- #[test]
1677:        browser.request_permission("https://denied.example");  <- #[test]
```
Both call sites at 1666/1677 are inside `#[test]` functions in the same file — zero production
callers anywhere in the crate, matching the F-BRW-06 UNREACHABLE finding this row's approach
cites. There is genuinely no user gesture, socket call, or code path in the shipped app that
would trigger a permission prompt to confirm-absent; "trigger access, confirm no new prompt"
has no seam to drive.

**Claim:** could-not-reach for the owed half — not a lane limitation but an absent production
caller (`request_permission` is dead code outside tests). The persisted-reload half remains
correctly proven per the existing record (main.rs:8161/8234, re-confirmed by grep of the same
call sites this pass, unchanged). Do not close this row until F-BRW-06 gets a real caller.

No new captures (grep-only re-confirmation).

## F-BRW-08 — ledger line 257, currently half-proven

**Drove:** seeded two distinct origins directly into the running instance's own DB (`python3`
stdlib `sqlite3`, no source edit, exercising the existing `browser_origin_grant` table from
migration v11): `https://w11-origin-a.example`, `https://w11-origin-b.example`. Confirmed both
present with a fresh `SELECT`. Then, live on the Wayland lane: `ctl surface.settings.open`,
`ctl surface.settings.select section=permissions`, `shot before-revoke-all`, then a real
left-click at `1272 158` — one row-height (44px) above the coordinate
(`1272 202`) that the prior E06-brw sweep proved lands on the per-origin `Revoke` button,
landing on the card's header row where `render_browser_grants` (settings.rs:3038) places the
"Revoke all" pill — then `shot after-click-158`.

**Observed:** DB re-query after the click returned `[]` — both origins gone in a single click,
which a per-origin Revoke button could not do (it only ever removes the one row wired to its
own `on_click`); this is decisive that the click landed on "Revoke all" specifically, wired to
`Db::revoke_all_browser_origins` (main.rs:8253-8255), not two lucky clicks on individual rows.
The capture region also changed in stddev/mean between before (16.8/46.4) and after
(13.9/33.9), consistent with both origin rows disappearing and the empty-state message
appearing.

**Claim:** exercised-working. Both halves the ledger names are now proven live: the
single-origin `Revoke` path (already proven in E06-brw), and this pass's owed
multi-origin `Revoke all` path — seeded 2 origins, one real click cleared both, confirmed by
DB re-query and a changed capture region.

Captures: `reference/linux-progress/wavea-W11-ctrl+brw+sid/brw08/{01-baseline,02-before-revoke-all,03-after-click-158}.png`.

## F-SID-12 — ledger line 81, currently half-proven

**Drove:** re-confirmed independently: `grep -n "BTN_LEFT\|button" Scripts/wayland-virtual-pointer.c`.

**Observed:**
```
132:  zwlr_virtual_pointer_v1_button(pointer, milliseconds(), 0x110,
137:  zwlr_virtual_pointer_v1_button(pointer, milliseconds(), 0x110,
```
`0x110` is `BTN_LEFT`, hardcoded with no button parameter anywhere in the file —
`Scripts/wayland-drive.sh`'s `click` action can only ever send a left button on this lane.
There is no command-palette or socket path to "Set as Primary" that substitutes for a
right-click context menu (the command palette opens on `ctrl-shift-p`, itself a modifier
chord, also out of scope here per `WAYLAND-LANE.md`). No new gesture became available since
the prior pass; matches the existing record exactly.

**Claim:** partially-exercised. Catalog/dispatch half
(`worktree_primary_context_transition_reaches_the_catalog` route, sidebar.rs:694-708,
main.rs:3209-3248) remains correctly proven by source inspection; the gesture half
(right-click a worktree row) is could-not-reach on Wayland — needs `DISPLAY=:1`.

No new captures (grep-only re-confirmation, matches prior evidence).

## F-SID-18 — ledger line 87, currently FAILED — absent

**Drove:** confirmed a fresh worktree does NOT start on the empty state (discriminating fact
for this row: `ctl project.add` then `ctl panel.list` returned two panes — `pane-0`/Chat and
`pane-1`/Terminal — so the "No Terminals" empty state (`main.rs:5416-5461`,
`empty-worktree`/`empty-worktree-new-terminal`) can only be reached by first closing both
default tabs). The per-tab close control (`workspace-tab-close-{id}`, main.rs:5634-5654) only
renders `.when(active, ...)`, is a plain left-click (no chord), and `request_close_tab_by_id`
closes a non-dirty tab immediately with no confirmation — so this should be reachable on this
lane in principle.

Attempted to locate the close-button pixel position by column/row-scanning the tab-bar region
with `convert ... -crop ... standard_deviation` (a content band was found consistently at
roughly x=440-580, y=25-40 across two resolutions, matching where the "Terminal" tab's label
and close icon should sit) and tried three blind clicks at candidates derived from that scan —
`(572,48)`, `(570,33)` twice, at both 1400x900 and 1715x972 — each followed by a forced-repaint
shot and a `panel.list` re-check. **None reduced the pane count**; `panel.list` returned the
same two panes after every attempt.

**Not reached:** could not empirically locate the close-button hitbox from image analysis
alone (no OCR available, and `panel.list`/pixel-region scanning cannot distinguish "missed the
button" from "hit it and it silently no-op'd" without a visual read I cannot perform). This
would need either a sighted pass to read the exact button coordinates from a capture, or a
socket method to close a tab by id (none exists — `tab.select`/`tab.cycle` are the only
`tab.*` socket methods).

**Corroboration only, not closing evidence:** the drawn GPUI test
`tests::drawn_selected_worktree_without_tabs_offers_a_new_terminal` (main.rs:11849) passed:
```
test tests::drawn_selected_worktree_without_tabs_offers_a_new_terminal ... ok
```
It simulates a real click (`cx.simulate_click`) on the empty state's "New Terminal" button in
GPUI's own test harness and asserts a terminal tab replaces the empty state. This is a test,
not a live-pixel PASSED — reported as a test per the evidence rules.

**Claim:** could-not-reach. The default-tabs half is now confirmed live (worktrees open with
Chat+Terminal, not empty — a genuine, newly-observed fact). Reaching the empty state itself,
and clicking its "New Terminal" button, needs either a sighted capture-reading pass or a
close-by-id socket method neither of which this pass had.

Captures: `reference/linux-progress/wavea-W11-ctrl+brw+sid/sid18{,b,c,d}/` and
`sid18-probe/` (blind-click attempts and panel.list probes, all non-discriminating — recorded
for the record, not as proof).
