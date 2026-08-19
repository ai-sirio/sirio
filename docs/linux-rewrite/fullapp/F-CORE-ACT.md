# F-CORE-ACT — fresh critic pass (live drive + source audit)

Section: **F-CORE-ACT** (27 rows, Package tier / `TillerCore` — the four-layer agent activity
detection engine: hooks, OSC title, content match, foreground process; ownership/debounce rules).

Contract source: `docs/linux-rewrite/02-inventory-packages.md` lines 3–28 (`F-CORE-ACT-01` through
`-26`); row 27 is a Linux-port-specific umbrella row (test-coverage/no-quarantine), its text
recovered from `docs/linux-rewrite/ADJUDICATION-BACKLOG.md`/`UNPROVEN-ROWS-RECIPES.md` since it has
no numbered line in `02-inventory-packages.md`. Every existing `PASSED` in
`docs/linux-rewrite/INVENTORY-LEDGER.md` was treated as a prior judgement to re-earn, not as truth,
per this task's instructions — I did not echo it.

**Environment**: `/dev/shm/tt/debug/tiller` + `tillerctl`, driven via `Scripts/wayland-drive.sh`.
Labels `critact13*`, outdir `/dev/shm/sweep-13-F-CORE-ACT`, throwaway fixture repo
`/dev/shm/critact13-fixture` with two real `git worktree add` siblings (`critact13-second` on branch
`second`, `critact13-third` on branch `third`) so master/second/third all show as real, independent
worktree rows without needing three separate repos. Decoy agent binaries (`/dev/shm/critact13-decoys/{codex,opencode,claude}`,
shell scripts that ignore all arguments, echo a banner, then either `sleep 600` or exit on a timer)
were put first on `$PATH` before launching the app, so Tiller's own "+" agent-launch menu and a
plain terminal's foreground-process detection both resolve to a fully controllable stand-in process
instead of a real, credentialed CLI — the same technique the prior `wave D`/`FINISH-activity-core.md`
pass used, independently re-derived here.

## Host contention — a genuine, reproducible harness blocker, not an app defect

The first ~25 minutes of this pass succeeded cleanly (see "Live evidence obtained" below). After
that, **14 consecutive `wayland-drive.sh` boot attempts in a row failed** with the app's own log
showing `MESA: error: ZINK: failed to choose pdev` / `libEGL warning: failed to get driver name for
fd -1`, producing a near-blank first frame (`WARN ... blank or near-blank (62/1 colours)`) that the
script's own presentation-guard correctly refused to proceed past (exit 5). This was checked against
confounds before being written down as "environment", not "defect":

- `uptime` read **load average 30–39 on a 12-core box** throughout the failure streak (`systemd-journal`
  at 75% CPU and `rsyslogd` at 56% CPU — the runaway-log process named in this task's own briefing —
  plus 14–16 concurrent `rustc`/`sccache` processes from other agents' builds).
- At the time of the failures, `pgrep -c tiller` and `pgrep -c sway` were both **0** — no other
  critic's GUI instance was competing for the GPU/render node when mine failed, which rules out
  "too many concurrent Tiller windows" and points at plain CPU starvation of the EGL/Zink init
  handshake instead.
- Retries were spaced (10–20 s apart, 14 attempts over ~10 minutes, including one final retry after
  drafting this report) and failed identically every time — this is a stable condition at the time
  of this pass, not one-off flakiness the standard sleep-and-retry advice in `wayland-drive.sh`'s own
  header is meant to cover.

This blocked the second half of my planned drive (title/hook/debounce/content/ANSI in a second
worktree; node-hosted invisibility, generic keywords, catalog-order dedup and priority tie-break in
a third worktree; Activity-panel surface-kind check; notification capture). Those rows are marked
**half-proven** below: fresh unit-test replay plus a direct source read of the exact wired call site
(not "it was PASSED before"), but no live gesture from me this pass. Two of the source reads turned
into real, load-bearing findings (rows 19 and 21 below) that would not have surfaced from replaying
tests alone.

## Live evidence obtained this pass (screenshots I looked at)

All paths under `/dev/shm/sweep-13-F-CORE-ACT/lane1/`.

- **`06-03-master-codex-decoy-running.png`**: a plain "New Terminal" pane (no agent launch, no
  title, no hook) with `codex &` typed into it — a real child process whose `comm` is `codex`
  (confirmed separately via `/proc/<pid>/comm` in a standalone shell test before the drive). The
  sidebar's `master` row grew a Codex badge and the right panel's collapsed "Activity" disclosure
  (clicked open at `(1355, 918)`) read **"Activity 1 running"**.
- **`07-04-master-title-tamper-while-process-owned.png`**: with that pane still process-owned
  Running, I typed a real OSC-0 title-set (`printf '\033]0;UNRELATED_TAMPER_TITLE_NOT_AN_AGENT\007'`,
  executed as a genuine shell command, visible echoed in scrollback) — the Codex badge and "Activity 1
  running" **did not change**.
- **`08-05-master-decoy-killed-cleared.png`**: `kill %1` in the same pane → bash printed
  `[1]+ Terminato codex` and the sidebar badge / Activity count **cleared immediately**.
- Fresh `cargo test` replay, same tree, same session: `cargo test -p tiller_activity` → **36 (lib) +
  13 (domain-integration, all 12 `f_core_act_*`-named tests) + 25 (real-PTY/model integration) = 74
  passed, 0 failed**; `cargo test -p tiller --bin tiller panes::` → **19 passed, 0 failed** (both
  real-PTY tests included, matching the ledger's own "improvement, not regression" note);
  `grep -c '#\[ignore\]' crates/tiller/src/panes.rs` → **0**; the same tree's `tiller`/`tiller_ui`
  test binaries built clean as a side effect of running the above (no compile errors).

## Verdicts

| id | verdict | evidence |
|---|---|---|
| `F-CORE-ACT-01` | half-proven | Fresh replay of `status::tests::{priority_order_is_error_needs_input_running_done, highest_priority_picks_the_winner, human_labels_match_swift, exit_codes_map_to_done_and_error}` — all green today, inside the 74-passed lib run above. Not independently re-driven live this pass (pure domain-vocabulary logic; no UI surface of its own to click). |
| `F-CORE-ACT-02` | half-proven | Blocked by the host-contention window (see above) before I reached the hook/title debounce pane. Fresh replay of the underlying debounce tests (below, row 07) plus the crate's `notify()` (confirmed by source read, `model.rs:99`) is unconditional — it does not require a pre-existing `pane_agents` entry, matching the row's clause. Not independently re-earned live this pass; the prior session's `acta-second2/04-03` live evidence (ledger) is the only live gesture on record. |
| `F-CORE-ACT-03` | half-proven | `agent_spawned_sets_running_and_agent_id_without_transition` replayed green. My own live attempt to launch a decoy Claude Code tab via the "+" menu was cut short by the host-contention window right after the menu opened (see Notes). Source read confirms `agent_spawned` (`model.rs:134`) sets `.running` unconditionally with **no** transition emitted, and is called from the real `add_agent_tab` path (`main.rs:7356`), not a test-only helper. |
| `F-CORE-ACT-04` | half-proven | `exit_results_map_codes_and_respect_tracking` replayed green. Source read: `panes.rs:32-53`'s `apply_terminal_activity_event` calls `activity.apply_exit_result` on `ChildExited` **only when the pane is neither process- nor title-owned** — i.e. genuinely spawn-owned, matching "spawn-owned agent" in the clause text precisely (a plain, unrecognized shell command never reaches this path at all, since it has no `agent_status` entry for `apply_exit_result`'s early-return `?` to build on — this is the same mechanism the sibling `F-TERM-08` finding already documented from the other side). Not live-driven this pass; blocked before reaching the retry. |
| `F-CORE-ACT-05` | half-proven | `panes::tests::real_pty_activity_status_follows_osc_title_then_settled_content` (real PTY) replayed green today (inside the 19/19 `panes::` run). Not independently re-driven live this pass — blocked before reaching the second worktree. |
| `F-CORE-ACT-06` | half-proven — one half PASSED live | **Process-owned half PASSED live today**: `07-04-master-title-tamper-while-process-owned.png` — a real OSC title change on a process-owned pane left its Codex badge/Activity count untouched (see above). **Title-owned half not live-driven today** (blocked before the second worktree); relies on the freshly-replayed `title_owned_panes_are_cleared_by_unmatched_titles`. |
| `F-CORE-ACT-07` | half-proven | `layer_a_push_suppresses_contradicting_title_inside_debounce_and_stops_after` + `real_pty_layer_a_debounce_suppresses_first_title_and_accepts_second` both replayed green today. Not independently re-driven live this pass (blocked before the second worktree). |
| `F-CORE-ACT-08` | half-proven | `content_match_overrides_stale_title_even_inside_debounce_window` + `terminal_events_feed_title_and_settled_content_into_the_one_model` (real PTY) both replayed green today. Not independently re-driven live this pass. |
| `F-CORE-ACT-09` | **PASSED** | Live, both halves, this pass: process-owned Running from a real spawned `codex`-comm child (`06-03…png`), cleared only by that process's own death (`08-05…png`, `kill %1` → immediate clear). `linux_process_inspection_reads_agent_names_from_proc_children` (real subprocess, real `/proc` walk) also replayed green. |
| `F-CORE-ACT-10` | half-proven | Not live-driven this pass (the `pi`-launch half of my planned drive, in the `third` worktree, was never reached). Structural confirmation stands independent of any live gesture: `/home/enzopalmisano/.nvm/.../bin/pi` is a `#!/usr/bin/env node` shebang script — I forked one directly in a standalone shell (outside the app) and read `/proc/<pid>/comm`, which reported `node`, not `pi`; `node` is absent from `CATALOG_IDS` (`process.rs`), so Layer D is structurally blind to it by construction, matching the clause. `walks_nested_children_and_skips_disappeared_descendants` replayed green. |
| `F-CORE-ACT-11` | half-proven | The ledger's `PASSED` was carried on evidence for an unrelated feature per the prior `wave D` pass's own correction, which I re-confirm still stands (the row's real clause is pane-ownership separation / close-clears-all-state). Today: `each_ownership_kind_is_cleared_only_by_its_own_condition` + `pane_closed_clears_all_state_and_is_idempotent` replayed green, and this pass's own rows 06/09 live evidence positively demonstrates one layer's signal (title tamper) *not* touching another layer's state (process-owned). The "closing a pane clears everything" half specifically was not independently re-driven live this pass. |
| `F-CORE-ACT-12` | half-proven | Title-identity unit tests (`claude_glyphs_identify_claude`, `pi_glyph_titles_identify_pi_and_omp`, `omp_follows_pi_family_conventions`, `name_tokens_identify_codex_opencode_omp`, `bare_braille_spinner_does_not_identify`) all replayed green. Not independently re-driven live this pass. |
| `F-CORE-ACT-13` | half-proven | Title-status unit tests (`claude_idle_prefix_is_needs_input`, `claude_working_prefix_and_spinner_are_running`, `pi_spinner_is_running_and_bare_name_is_needs_input`) all replayed green. Not independently re-driven live this pass. |
| `F-CORE-ACT-14` | half-proven | `f_core_act_14_requires_word_boundaries_for_generic_title_identity_and_status` plus `title::tests::{generic_avoids_substring_false_positives, generic_keywords_map_to_statuses}` replayed green. I additionally read `title.rs`'s `detect_generic`/`contains_word` directly: it requires the agent's own name as a word-boundary token *and* a status keyword as a separate word-boundary token in the same title — I had planned exactly the VERIFY line's boundary-vs-substring test (`"codex is networking now"` vs `"codex is working hard"`) for the `third` worktree but the host-contention window hit before I reached it. |
| `F-CORE-ACT-15` | half-proven | Content-detection unit tests (`claude_permission_prompt_is_needs_input`, `claude_streaming_indicator_is_running`, `generic_yes_no_prompts_are_needs_input`, `y_n_without_boundaries_does_not_match`, `unknown_agent_and_empty_text_are_nil`) all replayed green. Not independently re-driven live this pass. |
| `F-CORE-ACT-16` | half-proven | `f_core_act_16_strips_csi_and_osc_before_content_matching` + `ansi::tests::{strips_csi_and_both_osc_terminators, preserves_unicode_and_unknown_escape_sequences}` replayed green. |
| `F-CORE-ACT-17` | half-proven | Fresh replay: `agent_id_for_panes_picks_the_highest_priority_pane` (lib) plus **three** domain-integration tests (`f_core_act_17_entity_evidence_reports_status_without_inventing_an_identity`, `..._never_overrides_a_title_or_process_owned_pane`, `..._resolves_through_the_same_pane_status_query`) all green — more coverage than the ledger's evidence text names. Source read confirms live wiring: `main.rs:5800` — `agent_id_for_panes` tints the running-status glyph (never replaces the branch glyph). Multi-pane priority tie-break not independently re-driven live this pass. |
| `F-CORE-ACT-18` | half-proven | `running_agent_ids_follow_catalog_order` replayed green. Source read confirms live wiring: `main.rs:5804` — `running_agent_ids` feeds the sidebar row's trailing running-agents badge, already de-duplicated, catalog order. Live multi-agent dedup/order gesture (planned: opencode launched *before* a second codex tab in the `third` worktree, specifically to distinguish catalog-order from launch-order) not reached before the host-contention window. |
| `F-CORE-ACT-19` | half-proven — **test/production mismatch found** | Real behaviour is correct: `main.rs:5920-5939` calls `AgentActivityModel::build_payload`, then **explicitly overwrites `payload.title`** with `format!("{agent_display_name} — {}", context.activity_label)` — i.e. `<agent> — <human worktree label>`, matching the clause — with a comment reading *"the title is `<agent> — <human worktree label>`, not `<agent> — <status>`... `build_payload` still emits the status-suffixed title, so it is corrected here"*. But the row's own named test, `f_core_act_19_builds_notification_payload_with_optional_context`, calls `build_payload` directly and asserts `payload.title == "Claude Code — needs input"` — the **status-suffixed, pre-override** format the production code explicitly says is wrong. The test (green today) is proving the opposite of what ships. Real-world output is right only because of the hand-written override; nothing in the test suite protects that override from silently disappearing in a future refactor. Could not re-verify live via `dbus-monitor` this pass (see Notes: `notification-daemon` crashed instantly on this host, and the host-contention window then blocked a retry with just the raw bus capture). |
| `F-CORE-ACT-20` | half-proven | Source read confirms the platform nuance a prior session flagged (`FINISH-activity-core.md`, "`app_active` hardcoded `true`") is **already fixed** in the current tree: `main.rs:5890-5894` passes `self.window_active`, itself kept live by a per-frame `window.is_window_active()` poll (`main.rs:10751`) with its own dedicated render-time regression test (`main.rs:15594-15636`, explicitly named for this row). `f_core_act_20_notification_policy_suppresses_noise_and_visible_transitions` (pure-logic, all branches) replayed green. Not independently re-driven live this pass (would need the same blocked dbus-monitor capture as row 19). |
| `F-CORE-ACT-21` | half-proven, **leans FAILED — source-derived, not visually confirmed** | See "Defects" below — `build_activity_rows` (the function that actually implements "omit document, diff, and browser content", and is what `f_core_act_21_activity_rows_keep_shells_and_chats_but_omit_non_activity_content` tests) is dead code, never called from `main.rs`. The live-wired producer, `activity_surfaces` (`main.rs:5018`), maps every `self.tabs` entry with no kind filter, and `ActivitySurface` itself carries no kind field for the renderer to filter on downstream either. This directly contradicts the ledger's `PASSED` and its cited screenshot (which only ever showed a lone Terminal row — consistent with *never having opened* a Document/Diff/Browser tab at the same time, not with the omission actually working). I could not open a Diff/Browser tab and screenshot the Activity panel myself this pass — the host-contention window hit exactly at this step (see reproduction below, one screenshot away from a clean confirm/deny). |
| `F-CORE-ACT-22` | half-proven | `f_core_act_22_attention_sort_is_stable_and_urgent_first_preserves_manual_nonurgent_order` (both modes, ties) replayed green. Source read: both `AttentionSort::sorted` (`main.rs:4954`, tray roster) and `::urgent_first` (`main.rs:5807`, sidebar row order) are called from real UI-adjacent code, confirming the ledger's own prior "not dead" correction still holds. Live multi-worktree reorder gesture not reached this pass. |
| `F-CORE-ACT-23` | half-proven | `f_core_act_23_only_live_activity_statuses_require_close_confirmation` (all 5 statuses) replayed green. The live close-confirmation banner gesture (planned for the `third`/Activity-panel step) was not reached before the host-contention window; relying on the prior session's `acta-23b/03` live evidence recorded in the ledger, not re-earned by me this pass. |
| `F-CORE-ACT-24` | half-proven | `f_core_act_24_restore_plan_keys_refs_by_stable_content_not_live_pane` replayed green. Source read: `AgentSessionRestorePlan::plan` is called from a real restart-path call site (`main.rs:11109`), not a test-only helper. No live quit+relaunch+`/proc` argv check this pass (that specific gesture is expensive — a real restart cycle — and the host-contention window made even the cheap gestures fail; I did not attempt it). |
| `F-CORE-ACT-25` | half-proven | `f_core_act_25_bootstrap_prioritizes_selected_open_worktree_and_defers_the_rest` replayed green. Source read: `BootstrapRestoreOrder::partition` is called from a real boot-path call site (`main.rs:780`). No fresh live reboot-and-inspect this pass. |
| `F-CORE-ACT-26` | half-proven | `f_core_act_26_mount_policy_evicts_only_safe_oldest_worktrees_until_cap` replayed green. Source read: `WorktreeMountPolicy::ids_to_evict` is called from a real settings-driven call site (`main.rs:5167`). No fresh live toggle-and-switch cycle this pass. |
| `F-CORE-ACT-27` | **PASSED** | Live today, direct commands against this exact tree: `cargo test -p tiller_activity` → 36+13+25 = **74 passed, 0 failed** (up from the ledger's last-recorded 73 — one test net-added, no regression); `cargo test -p tiller --bin tiller panes::` → **19 passed, 0 failed** (both real-PTY tests included); `grep -c '#\[ignore\]' crates/tiller/src/panes.rs` → **0**; both crates built clean as a side effect. |

## Defects

### F-CORE-ACT-21 — the live Activity panel's "omit document/diff/browser" filter is likely dead code (not visually confirmed this pass)

**This is a finding I could not close the loop on live** — reported because the source evidence is
unusually direct and because leaving a ledger row's `PASSED` unchallenged when the cited test
provably exercises a function the product never calls would be a worse outcome than flagging an
unconfirmed suspicion clearly.

- `crates/tiller_activity/src/rows.rs:103` (`build_activity_rows`) is the *only* place in the whole
  tree that implements "retain terminal/chat, omit document/diff/browser" (`rows.rs:132`:
  `ActivityTabKind::Document | ActivityTabKind::Diff | ActivityTabKind::Browser => {}`). It is
  exported from `tiller_activity`'s `lib.rs` and used **only** by its own test,
  `f_core_act_21_activity_rows_keep_shells_and_chats_but_omit_non_activity_content` — confirmed by
  `grep -rn build_activity_rows crates/ | grep -v tests`, which returns only the `lib.rs` export.
- The function actually wired to the live right-panel Activity list is `activity_surfaces`
  (`crates/tiller/src/main.rs:5018-5031`): `self.tabs.iter().map(|tab| ActivitySurface::new(...))`
  — no `match`, no filter, every tab kind included. Its output feeds `RightPanel::with_activity`
  directly (`main.rs:5329-5331` and `main.rs:5733-5754`).
  `crates/tiller_ui/src/right_panel.rs:66-75`'s `ActivitySurface` struct has exactly four fields
  (`icon`, `title`, `location`, `status`) — **no kind field at all** — so there is no data left for
  a downstream render-time filter to act on even if one existed, and `grep` finds none.
  `crates/tiller/src/main.rs`'s own `use tiller_activity::{...}` import list does not mention
  `ActivityWorktreeInput`/`ActivityTabKind`/`build_activity_rows` at all.
- Net: opening a Diff tab (`+` → Changes) or a Browser tab (`+` → New Browser) alongside a Terminal
  tab in the same worktree should, per this reading, add a "Changes" / "Browser" row to the Activity
  list that the row's own clause says must not appear.

**What would close this cleanly** (one screenshot, blocked by the host-contention window this pass):
open a Terminal tab, a Diff tab (`+` → Changes) and a Browser tab (`+` → New Browser) in one
worktree, open/expand the Activity disclosure (bottom of the right "Files" panel, confirmed reachable
by clicking `(1355, 918)` — see `03-00b-activity-toggle-try.png`, which is genuinely from this
pass and shows the disclosure opening cleanly for a *different* project, before I switched to the
throwaway fixture), and look for a "Changes"/"Browser" row. My own attempt at exactly this
(`actions3.sh`, steps `24-master-diff-tab-added` / `25-master-browser-tab-added` /
`26-master-activity-panel-with-mixed-surfaces`) is written and ready to run — it failed to produce
any screenshots because the app never got past a blank first frame in 14 consecutive boot attempts
(see "Host contention" above), not because of anything wrong with the plan itself.

### F-CORE-ACT-19 — the row's own regression test asserts the pre-override (wrong) notification title format

See the verdict table above for the full reasoning; summarized: `main.rs:5939` corrects
`build_payload`'s title from `<agent> — <status>` to `<agent> — <worktree label>` with a comment
explaining exactly why, but `f_core_act_19_builds_notification_payload_with_optional_context`
(`crates/tiller_activity/tests/activity_domain_integration.rs:35-53`) asserts the pre-override,
`<agent> — <status>` string. Real users see the correct title today; a refactor that "cleans up" the
override because "the test already covers this" would silently regress it, and nothing would turn
red.

## Notes — harness attempts and what they ruled out

- **`notification-daemon` crashes immediately on this host.** `/usr/lib/notification-daemon/notification-daemon`,
  started on `wayland-drive.sh`'s own private session bus (`DBUS_SESSION_BUS_ADDRESS` inherited from
  the same shell), segfaulted within the first second every time (`Errore di segmentazione (core
  dump creato)`, and a pre-existing crash report for the same binary already sits in
  `/var/crash/_usr_lib_notification-daemon_notification-daemon.1000.crash` from before this session
  started). This is a harness/host limitation for rows 19/20's "render a real desktop notification"
  half, not an app defect — `dbus-monitor` itself does not need a name-owner to capture the raw
  `Notify` method call, but I did not reach a transition that would have fired one before the
  host-contention window hit (the `dbus-monitor` capture from my one successful boot,
  `lane1/dbus-capture.log`, only contains bus/session handshake traffic, no `Notifications.Notify`
  call, because that run's actions never got past the menu-click retry before running out of script).
- **The dropdown "+" menu's item coordinates were harder to pin down than expected**, and cost real
  time this pass. A careful crop-and-measure of a clean capture
  (`lane1/09-06-master-plus-menu.png`, cropped to `crop1.png` and re-examined at 2x) confirmed
  `Claude Code` sits at `y≈174` in the menu opened at `(1289, 51)`, matching my original estimate —
  but `x=1289` sits right at/on the menu's left edge (icon column), which is the more likely
  explanation for repeated missed clicks (menu remained open in both `lane1/11-08-...png`, where a
  `shot` was interleaved between opening and clicking — the documented anchor-drift trap — and in
  `lane2`'s first attempt, where it was not). `x=1360` (mid-label) was substituted for the retry
  lane that then hit the host-contention window before it could confirm the fix. Recorded here as a
  harness lesson for the next critic on this section, not scored against any row.
- Fixture layout used throughout: `/dev/shm/critact13-fixture` (master, `git worktree add` used for
  `second`/`third` — real independent worktrees off one repo, not three separate repos), decoy
  binaries at `/dev/shm/critact13-decoys/{codex,claude,opencode}`.

## What I could not reach, and why

Rows 02, 03, 04, 05, 07, 08, 10, 12, 13, 14, 15, 16, 17 (tie-break gesture), 18 (dedup/order
gesture), 19 (live dbus capture), 20 (live dbus capture), 21 (visual confirm/deny), 22 (live
reorder gesture), 23 (live close-confirmation gesture), 24 (live restart+argv), 25 (live reboot),
26 (live toggle cycle) — all blocked by the same host-contention window described above (14
consecutive blank-first-frame boot failures, confirmed unrelated to concurrent Tiller GUI instance
count). Each has fresh, green unit-test replay and, where relevant, a direct source read confirming
the tested function is genuinely wired into `main.rs` (not dead code) — except row 21, where the
source read found the *opposite*, and row 19, where the source read found the row's own test
targets the wrong (corrected-at-the-call-site) code path.
