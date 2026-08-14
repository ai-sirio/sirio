# INVENTORY-LEDGER — per-entry status of all 388 entries

Written by pireview (the critic). Pass 9 built it; pass 10 converted the bulk of
`NOT EXERCISED` into real verdicts (see `CRITIC-baseline.md` PASS 10). Pass 10 snapshot:
whole tree copied to `/tmp/critic-pass10` at **2026-08-13T12:40:06Z**; build clean in
14.14 s; headless launch served `TILLER_SOCKET=/tmp/critic10.sock`; live exercises: real
claude usage fetch (Success 4%/30%), `sleep` pane surviving select-workspace away/back.
Pass 9 snapshot: whole tree copied to
`/tmp/critic-pass9` at 2026-08-13T12:27:17Z (14:27:17 CEST); `cargo build -p tiller
-p tiller_control` clean in 4.87 s; headless launch served `TILLER_SOCKET=/tmp/critic9.sock`
(ping pongs, 42 methods advertised, `browser.*` answers its specific unsupported error,
the P37 mutation doors `surface.changes.stage/unstage/discard/stage_all/discard_all` exist).

Pass 12 snapshot: whole tree copied to `/tmp/critic-pass12`; build clean with shared
`CARGO_TARGET_DIR=/tmp/critic-target`; all suites run from the snapshot. Pass 12 applied
`PASSED-AUDIT.md` (fable): 10 false PASSEDs confirmed and re-marked, 4 unreplayable rows
given named drawn tests or downgraded, 4 doubts resolved, `F-SID-14` and six `F-GIT` stale
FAILEDs flipped, Totals recomputed from the body. New critic tests added in the snapshot:
projects-header/add/rows render, filter narrows+restores, project chevron hide/restore
(all sidebar.rs). Two P50 bin tests fail deterministically (see findings log).
Pass 11 snapshot: whole tree copied to `/tmp/critic-pass11` at **2026-08-13T13:26:20Z**.
The live tree did not compile (builder mid-edit in `main.rs:3596`, split-path refactor —
repaired in the snapshot only, never the live tree). All quiet-crate suites green
(120 tiller_ui lib, 67 tiller bin, 27 tiller_usage, 46 tiller_git, persistence/activity/
project/agents/acp/markdown all green); a mid-pass disk outage invalidated some transient
results, all re-run green afterwards. Headless launches served `TILLER_SOCKET=/tmp/critic11.sock`
with throwaway `TILLER_DB`s. Live exercises: pane launch-error surface for a missing CLI,
close-cancels-registration (6 ms), socket disable effect, scrollback capture->quit->relaunch->replay
(restored pane showed the previous session's 15:54 prompt inside a 15:56 session), `$SHELL`
preference (sh prompt under SHELL=/bin/sh), 1 MiB line-cap boundary pinned. New critic tests
added in the snapshot: usage timeout+PTY-termination, activity-row click/close, remove-project
confirmation prompt, socket disable effect. Pass 11 converted the entire half-proven bucket
(see `CRITIC-findings-log.md` PASS 11).

One row per entry from both inventory files, in inventory order. This ledger consolidates
critic passes 1–8 from `CRITIC-baseline.md`; pass 9 exercised nothing new beyond the
snapshot health check and the environment fact that `opencode`/`omp` are absent from PATH.
**The ledger wins over `INVENTORY-STATUS.md` wherever they disagree.**

Verdict vocabulary: `PASSED` · `FAILED — absent` · `FAILED — defective` ·
`UNREACHABLE` (with reason) · `N/A — platform` · `NOT EXERCISED — blocked on display`
(appearance only) · `NOT EXERCISED` · `half-proven` (with which half).
`judged` names the critic pass that established the verdict, or
`builder-claimed, unverified` / `never claimed` where no independent critic pass has
touched the entry — those rows do **not** count toward done.

## App target — `01-inventory-app.md` (217)

### Window and application shell (12)

| id | verdict | evidence | judged |
|---|---|---|---|
| `F-WIN-01` | FAILED — absent | gear→Settings→Back real (pass 1 display; pass 8 socket surface.settings.open/select/read live); the clause's `⌘,` chord has no binding — no `ctrl-,` chord in any crate (exact grep, pass 12) | pass 12 |
| `F-WIN-02` | PASSED | ctrl-t chord (main.rs:115) dispatches `NewTerminalTab` → `NewTabAction::NewTerminal`; replayed `linux_window_command_chords_dispatch_typed_shell_actions` (pass 14) plus live tab-strip exercise this pass | pass 14 |
| `F-WIN-03` | PASSED | replayed `linux_window_command_chords_dispatch_typed_shell_actions` and `save_command_is_disabled_without_an_active_file_and_explains_why` — Ctrl+O opens the picker and Ctrl+S routes through `handle_save_file` with typed `NoActiveFile` gating (pass 14) | pass 14 |
| `F-WIN-04` | PASSED | replayed `titlebar_controls_emit_shell_visibility_events` (tiller_ui, green) and the chord test — Ctrl+Shift+S dispatches `ToggleSidebar`; caveat: the cited palette test is in the currently-failing tiller-bin cluster (owner codex12, see findings log pass 14), the sidebar route itself is independently green | pass 14 |
| `F-WIN-05` | PASSED | replayed `linux_window_command_chords_dispatch_typed_shell_actions` + `titlebar_controls_emit_shell_visibility_events` — Ctrl+Shift+I dispatches `ToggleRightPanel` (pass 14) | pass 14 |
| `F-WIN-06` | FAILED — defective | user ruling 2026-08-13: browser feature is IN scope — "niente webview" bans an Electron-style shell, not a web engine behind the browser surface; the pass-8 N/A was wrong. Browser tab absent: `NewTabAction::NewBrowser` is an empty match arm (main.rs:4215) yet the production menu still offers "New Browser" (tab_bar.rs:561) — a live entry that does nothing, with no feedback (frame stage-menu-4.png shows it in the open menu). Asymmetry: the socket answers browser.* with a specific unsupported error; the UI is silent. P72 compositing spike queued | pass 14 |
| `F-WIN-07` | FAILED — absent | session.restore (pass 3) + restore_* tests green (pass 8); the clause's History-menu route is absent — the row disclosed this while marked PASSED; a disclosed-absent clause cannot stand as PASSED (pass 12) | pass 12 |
| `F-WIN-08` | N/A — platform | macOS hide-on-close delegate | pass 8 |
| `F-WIN-09` | N/A — platform | macOS title-bar preference | pass 8 |
| `F-WIN-10` | FAILED — absent | no toast implementation, only a theme radius token | pass 8 |
| `F-WIN-11` | N/A — platform | Sparkle updater; no Linux update code | pass 8 |
| `F-WIN-12` | N/A — platform | TCC onboarding sheet | pass 8 |

### Projects and sidebar (19)

| id | verdict | evidence | judged |
|---|---|---|---|
| `F-SID-01` | PASSED | drawn: `projects_header_add_control_and_project_rows_render` green — now a **live-repo test** (sidebar.rs; re-established pass 13 after the pass-12 copy died in /tmp with the reboot) — the Projects header's + control and all four fixture project rows render, and clicking + opens the platform picker. Pixel half: frames A2-02 (nine row text-bands), S-plus-clicked (+ hover + ashpd FileChooser request created on the D-Bus) | pass 13 |
| `F-SID-02` | PASSED | drawn: `filter_narrows_rows_and_clearing_restores_them` green — live-repo test (sidebar.rs, pass 13): keystrokes reach the filter state, matching row stays while eight others disappear, clearing restores all four project rows. Display half: frames A2-03/A2-04 — typing `waku` narrows 9 text-bands to 4, backspace restores 9 | pass 13 |
| `F-SID-03` | PASSED | drawn: + click → dir-picker → AddProject → shell add_project | pass 8 |
| `F-SID-04` | PASSED | drawn: `project_chevron_hides_and_restores_children` green — live-repo test (sidebar.rs, pass 13): chevron click reveals a collapsed project's worktree/tab rows, second click hides them. Display half: frames P1-collapsed/E1-expand — the same row toggles 9 text-bands ↔ 4 on the live display | pass 13 |
| `F-SID-05` | PASSED | drawn selection event + live select-workspace mounts tabs | pass 8 |
| `F-SID-06` | FAILED — absent | status dot computed only for RowKind::Worktree; project rows never get a badge, collapsed or not — the badge half is not built (state half real, pass 8) | pass 11 |
| `F-SID-07` | FAILED — absent | P46 builder claim: typed project-settings route and sheet; replay `sidebar_context_items_explain_git_eligibility_and_list_every_new_surface`; palette lists `Project Settings`; independent critic verification required | builder-claimed, unverified |
| `F-SID-08` | FAILED — absent | P46 builder claim: typed `InitializeGit` route with `AlreadyGitProject`; replay `sidebar_context_items_explain_git_eligibility_and_list_every_new_surface`; palette preserves the typed reason; independent critic verification required | builder-claimed, unverified |
| `F-SID-09` | FAILED — absent | P46 builder claim: context-menu `RevealInFileManager` route; replay `right_click_context_menu_dispatches_a_typed_worktree_action`; palette lists `Reveal in File Manager`; independent critic verification required | builder-claimed, unverified |
| `F-SID-10` | PASSED | pass 8 remove flow + shell remove_project real; pass 13 drawn (live-repo): `remove_project_context_item_confirms_before_emitting` green (sidebar.rs) — right-click project row → Remove Project → platform prompt up with nothing emitted, Cancel emits nothing, "Remove from Tiller" emits `RemoveProject` with the project's id (replaces the pass-11 auto-confirming-prompt test, which lived only in a wiped snapshot) | pass 13 |
| `F-SID-11` | FAILED — absent | worktree rows render path + agent-status dot only; branch/comment/primary text absent from the row render | pass 11 |
| `F-SID-12` | half-proven | catalog half: `set_primary_flips_the_application_level_marker` green (session.rs, pass 13) — set clears project siblings, unset leaves none, unknown path errors, other projects untouched. Sidebar half: the typed `ContextAction{Worktree, SetPrimary/UnsetPrimary}` dispatch is the same proven mechanism as the green NewTab context test; the menu itself photographed (H2-rc-wt). Full-route drawn test `worktree_primary_context_transition_reaches_the_catalog` exists (main.rs) but the whole tiller-bin gpui suite is currently blocked by a zbus non-determinism flake — pre-existing `probe_escape_dispatch` fails identically (findings log) — so the end-to-end route is unproven until that lands | pass 13 |
| `F-SID-13` | PASSED | drawn test against real git: worktree created | pass 8 |
| `F-SID-14` | PASSED | pass 12: drawn `right_click_context_menu_dispatches_a_typed_worktree_action` green (sidebar.rs:2125) — right-click draws the menu, New Terminal click emits typed `ContextAction{Worktree, NewTab}`; shell subscribes → `handle_sidebar_context_action` → select worktree if needed → `open_action` (main.rs:2101-2103, 2514-2584); palette route emits the same typed event (main.rs:5086-5094); open_action tab creation proven by F-TAB-03/04/05. Stale FAILED flipped — same defect as a false PASSED with the sign reversed | pass 12 |
| `F-SID-15` | FAILED — absent | pass 12: the worktree context menu (sidebar.rs:493-553) ends at New Chat — no Remove Worktree item; removal is the hover × → `remove_worktree` with NO confirmation (sidebar.rs:1055-1082, wired :1648); drawn `remove_button_removes_the_worktree` (sidebar.rs:2245) green — proves the × route, not the clause's menu+confirm route | pass 12 |
| `F-SID-16` | FAILED — absent | drag reorder removed by design; order untouched | pass 8 |
| `F-SID-17` | FAILED — absent | drag reorder removed by design | pass 8 |
| `F-SID-18` | FAILED — absent | no "No Terminals" empty state | pass 8 |
| `F-SID-19` | FAILED — absent | no ⌘T binding | pass 8 |

### Project creation and project settings (18)

| id | verdict | evidence | judged |
|---|---|---|---|
| `F-PRJ-01` | FAILED — absent | exercised on the display (pass 13): the + control opens exactly one choice — the platform folder picker (ashpd FileChooser request created on the D-Bus, frame S-plus-clicked). Clone-from-URL and Create-new-project forms do not exist anywhere in the Linux build (start_add_project prompts for a directory and nothing else) | pass 13 |
| `F-PRJ-02` | PASSED | pass 13, exercised: `tillerctl project add` fixture-proj over the socket → catalog row persisted (project p-5c1c99d8fc9dca99, worktreeCount 2) and the sidebar renders the new project (frame A2-02). The picker→event half is the green drawn `add_project_picker_reports_the_chosen_directory`; the event→shell→catalog subscription is the same proven route as F-SID-03 | pass 13 |
| `F-PRJ-03` | FAILED — absent | exercised (pass 13): adding a non-git folder succeeds silently — frame A2-02 shows plain-folder added with no prompt, no modal, no notice. The Initialize-Git / Add-without-Git / Cancel prompt does not exist in the Linux add flow | pass 13 |
| `F-PRJ-04` | FAILED — absent | pass 13: insertion failures are eprintln-only (`[projects] {error}` in main.rs) — no error surface in the UI. The only UI error path is the picker-open failure notice in start_add_project, which is not the clause's insertion error | pass 13 |
| `F-PRJ-05` | FAILED — absent | no clone-from-URL form exists in the Linux build (no clone UI, no clone socket method) — nothing to exercise | pass 13 |
| `F-PRJ-06` | FAILED — absent | no clone form exists — empty-URL disablement and double-start guards have no surface to live on | pass 13 |
| `F-PRJ-07` | FAILED — absent | no clone form exists — no failure/retry surface | pass 13 |
| `F-PRJ-08` | FAILED — absent | no create-new-project form exists — the Add flow only browses folders | pass 13 |
| `F-PRJ-09` | FAILED — absent | no create form exists — empty-name/duplicate-submission guards have no surface | pass 13 |
| `F-PRJ-10` | FAILED — absent | no create form exists — no failure surface | pass 13 |
| `F-PRJ-11` | FAILED — absent | pass 13: the Linux project-settings sheet (sidebar.rs render_project_settings) has no trash control — name, path, repository type, Close, id only. Removal lives in the context menu with a confirmation prompt (F-SID-10, drawn test green); `catalog.remove` deletes the catalog row only, never disk — but the clause's Project-Settings route is not built | pass 13 |
| `F-PRJ-12` | FAILED — absent | the settings sheet shows `Repository: Git/Folder` read-only; no repository-type switch, no display-name edit — the sheet is explicitly read-only until a persistence contract exists | pass 13 |
| `F-PRJ-13` | FAILED — absent | no icon colour/reset controls — the settings sheet renders no icon UI at all | pass 13 |
| `F-PRJ-14` | FAILED — absent | no avatar/GitHub/PNG/favicon controls exist | pass 13 |
| `F-PRJ-15` | FAILED — absent | no SF-Symbol icon grid (the sfsymbol renderer exists for in-app glyphs; there is no icon picker) | pass 13 |
| `F-PRJ-16` | FAILED — absent | no emoji picker, no project-icon emoji input | pass 13 |
| `F-PRJ-17` | FAILED — absent | no default-worktree-base options — the create-worktree prompt always derives the parent via derive_worktree_path | pass 13 |
| `F-PRJ-18` | FAILED — absent | no custom worktree location or default-parent control — the location is derived, not choosable | pass 13 |

### Tabs, panes, navigation (28)

| id | verdict | evidence | judged |
|---|---|---|---|
| `F-TAB-01` | FAILED — absent | strip renders icon/title/close; no dirty indicator anywhere (tab_is_dirty feeds only the close confirmation); status cell shows only Done ✓ — Running/Error/Idle invisible | pass 11 |
| `F-TAB-02` | PASSED | P65: `tab-overflow-button` + `tab-overflow-menu` — `visible_tab_count` hides over-budget tabs, the menu lists every hidden tab and marks the active one; replayed `drawn_all_tabs_overflow_lists_every_hidden_tab_and_marks_the_active_one` (main.rs:7761) | pass 14 |
| `F-TAB-03` | PASSED | drawn +-menu dispatches New Terminal; shell creates real tab | pass 8 |
| `F-TAB-04` | PASSED | drawn +-menu New Terminal action; shell open_action creates tab | pass 8 |
| `F-TAB-05` | PASSED | menu lists all 5 adapters -> add_agent_tab (pass 8); pass 11 live: a pane launched for a missing CLI reports the launch error in its terminal (opencode: command not found in pane scrollback) | pass 11 |
| `F-TAB-06` | N/A — platform | no browser; NewBrowser typed no-op | pass 8 |
| `F-TAB-07` | PASSED | live, pass 14+15: New Chat → picker lists exactly the installed ACP agents (Claude Code, Codex — pi/opencode/omp correctly excluded by `is_available() && acp_program().is_some()`, tab_bar.rs:467; frame pass14/m2-01-picker-open.png); choosing either opens a chat tab, and both tabs completed real agent turns over ACP (codex/claude session JSONL nonces) | pass 15 |
| `F-TAB-08` | half-proven | live, pass 15: bare PATH launch → the New Chat submenu renders the no-agent fallback (two text lines at the picker position — "Other agents…" + "No supported agent found on PATH" per render_chat_empty, frame pass15/l1-01-picker-bare-path.png); the clause's second half is absent — the fallback carries no click handler, selecting it does NOT open Agents settings | pass 15 |
| `F-TAB-09` | PASSED | P65: pane context menu has Open File → picker → `add_file_tab` editor; replayed `drawn_tab_context_open_file_uses_the_picker_and_adds_an_editor_tab` (main.rs:7787) | pass 14 |
| `F-TAB-10` | PASSED | chords + live split (pass 8, pane-3 created); pass 12: the clause's pane-menu route now exists — drawn `right_click_resolves_this_terminal_and_draws_all_context_actions` green (tiller_terminal lib.rs:1751): right-click draws all 10 items, Split Right click emits typed `TerminalContextEvent` with real pane/terminal ids; shell subscription routes to `split_terminal_at` (main.rs:2325-2341); mapping test `terminal_context_app_actions_have_workspace_routes` green | pass 12 |
| `F-TAB-11` | FAILED — absent | split_disabled_reason unit-tested but zero UI callers — never rendered; the pass-12 pane context menu (drawn test green) does not carry disabled-reason entries | pass 12 |
| `F-TAB-12` | FAILED — absent | no move-tab UI | pass 8 |
| `F-TAB-13` | FAILED — absent | no move-tab menu or empty state | pass 8 |
| `F-TAB-14` | FAILED — absent | no rename anywhere | pass 8 |
| `F-TAB-15` | FAILED — absent | ✕-close real (pass 2 display; close_tab_by_id); pass 12: no tab context menu exists — the clause's context-menu Close Tab route is absent; "Close Tab" exists only as a palette entry (drawn palette dispatch closes tabs, main.rs:6313 — a different surface from the clause's) | pass 12 |
| `F-TAB-16` | FAILED — absent | close_tab has no confirmation and no dirty check | pass 8 |
| `F-TAB-17` | FAILED — absent | no close-others/right | pass 8 |
| `F-TAB-18` | FAILED — absent | no tab drag; only pane divider drags | pass 8 |
| `F-TAB-19` | PASSED | ctrl-tab/ctrl-shift-tab bound; handler = live tab.cycle; chord fixture green | pass 8 |
| `F-TAB-20` | PASSED | ctrl-1..9 bound; handler = live tab.select; chord fixture green | pass 8 |
| `F-TAB-21` | FAILED — absent | no Tab menu | pass 8 |
| `F-TAB-22` | PASSED | ctrl-alt-arrows bound; live pane.focus moved focus | pass 8 |
| `F-TAB-23` | FAILED — absent | right/down splits real (pass 8); pass 12: the pane context menu now exists (drawn test green) but carries only right/down — no left/up split actions anywhere | pass 12 |
| `F-TAB-24` | FAILED — absent | vacuous: no tab drag to cancel | pass 8 |
| `F-TAB-25` | FAILED — absent | still no attach-to-terminal code; P65 built Move-to-Pane (a different feature) and explicitly refused this one (assigned-but-absent recheck, pass 14) | pass 14 |
| `F-TAB-26` | PASSED | pass 12: drawn `right_click_resolves_this_terminal_and_draws_all_context_actions` green (tiller_terminal lib.rs:1751) — menu draws all 10 items, clicks emit typed events with resolved pane/terminal ids; main-shell subscription routes clicked-pane split/close events (main.rs:2325-2341); mapping test `terminal_context_app_actions_have_workspace_routes` green | pass 12 |
| `F-TAB-27` | PASSED | P65: context menu Resume Chat (disabled when no retained chat) → `resume_chat` reopens a chat tab and restores the retained transcript; replayed `drawn_tab_context_resume_chat_reopens_the_retained_session` (main.rs:7833). Pass-14 caveat CLOSED by P73 (live pass 16): tab.agent_id is stored ('codex' in the DB) and restore/resume relaunch the retained agent's adapter — after quit+relaunch the Codex tab spawned codex-acp, not claude-agent-acp (frames/ps pass16/p73-*) | pass 16 |
| `F-TAB-28` | FAILED — absent | no ⌘W; ctrl-alt-w closes a pane, no-op on single tab | pass 8 |

### Chat surface over ACP (37)

| id | verdict | evidence | judged |
|---|---|---|---|
| `F-CHAT-01` | PASSED | P53 socket door replayed: `chat_door_streams_stops_and_restores_transcript_over_a_real_socket` green (control_integration, 45 green) — ChatSession start/send/readback with incremental ACP output over a real socket; UI wiring exercised live pass 14 (picker → chat → agent reply) | pass 14 |
| `F-CHAT-02` | FAILED — absent | pass 12: no auth state exists — case-insensitive `auth` over chat.rs matches once, inside a test's fake-agent script (chat.rs:3974); tiller_acp/src has zero matches. What exists is a generic connection-error banner with Retry (`Entry::Error{retryable,kind}`, chat.rs:87-91, `ErrorKind::Connection`); drawn `a_stream_that_dies_mid_reply_states_the_error_and_retry_recovers` (chat.rs:3587) + `failed_launch_can_retry_and_complete` (chat.rs:3941) green — error+Retry, not auth | pass 12 |
| `F-CHAT-03` | NOT EXERCISED | disconnected-agent state never exercised | never claimed |
| `F-CHAT-04` | PASSED | drawn enter_sends_and_shift_return_inserts_a_newline green: Return sends, Shift+Return inserts a newline and does not send (snapshot 2026-08-13T13:26Z; composer is pi's live file) | pass 11 |
| `F-CHAT-05` | NOT EXERCISED | composer-disable state never exercised | never claimed |
| `F-CHAT-06` | PASSED | drawn tests `enter_during_a_stream_queues_and_the_turn_end_sends_it_exactly_once`, `removing_the_queued_item_means_nothing_sends_when_the_turn_ends`, `stopping_via_click_with_a_queued_item_still_sends_it` all green (tiller_ui suite, 180 green, pass 14) | pass 14 |
| `F-CHAT-07` | PASSED | drawn tests `stop_click_cancels_the_stream_and_the_transcript_states_it` + `escape_cancels_the_stream_and_the_transcript_states_it` green (tiller_ui suite, pass 14) | pass 14 |
| `F-CHAT-08` | PASSED | live, pass 15: streaming observed (20 transcript bands 7s after send), the round control at the composer's right accepted a click (9.2k px state change) and the transcript settled (19 bands, stable); the agent was too fast to catch an active-stream cancel live (session JSONL shows the full 200-line reply) — the cancel path rests on drawn `stop_click_cancels_the_stream_and_the_transcript_states_it` + the P53 socket-door stop test (both green) | pass 15 |
| `F-CHAT-09` | PASSED | drawn `slash_popup_filters_and_inserts_a_skill_token` green (tiller_ui suite, pass 14) | pass 14 |
| `F-CHAT-10` | PASSED | drawn `at_mention_popup_lists_files_and_inserts_a_file_chip` green (tiller_ui suite, pass 14) | pass 14 |
| `F-CHAT-11` | PASSED | drawn `attach_control_accepts_one_image_and_rejects_the_rest` green (tiller_ui suite, pass 14) | pass 14 |
| `F-CHAT-12` | PASSED | chip × removal exercised by the drawn attach test's removal half, green (tiller_ui suite, pass 14) | pass 14 |
| `F-CHAT-13` | NOT EXERCISED | file drop into chat never exercised | never claimed |
| `F-CHAT-14` | PASSED | drawn `overflow_menu_toggles_follow_and_resets_to_a_new_conversation` green (tiller_ui suite, pass 14) | pass 14 |
| `F-CHAT-15` | NOT EXERCISED | permission-mode pill never exercised | never claimed |
| `F-CHAT-16` | FAILED — absent | pass 12: no search input, no no-match state, no "Recommended" string in chat.rs/composer.rs (grep, zero hits); drawn `model_picker_selects_an_agent_advertised_model_and_escape_dismisses` (chat.rs:3743) green — proves select+escape only | pass 12 |
| `F-CHAT-17` | PASSED | drawn `model_picker_offers_effort_levels_and_updates_the_selection` green (tiller_ui suite, pass 14) | pass 14 |
| `F-CHAT-18` | FAILED — absent | pass 12: popover renders percent-used, used/size tokens, optional Cost line (chat.rs:2388-2438); the clause's input/output/cache breakdown rows have no code; drawn `context_ring_shows_reported_usage_and_escape_dismisses_popover` (chat.rs:3791) green — usage+escape, never the breakdown | pass 12 |
| `F-CHAT-19` | PASSED | drawn `context_ring_warns_above_eighty_percent` green (tiller_ui suite, pass 14) | pass 14 |
| `F-CHAT-20` | NOT EXERCISED | tail-follow code exists (chat.rs), unexercised | pass 8 |
| `F-CHAT-21` | FAILED — absent | no Thinking expand/collapse | pass 8 |
| `F-CHAT-22` | FAILED — absent | no grouped-steps expansion | pass 8 |
| `F-CHAT-23` | FAILED — absent | pass 12: `Entry::ToolCall` carries {id,title,status} only (chat.rs:72-76); render is a static title+status row (chat.rs:1861-1875) with no click handler, no expand, no output/diff/location links, no Dismiss; the pass-1 Pending→Completed evidence is true and proves a different claim | pass 12 |
| `F-CHAT-24` | NOT EXERCISED | **the pass-8 "absent" was wrong.** `PlanApproval` in `chat.rs` is commented `(F-CHAT-24)` and carries `request_id`/`options`/`resolved`/`expired`; `PlanEntryRow` holds each plan row; test `a_plan_renders_approval_attaches_and_the_plan_advances` (`chat.rs:4719`) is named for it. Code exists and is tested — **no critic has exercised it live**, so it is not PASSED | orchestrator audit, 2026-08-14 |
| `F-CHAT-25` | NOT EXERCISED | **the pass-8 "absent" was wrong.** `AnswerTextInput` (placeholder + prefill), `actions!(chat_question_answer, [SendAnswer, CancelAnswer])`, `answer_question_text` (`chat.rs:1381`), `cancel_question` (`:1412`), `render_question_answer_row` (`:1853`), test `cancel_on_a_question_closes_it_without_an_answer` (`:4551`). Both the text answer and the cancel exist — **unexercised live** | orchestrator audit, 2026-08-14 |
| `F-CHAT-26` | NOT EXERCISED | **the pass-8 "absent" was wrong.** `pending_question()` at `chat.rs:1490` returns the pending entry and its prompt. Exists — **unexercised live** | orchestrator audit, 2026-08-14 |
| `F-CHAT-27` | NOT EXERCISED | **the pass-8 "absent" was wrong.** `PlanApproval.expired` plus test `a_question_whose_turn_ends_unanswered_expires_instead_of_waiting` (`chat.rs:4625`) — the state exists and the turn-end transition is tested. **Unexercised live** | orchestrator audit, 2026-08-14 |
| `F-CHAT-28` | FAILED — absent | no subagent task cards | pass 8 |
| `F-CHAT-29` | NOT EXERCISED | copy code exists (chat.rs), unexercised | pass 8 |
| `F-CHAT-30` | NOT EXERCISED | code-block copy never exercised | never claimed |
| `F-CHAT-31` | FAILED — absent | no chat diff preview | pass 8 |
| `F-CHAT-32` | FAILED — absent | no edit summary | pass 8 |
| `F-CHAT-33` | NOT EXERCISED | turn errors/MCP warnings never exercised | never claimed |
| `F-CHAT-34` | FAILED — absent | no chat history menu | pass 8 |
| `F-CHAT-35` | FAILED — absent | no no-past-chats empty state | pass 8 |
| `F-CHAT-36` | PASSED | drawn `no_models_fallback_shows_a_plain_agent_badge` green (tiller_ui suite, pass 14) | pass 14 |
| `F-CHAT-37` | PASSED | empty transcript + usable composer | pass 1 |

### Files, changes, activity (22)

| id | verdict | evidence | judged |
|---|---|---|---|
| `F-CHG-01` | FAILED — absent | right panel Files+Activity only; Changes moved to a Diff tab | pass 5 |
| `F-CHG-02` | FAILED — absent | pass 13, exercised: `close-workspace` → `current-workspace` = none, then `surface changes open` still serves the last worktree's data (transcript) — there is no no-worktree state and no explanation text anywhere in changes.rs/right_panel.rs. Frame CHG-02b documents it | pass 13 |
| `F-CHG-03` | half-proven | live, pass 15: Refresh button exists and works (click → 6.5k px re-render → settle; the pass-11 "no Refresh button" was stale); loading/error/Retry states exist in code (files-loading, files-error + Retry action, right_panel.rs:670-700) but unexercised live — the error path needs an induced failure (fable's honest note stands); loading is too brief to capture on a small repo | pass 15 |
| `F-CHG-04` | PASSED | drawn clicking_a_drawn_directory_row_expands_and_collapses_it green (real mouse clicks; collapse assert run_until_parked-hardened) + double_clicking_a_drawn_file_row_emits_open_file green | pass 11 |
| `F-CHG-05` | PASSED | live, pass 15: click selects a file row (13.5k px), Down moves the selection (24k px), Up moves it back, Space produces a small state change (2.4k px, dir-expansion toggle); Return not exercised | pass 15 |
| `F-CHG-06` | half-proven | pass 13: data half proven by socket transcript — `surface changes read` reports Staged/Changed/Untracked sections with correct per-file counts on a dirty fixture. Pixel half: three per-file status dots visible in the right-panel files tree (frame CHG-01). The symbol-per-status mapping and an explicit Files refresh were not exercised per-state, and no named drawn test covers the tree decorations — the drawn-test tier is owed | pass 13 |
| `F-CHG-07` | PASSED | clean/empty tree 0/0/0 ready=true matches porcelain | pass 5 |
| `F-CHG-08` | PASSED | sections+counts match porcelain through MM/RM/UU/binary | pass 5 |
| `F-CHG-09` | PASSED | drawn error/Retry click recovers from removed .git; green ×3 | pass 7 |
| `F-CHG-10` | PASSED | drawn stage/unstage mutates real checkout; green ×3 (pass 8) | pass 8 |
| `F-CHG-11` | FAILED — absent | live, pass 15: the staged header's right-edge control is the section collapse toggle (click → 36k px view change, git status before/after identical — no mutation); stage-all/discard-all stay drawn-tested only (green, pass 8); Unstage all and section-level row-moving actions still absent | pass 15 |
| `F-CHG-12` | PASSED | drawn section/row/context-band clicks + diff data tier | pass 7 |
| `F-CHG-13` | FAILED — absent | ↗ opens read-only File tab; no Open-diff action | pass 5 |
| `F-CHG-14` | PASSED | drawn discard + confirmation; green ×3 (pass 8) | pass 8 |
| `F-CHG-15` | PASSED | binary counts match numstat (pass 5); unavailable/retry halves now built and green: broken-repo retry recovers, error state renders + Retry clickable, diff-unavailable row test | pass 11 |
| `F-CHG-16` | FAILED — absent | no resolve-in-terminal action | pass 5 |
| `F-CHG-17` | PASSED | numbered lines + hunk headers match real file (data tier) | pass 5 |
| `F-CHG-18` | FAILED — absent | no drag handlers in changes.rs | pass 7 |
| `F-CHG-19` | PASSED | rows render + row-click switches tab (pass 1); pass 11 drawn: clicking a drawn activity row emits SelectActivity and clicking its close-X emits CloseActivity (no select leak) | pass 11 |
| `F-CHG-20` | FAILED — absent | builder claim confirmed by reading (pass 13): changes.rs has no activity/running-count element and renders no Activity section at all — there is nothing to photograph | pass 13 |
| `F-CHG-21` | PASSED | hover close-X renders (pass 1); pass 11 drawn: clicking the close control emits CloseActivity; host dispatch -> workspace.close_tab code-verified | pass 11 |
| `F-CHG-22` | FAILED — absent | surface ActivityStatus models 4 of the 5 claimed statuses — NeedsInput absent from the activity panel; glyph mapping is data-tier only, no variety test | pass 11 |

### Documents and editors (13)

| id | verdict | evidence | judged |
|---|---|---|---|
| `F-EDIT-01` | PASSED | clicking `Code` switches the surface from rendered Markdown to raw source with line numbers, `Code` active and `Preview` dimmed (`orch7-code-toggle.png` against `orch7-preview.png`). The P39 claim of a drawn switch, unverified for four passes, is confirmed and the toggle works | orchestrator drive, 2026-08-14 |
| `F-EDIT-02` | PASSED | formatting toolbar renders in Code mode with B, I, H, List and Link (`render_markdown_toolbar`, `file_view.rs:326`) **and acts**: with line 3 selected, clicking `B` wrapped exactly the selection — `ORIGINAL-CONTENT-MARKER` → `**ORIGINAL-CONTENT-MARKER**`, confirmed on disk after `ctrl-s`. Upgraded from 'renders' to 'exercised' | orchestrator drive, 2026-08-14 |
| `F-EDIT-03` | FAILED — absent | no manual-preview state; hardcoded 1 MiB notice instead; P39 claim unverified | pass 3 |
| `F-EDIT-04` | PASSED | Linux chord ctrl-s wired in main.rs:116 → `SaveFile` action → `handle_save_file` saves every File view in the active tab, gated on `TabKind::Editor` (`NoActiveFile` reason when disabled); replayed `linux_window_command_chords_dispatch_typed_shell_actions` — the pass-3 'no ⌘S binding' was stale | pass 14 |
| `F-EDIT-05` | FAILED — absent | the **banner** is absent, but the machinery behind it is not: `file_view.rs:1075 conflict_detection_and_resolutions_work_through_the_view` drives `check_external`, `Conflict`, and both resolutions *through the view*, asserting "Reload adopted the on-disk content" and that Keep leaves the tab dirty. So this is an unmounted UI, not a missing feature — **do not rebuild the model**, draw the banner over it | orchestrator drive, 2026-08-14 |
| `F-EDIT-06` | PASSED | save works end-to-end, live: selected line 3 (`Home`/`shift+End`), clicked `B`, pressed `ctrl-s`, and the file on disk changed from `ORIGINAL-CONTENT-MARKER` to `**ORIGINAL-CONTENT-MARKER**` — verified by `cat`, not by screenshot (`orch11-bold.png`). The pass-3 'no save path' was stale and contradicted `F-EDIT-04`; this resolves that contradiction in `F-EDIT-04`'s favour | orchestrator drive, 2026-08-14 |
| `F-EDIT-07` | PASSED | `Markdown` language badge in the editor header; fenced block labelled `bash` with comments coloured apart from commands, inline code spans in their own colour (`orch5-ctx-open.png`). Keyword-set highlighter at `file_view.rs:748`, not a grammar engine | orchestrator drive, 2026-08-14 |
| `F-EDIT-08` | PASSED | `add_file_tab` collects open File paths and, via `file_path_is_already_open` (main.rs:2073), re-selects the existing tab instead of duplicating; replayed `opening_the_same_file_twice_reuses_one_editor_tab_path` (main.rs:8402) | pass 14 |
| `F-EDIT-09` | PASSED | drawn double-click emits OpenFile green; host dispatch OpenFile -> add_file_tab code-verified (File tab constructed, active_tab switched) | pass 11 |
| `F-EDIT-10` | PASSED | right-click on a file row opens Open / Reveal in File Manager / Copy Path (`orch4-rclick-file.png`), and Open really opens the editor (`orch5-ctx-open.png`). Separate defect: the menu renders at the panel top, not at the pointer | orchestrator drive, 2026-08-14 |
| `F-EDIT-11` | PASSED | right-click → `Copy Path` places the exact absolute path on the X CLIPBOARD selection: read back with `xclip -o -selection clipboard` **while the app was still alive** (the selection dies with the owning process) and it held `/home/enzopalmisano/Scrivania/Progetti/tiller/aaa-critic-scratch.md`, the file actually clicked. The pass-7 'no copy-path code' was stale | orchestrator drive, 2026-08-14 |
| `F-EDIT-12` | FAILED — absent | no product drag; payload-drag fixture is harness-only | pass 7 |
| `F-EDIT-13` | PASSED | drawn a_missing_file_tab_reports_the_specific_state green (missing path input shows the 'does not exist' message); unreadable distinct message + binary/too-large states tested at editor model | pass 11 |

### Persistence and lifecycle (8)

| id | verdict | evidence | judged |
|---|---|---|---|
| `F-PER-01` | PASSED | replayed `chat_transcript_survives_process_relaunch_with_tool_and_permission_outcome` (persistence_integration, 34 green) and `chat_door_streams_stops_and_restores_transcript_over_a_real_socket` (control_integration, 45 green) — SQLite transcript migration/save/load + socket readback after relaunch (pass 14) | pass 14 |
| `F-PER-02` | PASSED | select-workspace → DB write → relaunch → selection restored | pass 3 |
| `F-PER-03` | PASSED | 2 splits persist across quit/relaunch (pane-1/2/3 present) | pass 4 |
| `F-PER-04` | PASSED | pass 11 live end-to-end: terminal output captured into the session DB on quit and replayed on relaunch — the restored pane scrollback showed the previous session's 15:54 prompt inside a 15:56 session; replay test green | pass 11 |
| `F-PER-05` | PASSED | session.restore restoredCount 2 + worktree re-selected | pass 3 |
| `F-PER-06` | FAILED — defective | compound-command panes orphan process groups on quit (pass 6); simple panes flush (pass 4) | pass 6 |
| `F-PER-07` | FAILED — absent | pass 13: project icon/name editing does not exist (F-PRJ-12..16 all absent), so there is nothing to persist; no settings write door exists to exercise a persistence roundtrip. The snapshot-mapping test `persisted_settings_map_to_the_ui_snapshot_and_back` covers mapping, not a quit/relaunch roundtrip | pass 13 |
| `F-PER-08` | N/A — platform | no browser on Linux | pass 2 |

### Browser (9)

| id | verdict | evidence | judged |
|---|---|---|---|
| `F-BRW-01` | FAILED — absent | no browser surface; browser.* answers specific unsupported errors (pass 8) | pass 8 |
| `F-BRW-02` | FAILED — absent | no browser surface; browser.* answers specific unsupported errors (pass 8) | pass 8 |
| `F-BRW-03` | FAILED — absent | no browser surface; browser.* answers specific unsupported errors (pass 8) | pass 8 |
| `F-BRW-04` | FAILED — absent | no browser surface; browser.* answers specific unsupported errors (pass 8) | pass 8 |
| `F-BRW-05` | FAILED — absent | no browser surface; browser.* answers specific unsupported errors (pass 8) | pass 8 |
| `F-BRW-06` | FAILED — absent | no browser surface; browser.* answers specific unsupported errors (pass 8) | pass 8 |
| `F-BRW-07` | FAILED — absent | no browser surface; browser.* answers specific unsupported errors (pass 8) | pass 8 |
| `F-BRW-08` | FAILED — absent | no browser surface; browser.* answers specific unsupported errors (pass 8) | pass 8 |
| `F-BRW-09` | FAILED — absent | no browser surface; browser.* answers specific unsupported errors (pass 8) | pass 8 |

### Status bar / usage (6)

| id | verdict | evidence | judged |
|---|---|---|---|
| `F-USE-01` | FAILED — absent | pass 12: render shows gear + three hardcoded provider segments + branch·path (status_bar.rs:200-270); the refresh control never renders — `on_refresh` is a dead builder API with zero call sites (status_bar.rs:57, 83-84); zero tests in status_bar.rs | pass 12 |
| `F-USE-02` | FAILED — absent | pass 12: `tooltip` has zero case-insensitive matches in status_bar.rs — the pass-1 evidence names a thing not in the tree; StatusBar hardcodes three providers with no visibility coupling (status_bar.rs:47-66) | pass 12 |
| `F-USE-03` | FAILED — absent | pass 12: `segment_text` renders every `Unavailable(reason)` as the same "—" (status_bar.rs:171); `UsageReason::{NotInstalled,LoggedOut,Error}` (tiller_usage model.rs:64-70) are indistinguishable on the surface; zero tests in status_bar.rs; only Loaded was ever observed (pass 1 Codex 67%) | pass 12 |
| `F-USE-04` | N/A — platform | menu-bar-only AgentRosterView; no Linux counterpart | pass 10 |
| `F-USE-05` | N/A — platform | depends on the absent roster | pass 10 |
| `F-USE-06` | FAILED — absent | NotificationPolicy exists in crate; zero app callers of should_notify/build_payload — no delivery path | pass 10 |

### Control socket automation (9)

| id | verdict | evidence | judged |
|---|---|---|---|
| `F-AUTO-01` | PASSED | drawn toggle + resolved socket path (pass 7); pass 11: controller test green — enable listens (connect ok), disable refuses connections, re-enable listens again | pass 11 |
| `F-AUTO-02` | PASSED | panel.create live | pass 3 |
| `F-AUTO-03` | PASSED | panel split/list/write/key/read/wait/focus/close live | pass 3 |
| `F-AUTO-04` | PASSED | notify + worktree.set + session.ref live | pass 3 |
| `F-AUTO-05` | PASSED | workspace list/current/select/create/close live | pass 3 |
| `F-AUTO-06` | FAILED — absent | socket create/list/clear round-trip live (pass 3); the delivery conjunct has no path — F-USE-06 records zero app callers of should_notify/build_payload; in-memory listing is not delivery (pass 12) | pass 12 |
| `F-AUTO-07` | PASSED | ping/identify/capabilities live | pass 3 |
| `F-AUTO-08` | PASSED | session.restore restoredCount 2 | pass 3 |
| `F-AUTO-09` | N/A — platform | no browser; explicit unsupported error exercised | pass 3 |

### Settings (25)

| id | verdict | evidence | judged |
|---|---|---|---|
| `F-SET-01` | PASSED | categories render; live surface.settings.select | pass 8 |
| `F-SET-02` | PASSED | live, pass 15: Settings opened via the control socket (the status-bar gear of the recipe does not exist — on_settings is never wired to a rendered element, status_bar.rs), then Escape closed the surface and returned the main UI (634k px delta, frame pass15/j3-02). Caveat: the workspace-level drawn test `escape_closes_the_settings_surface` still fails in the tiller bin (the palette-test harness cluster — owner codex12), while the live feature works | pass 15 |
| `F-SET-03` | PASSED | Check for Updates removed by design — no updater on Linux (zero Sparkle/update-channel code in the tree, pass 14 re-check); drawn absence test `general_settings_state_the_version_and_no_dead_controls` green (tiller_ui suite, 180 green) | pass 14 |
| `F-SET-04` | half-proven | UI half verified: drawn `general_surface_settings_flow_into_the_persistence_contract` green (tiller_ui suite, pass 14); DB half still absent — the persisted schema does not carry the surface-settings keys yet (main.rs test comment: loaded snapshots start at defaults; P58 handoff to codex11 open) | pass 14 |
| `F-SET-05` | half-proven | UI half verified: drawn `general_surface_settings_flow_into_the_persistence_contract` + `summarizer_picker_is_gated_on_auto_naming_and_selects` green (tiller_ui suite, pass 14); DB half absent — persisted schema lacks the key (P58 handoff open); rename behavior still has no consumer | pass 14 |
| `F-SET-06` | half-proven | UI half verified: drawn `general_surface_settings_flow_into_the_persistence_contract` green (tiller_ui suite, pass 14); DB half absent — persisted schema lacks the keys (P58 handoff open); trimming still has no consumer | pass 14 |
| `F-SET-07` | half-proven | UI half verified: drawn `general_surface_settings_flow_into_the_persistence_contract` green (tiller_ui suite, pass 14); DB half absent — persisted schema lacks the keys (P58 handoff open); eviction stays absent (ACT-26 is its own row) | pass 14 |
| `F-SET-08` | PASSED | drawn toggle + resolved path (pass 8); Copy install removed by design — no install mechanism exists on this platform (F-CTRL-CLI-02's absent row); drawn absence test `general_settings_state_the_version_and_no_dead_controls` green (pass 14) | pass 14 |
| `F-SET-09` | FAILED — defective | provisioner exists and is tested (`tiller_project/skill.rs` `agent_skill_install_command`) — the pass-6 absent claim is stale — but it has zero app callers: the settings Install Skill button (settings.rs:1739) renders with an empty handler `|_, _, _| {}` and clicking does nothing | pass 14 |
| `F-SET-10` | half-proven | UI half verified: drawn `refresh_now_re_runs_provider_discovery` + `settings_visibility_toggles_reach_the_usage_bar` + `usage_bar_consumes_visibility_and_interval_preferences` green (tiller_ui suite, pass 14); DB half absent — persisted schema lacks the keys (P58 handoff open) | pass 14 |
| `F-SET-11` | FAILED — absent | Loading/Loaded/Stale + dimming visible; the four unavailable reasons (Not found/Logged out/Timed out/Error) all render the same '—' — 4 visuals for 7 claimed states | pass 11 |
| `F-SET-12` | FAILED — absent | no cookie UI or state | pass 10 |
| `F-SET-13` | FAILED — absent | no cookie UI or state | pass 10 |
| `F-SET-14` | FAILED — absent | status-only provider cards; no add/re-auth/remove | pass 10 |
| `F-SET-15` | FAILED — absent | no multi-account model | pass 10 |
| `F-SET-16` | FAILED — absent | pass 12: "Search agents" is a static text child in a pill-shaped div, not an input (settings.rs:1183); Refresh's handler is the literal no-op `|_, _, _| {}` (settings.rs:1187-1189); no timestamp exists; drawn `agent_rows_render_what_discovery_found` (settings.rs:1839) green proves rows only — every interactive conjunct is dead chrome (contradicts F-SET-17 FAILED, same absent registry) | pass 12 |
| `F-SET-17` | FAILED — absent | no agent registry | pass 10 |
| `F-SET-18` | FAILED — absent | availability badges only; no install/update/retry actions | pass 10 |
| `F-SET-19` | half-proven | pass 13: mechanism proven by the live-repo drawn test `appearance_controls_drive_theme_translucency_and_font_size` (settings.rs, green) — clicking the Light segment flips snapshot.theme to Light. Pixel half: the Appearance page is photographed (SET-01, theme segment card visible) but no light-theme frame was captured — the display click on the segment would not land (XTEST flake) and the socket has no settings write door | pass 13 |
| `F-SET-20` | half-proven | pass 13: the same named drawn test proves the translucency toggle and the interface font stepper (+13→14) mutate the surface state. Pixel halves (window material change, text size change) not captured; also a finding: translucency is NOT in the SettingsSnapshot persistence contract (P58 rule) — the toggle's value is dropped on save | pass 13 |
| `F-SET-21` | FAILED — absent | pass 12: exactly one Files icon choice on Linux — `SEGMENTED_FILE_ICONS=["Material"]` (settings.rs:30), `file_icon_choices()` (settings.rs:161-168); drawn `selecting_the_listed_file_icon_set_changes_the_snapshot` (settings.rs:2109) clicks segment 0 and asserts the only possible value — nothing can change; plus F-CORE-FILE-08: the tree renders only generic File/FolderFill icons | pass 12 |
| `F-SET-22` | FAILED — absent | pass 13: the Agent Colors section renders five rows with coloured glyphs and display-only `color_swatch` pills (no on_click — no colour choice exists to exercise). Agents page photographed (SET-02) | pass 13 |
| `F-SET-23` | N/A — platform | TCC permissions | pass 8 |
| `F-SET-24` | N/A — platform | browser-origin permissions | pass 8 |
| `F-SET-25` | N/A — platform | TCC refresh on activate | pass 8 |

### Terminals and agents — app (11)

| id | verdict | evidence | judged |
|---|---|---|---|
| `F-TERM-01` | PASSED | real login shell on pts, breadcrumb | pass 1 |
| `F-TERM-02` | FAILED — absent | no empty-pane prompt | pass 7 |
| `F-TERM-03` | FAILED — absent | exit-status data real via panel.state (0/3/137); no surface renders exit/signal status — the strip shows only activity-layer Done ✓ | pass 11 |
| `F-TERM-04` | NOT EXERCISED | pass 12: the terminal context menu now exists (10 items, drawn test green, tiller_terminal lib.rs:1751); Copy/Paste handlers real (lib.rs:718-750: selection/scrollback → clipboard; clipboard → input) — per-action clipboard effects not drawn-tested | pass 12 |
| `F-TERM-05` | PASSED | pass 12: drawn `right_click_resolves_this_terminal_and_draws_all_context_actions` green (tiller_terminal lib.rs:1751) proves delegated typed events; mapping test `terminal_context_app_actions_have_workspace_routes` covers SetTitle; shell routes SetTitle → `set_terminal_title` → `tab.title = "Terminal {id}"` (main.rs:2436-2451); copy-context/clear are terminal-local handlers (lib.rs:718-750) | pass 12 |
| `F-TERM-06` | NOT EXERCISED | pass 12: the terminal context menu now exists (drawn test green); CopyPaneId/CopyTerminalId write the real identity strings to the clipboard (lib.rs:737-742) — clipboard content not drawn-tested | pass 12 |
| `F-TERM-07` | PASSED | agent launch/identity real for installed CLIs (pass 4/6); pass 11 live: choosing a missing CLI still creates the terminal and the pane reports the launch error | pass 11 |
| `F-TERM-08` | FAILED — defective | process-group leak on close/quit; no confirmation prompt | pass 6 |
| `F-TERM-09` | half-proven | pass 13: state core re-proven by transcript — `panel state` reports running for a live `sleep 15` pane. Pixel half NOT observed: frames TERM-11/TERM-12 are pixel-identical (AE=0 across the running→finished transition) — no visible indicator change in the tab strip or sidebar across the transition | pass 13 |
| `F-TERM-10` | PASSED | LIVE pass10: sleep 300 survived select-workspace away+back via socket; scrollback intact | pass 10 |
| `F-TERM-11` | FAILED — absent | no no-worktree empty state | pass 7 |

## Package tier — `02-inventory-packages.md` (171)

### F-CORE-ACT — agent activity model (27)

| id | verdict | evidence | judged |
|---|---|---|---|
| `F-CORE-ACT-01` | PASSED | 70 tests green; status.rs 4 states+labels+priority; status_for_panes priority test | pass 10 |
| `F-CORE-ACT-02` | half-proven | sidebar half live (tab_status reads activity.status; FABLE-04 exoneration stands) and socket notify wired; the model-level transition-driven notification half (build_payload/should_notify) has zero callers — falls with ACT-19/20 | pass 14 |
| `F-CORE-ACT-03` | PASSED | agent_spawned sets running+identity, returns () by construction; test asserts no transition | pass 10 |
| `F-CORE-ACT-04` | PASSED | from_exit_code 0->done else error; apply_exit_result tests incl untracked/closed->None | pass 10 |
| `F-CORE-ACT-05` | PASSED | replayed `panes::tests::real_pty_activity_status_follows_osc_title_then_settled_content` — ok (pass 16, full panes:: run: 16 passed 2 failed; this test among the 16) | pass 16 |
| `F-CORE-ACT-06` | FAILED — defective | the claim's own cited test `panes::tests::process_owned_status_survives_title_and_child_exit_events` FAILS, reproducibly (pass 14 and pass 16 runs, panes.rs untouched since 19:39 — stable, not transient): process-owned state does not survive the title+child-exit sequence the claim describes | pass 16 |
| `F-CORE-ACT-07` | FAILED — defective | one of the claim's two cited tests fails reproducibly: `layer_a_debounce_still_suppresses_two_title_events_in_order` ok, but `real_pty_layer_a_debounce_suppresses_first_title_and_accepts_second` FAILED (pass 16) — the real-PTY debounce half does not hold | pass 16 |
| `F-CORE-ACT-08` | PASSED | replayed `panes::tests::terminal_events_feed_title_and_settled_content_into_the_one_model` — ok (pass 16) | pass 16 |
| `F-CORE-ACT-09` | PASSED | replayed `panes::tests::process_refresh_preserves_process_ownership_until_process_gone` — ok (pass 16, among the 16 passing panes:: tests) | pass 16 |
| `F-CORE-ACT-10` | builder-claimed, unverified | Layer-D tick calls the bounded existing `/proc` walk with the pane shell PID every 500ms | P50 builder claim |
| `F-CORE-ACT-11` | FAILED — defective | the claim's cited test `panes::tests::process_owned_status_survives_title_and_child_exit_events` FAILS reproducibly (pass 16; stable since pass 14) — ownership-gated clearing does not survive the described sequence | pass 16 |
| `F-CORE-ACT-12` | PASSED | overturn pass9 UNREACHABLE: pure classification, every clause case unit-tested (glyphs/pi/pi:/names/bare spinner) | pass 10 |
| `F-CORE-ACT-13` | PASSED | overturn pass9 UNREACHABLE: detect_claude/detect_pi_family; every clause mapping tested | pass 10 |
| `F-CORE-ACT-14` | PASSED | WAITING/IDLE/WORKING keyword lists + boundary negatives ("already","reworking",codex-notes) tested | pass 10 |
| `F-CORE-ACT-15` | PASSED | overturn pass9 UNREACHABLE: content.rs matches Swift ScreenManifest; proceed/esc/y-n/confirm/nonmatch tested | pass 10 |
| `F-CORE-ACT-16` | PASSED | strip_ansi CSI+OSC(BEL+ST) test; content detector uses stripped text | pass 10 |
| `F-CORE-ACT-17` | PASSED | status priority tested; identity picks status-priority pane == Swift agentIdForWorktree (clause wording imprecise); zero app callers — package capability proven, wiring owed (`agent_id_for_panes` dead, model.rs:377; DEAD-MODELS FABLE-05, re-swept pass 14) | pass 14 |
| `F-CORE-ACT-18` | PASSED | running_agent_ids dedup + catalog-order test; zero app callers — package capability proven, wiring owed (model.rs:399; DEAD-MODELS FABLE-05, re-swept pass 14) | pass 14 |
| `F-CORE-ACT-19` | FAILED — defective | `build_payload` (model.rs:420) has zero out-of-crate callers — nothing is ever emitted; the payload tests are real but built ≠ emitted (FABLE-04 overturn, re-swept pass 14: still dead) | pass 14 |
| `F-CORE-ACT-20` | FAILED — defective | `should_notify` (notification.rs:19) has zero callers — there is no delivery to compare; the suppression-rule tests prove policy, not delivery (FABLE-04 overturn, re-swept pass 14) | pass 14 |
| `F-CORE-ACT-21` | PASSED | rows test: terminal+chat kept, doc/diff/browser omitted; None->Idle code-verified (test gap: no unrecognized-pane row) | pass 10 |
| `F-CORE-ACT-22` | PASSED | sorted+urgent_first tests; sort_by_key stable for ties; the `urgent_first` half has zero app callers — package capability proven, wiring owed (sort.rs:20; DEAD-MODELS FABLE-05 partial, re-swept pass 14); the sorted half stays live | pass 14 |
| `F-CORE-ACT-23` | PASSED | requires_close_confirmation tested for all five states; zero app callers — package capability proven, wiring owed (activity.rs:30; DEAD-MODELS FABLE-05, re-swept pass 14; F-TERM-08 is the consumer-side row) | pass 14 |
| `F-CORE-ACT-24` | FAILED — absent | planner resumable/prunable split tested; zero app callers of bootstrap::partition — dead code (snapshot 2026-08-13T13:26Z) | pass 11 |
| `F-CORE-ACT-25` | FAILED — absent | partition order (selected/open/deferred) tested; zero app callers — launch remount planning never invoked | pass 11 |
| `F-CORE-ACT-26` | FAILED — absent | ids_to_evict tested; zero app callers — no eviction side effect exists | pass 11 |
| `F-CORE-ACT-27` | PASSED | quarantines genuinely removed: `grep -c ignore panes.rs` = 0 and the full `panes::` filter executes (16 passed, 2 failed — the 2 failures are ACT-06/11/07's real-PTY tests, named there, not quarantines); `cargo test -p tiller_terminal` 22 green (pass 14/16); the row's stale clauses (SettingsSnapshot blocker, formatting gate) are gone — the bin compiles | pass 16 |

### F-CORE — domain, files, usage, workspace (45)

| id | verdict | evidence | judged |
|---|---|---|---|
| `F-CORE-DOM-01` | PASSED | project + both worktrees round-trip DB across restart | pass 2 |
| `F-CORE-DOM-02` | PASSED | defaults_prefer_explicit_values_then_primary_and_sibling test | pass 10 |
| `F-CORE-DOM-03` | FAILED — absent | default_project_base code-verified (TILLER_PROJECTS_DIR -> XDG -> HOME/Tiller/projects); no test and zero app callers — dead function | pass 11 |
| `F-CORE-DOM-04` | PASSED | 5 filter tests: branch match, case-insensitive, empty query, nested tab title | pass 10 |
| `F-CORE-DOM-05` | PASSED | ordering_ignores_unknown_and_noop_moves | pass 10 |
| `F-CORE-DOM-06` | PASSED | tab_order_wraps_and_numeric_selection_validates | pass 10 |
| `F-CORE-DOM-07` | PASSED | auto_naming_requires_first_run_or_both_throttles; zero app callers — package capability proven, wiring owed (`should_request`/`record_request` dead, domain.rs:100/106; DEAD-MODELS FABLE-05, re-swept pass 14) | pass 14 |
| `F-CORE-DOM-08` | PASSED | once_gate_runs_only_the_first_callback | pass 10 |
| `F-CORE-WSP-01` | PASSED | legacy_content_exposes_only_terminal_panes_and_chat_tab_activity | pass 10 |
| `F-CORE-WSP-02` | PASSED | ids tests + document_identity_is_worktree_scoped_and_resolves_symlinks | pass 10 |
| `F-CORE-WSP-03` | PASSED | empty layout + InvalidFraction(1001) tested; binary split code-verified | pass 10 |
| `F-CORE-WSP-04` | FAILED — absent | all 8 commands + classify tested; LayoutCommand has zero app callers (dead enum) | pass 11 |
| `F-CORE-WSP-05` | PASSED | command_classes_distinguish_structural_and_nonstructural_changes | pass 10 |
| `F-CORE-WSP-06` | PASSED | validate() covers full reject list; malformed snapshot falls back empty | pass 10 |
| `F-CORE-WSP-07` | PASSED | snapshots_are_versioned_canonical_and_malformed_data_falls_back_empty | pass 10 |
| `F-CORE-WSP-08` | FAILED — absent | WorkspaceTabViewState has all fields; session store never persists view_state | pass 11 |
| `F-CORE-FILE-01` | PASSED | tree tests: path hazards, .git excluded, dirs-first localized sort | pass 10 |
| `F-CORE-FILE-02` | PASSED | classify tests: supported/unsupported/oversize/relative/absolute | pass 10 |
| `F-CORE-FILE-03` | FAILED — absent | shell-quoting tested (spaces/quotes/non-ASCII); zero app callers — the quoted string is never written to a pane | pass 11 |
| `F-CORE-FILE-03A` | N/A — platform | NSItemProvider loader Apple-only; no Linux multi-file ordered resolver (single-path classification covers Linux) | pass 10 |
| `F-CORE-FILE-04` | FAILED — defective | `resolve_file_link` (file_link.rs:12) has zero callers outside its own crate and no click-to-open machinery exists (cmd/ctrl-click\|open_link\|hovered_link: zero); the link tests are real, nothing wires a click to them (FABLE-04 overturn, re-swept pass 14) | pass 14 |
| `F-CORE-FILE-05` | PASSED | wrap/prefix selection-preserving tests | pass 10 |
| `F-CORE-FILE-06` | FAILED — absent | atomic save/reload/conflict/deletion tested; FileSystemEventMonitor (inotify) has zero callers outside its crate — watcher->auto-reload never wired, rename never surfaced | pass 11 |
| `F-CORE-FILE-07` | PASSED | inotify monitor + real create/modify/remove events test | pass 10 |
| `F-CORE-FILE-08` | FAILED — absent | no per-file icon key lookup (only generic File/FolderFill icons) | pass 10 |
| `F-CORE-TERM-01` | PASSED | xterm byte table tested for all 9 keys | pass 10 |
| `F-CORE-TERM-02` | NOT EXERCISED | pass 12: the menu with all 10 actions now exists (context_menu.rs ITEMS) and is drawn-tested green (tiller_terminal lib.rs:1751); per-action clipboard/title/split/clear/close effects not individually exercised | pass 12 |
| `F-CORE-TERM-03` | PASSED | SplitTree split/remove-with-collapse/leaf enumeration tests | pass 10 |
| `F-CORE-SET-01` | FAILED — absent | refresh clamp + TILLER_SOCKET_ENABLE policy tested; resume/autoname/translucency/retention/mount-cap/widths exist in SettingsPolicy but are never persisted (AppSettings: 5 keys) — report-only | pass 11 |
| `F-CORE-SET-02` | N/A — platform | TCC model macOS-only (matches F-SET-23 precedent) | pass 10 |
| `F-CORE-USG-01` | PASSED | from_percent clamps 0-100, non-finite->0; has_any; reducer tests | pass 10 |
| `F-CORE-USG-02` | PASSED | real wham capture + secondary window + labels + malformed + ANSI tests | pass 10 |
| `F-CORE-USG-03` | PASSED | accepts_only_the_expected_usage_percent_field | pass 10 |
| `F-CORE-USG-04` | PASSED | cookie normalization + workspace id + real react-flight capture test | pass 10 |
| `F-CORE-USG-05` | PASSED | CODEX_HOME+~/.codex, token merge-save, 8-day refresh tests; the 8-day-refresh half (`needs_refresh`, codex.rs:51) has zero app callers — package capability proven, wiring owed (DEAD-MODELS FABLE-05 partial, re-swept pass 14); the merge-save half stays live | pass 14 |
| `F-CORE-USG-06` | FAILED — absent | 401 classification tested; transport is hardcoded curl+URL — no injectable seam, controlled success/other-error HTTP tests impossible; live refresh would rotate real tokens | pass 11 |
| `F-CORE-USG-07` | FAILED — absent | fetch bounded + outcome mapping tested; no injectable transport and a live exercise would refresh the user's real tokens (last_refresh unset) — unexercisable as built | pass 11 |
| `F-CORE-USG-08` | PASSED | LIVE pass10: real claude PTY fetch -> Success (session 4%, weekly 30%) in 11.6s; bounded/not-installed paths in code | pass 10 |
| `F-CORE-USG-09` | PASSED | reducer stale-on-timeout + provider catalog/preference tests | pass 10 |
| `F-CORE-AUTH-01` | PASSED | claude json + codex first-nonempty-line identity tests; zero app callers — package capability proven, wiring owed (`parse_claude_json` dead, account.rs:50; DEAD-MODELS FABLE-05, re-swept pass 14) | pass 14 |
| `F-CORE-AUTH-03` | N/A — platform | Keychain macOS-only; no keyring store built; absence handled explicitly (unknown state test) | pass 10 |
| `F-CORE-UI-01` | PASSED | appearance_follows_system_only_in_system_mode | pass 10 |
| `F-CORE-UI-02` | PASSED | updater_reaches_every_user_visible_state_and_clamps_progress | pass 10 |
| `F-CORE-AUTH-02` | PASSED | exact non-GUI install command exposed + tested | pass 10 |
| `F-CORE-PLAT-01` | N/A — platform | macOS-15 manifest does not exist in the rewrite; VERIFY itself says reference gap | pass 10 |

### F-CTRL — control socket (34)

| id | verdict | evidence | judged |
|---|---|---|---|
| `F-CTRL-WIRE-01` | PASSED | NDJSON framing, id echo, sorted keys on the wire | pass 6 |
| `F-CTRL-WIRE-02` | PASSED | replayed `oversized_complete_request_line_is_rejected_before_dispatch` green (control_integration, 45 green, pass 14) — cap check before complete-line dispatch | pass 14 |
| `F-CTRL-WIRE-03` | PASSED | fresh connection per round trip; canonical error | pass 6 |
| `F-CTRL-WIRE-04` | PASSED | TILLER_SOCKET wins; XDG default created 0600 | pass 6 |
| `F-CTRL-PANEL-01` | PASSED | create/split/list/write/key/read/wait/focus/close live on the wire | pass 6 |
| `F-CTRL-PANEL-02` | PASSED | create/split/list/write/key/read/wait/focus/close live on the wire | pass 6 |
| `F-CTRL-PANEL-03` | PASSED | create/split/list/write/key/read/wait/focus/close live on the wire | pass 6 |
| `F-CTRL-PANEL-04` | PASSED | create/split/list/write/key/read/wait/focus/close live on the wire | pass 6 |
| `F-CTRL-PANEL-05` | PASSED | create/split/list/write/key/read/wait/focus/close live on the wire | pass 6 |
| `F-CTRL-PANEL-06` | PASSED | create/split/list/write/key/read/wait/focus/close live on the wire | pass 6 |
| `F-CTRL-PANEL-07` | PASSED | create/split/list/write/key/read/wait/focus/close live on the wire | pass 6 |
| `F-CTRL-PANEL-08` | PASSED | create/split/list/write/key/read/wait/focus/close live on the wire | pass 6 |
| `F-CTRL-PANEL-09` | PASSED | create/split/list/write/key/read/wait/focus/close live on the wire | pass 6 |
| `F-CTRL-NOTIFY-01` | PASSED | agent statuses accepted, unknown rejected; user mode listable | pass 6 |
| `F-CTRL-NOTIFY-02` | PASSED | replayed `tillerctl_rejects_ambiguous_notify_modes` green (pass 14) — title combined with agent-mode options errors before socket use | pass 14 |
| `F-CTRL-SESSION-01` | PASSED | session.ref survived relaunch; resumed real claude session | pass 6 |
| `F-CTRL-WORK-01` | FAILED — defective | worktree.set comment gone after relaunch; no comment column | pass 6 |
| `F-CTRL-SYS-01` | PASSED | replayed `tillerctl_ping_prints_pong` green (tillerctl bin tests, pass 14) — app responds with documented `{pong:true}` | pass 14 |
| `F-CTRL-SYS-02` | PASSED | explicit workspace / TILLER_PANE_ID env / fallback / no-context live | pass 6 |
| `F-CTRL-WORK-02` | PASSED | rows sorted, selected flag live | pass 6 |
| `F-CTRL-WORK-03` | PASSED | non-git project rejected with distinct error | pass 6 |
| `F-CTRL-WORK-04` | PASSED | select by id and exact path, current, no-selection error | pass 6 |
| `F-CTRL-WORK-05` | PASSED | replayed `workspace_close_terminates_process_group_when_worktree_is_missing` green (control_integration, pass 14) | pass 14 |
| `F-CTRL-NOTIFY-03` | half-proven | create/list/clear half live (pass 6); system-posting half absent — zero notify-send/zbus/org.freedesktop.Notifications paths outside tiller_theme's unrelated portal code (pass 14 re-check); the pass-6 'platform N/A' was absorption, Linux has org.freedesktop.Notifications (FABLE-04) | pass 14 |
| `F-CTRL-SESSION-02` | PASSED | restore-session returns success, no row duplication | pass 6 |
| `F-CTRL-BROWSER-01` | N/A — platform | documented unsupported responses, all 10 methods exercised | pass 6 |
| `F-CTRL-BROWSER-02` | N/A — platform | documented unsupported responses, all 10 methods exercised | pass 6 |
| `F-CTRL-BROWSER-03` | N/A — platform | documented unsupported responses, all 10 methods exercised | pass 6 |
| `F-CTRL-BROWSER-04` | N/A — platform | documented unsupported responses, all 10 methods exercised | pass 6 |
| `F-CTRL-BROWSER-05` | N/A — platform | documented unsupported responses, all 10 methods exercised | pass 6 |
| `F-CTRL-BROWSER-06` | N/A — platform | documented unsupported responses, all 10 methods exercised | pass 6 |
| `F-CTRL-CLI-01` | PASSED | replayed `tillerctl_accepts_socket_before_command_as_documented` green (pass 14); executed transcript `tillerctl --socket /tmp/p55-cli.sock ping` returned `pong` | pass 14 |
| `F-CTRL-CLI-02` | FAILED — absent | no shim/install mechanism, bare tillerctl relies on PATH | pass 6 |
| `F-CTRL-PLAT-01` | PASSED | platform-neutral serde/libc manifest; same NDJSON on Linux | pass 6 |

### F-AGENT — agent adapters (20)

| id | verdict | evidence | judged |
|---|---|---|---|
| `F-AGENT-API-01` | FAILED — absent | catalog + hook flags exact (pass 6); trait has no summarizer method (claim requires an optional noninteractive summarizer command) | pass 11 |
| `F-AGENT-CLAUDE-01` | PASSED | real claude TUI in pane; both hooks fired; ref resumed | pass 6 |
| `F-AGENT-CLAUDE-02` | PASSED | merge semantics: only 5 hook events replaced, rest byte-identical | pass 6 |
| `F-AGENT-CLAUDE-03` | PASSED | claude -p ran live | pass 6 |
| `F-AGENT-CODEX-01` | PASSED | no user-global writes; -c notify override loads in real codex (no TOML trap) | pass 6 |
| `F-AGENT-CODEX-02` | PASSED | codex exec --output-last-message ran live | pass 6 |
| `F-AGENT-OPENCODE-01` | UNREACHABLE — opencode not installed | command -v empty; env verified 14:27 | pass 9 |
| `F-AGENT-OPENCODE-02` | UNREACHABLE — opencode not installed | command -v empty; env verified 14:27 | pass 9 |
| `F-AGENT-OPENCODE-03` | FAILED — absent | no summarizer-command generator exists in the port (grep `run --pure`/summarizer → nothing in tiller_agents/tiller_project; the only summarizer code is the settings SummarizerChoice, no command); opencode itself is now installed (1.18.18) so this is absence, not environment | pass 16 |
| `F-AGENT-PI-01` | PASSED | bare launch + resume shape; pi --print ran live | pass 6 |
| `F-AGENT-PI-02` | PASSED | bare launch + resume shape; pi --print ran live | pass 6 |
| `F-AGENT-OMP-01` | FAILED — defective | wrong binary name: the real distribution installs `oh-my-pi` (package.json bin = {oh-my-pi}, no alias — verified pass 16), but the adapter's id/command use `omp` (omp.rs:35,65) — `find_executable_on_path("omp")` is None and `omp --hook …` would fail at spawn; the product IS installed, the name the adapter seeks does not exist | pass 16 |
| `F-AGENT-OMP-02` | FAILED — defective | unreachable because of the same wrong-name defect as OMP-01 (adapter seeks `omp`, distribution ships `oh-my-pi`); the hook file generation itself (omp-hook.ts) is name-correct but the session can never launch to emit start/turn/shutdown events | pass 16 |
| `F-AGENT-OMP-03` | FAILED — absent | two layers: no summarizer-command generator exists anywhere in the port (grep summarizer → only the settings choice, no command), and the launch name `omp` doesn't match the distribution's `oh-my-pi` (OMP-01) | pass 16 |
| `F-AGENT-SAFE-01` | half-proven | worktree-local half measured pass 11 (fake HOME unchanged); skill-provisioning half absent — only the npx command builder exists (in `tiller_project`, not `tiller_agents`: scope correction to the pass-11 evidence), no management-marker or overwrite-refusal logic anywhere (pass 14 re-check) | pass 14 |
| `F-AGENT-SAFE-02` | PASSED | session_sources tests replayed green (`hook_migration_rewrites_only_stale_tillerctl_leading_paths`, `hook_migration_is_a_noop_for_current_non_tillerctl_or_malformed_settings`, `hook_migration_updates_a_file_and_missing_files_are_noops`, tiller_agents suite 33 green, pass 14); zero app callers — package capability proven, wiring owed: nothing invokes `ClaudeHookMigrator` at launch/session | pass 14 |
| `F-AGENT-SESSION-01` | PASSED | `session_validator_checks_claude_and_codex_files_but_trusts_other_agents` replayed green (tiller_agents suite, pass 14); zero app callers — package capability proven, wiring owed: the app resume flow never calls `AgentSessionValidator` | pass 14 |
| `F-AGENT-SESSION-02` | PASSED | session_sources reader tests replayed green (tiller_agents suite, pass 14); zero app callers — package capability proven, wiring owed: no transcript/history surface consumes these sources | pass 14 |
| `F-AGENT-SESSION-03` | PASSED | shell_quote/json_string_literal round-trips, no slash-escaping | pass 6 |
| `F-AGENT-PLAT-01` | PASSED | platform-neutral manifest, plain CLI processes, worktree-local outputs | pass 6 |

### F-GIT — git tier (16)

| id | verdict | evidence | judged |
|---|---|---|---|
| `F-GIT-RUN-01` | PASSED | success/failure/timeout/launch-failure; deadline enforced; no output limits/cancellation | pass 5 |
| `F-GIT-RUN-02` | PASSED | pass 12: `streaming_runner_delivers_stderr_before_the_child_exits` green (tests/p41_git_behaviors.rs:69); `GitRunner::run_streaming` + free fn (git.rs:107-131, 232-237); zero app callers — package capability proven, wiring owed | pass 12 |
| `F-GIT-REPO-01` | PASSED | .git dir/file true, non-repo false, has_head 0/1 | pass 5 |
| `F-GIT-BRANCH-01` | PASSED | pass 12: `branch_listing_preserves_spaces_in_names` green (p41_git_behaviors.rs:109); `git branch --list --format=%(refname:short)`, line-based parse (branches.rs); zero app callers | pass 12 |
| `F-GIT-WT-01` | PASSED | create with/without base, duplicate refusal, dirty-removal refusal, clean removal | pass 5 |
| `F-GIT-CLONE-01` | PASSED | pass 12: `clone_from_local_repository_reports_receiving_progress` green (p41_git_behaviors.rs:122) — incremental Receiving objects % + invalid-source CommandFailed through the runner; zero app callers | pass 12 |
| `F-GIT-REMOTE-01` | PASSED | pass 12: `remote_parsing_supports_github_ssh_https_and_project_suffixes` green (p41_git_behaviors.rs:169); `github_owner_from_url` + `project_name` (remote.rs); zero app callers | pass 12 |
| `F-GIT-STATUS-01` | PASSED | porcelain-v2 -z parse matches v1: MM/rename/UU/spaces | pass 5 |
| `F-GIT-STATUS-02` | PASSED | pass 12: `directory_status_aggregates_ancestors_with_precedence_and_renames` green (p41_git_behaviors.rs:216); conflicted>changed>untracked precedence, rename paths both feed ancestors (directory_status.rs); zero app callers | pass 12 |
| `F-GIT-ACT-01` | PASSED | stage/unstage/stage_all/discard/discard_all verified in git after each call | pass 5 |
| `F-GIT-ACT-02` | PASSED | pass 11: stage_refuses_a_conflicted_path_without_changing_git + stage_all_refuses_every_conflicted_path_before_mutation green (worktree unchanged verified); stale+duplicate tested; empty-selection branch code-verified | pass 11 |
| `F-GIT-DIFF-01` | PASSED | tracked/untracked/binary/added/deleted/renamed/no-HEAD hunks correct | pass 5 |
| `F-GIT-DIFF-02` | PASSED | tracked/untracked/binary/added/deleted/renamed/no-HEAD hunks correct | pass 5 |
| `F-GIT-DIFF-03` | PASSED | pass 12: `side_by_side_preserves_hunks_pairs_runs_and_drops_metadata` + `side_by_side_handles_real_rename_binary_and_large_context` green (p41_git_behaviors.rs:268, :307); zero app callers | pass 12 |
| `F-GIT-DIFF-04` | PASSED | untracked cap is 500,000 bytes, matching the Swift reference exactly (data.count <= 500_000 in GitDiffStats.swift); over-cap files fall back to the diff count (150,000-line file reports its real count, test green); binary -> is_binary | pass 11 |
| `F-GIT-PLAT-01` | PASSED | git via PATH; whole flow on Linux; process-group kill on timeout | pass 5 |

### F-PERSIST — persistence package (13)

| id | verdict | evidence | judged |
|---|---|---|---|
| `F-PERSIST-DB-01` | PASSED | open(path)+in_memory; forward-only v1-v5 tested w/ rows intact; corrupt/empty/truncated/newer-schema refused; 64MiB cap; concurrent first-open+writers tested (22 it-tests) | pass 10 |
| `F-PERSIST-DB-02` | PASSED | persistence suites replayed green (5 unit + 34 integration, pass 14): canonical schema has projects/worktrees/tabs/tab-state/settings/sidebar/session-refs/chat_turn/quarantine; Swift-only legacy fields stay absent by design | pass 14 |
| `F-PERSIST-DB-03` | PASSED | replayed `worktree_metadata_and_exact_path_survive_a_relaunch` green (persistence_integration, pass 14) — comment + nullable created_at/updated_at persist | pass 14 |
| `F-PERSIST-DB-04` | PASSED | SessionTabState scrollback byte-exact round trip + 256KiB bound at encode AND decode (tests) | pass 10 |
| `F-PERSIST-DB-05` | PASSED | replayed `chat_transcript_survives_process_relaunch_with_tool_and_permission_outcome` green (persistence_integration, pass 14) — additive chat_turn stores rendered turns and survives relaunch; legacy Swift tab-table schema absent by design | pass 14 |
| `F-PERSIST-DB-06` | PASSED | v5 session-ref upsert/load/delete + reopen replayed green (persistence_integration, pass 14); agent-account rows deliberately absent (owned by tiller_usage) | pass 14 |
| `F-PERSIST-DB-07` | PASSED | replayed green (persistence_integration, pass 14): v9 quarantine_record moves malformed tab-state/chat payloads with original bytes, valid siblings survive; physical corruption remains untouched `Corrupt` | pass 14 |
| `F-PERSIST-DB-08` | PASSED | replayed `a_duplicate_primary_is_reconciled_and_future_writes_are_rejected` + `exact_path_lookup_does_not_normalize_nearby_paths` green (persistence_integration, pass 14) — v8 one-primary integrity + exact path equality | pass 14 |
| `F-PERSIST-DB-09` | PASSED | replayed `a_corrupt_tab_is_skipped_and_quarantined_while_valid_tabs_survive` green (persistence_integration, pass 14) | pass 14 |
| `F-PERSIST-DB-10` | PASSED | session_references_upsert_load_and_delete + survive store reopen (tests) | pass 10 |
| `F-PERSIST-DB-11` | PASSED | replayed `current_schema_contains_named_persistence_migrations` green (persistence_integration, pass 14) — v5..v9 named migrations present, CURRENT_SCHEMA_VERSION == MIGRATIONS.len() | pass 14 |
| `F-PERSIST-DB-12` | UNREACHABLE — no terminalTab table in the Linux schema | no v17 rename exists; the described mismatch cannot manifest in this lineage | pass 10 |
| `F-PERSIST-PLAT-01` | PASSED | $TILLER_DB->checkout-scoped(XDG)->user-wide tested; creation/migration/concurrency exercised on Linux path (two-process tests) | pass 10 |

### F-TERM — terminal package (17)

| id | verdict | evidence | judged |
|---|---|---|---|
| `F-TERM-PTY-01` | PASSED | spawn/exec/cwd/env/output live (pass 1); resize_reaches_the_child_pty test green; exit reported via panel.state/panel.wait live (0/3/137) | pass 11 |
| `F-TERM-PTY-02` | PASSED | exits 0/3/137 live (pass 4); child_exit_status_preserves_normal_and_signal_termination test green | pass 11 |
| `F-TERM-PTY-03` | PASSED | replayed `terminal_child_receives_the_pane_id_environment` + `stable_identity_is_inherited_by_the_lazily_spawned_pty` green (tiller_terminal suite 22 green, pass 14) — PTY fork delayed until set_identity, pane ID passed as TILLER_PANE_ID; the real-app tillerctl-notify round trip over a live PTY pane remains unexercised by a critic (pass 14) | pass 14 |
| `F-TERM-REG-01` | PASSED | panel create/split/list/write/read/wait/focus/close live | pass 6 |
| `F-TERM-REG-02` | PASSED | wait exit/timeout/unknown live (pass 6); pass 11 live: panel.close then panel.wait returns immediately (6 ms, unknown pane) — close cancels pending registration, no live registration left | pass 11 |
| `F-TERM-SCR-01` | PASSED | 262144 bytes exactly; newest line kept, oldest evicted | pass 4 |
| `F-TERM-SCR-02` | FAILED — absent | no 200ms settle / 120ms resize debouncer anywhere in the terminal crate | pass 10 |
| `F-TERM-PTY-04` | PASSED | pass 11 live: app launched with SHELL=/bin/sh spawns an sh prompt in the pane ($SHELL preference real, bash fallback otherwise); TERM fixed xterm-256color; xterm-ghostty + first-resize settle are the macOS-isms the PLATFORM note exempts | pass 11 |
| `F-TERM-PTY-05` | FAILED — absent | live, pass 15: `tillerctl panel write pane-1 "sleep 8"` — frames at running (+1.5s) and done (+10.5s) are byte-identical (0 px delta): no activity/lifecycle change surfaces in the sidebar while a command runs; content-match and pane-exit activity callbacks remain unwired | pass 15 |
| `F-TERM-PTY-06` | FAILED — absent | terminal_file_drop has zero callers; no drop-to-pane path | pass 10 |
| `F-TERM-PTY-07` | FAILED — absent | no stable-host/generation/teardown abstraction | pass 10 |
| `F-TERM-PTY-08` | FAILED — absent | no pane cache | pass 10 |
| `F-TERM-SPLIT-01` | FAILED — absent | right/down splits + layout persistence live (pass 4); divider is a 1px seam (SEAM_WIDTH=1.0) vs the reference's 6px; no 160px minimum enforced anywhere | pass 11 |
| `F-TERM-UI-01` | NOT EXERCISED | pass 12: right-click hit testing + full 10-item menu + delegation drawn-tested green (tiller_terminal lib.rs:1751); every-action invocation unexercised | pass 12 |
| `F-TERM-UI-02` | FAILED — absent | no URL router | pass 10 |
| `F-TERM-USG-01` | PASSED | hidden claude PTY fetch ran live (pass 1); pass 11: new test green — PATH-stubbed claude fetch returns TimedOut and the hidden PTY child is killed on drop (termination verified via stub pid) | pass 11 |
| `F-TERM-PLAT-01` | PASSED | gpui + alacritty_terminal only; no webview/HTML renderer | pass 10 |

## Totals

| verdict | count |
|---|---|
| PASSED | **196** |
| half-proven | **15** |
| FAILED — absent | **127** |
| FAILED — defective | **13** |
| UNREACHABLE | **3** |
| N/A — platform | **22** |
| NOT EXERCISED | **12** |
| NOT EXERCISED — blocked on display | **0** |
| builder-claimed, unverified | **1** |
| **total** | **389** |

Entries never independently judged by the critic: **68**
(**25** never claimed by anyone; **22** builder-claimed, unverified; **8** P56;
**7** P50 builder claim; **6** P55 builder).
`NOT EXERCISED — blocked on display` and `UNREACHABLE` rows *are* critic-judged
(as appearance-blocked / environment-impossible), so they are not counted in
the never-judged figure.

**Do not maintain this block by hand — run `python3 Scripts/ledger-totals.py --write`.**
It has been wrong three times. Twice it was merely stale. The third time is the one that
matters: the critic recomputed it correctly at the end of pass 12 and it was wrong again
within minutes, because builders append and claim rows continuously while only the critic
recomputes. A hand-maintained aggregate over a concurrently-edited table is not
occasionally stale, it is guaranteed stale, and every agent picks its next piece from
these numbers. The script also rejects a row whose evidence contains a raw `|`, which
silently shifts every column after it — one such row (`F-SET-05`) had been hiding the
`judged` column from any count, which is why the never-judged figure read 54 against a
true 68.
