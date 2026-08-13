# Exercise recipes for the 33 unproven rows (FABLE-10)

The ledger's 13 `half-proven` + 12 `NOT EXERCISED` + 8 `builder-claimed, unverified` rows are
correctly filed: the work exists (or is claimed) and only the proof is missing. That makes them
the cheapest PASSED conversions on the board — each needs a route, not construction. This doc is
the routes.

**A recipe says what to DO, never what to CONCLUDE.** Nothing here marks anything PASSED; every
verdict remains `pireview`'s. For `half-proven` rows the recipe drives ONLY the missing half and
names the proven half so the critic does not re-drive it. Report what the screen (or test
runner) showed, verbatim where wording matters.

Doors re-verified against the live tree today (2026-08-13), not taken from ledger prose:
`cargo test -p tiller --no-run` compiles, EXIT 0, from `rust/` — the "incomplete SettingsSnapshot
initializer" blocker recorded inside F-CORE-ACT-27's row text is **stale**; `panes.rs` carries
zero `#[ignore]`; the terminal context menu still has exactly 10 items; the composer has a live
status pill (`chat-status` / `chat-connecting`) with offline/connecting/working/mode states; and
`add_chat_tab` still takes no `&mut Window`, so the composer structurally cannot receive focus on
tab creation — an open defect (pi's file), not a critic error.

## Group 0 — no app launch: test replays (Cluster A, 8 rows)

All eight are P50/P59 builder claims whose cited evidence IS a test. Replay from `rust/` with
`cargo test -p tiller panes::<name>` — the `panes::` filter also dodges the pre-existing
`probe_escape_dispatch` zbus flake that lives elsewhere in the bin suite. Report each runner
result line verbatim.

- `F-CORE-ACT-05` + `F-CORE-ACT-08` — run `panes::real_pty_activity_status_follows_osc_title_then_settled_content`
  and `panes::terminal_events_feed_title_and_settled_content_into_the_one_model`.
- `F-CORE-ACT-07` — run `panes::layer_a_debounce_still_suppresses_two_title_events_in_order`
  and `panes::real_pty_layer_a_debounce_suppresses_first_title_and_accepts_second`.
- `F-CORE-ACT-06` + `F-CORE-ACT-11` — run `panes::process_owned_status_survives_title_and_child_exit_events`.
- `F-CORE-ACT-09` — run `panes::process_refresh_preserves_process_ownership_until_process_gone`.
- `F-CORE-ACT-10` — no named test carries this claim: the 500ms interval is
  `process_signal_interval()` (panes.rs:25), the tick wiring `start_process_signal_refresh`
  (main.rs). Live half rides Group 2's session: `kill -9` the agent's process from another pane
  and report the wall-clock delay until the pane's status display changes.
- `F-CORE-ACT-27` — the umbrella: run `cargo test -p tiller panes::` and
  `cargo test -p tiller_terminal`; report both summary lines (pass/fail counts), plus
  `grep -c ignore rust/crates/tiller/src/panes.rs`.

Flag for the header, not a verdict: F-CORE-ACT-27's row text says the named `tiller` tests
"cannot execute in this worktree" — that clause is stale as of today (door above). The other
seven claims read consistent with the code; ACT-10 is the only one not test-backed.

## Group 1 — one plain launch, no agent needed (9 rows)

### Terminal context menu — right-click inside a terminal pane. The 10 items: Copy, Paste, Copy Context, Set Title, Copy Pane ID, Copy Terminal ID, Split Right, Split Down, Clear Terminal, Close Terminal…

- `F-TERM-04` — select output text, menu → Copy, paste into a second pane; copy a string from an
  editor tab, menu → Paste into the terminal. Report what each target received.
- `F-TERM-06` — menu → Copy Pane ID, paste into the pane; menu → Copy Terminal ID, paste. Report
  both pasted strings verbatim.
- `F-TERM-UI-01` + `F-CORE-TERM-02` — invoke each of the 10 items once (Set Title: type a name
  and confirm; Clear Terminal after `seq 100`; Close Terminal… last). Report, per item, what
  changed on screen. This one sweep covers both rows and finishes the two above.
- `F-TERM-09` — missing half: the visible indicator (the state core is already proven by socket
  transcript). Run `sleep 15`; while it runs and after it ends, report the tab's cell in the tab
  strip and the worktree dot in the sidebar. Pass 13's frames showed no change (AE=0) — report
  whatever is there, including nothing.

### Files tree — right panel (`ctrl-shift-i`), Files tab

- `F-CHG-06` — missing half: symbol-per-status and an explicit refresh (data + one frame already
  proven). Fixture one staged, one modified, one untracked file; report the symbol next to each
  row; `git add` the untracked one from a pane, click Refresh, report each row's symbol again.
  (The named drawn test for tree decorations stays owed to the builder tier, not the critic.)

### Sidebar

- `F-SID-12` — missing half: the end-to-end route (catalog test and dispatch mechanism are
  proven). Right-click a worktree row → Set as Primary; report the row's appearance and what the
  palette offers for Set/Unset Primary afterwards. Relaunch and report which worktree carries the
  marker. (The full-route drawn test `worktree_primary_context_transition_reaches_the_catalog`
  exists but sits behind the pre-existing `probe_escape_dispatch` zbus flake — builder tier.)

### Settings pixels — status-bar gear → Appearance

- `F-SET-19` — missing half: the light-theme pixel (mechanism proven by drawn test). Click the
  Light segment by hand and report what the window looks like — the automated XTEST click is
  exactly what would not land, so a live click is the point.
- `F-SET-20` — missing halves: both pixels. Toggle Translucency and report the window's material;
  step Interface font size + then −, report the visible text size. (The row already records that
  translucency is absent from the SettingsSnapshot persistence contract — an on-file finding,
  not part of this route.)

## Group 2 — one launch with an ACP agent on PATH: chat (Cluster C, 8 rows)

Shares FABLE-09's session-2 launch. Open ONE chat via the tab strip's New Chat picker (the
pass-14-proven path) and keep it for all eight rows. **The composer does not take focus on tab
creation — click it before typing** (structural: `add_chat_tab` has no `&mut Window`). The
composer's status pill is `chat-status` / `chat-connecting`.

- `F-CHAT-03` — in the first moments after creation, report the pill (dot colour + word). Then,
  after a turn, `kill -9` the agent's process from a terminal pane and report pill, composer,
  and any new transcript entry. (An "offline" pill state exists when no client is attached.)
- `F-CHAT-05` — in each state — connecting, mid-stream, and after the kill above — type in the
  composer and press Enter; report what the send control shows and whether the text left the
  composer.
- `F-CHAT-15` — after one completed turn the pill shows a mode word with `⌄` ("Ask"). Click it;
  report the entries offered; choose one; report the pill afterwards.
- `F-CHAT-20` — send a prompt that yields a long streaming reply; scroll up mid-stream and report
  whether the view keeps jumping to the bottom; scroll back down and report.
- `F-CHAT-29` — open the chat's overflow menu (⋯) and report its entries top to bottom; then
  focus the transcript, press `ctrl-c`, paste into a terminal pane, report what arrived. (Copy
  is bound as literal `cmd-c` in the ChatTranscript/ChatComposer contexts — whether `ctrl-c`
  reaches it on Linux is precisely what this exercise shows.)
- `F-CHAT-30` — ask the agent for a fenced code block; hover the rendered block and report any
  control that appears; if none, select text inside the block, `ctrl-c`, paste, report. (No
  per-block copy control was found in code — the attempt decides.)
- `F-CHAT-13` — drag a file from the desktop file manager onto the transcript, then onto the
  composer; report what happens in each place. (No file-drop handler was found in chat.rs —
  `transcript_dragging` is pointer-drag state, not a drop target. The attempt decides.)
- `F-CHAT-33` — start a turn and `kill -9` the agent mid-stream; report the transcript's last
  entry verbatim (Error entries have their own render arm). MCP warnings need an MCP-configured
  agent — if none is installed, report that as unreachable today rather than improvising one.

## Blocked halves — no launch to spend on these (5 rows + 3 half-rows)

### Cluster B rows whose missing half is the DB (P58 handoff, codex11)

`F-SET-04` `F-SET-05` `F-SET-06` `F-SET-07` `F-SET-10` — the UI halves are already proven by
named drawn tests (pass 14): do NOT re-drive them. The missing half is the persisted schema
(SettingsSnapshot keys). The route, pre-written for the day P58 lands: set each control to a
non-default value, quit, relaunch, report each control's state. Three of these rows also carry a
construction remainder that no recipe can reach: SET-05's rename behavior and SET-06's trimming
have no consumer yet, and SET-07's eviction is its own row (ACT-26).

### Not exercisable from any surface — the dependency, stated

- `F-CORE-ACT-02` — missing half is the model-level notification wiring: `build_payload` /
  `should_notify` have zero callers outside `tiller_activity` (re-checked today). Falls with
  ACT-19/20; no route exists until they are wired.
- `F-CTRL-NOTIFY-03` — missing half is system posting: zero `notify-send` /
  `org.freedesktop.Notifications` paths in any crate (re-checked today). Needs a poster first.
- `F-AGENT-SAFE-01` — missing half is skill provisioning: only the npx command builder exists
  (`tiller_project/src/skill.rs`), no management-marker or overwrite-refusal logic (pass-14
  re-check; FABLE-09 concurs). Builder-half replay: `cargo test -p tiller_project`.

## Accounting

33 rows: 8 converted by test replay with no launch (ACT-10's live half rides the agent launch),
9 in one plain launch, 8 in one agent launch that piggybacks FABLE-09's session 2, 5 blocked on
P58 with their route pre-written, and 3 halves not exercisable with the dependency named.
F-SET-05's ledger row now renders as four columns (line 293); the mis-shaped cell the FABLE-10
brief warned about survives only as narrative at line 569.
