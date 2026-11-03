# Unread sessions: the sidebar shows which agents stopped while you were away

**Date:** 2026-11-01
**Status:** design
**Mockup:** option A of the brainstorm screen `sidebar-unread.html` (kept under
`.superpowers/brainstorm/`, not tracked). Where it and this document differ, this
document wins.
**Touches:** `sirio_activity` (the notification predicate), `sirio` (`main.rs`: status
sync, the transition funnel, notifications, restore; `tray.rs`; `native_notification.rs`),
`sirio_ui` (`sidebar/row.rs`, `sidebar/mod.rs`, `sidebar/sessions.rs`), `sirio_persistence`
(one migration on the `tab` table).
**Origin:** a brainstorm on what to take from OpenIDEA (`l0ng-ai/openidea`), which marks a
task unread until you open it. Nothing is ported from its code; the idea is.

Facts below are tagged **[read]** (read in Sirio's code at `490c5067`, not reproduced) or
**[decided]** (settled with Enzo in the brainstorm). Choices the brainstorm did not settle
are marked **[my call]**; Enzo reviews them with this document.

## §0 Intent

Enzo works from the sidebar's **tree** view with several agents at once, and in his words
*"mi sfuggono quelli finiti"*: the agents that stopped slip past him. Success is simple: when
an agent stops working while he is not looking at it, the tree says so, and keeps saying so
until he has looked.

## §1 What is broken today

These are why agents slip past, and each one the design has to answer.

| # | Fact | Where |
|---|------|-------|
| F1 | Running and Done are the **same green**; only motion separates them. | [read] `sirio_ui/src/sidebar/row.rs:50-63` |
| F2 | A worktree row, a split tab and a collapsed project draw **one** status, the most urgent of their panes: error, needs-input, running, done. A finished agent beside a running one is hidden behind "running". | [read] `sirio/src/main.rs:11095` (`tab_status`), `:12775`, `sidebar/mod.rs:2972` (`collapsed_project_status`) |
| F3 | There is no "seen". Done clears only on a title change, a process exit or a closed pane. A Done from an hour ago and one from a minute ago look identical. | [read] `sirio_activity/src/model.rs:232-252` |
| F4 | **A chat in a worktree that is not selected never updates its status.** A chat's status comes only from Layer E (`surface_evidence`: streaming → Running, a completed turn → Done), pushed by `sync_entity_evidence`, which walks `self.tabs` only — the selected worktree's tabs. `set_entity_status` has no other caller. The `TurnEnded` handler also searches only `workspace.tabs`. A chat left running in another worktree keeps spinning after it finishes, until its worktree is selected again. Probably the main reason agents slip past. | [read, not reproduced] `main.rs:11054`, `:11066`, `:9001` |
| F5 | A chat waiting on a permission or a question is still `is_streaming()`, so it shows **Running**, never needs-input. | [read] `main.rs:11009-11024`, `sirio_ui/src/chat/mod.rs:5633` (`pending_question`) |
| F6 | The terminal CLIs end a **turn** with `needs-input`, not `done`: Claude's `Stop`, `Notification` and `SessionStart` hooks send needs-input, only `SessionEnd` sends done. Codex and omp do the same. | [read] `sirio_agents/src/claude.rs:129-146`, `codex.rs:92,143`, `omp.rs:23` |
| F7 | Notifications: a chat's turn end posts none; the "visible" check ignores the secondary centre pane; the payload names `self.working_directory`, so a background pane's notification carries the **selected** worktree's branch and project; a click does nothing (Sirio never registers `on_system_notification_response`, and `notify-send` is fire-and-forget). Tags are `sirio-pane-<pane id>`, and pane ids are re-minted across restarts. | [read] `main.rs:12914-12975`, `:4151-4180`, `native_notification.rs:29-58` |
| F8 | No status is persisted. A terminal whose agent exited while Sirio was closed comes back Done through Layer E with no transition. Chats are not hosted by `sirio-host`, so a chat cannot finish while Sirio is closed. | [read] `sirio_terminal/src/link.rs:600-629`, `main.rs:~25819`, `sirio_ui/src/chat/mod.rs:4175,4272` |
| F9 | The tray roster lists only the control snapshot's panes, so parked worktrees are missing from it. | [read] `main.rs:11357` (`tray_roster_snapshot`) |

## §2 Goals, non-goals, anti-goals

**Goals**

1. A session that **stopped working** while you were not looking is marked **unread** in
   the tree until you look at it. [decided, option A]
2. The marker is never hidden by another session's status (F2). [decided]
3. The statuses it rests on are true for every session, including chats in worktrees that
   are not selected (F4) and chats waiting on you (F5). [decided: F4 fixed here; F5 my call]
4. The same marker in the tray roster and the Sessions view. [decided]
5. Notifications: a chat that stops posts one as a terminal agent does; clicking a
   notification brings Sirio forward on that session and counts as seeing it. [decided]
6. It survives a restart, including a terminal agent that finished while Sirio was closed
   (as far as the platform lets us know, §6). [decided]

**Non-goals** [decided]

- Reordering the tree by urgency, or by unread. The order stays the one dragged by hand.
- An inbox strip ("Da vedere") at the top of the sidebar (option B).
- Moving unread rows to the top of their project (option C).
- A new colour for Done versus Running (F1). The blue marker carries the news.
- Steering, rewinding or forking a turn; Codex native (another worktree owns it).

**Anti-goals** — it would be a failure, even working, if:

- the marker appears while you are looking at the session, or lingers after you have;
- a notification for a background session names the wrong worktree or branch (F7);
- a release build gains a control-socket method that writes anything;
- the tree's row order changes.

## §3 What counts as unread

A **session** is one tab, keyed by its `persistence_id` (stable across restarts,
`session.rs:1333`, `:1932`). A split tab is one session: the unread mark is stored and drawn
per tab. [my call: the pill is per tab, and pane ids are re-minted across restarts]

**Stopping is detected per pane.** A *stop* is one **pane** moving from Running to a
settled state (Done, NeedsInput or Error), or a pane's first status being already settled
(an agent identified from its title while asking a question), or any pane moving into
Error from anything. A stop in any pane of a tab marks the tab. [my call, replacing the
brainstorm's "Done or Error", because of F6: a terminal agent ends its turn in NeedsInput.
Per pane, not on the tab's aggregated status: otherwise a pane that finishes beside a
running one is missed, which is F2 again]

Moves between settled states other than into Error (NeedsInput → Done when a CLI exits
after ending its turn) are not stops: the session already stopped once.

**On screen.** A session is on screen when all of these hold:
- the Sirio window is active (`window_active`, `main.rs:5618`); [my call: a session that
  stops while Sirio is in the background has not been seen]
- its worktree is the selected one (`has_current_worktree()` and the tab belongs to it);
- its tab is the one shown in the primary centre pane, or in the secondary one while
  `secondary_pane_visible()` (`main.rs:23165`, `tab_machinery.rs:127-160`).

**Rule.**
- When a session stops and is not on screen, it becomes unread.
- When an unread session comes on screen, it stops being unread. Selecting its tab,
  switching to its worktree, opening the secondary pane on it, focusing the window and
  clicking its notification all do this.
- Unread survives the session starting to work again (a queued message draining, a reply
  through a hook): the marker sits beside the spinning glyph until you look. [my call]
- Closing the tab, the worktree or the project drops its unread state.

## §4 What the sidebar draws [decided, option A]

- **Pill:** a ring in the unread colour around the tab's pill, and its status dot in that
  colour.
- **Worktree row:** a dot in the unread colour at the row's leading edge and the title in
  a heavier weight, while any of its tabs is unread. The status glyph at the end of the row
  is unchanged: it still shows the most urgent status (F2 stays as it is; the dot is what
  defeats it).
- **Collapsed project row:** a count of its unread tabs, in a small badge in the unread
  colour, in place of nothing or beside the existing glyph.
- **Sessions view:** each unread `SessionRow` gets the same dot and weight.
- **Tray roster:** an unread worktree's entry is marked (a glyph before its label; the
  builder picks one the native menus render on all three platforms). F9 is fixed so parked
  worktrees appear.
- **Unread colour:** one colour from the Ely palette that is none of success, warning or
  danger, in light and dark. The mockup used blue. [my call: the palette's info/accent
  slot; the builder picks the token and checks it on Onice's pure-black base]
- The row's context line is unchanged. [my call: the mockup's "Pi ha finito · 2 min" sub-line
  is not built]

Every new input to a row, pill or session row goes into the struct the row is diffed by
(`RowInputs`, `SidebarPill`, `SessionRow`), or the row will not repaint.

## §5 The statuses underneath

**F4: parked chats.** Layer E must reach every mounted chat, not only the selected
worktree's: the chats in `parked_worktree_tabs` and in `retained_worktree_chats`.
`TurnEnded` must find a chat wherever it is mounted. The builder chooses how (walk the
parked strips in `sync_entity_evidence`, or have each chat push its own evidence when it
changes); the cost must stay one pass per dirty sync, never a per-frame walk.

**Parked terminals.** The same reach applies to terminals: Layer E's shell evidence
(`collect_terminal_shell_evidence`, `main.rs:6591`) and the process-tree poll
(`terminals_to_poll`, `main.rs:9839`) cover the parked strips' terminals as well as
`self.tabs`, at the same cadence. Hooks (Layer A) and child exits already reach a parked
pane, because they arrive by pane id. [my call: goal 3 says every session]

**F5: a chat waiting on you.** A chat with a pending question or permission reports
NeedsInput, not Running. [my call]

**One funnel.** Today transitions are noticed in four places that each decide on
notifications (`main.rs:6095`, `:9619`, `:9685`, `:9795`), Layer E's `set_entity_status`
returns no transition, and a chat's turn end only renames. All of them — hook pushes,
process exit, title and content signals, Layer E (terminals and chats), a chat's turn end
and question — go through **one** function that receives the session, the pane, the old
and the new status. That function decides, from one predicate:

- whether the session becomes unread (§3);
- whether a notification is posted. **The same stop decides both**: a notification is
  posted exactly when a stop happens and the session is not on screen.
  `NotificationPolicy::should_notify` (`sirio_activity/src/notification.rs:19`) changes
  accordingly: today it fires on any change into a settled status, so a CLI that ends its
  turn (needs-input) and then exits (done) notifies twice; after this it notifies once.
  `pane_visible` is replaced by §3's "on screen", which counts the secondary pane and the
  window; [my call]
- and builds the payload from the **session's own** worktree, branch and project, not from
  `self.working_directory`.

The predicate is pure and lives where it can be proven without a window (§9).

## §6 Restart

What is persisted, per tab, keyed by `persistence_id`: **unread**, and **the last status
the app knew** (running or settled). [my call: two nullable columns on the `tab` table, one
migration]

They are written with targeted updates by id, the way `touch_tabs` writes `last_event_at`
(`sirio_persistence/src/db.rs:729`), each time a session stops, starts or is seen. They are
not written through the layout save: that save writes only the selected worktree's rows
(`main.rs:7443`, `session.rs:1291`), so a parked tab that went unread would be lost.
[read; the builder confirms before relying on it]

On restore:
- a tab saved unread comes back unread;
- a **terminal** tab whose last known status was Running, and whose first status after
  restore is settled, is unread: the agent stopped while Sirio was closed. The usual case is
  a terminal agent whose CLI exited; the host replays the exit (F8).
- a **chat** tab never gains a mark on restore. A restored chat with a transcript reads Done
  at once (`has_completed_turn`, `sirio_ui/src/chat/mod.rs:~4177`), but a chat that was
  streaming at quit was cut off by that quit, which the user did; it did not stop while they
  were away. [my call]

Limits, stated plainly in the code and the release notes:
- A CLI whose **turn** ended while Sirio was closed, and which is still open, cannot be
  detected: `sirioctl` drops an implicit-target hook when Sirio is not running
  (`sirio_control/src/bin/sirioctl.rs:2405-2412`). It comes back with whatever status its
  title or process gives it, and no marker.
- Chats cannot stop while Sirio is closed (F8), so a chat's marker after restart is only the
  one saved before quitting.

## §7 Notifications

- A chat that stops (turn end, question) posts a notification through the same funnel and
  policy as a terminal agent. [decided]
- **Tag** by session: `sirio-tab-<persistence_id>`. A newer notification for a session
  replaces its older one, and a notification left in macOS's Notification Center from a
  previous run still names the right session.
- **Click** = bring the window forward and activate it, select the session's worktree and
  tab (the tray jump path, `ControlAction::TrayJump` → `control_select_worktree_and_jump`),
  which makes the session seen.
- **When a session is seen**, its notification is retracted where the platform allows
  (`dismiss_system_notification` on macOS). [my call]

Per platform:

| Platform / route | Post | Click |
|---|---|---|
| macOS, native (bundled, permission granted) | `show_system_notification` (today) | `cx.on_system_notification_response`, registered once at startup; the response's `tag` names the session |
| macOS, osascript fallback | `display notification` (today) | **none**: AppleScript notifications report no click. Documented. |
| Linux | **gpui's own route** instead of `notify-send`: the vendored `gpui_linux` already implements `show_system_notification` over D-Bus with `notify-rust`, with a `default` action on the body (`rust/vendor/gpui_linux/src/linux/system_notifications.rs`). Sirio's Linux arm (`main.rs:4175`) moves to `native_notification::post`, as macOS's native route does. | `cx.on_system_notification_response`, the same handler as macOS. |
| Windows | balloon (today) | **none**: gpui_windows' toast activation needs an app identity (AUMID) Sirio does not register. Documented; a follow-up. [my call] |

Linux limits, accepted [my call]: gpui's `dismiss` is a no-op there (closing a popup needs
the server-assigned id of a live handle), so a replaced or seen session's popup stays until
the daemon expires it. Clicking such a stale popup still carries its tag and opens its
session, which is harmless. Whether a body click invokes `default` depends on the daemon:
the builder checks mako (Enzo's Omarchy desktop) by hand; a daemon that ignores it leaves the
notification informational, which is today's behaviour.

## §8 Constraints from the repo

- `sirio_perf` labels stay static strings; no title, path or prompt in a span name.
- Test observability goes through debug-only control-socket reads
  (`served::served_methods(debug)`): per-tab unread for the sidebar, and a record of posted
  notifications. Nothing new is served in a release build.
- macOS is the reference platform; the macOS-only paths here (native click, retract) are
  `cfg`-gated and get a hand check (§10).
- Version: this is a `feat:`. If it is the cycle's first `feat:` when it lands, it moves the
  workspace version from patch to minor through `Scripts/set-workspace-version.sh`.
- Implementation starts from `main` after block 3 (composer ↑ recall and Shift+Tab,
  branch `feat/composer-recall-mode-cycle`) has landed: both touch `main.rs`.

## §9 Tests

The proof is end to end, against the real binaries, each run leaving an artifact.

- **Parked chat (F4) and its notification.** On `test-chat-session-panel-e2e.sh`'s paced
  trace stub (`TRACE_STEP_MS`): start a turn in a chat in worktree A, select worktree B,
  let the turn end. Assert through the debug reads that A's chat is Done and unread, that A's
  row carries the marker, and that one notification was recorded naming A's branch and
  project. The isolated run gets a private D-Bus session bus with a small recording
  `org.freedesktop.Notifications` server (python3-gi, already used by the clipboard
  scenario) that logs every `Notify` and can emit `ActionInvoked(id, "default")` as a body
  click would; the click selects A and its tab, activates the window, and the marker
  clears. Captures before and after.
- **One notification per stop.** A terminal agent that ends its turn (needs-input) and then
  exits (done) while not on screen is notified once.
- **Seen in the secondary pane.** A session that stops while shown in the secondary pane
  of an active window is never unread and posts no notification.
- **Chat waiting on a question (F5).** It reports needs-input, and stops/unread as above.
- **Terminal agent.** On `test-sirioctl-e2e.sh`'s stub agents: a hook moves a terminal from
  running to needs-input while another worktree is selected → unread, notified with the
  right worktree.
- **Restart.** On `test-host-sessions-e2e.sh`: a terminal agent running at quit, its
  process exits while the app is down, relaunch → unread. A tab saved unread comes back
  unread.
- **Units.** Only the pure predicate of §5 (stop/seen/notify decisions) is tested in
  isolation, and its failure modes are written before its code.

Each scenario is recorded in `docs/agents/e2e-tests.md`.

## §10 macOS hand check (Enzo)

On a bundled build with notification permission granted:
1. A chat in a non-selected worktree finishes → a notification names its worktree; a click
   brings Sirio forward on that chat and the marker clears.
2. Quit, relaunch: an old notification in Notification Center still opens the right session.
3. With the permission denied (osascript route): the notification shows, a click does
   nothing, and nothing breaks.
4. The marker colour reads on light, dark and Onice.

## §11 Left to the builder

The unread colour token and the tray glyph; the migration's column names; how Layer E
reaches parked chats and terminals; the funnel's name and signature; the debug read's
shape; the recording notification server's shape.
