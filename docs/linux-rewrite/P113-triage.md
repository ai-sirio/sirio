# P113 triage — defective-row root causes

**Method.** This is a reading-only triage of the evidence column in
`INVENTORY-LEDGER.md`; no UI, socket, build, or test was driven. Locations below
are the locations named by that evidence, not independently verified diagnoses.

## Inventory census

The frozen inventory denominator is **389 `F-` rows**. The project's ledger gate,
`python3 Scripts/ledger-totals.py`, counts **47** of those rows as
`FAILED — defective` and reports `Totals block matches the body`.

Two supplementary ACP records also use that verdict text: `ACP-08` and
`ACP-12`. They are not `F-` inventory entries and do not enter the frozen
389-row denominator or the 47-row defective count. `ACP-12` explicitly belongs
to `F-CHAT-15`; both ACP records remain analysed below as supplementary
sub-rows, not as extra inventory defects.

## Separate queues before builder dispatch

### Re-drive-only browser-excuse queue: none

The six rows that say the old `N/A — platform` browser premise is void are
`F-AUTO-09` and `F-CTRL-BROWSER-02` through `F-CTRL-BROWSER-06`. None is merely
waiting to be exercised: each records either an observed bad response or a
specific code-path defect (and five were already socket-probed at 03:02:56).
They belong to the browser-control builder cluster below, with later verification
in a separate snapshot/drive lane. Reclassifying any of them as re-drive-only
would discard its own contrary evidence.

### Reproducibly failing named tests — urgent, cheapest queue

These are the three defective *inventory rows* whose own evidence says a named
test is reproducibly red; do not merge them into UI work or run them in this
shared tree.

| Root cause / rows unblocked | Rows and evidence | Where / builder action |
|---|---|---|
| Process-owned clearing sequence — **2** | `F-CORE-ACT-06`, `F-CORE-ACT-11`: both cite `panes::tests::process_owned_status_survives_title_and_child_exit_events` failing in pass 14 and pass 16, with `panes.rs` unchanged. | `rust/crates/tiller/src/panes.rs`. Reproduce only in a snapshot tree, trace ownership through title and child-exit events, then make the stated invariant hold. One fix can clear both rows. |
| Real-PTY Layer-A debounce — **1** | `F-CORE-ACT-07`: `real_pty_layer_a_debounce_suppresses_first_title_and_accepts_second` failed reproducibly; its non-PTY companion passed. | `rust/crates/tiller/src/panes.rs` and its PTY signal path. Fix the real-PTY timing/order distinction, not the already-green unit-only case. |

### Superseded-pass rows — retain verdict, then re-drive after fixes

`F-PRJ-01`, `F-PRJ-13`, and `F-PRJ-15` say **“pass 13 is superseded”**, but
none retracts its present defective verdict: each immediately records a current
failure (transparent/unreadable plus menu; chosen project icon discarded; chosen
glyph discarded). They are not eligible for a ledger flip. `F-PRJ-13` and
`F-PRJ-15` share a builder cause; `F-PRJ-01` does not. Re-drive them after their
respective fixes.

## Ranked root-cause queue

Counts are rows a cause can return to a drivable state, not a claim that every
row in a cluster has the same implementation patch. Equal-count causes are
ordered by confidence/actionability.

### 1. Missing tab-strip context menu — 6 rows, high confidence

**Rows:** `F-TAB-12`, `F-TAB-13`, `F-TAB-14`, `F-TAB-15`, `F-TAB-17`,
`F-TAB-21`.

**Evidence:** every row says repeated tab-strip right-clicks produced no Tab
menu. The blocked operations differ only because they are menu entries: Move
Existing Tab, the no-eligible explanation, Rename, Close Tab, Close Others/
Close Tabs to Right, and a general move action. `F-TAB-21` has the useful control
that terminal-body right-click works, isolating this to the tab strip.

**Likely location:** `rust/crates/tiller_ui/src/tab_bar.rs` (and the tab-strip
event/menu wiring nearby). **Builder action:** restore/attach the strip's
right-click menu and populate its context-sensitive entries; then re-drive all
six operations. This evidence does *not* justify merging `F-TAB-02` (overflow
chevron) with this cause.

### 1. Success-only browser control protocol — 6 rows, high row impact but broad/speculative

**Rows:** `F-AUTO-09`, `F-CTRL-BROWSER-02`, `F-CTRL-BROWSER-03`,
`F-CTRL-BROWSER-04`, `F-CTRL-BROWSER-05`, `F-CTRL-BROWSER-06`.

**Evidence:** all report `ok:true`/`queued:true` without a result or allowed
error. `F-CTRL-BROWSER-02` identifies `ControlAction::Browser` as the only
variant without `reply` and the unconditional reply at `main.rs:1694-1700`;
`F-CTRL-BROWSER-03/04/05/06` locate the no-op arm at `main.rs:4638-4643`
(and the inadequate navigate/act arms immediately above it). `F-AUTO-09`
observed that same empty-success shape for eight browser methods.

**Likely location:** `rust/crates/tiller/src/main.rs`, browser control dispatch
and `ControlAction`. **Builder action:** design a real reply/error contract
first, then implement or explicitly reject each operation rather than queueing
nothing. This is six rows behind one architectural fault, but it is not a
one-line fix; do not promise six passes from a superficial reply-field patch.

### 3. Project-icon selection never crosses the settings boundary — 2 rows, high confidence

**Rows:** `F-PRJ-13`, `F-PRJ-15`.

**Evidence:** both saw selection work inside the six-glyph/settings UI and saw
the original sidebar icon after Close. `F-PRJ-13` names the unwired
`on_change(ProjectIcon)` seam; `F-PRJ-15` says the grid output is discarded.

**Likely location:** `rust/crates/tiller_ui/src/sidebar.rs` plus the project
settings model/persistence seam named in `SEAMS.md`. **Builder action:** wire
selection through the close/apply path into the project model and sidebar row;
fix `F-PRJ-13`'s clipped Colour row separately if it remains after that.

### 3. UI-chat persistence bypasses the persistence path — 2 rows, high confidence

**Rows:** `F-PER-01`, `F-PERSIST-DB-05`.

**Evidence:** `F-PER-01` observed two real UI conversations produce zero
`chat_turn` and `session_ref` rows and restore an empty transcript, while other
objects persisted. `F-PERSIST-DB-05` traces the split: store/replay tests cover
`tiller_acp`'s `ChatSession`, while `tiller_ui`'s `ChatView` has no persistence
reference and only restores an in-memory `retained_chats` list.

**Likely location:** `rust/crates/tiller_ui/src/chat.rs`,
`rust/crates/tiller_acp/src/chat.rs`, and `rust/crates/tiller/src/session.rs`.
**Builder action:** make the user-facing `ChatView` write turns/session refs and
load them into a restored UI session; a later live restart re-drive decides both
rows. The green store test is not sufficient evidence for either.

### 3. Agent mode pill is display-only — 1 inventory row (+ `ACP-12` sub-row), high confidence

**Inventory row:** `F-CHAT-15`. **Supplementary sub-row:** `ACP-12`.

**Evidence:** `F-CHAT-15` records correct pills in each state but no chooser
after idle/offline clicks and 2s/4s waits; `ACP-12` explicitly points back to
that owning row.

**Likely location:** `rust/crates/tiller_ui/src/chat.rs`. **Builder action:**
attach/open the mode menu and implement its selections; re-drive choices for
each available mode. This is one defect reported at a feature and ACP-contract
level, not two inventory defects.

### 3. `notification.create` stores a record but never posts — 2 rows, high confidence

**Rows:** `F-AUTO-06`, `F-CTRL-NOTIFY-03`.

**Evidence:** both identify `record_notification` pushing an in-memory vector;
`F-CTRL-NOTIFY-03` additionally records an empty `dbus-monitor` capture and
that the only poster caller is activity-transition code, never `create`.

**Likely location:** `rust/crates/tiller/src/main.rs` notification control
handler and `post_desktop_notification`. **Builder action:** explicitly choose
whether `notification.create` is an inbox API or delivery API; if delivery is
required by the row, call/post through the existing notifier and return its
outcome. One change can satisfy both rows.

## One-row causes — keep separate

These are ranked after the multi-row causes because each unblocks one current
row. They are deliberately not grouped merely by screen or subsystem.

| Cause (1 row) | Evidence / row | Likely location and builder action |
|---|---|---|
| Stale New Browser menu action | `F-WIN-06`: production menu offers New Browser but `NewTabAction::NewBrowser` is an empty match arm; UI is silent. This is not merged with the browser-control API cluster: it is an in-app menu route. | `rust/crates/tiller/src/main.rs:4215`, `rust/crates/tiller_ui/src/tab_bar.rs:561`; make this menu route create a browser or surface an error. |
| Transparent project-add menu | `F-PRJ-01`: the three-item menu exists, but lacks an opaque background and makes Open Project unreadable. | `rust/crates/tiller_ui/src/sidebar.rs`; supply opaque menu painting. |
| Premature tab overflow / empty all-tabs list | `F-TAB-02`: chevron appears before real pixel overflow and shows no list/selected marker. The evidence names a chevron, not the absent right-click menu. | `rust/crates/tiller_ui/src/tab_bar.rs`; correct overflow measurement/list rendering. |
| Dirty-tab close discards edits | `F-TAB-16`: closing a dirty tab silently drops `y`; two-dirty precondition was unreachable. | tab-close/dirty-state handling under `rust/crates/tiller_ui`; add confirmation/discard path before re-drive. |
| Wrong left split placement | `F-TAB-23`: live `pane.split` works in all directions, but left inserts on the right. It must not be merged with `F-TERM-SPLIT-01`: that row additionally says cache lifecycle is unintegrated. | split layout/order code in `rust/crates/tiller/src/main.rs` / terminal pane layer; correct left insertion. |
| Removing a chat chip kills keyboard focus | `F-CHAT-12`: model removal works, but five recovery attempts do not restore typing until leaving/re-entering the tab; `chat.rs:3786` focus call is insufficient. | `rust/crates/tiller_ui/src/chat.rs`; repair focus/event routing after the × action. |
| Follow Edited Files is a toggled dead bool | `F-CHAT-14`: `following_edited_files` has only self/menu/test references; `FileSystemEventMonitor` has no consumer references. | `rust/crates/tiller_ui/src/chat.rs` and `rust/crates/tiller_markdown/src/file_events.rs`; connect the watcher/consumer, not just the test bool. |
| Changes Refresh has no observable transition | `F-CHG-03`: immediate and delayed captures show neither loading nor change. | Changes view under `rust/crates/tiller_ui`; implement state/refresh result, then re-drive inaccessible/Retry. |
| Changes keyboard selection is inert | `F-CHG-05`: arrows/Space move at most a cursor, never open/expand/change active file. | Changes view keyboard/action wiring under `rust/crates/tiller_ui`; bind selection activation. |
| Open diff does not isolate a diff tab | `F-CHG-13`: it opens generic Changes content still showing the multi-file list. | Changes tab routing under `rust/crates/tiller_ui`; create/select a file-specific diff surface. |
| Editor formatting/undo selection semantics | `F-EDIT-02`: Bold changes nothing, Italic wraps the line rather than selection, and Ctrl-Z fails. | editor command/selection integration under `rust/crates/tiller_ui`; fix selection transforms and undo transaction. |
| Ctrl-O gives no existing-file outcome | `F-EDIT-08`: no picker or focus/open-state change in four contexts. | editor open command/router under `rust/crates/tiller_ui`; choose picker vs focus-existing behavior and make it visible. |
| Compound-command process groups survive quit | `F-PER-06`: compound panes orphan groups; simple panes flush. | terminal process teardown in `rust/crates/tiller_terminal`; terminate/process-group-wait compound children. |
| Browser native child uses the wrong coordinate scale | `F-BRW-01`: native child is uniformly 0.8576x and offset, covering sidebar; evidence names `browser.rs:1176` GPUI Pixels passed as wry logical values. | `rust/crates/tiller_ui/src/browser.rs`; convert physical/logical coordinates correctly. |
| Browser Back contradicts static navigation bookkeeping | `F-BRW-02`: click lands but `go_back()` behaves as unavailable despite both navigation callbacks recording history. The evidence explicitly says not to fix blind. | `rust/crates/tiller_ui/src/browser.rs` history drain/enabled/input layers; first instrument the contradiction, then fix. |
| Browser address field cannot replace URL | `F-BRW-03`: Return navigates, but Ctrl-A/click-position fail and typing appends. | `rust/crates/tiller_ui/src/browser.rs` address input bridge; implement selection/caret behavior. |
| Browser failure contract is false-success | `F-BRW-04`: raw `browser.open` silently substitutes example.com; invalid navigations return success, blank title, no error. Kept separate from the six-row control cluster because this evidence does not establish the same dispatch arm. | browser command/error handling in `rust/crates/tiller/src/main.rs`; make invalid/open-failure outcomes explicit. |
| Restored agent panes lack notification identity | `F-USE-06`: notifier path exists but `pane_agents` is written only at spawn; restored panes never call `register_agent_id`. | `rust/crates/tiller/src/main.rs` restore/agent registration path; register restored pane identity. |
| Account-management callback is never installed | `F-SET-14`: Add Account is drawn, but `on_manage_account` has only a test caller and its documented unset state is inert. | `rust/crates/tiller_ui/src/settings.rs` and app wiring in `main.rs`; install the host-delegated action. |
| Translucency has no persistence or consumer | `F-SET-20`: setter does not call `changed()`, snapshot has no field, and no code consumes the flag. | `rust/crates/tiller_ui/src/settings.rs` plus app/window settings; persist and apply it. |
| Agent colour stops at `SettingsSnapshot` | `F-SET-22`: click/persistence test is real, but `agent_colors` has no consumer and is dropped by `app_settings_from_snapshot`. | `rust/crates/tiller_ui/src/settings.rs`, `rust/crates/tiller/src/main.rs:7582`; carry/read the setting in the agent UI. |
| Status indicators have no live signal lifecycle | `F-TERM-09`: badges appear/stick but do not track work or clear on death/relaunch; evidence ties this to dead hooks and absent title/content/process badge wiring. | activity merger and terminal/app lifecycle (not one widget); establish Linux signal sources and clearing. |
| File links have no UI click path | `F-CORE-FILE-04`: `resolve_file_link` and tests exist but have zero callers/click machinery. | `rust/crates/tiller_project/src/file_link.rs` plus editor/markdown UI; wire Cmd/Ctrl-click/open-link behavior. |
| Worktree annotation intentionally bypasses existing storage | `F-CTRL-WORK-01`: schema/model/upsert already preserve comment; `main.rs:328` deliberately labels `worktree.set` runtime-only. | `rust/crates/tiller/src/main.rs` control path; decide contract, then route to existing column if persistence is required. |
| Terminal split cache lifecycle unintegrated | `F-TERM-SPLIT-01`: left-placement defect overlaps `F-TAB-23`, but this row also requires `TerminalPaneCache` integration/recursive lifecycle. | `rust/crates/tiller_terminal` and split integration; fix cache lifecycle before claiming this full row. |

## ACP supplementary sub-rows — outside the inventory count

- `ACP-12` is the supplementary ACP detail for the `F-CHAT-15` mode-pill cause
  above; it adds no second inventory row.
- `ACP-08` records a separate ACP retry defect: retry respawns and returns idle
  without resending the prompt. Its likely location is the retry/session bridge
  under `rust/crates/tiller_acp`; retain and replay the failed turn or report its
  loss. It is analysed for builder planning, but is not one of the 47 `F-` rows.

## Accounting

The multi-row inventory causes and the dedicated test clusters cover 22 `F-`
rows (6 + 6 + 2 + 2 + 1 + 2 + 3). The one-row causes cover the remaining 25.
Total: **47 defective inventory rows**. The two ACP sub-rows above are
supplementary and are excluded from that total. No ledger row was edited: the
three superseded-pass entries still state an observed present defect, and the
browser-excuse entries state concrete failures rather than a re-drive-only
state.
