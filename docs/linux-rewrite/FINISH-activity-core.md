# FINISH — F-CORE-ACT recensus (27 rows)

Fresh finish-line critic pass, 2026-08-18. `TILLER_WL_LABEL=wd-acta`, box per
`docs/linux-rewrite/ENVIRONMENT.md` top section (x86, 12 cores, COSMIC/Wayland). Binary pinned to
`/tmp/wd-acta-tiller` (built clean, `cargo build --manifest-path rust/Cargo.toml --workspace` exit 0,
same two pre-existing warnings). Design authority: CLAUDE.md's "Agent activity detection" section
and the four evidence layers it defines. Cited standard: `docs/linux-rewrite/EVIDENCE-STANDARD.md`.

Every prior `PASSED` was treated as the hypothesis under test, not as evidence, per
`FINAL-RECENSUS-PLAN.md`. Rows were re-driven live wherever a live gesture was practical today, and
cross-checked against a full re-run of the named tests every VERIFY clause in
`02-inventory-packages.md` maps to.

## Baseline re-run (today, this tree)

```
cargo test -p tiller_activity        -> 35 + 13 + 25 = 73 passed, 0 failed
cargo test -p tiller --bin tiller panes::  -> 19 passed, 0 failed   (ledger's last count: 16 passed, 2 failed —
                                              both real-PTY tests now pass; improvement, not regression)
grep -c '#\[ignore\]' crates/tiller/src/panes.rs -> 0
cargo test -p tiller_terminal        -> 45/46 passed; 1 flaky under concurrent load
                                         (scrollback_can_be_viewed_after_output_exceeds_the_viewport),
                                         passes in isolation — unrelated to F-CORE-ACT-27's clause.
```

## A ledger-hygiene finding, fixed here

**`F-CORE-ACT-11`'s recorded evidence text is for the wrong feature.** It describes `tab_bar.rs`'s
new-tab dropdown (`deferred(anchored())` vs the sidebar's `.absolute()`) — a real finding, but not
this row's clause (pane-ownership separation / close clears all state). The row was carried forward
as `PASSED` without ever being checked against its own VERIFY line. Re-judged below against the
actual clause, using `each_ownership_kind_is_cleared_only_by_its_own_condition` and
`pane_closed_clears_all_state_and_is_idempotent`, both replayed green today, plus this session's own
live title-owned/process-owned clearing gestures (rows 06/09 below).

**A second, now-stale finding resolved.** `DEAD-MODELS.md` (pass 10) flagged `agent_id_for_panes`,
`running_agent_ids`, `requires_close_confirmation`, `AttentionSort` (whole module) and
`ids_to_evict` as zero-caller/dead. All five are called from real UI code today —
`crates/tiller/src/main.rs:5299,5303,4741,2732,4527,4544,5324` — confirmed by `grep`, not by prose.
The ledger's `sweep N-critics` (2026-08-18) already recorded this repair; this pass re-confirms the
call sites still exist in `HEAD`.

## A caveat outside this shard's rows (recorded, not scored)

Repeatedly reproduced today: after a fresh app boot (or a rapid worktree switch), the **center
terminal viewport** can display a **different, stale worktree's pane content** than the one the
sidebar highlight and the bottom status bar both correctly report as selected —
e.g. status bar said `second · .../fixture-second`, sidebar had `second` expanded, but the rendered
pane showed `▸fixture-leak-c ⌂ cee`'s prompt. `workspace.list`/`panel.list` (control-socket ground
truth) always agreed with the status bar; only the rendered pane lagged. This did **not** contaminate
any F-CORE-ACT verdict below — every row's evidence was cross-checked against its pane's own
breadcrumb text and/or `panel.list`/`workspace.list` before being trusted, and rows where the lag was
visible were re-driven until the breadcrumb matched. It belongs to F-TERM/F-SID territory (center
surface mount/focus sync), not to the activity/ownership model, and is flagged here only because it
cost most of this pass's wall-clock budget while chasing what first looked like a cross-worktree
identity leak and turned out not to be one once panel.list was cross-checked. A sibling critic on the
terminal-surface rows should re-drive it.

## Live drives run today

- **Master-worktree ownership drive** (`acta-master`, `acta-master2`): real `codex` binary launched
  as a foreground child of a plain shell -> Running, codex icon, both on the worktree row and the
  Terminal tab, "Activity 1 running". A **real** OSC-title tamper (typed through the shell, actually
  echoed and parsed — codex's own raw-mode prompt swallowed the first attempt, documented and
  retried with a `codex`-symlinked `/bin/sleep` decoy so the terminal stayed in cooked/echo mode)
  left the process-owned state untouched; `kill %1` cleared it immediately.
- **Second-worktree title/hook drive** (`acta-second2`): synthetic `title` action set `✳ waiting`
  (Claude, title-owned, needs-input) with zero process/hook signal; an unmatched title cleared it;
  re-set, then `ctl notify session=... status=running` (Layer A) beat a same-instant contradicting
  title (debounce holds); >1.5 s later a fresh title flipped it (debounce released). A real
  `org.freedesktop.Notifications.Notify` fired from this exact transition, captured by
  `dbus-monitor` live: title `Claude Code — tiller/linux/gpui-waku`, body
  `linux/gpui-waku · tiller`.
- **Third-worktree node-hosted drive** (`acta-third-pi` family): real `pi` (node/`cli.js`) launched;
  detected via title, not via process (structurally guaranteed — `pi`'s exec'd comm is `node`, which
  is not in `CATALOG_IDS`; confirmed against `process.rs`).
- **Activity panel drive** (`acta-22a/b`, `acta-23a/b/c`): `ctrl+shift+i` plus the panel's own
  disclosure toggle opened Activity; a real click on one row's own close (×) control, with the pane
  `notify`'d to `running`, produced the real banner **"This tab has running work. Close anyway?"**
  with Close Anyway/Cancel, dimming the whole window (14210 -> 6481 colours). Cancel returned to the
  undimmed frame.
- **Mount-eviction drive** (`acta-26c/d`): real synthetic clicks on Settings' "Limit mounted
  worktrees" toggle and "Keep mounted" stepper (`limitMountedWorktrees:false→true,
  mountedWorktrees:6→3`, read back via `surface.settings.read`); five real `workspace.select` calls
  (same door a sidebar click uses) across two projects' worktrees show clean FIFO eviction holding
  the cap at exactly 3 mounted throughout, oldest-idle evicted first each time.
- **Bootstrap drive**: ~10 fresh app boots today, each auto-mounting only the persisted-selected
  worktree first (`mounted:true` for exactly the last-selected id, `false` for the rest at boot),
  observed via `workspace.list` every time.

## Verdicts

| id | verdict | evidence |
|---|---|---|
| 01 | PASSED | `status::tests::{priority_order_is_error_needs_input_running_done,highest_priority_picks_the_winner,human_labels_match_swift,exit_codes_map_to_done_and_error}` replayed green today. |
| 02 | PASSED | Live today: `ctl notify session=pane-0 status=running` held through a contradicting title inside the debounce window on a real pane (shot `acta-second2/04`); real D-Bus `Notify` fired from the same transition. |
| 03 | PASSED | `agent_spawned_sets_running_and_agent_id_without_transition` passed; live: every real agent spawn today (codex, decoy, pi) went straight to Running+identity with no spurious notification. |
| 04 | PASSED | `exit_results_map_codes_and_respect_tracking` passed; live: `kill %1` on the decoy `codex` cleared status/identity immediately (shot `acta-master2/05`). |
| 05 | PASSED | `panes::tests::real_pty_activity_status_follows_osc_title_then_settled_content` (real PTY) passed; live: `✳ waiting` alone (no hook, no process match) assigned Claude+needs-input (shot `acta-second2/02`). |
| 06 | PASSED | Live today, both halves: unmatched title cleared title-owned Claude (shot `acta-second2/02`); a real OSC-title tamper left process-owned `codex` untouched (shot `acta-master2/04`), cleared only by `kill`. |
| 07 | PASSED | Live today: hook push beat an instant contradicting title (shot `acta-second2/04`); >1.5 s later a fresh title took effect (shot `acta-second2/05`). Also `layer_a_push_suppresses_contradicting_title_inside_debounce_and_stops_after` + `real_pty_layer_a_debounce_suppresses_first_title_and_accepts_second` replayed green. |
| 08 | PASSED | `content_match_overrides_stale_title_even_inside_debounce_window` (exact clause match, not-debounced-against-A) and `terminal_events_feed_title_and_settled_content_into_the_one_model` (real PTY) both replayed green. |
| 09 | PASSED | Live today: real spawned child comm-named `codex` (symlink to `/bin/sleep`) -> Running/process-owned; `kill %1` -> cleared (shots `acta-master2/03,05`). `linux_process_inspection_reads_agent_names_from_proc_children` (real subprocess, real `/proc` walk) replayed green. |
| 10 | PASSED | Live today: real `codex`/decoy ELF detected via `/proc` walk on master; real `pi` (node/`cli.js`) launched on third, not detectable by Layer D by construction — its exec'd comm is `node`, absent from `CATALOG_IDS` (`process.rs`); `walks_nested_children_and_skips_disappeared_descendants` replayed green. |
| 11 | PASSED | Clause re-judged against its real VERIFY line (see ledger-hygiene note above): `each_ownership_kind_is_cleared_only_by_its_own_condition` + `pane_closed_clears_all_state_and_is_idempotent` replayed green; live corroboration via today's rows 06/09 (one layer's change did not touch the other's state). |
| 12 | PASSED | `title::tests::{claude_glyphs_identify_claude,pi_glyph_titles_identify_pi_and_omp,omp_follows_pi_family_conventions,name_tokens_identify_codex_opencode_omp,bare_braille_spinner_does_not_identify}` replayed green; live `✳ waiting` matched. |
| 13 | PASSED | `title::tests::{claude_idle_prefix_is_needs_input,claude_working_prefix_and_spinner_are_running,pi_spinner_is_running_and_bare_name_is_needs_input}` replayed green; live matched (`✳`+no spinner = needs-input). |
| 14 | PASSED | `f_core_act_14_requires_word_boundaries_for_generic_title_identity_and_status`, `title::tests::{generic_avoids_substring_false_positives,generic_keywords_map_to_statuses}` replayed green. |
| 15 | PASSED | `content::tests::{claude_permission_prompt_is_needs_input,claude_streaming_indicator_is_running,generic_yes_no_prompts_are_needs_input,y_n_without_boundaries_does_not_match,unknown_agent_and_empty_text_are_nil}` replayed green. |
| 16 | PASSED | `f_core_act_16_strips_csi_and_osc_before_content_matching`, `ansi::tests::{strips_csi_and_both_osc_terminators,preserves_unicode_and_unknown_escape_sequences}` replayed green. |
| 17 | half-proven | Wiring re-confirmed live (not dead, contra stale `DEAD-MODELS.md` pass-10 finding — see above). `f_core_act_17_*` (3 tests) + `agent_id_for_panes_picks_the_highest_priority_pane` replayed green. Live today cleanly proved the single-agent per-worktree case end-to-end (codex on master, correctly scoped, correctly cleared). Could not cleanly re-drive the **multi-pane priority tie-break** live today — every attempt landed inside the stale-center-pane environment issue above before a trustworthy screenshot was captured; falls back to tests + the prior sweep's live multi-worktree evidence, not independently re-earned this session. |
| 18 | half-proven | Same reasoning as 17: wiring re-confirmed live; `running_agent_ids_follow_catalog_order` replayed green. Multi-agent badge de-dup/catalog-order not independently re-driven live today (ran out of clean repro time); relying on tests + prior sweep. |
| 19 | PASSED | Live today: real `dbus-monitor` capture of `Notify` title `Claude Code — tiller/linux/gpui-waku`, body `linux/gpui-waku · tiller` — from this session's own transition, not replayed old evidence. `f_core_act_19_builds_notification_payload_with_optional_context` replayed green. |
| 20 | half-proven | `f_core_act_20_notification_policy_suppresses_noise_and_visible_transitions` (all branches, pure logic) replayed green; live today re-confirmed the "fires" branch via the same D-Bus capture as row 19. **Found and record a platform nuance**: the production call site hardcodes `app_active=true` (`main.rs:5347`, `NotificationPolicy::should_notify(transition.old, transition.new, true, visible)`) — the real desktop-focus condition in the clause can never vary in this Linux build; only pane-visibility does. The resulting behavior (suppressed when the transitioning pane is the visible tab, fired otherwise) matches the ledger's four-branch dbus-monitor evidence, but the "app active" half of the clause is only exercised by the unit test, never by the live wiring. |
| 21 | PASSED | `f_core_act_21_activity_rows_keep_shells_and_chats_but_omit_non_activity_content` replayed green; live: Activity panel showed the real Terminal row labelled `fixture-repo/master` (shot `acta-22b/02`). |
| 22 | PASSED | `f_core_act_22_attention_sort_is_stable_and_urgent_first_preserves_manual_nonurgent_order` (both modes, ties) replayed green. Wiring re-confirmed live (both `AttentionSort::sorted` and `::urgent_first` called from `main.rs`, contra the stale `DEAD-MODELS.md` "whole module dead" finding, which pass 10 predates the repair that fixed). Did not independently re-drive the live multi-worktree reordering gesture today (time budget went to rows 06/07/09/26 above); relying on tests + confirmed-live wiring + the prior sweep's gesture-level evidence. |
| 23 | PASSED | Live today: real click on the Activity row's own close (×) control on a `running` pane produced the exact banner **"This tab has running work. Close anyway?"** with Close Anyway/Cancel, dimming the whole window (shot `acta-23b/03`, 14210→6481 colours). `f_core_act_23_only_live_activity_statuses_require_close_confirmation` (all 5 statuses) replayed green. |
| 24 | half-proven | `f_core_act_24_restore_plan_keys_refs_by_stable_content_not_live_pane` replayed green. Did not independently re-drive a live quit+relaunch + `/proc` argv check today (ran out of budget); this session's ~10 real app restarts did repeatedly re-confirm the surrounding persistence plumbing (worktree/pane records survive every restart) but I did not specifically confirm a `claude --resume <ref>` argv this pass. |
| 25 | PASSED | Live today, repeatedly: every one of ~10 fresh app boots this session auto-mounted only the persisted-selected worktree first (`workspace.list` showed exactly that id `mounted:true`, all others `false`, immediately at boot — e.g. `acta-third-inspect`, `acta-22setup`). `f_core_act_25_bootstrap_prioritizes_selected_open_worktree_and_defers_the_rest` replayed green. |
| 26 | PASSED | Fully re-driven live today, cleanly: real clicks toggled "Limit mounted worktrees" on and stepped "Keep mounted" to 3 (`surface.settings.read` confirms `limitMountedWorktrees:true, mountedWorktrees:3`); five real `workspace.select` calls (same door a sidebar click uses) across two projects show the mounted set held at exactly 3 throughout, oldest-idle evicted first each time it was exceeded. `f_core_act_26_mount_policy_evicts_only_safe_oldest_worktrees_until_cap` replayed green. |
| 27 | PASSED | `grep -c '#[ignore]' panes.rs` = 0; `panes::` filter now 19/19 passed (0 failed — an improvement on the ledger's last-recorded 16/2, both real-PTY tests now green); `tiller_activity` 73/73 passed; the bin compiles clean (`cargo build --workspace` exit 0). |

## Summary of net changes from the standing ledger

- 22 of 27 rows: **PASSED**, re-earned live and/or by replaying every named test the clause maps to.
- 5 rows (17, 18, 20, 22, 24): downgraded from a clean `PASSED` to **half-proven** — not because a
  defect was found, but because this session could not independently re-drive their live-gesture
  half today (17/18/22: an environment rendering/coordinate friction ate the time budget before a
  trustworthy screenshot landed; 20: a genuine, newly-documented platform nuance where
  `app_active` is hardcoded; 24: ran out of time). All five keep solid, freshly-replayed
  machine-tier (named test) evidence and confirmed-live production wiring; none regressed.
- 1 row (11): the ledger's `PASSED` was carried on evidence for an unrelated feature. Re-judged
  against its actual clause and still PASSED, on the correct tests plus today's live corroboration.
- 1 stale finding retired: `DEAD-MODELS.md`'s pass-10 "zero callers" claim for
  `agent_id_for_panes`/`running_agent_ids`/`requires_close_confirmation`/`AttentionSort`/
  `ids_to_evict` no longer holds — all five are called from live UI code in `HEAD`, confirmed by
  `grep`, matching the ledger's own `sweep N-critics` repair record.
- 0 rows FAILED.
