# Inventory status — where the 388 entries actually stand

Written by the orchestrator at 12:30 on 13/08; interaction-tier correction added on 13/08/2026.
**This file exists because of a process gap: the two
inventory files still contain 388 unticked `- [ ]` boxes.** Verdicts have been accumulating in the
critic's report and in builder replies for thirteen hours, and nothing consolidated them, so nobody
could answer "how far along is this" without reading everything.

## The rule this table obeys

The goal says done means **a critic that did not build a piece exercises the entry live and ticks
it.** So there are two different things here and they must not be added together:

- **Critic-confirmed** — exercised by `pireview`, which built none of it. These are the only ticks
  that count toward done.
- **Builder-claimed** — the builder exercised it and produced evidence. Real work, often with a
  live transcript, but it is a claim about its own piece and it awaits an independent pass.

Everything below is my reading of the reports. Where I am estimating, it says so. **Counts here are
not evidence; the evidence is in `CRITIC-baseline.md` and the pane reports.**

## PASS 9 — superseded by the ledger (authoritative)

`INVENTORY-LEDGER.md` now holds the definitive per-entry verdict for **all 388 entries**, judged
by pireview from critic passes 1–8 (snapshot 2026-08-13T12:27:17Z). **Where this file disagrees
with the ledger, the ledger wins.** Ledger totals:

| Verdict | Count |
|---|---|
| PASSED | **88** |
| half-proven | **46** |
| FAILED — absent | **90** |
| FAILED — defective | **4** |
| UNREACHABLE | **9** (opencode/omp not installed) |
| N/A — platform | **17** |
| NOT EXERCISED | **127** |
| NOT EXERCISED — blocked on display (appearance) | **7** |

Entries never independently judged by the critic: **120** (80 builder-claimed, unverified;
40 never claimed). The summary table below and the per-area rows are history; the ledger's
per-entry rows are the state.

## Summary

| State | Entries | Notes |
|---|---|---|
| Critic-confirmed PASSED | **88** | per-entry in `INVENTORY-LEDGER.md` (supersedes the ≈80 estimate below) |
| Builder-claimed, awaiting the critic | **≈38** | pass 4 is checking these now |
| FAILED — defect or structural absence | **≈16** | 8 `F-EDIT` absences, F-012, `session.ref`, plus 1; pass 6 adds WORK-01 (comment not persisted), CLI-02 (no shim), SAFE-02 + SESSION-01/02 (absent code) |
| UNREACHABLE — environment | **≈12** | `opencode`/`omp` not installed (6 F-AGENT entries) plus the earlier set |
| N/A — platform | **few + 6** | macOS TCC/Finder-specific, plus the six F-CTRL-BROWSER entries answering documented unsupported errors |
| NOT EXERCISED — blocked on display | **visual halves only** | interaction claims are reclaimable with the GPUI harness — **but pass 7 audited the reclamation and overturned 32 entries where no code exists**; see the pass 7 section below |
| Never touched | **see ledger** | 127 NOT EXERCISED (80 builder-claimed, 40 never claimed, 7 pass-judged) + 7 appearance-blocked — per-entry in `INVENTORY-LEDGER.md` |
| **Total** | **388** | 217 app + 171 packages |

## By area

### Closed or near-closed

| Area | Entries | Status |
|---|---|---|
| `F-AUTO` control socket & automation | 9 | Ledger: 7 PASSED (02–08, critic-confirmed at pass 3, not "awaiting pass 4"), F-AUTO-01 half-proven (toggle drawn, disable effect untested), F-AUTO-09 N/A (no browser). |
| `F-CORE-ACT` agent activity model | 26 | 19 PASSED, 0 FAILED, 3 UNREACHABLE (`opencode`/`omp`), 4 half-proven or blocked. Builder-claimed. |
| `F-PER` persistence & lifecycle | 8 | Ledger: 3 PASSED (02/03/05), 2 half (01/04), **F-PER-06 FAILED — defective** (pass 6: compound-command panes orphan process groups on quit), 07 appearance-blocked, 08 N/A. The "three quit cycles left zero stray processes" observation covered simple panes only; pass 6's compound-command leak supersedes it. |
| `F-TERM` terminals & agents | 11 | F-TERM-01 PASSED; **pass 7 correction:** F-TERM-02/04/05/06/11 have **no code** (no empty-pane/no-worktree state, no terminal context menu) — FAILED (absent), not reachable. F-TERM-03/09 state cores real with visual halves owed; F-TERM-07 partial. **F-TERM-08's termination half is a measured FAILED** (pass 6 WORK-05: compound-command panes leak process groups on close/quit; the confirmation prompt does not exist), not a reclaimable hold. F-TERM-10 not exercised. |

### Open, with work in flight

| Area | Entries | Status |
|---|---|---|
| `F-EDIT` documents & editors | 13 | The editor now exists (P34). **Pass 7 correction:** the builder's "drawn Code/Preview behavior for F-EDIT-01" is **not true** — no Code/Preview switch exists (only the F-EDIT-03 large-file preview lock); the builder's own comment in file_view.rs says the split is display-bound. F-EDIT-09's explorer double-click test exists but asserts only the emitted OpenFile event (not a resulting tab) and **fails under the pump budget in the 13:28 snapshot** (flaky live) — half-proven. F-EDIT-13's missing-file state is model-level. F-EDIT-10/11 (context menu) and F-EDIT-12 (drag) have **no code** — FAILED (absent). Typography and native/external effects remain separate debts. |
| `F-CHG` files, changes, activity | 22 | State/data tier **critic-confirmed** at pass 5 (07/08 full; 12/15/17 data). **Pass 7:** F-CHG-09's drawn error/Retry/recovery and F-CHG-08/12's drawn section/row/band clicks pass. F-CHG-10/11/14 drawn-button tests are the right kind of evidence but **fail deterministically in the 13:28 snapshot under the harness pump budget** (flake live) — half-proven until hardened. F-CHG-03/04's expand/collapse tests call `toggle_file` **directly (handler, not a drawn click)** — state evidence, not interaction. Keyboard navigation (F-CHG-05) and drag/drop (F-CHG-18) have **no code** — FAILED (absent). Activity (19-22) unexercised; exact appearance remains display-blocked. |
| `F-GIT` git tier (packages) | **16, not 19** | Critic-counted at pass 5: **8 PASSED · 2 PARTIAL · 6 FAILED** for absent behaviour — streaming runner, `branch --list`, clone, remote parsing, directory-status aggregation, side-by-side. |
| `F-TAB` tabs, panes, navigation | 28 | The drawn +-menu component dispatches every New Tab action (action-level; the shell effect is untested). **Pass 7 correction:** no chord-dispatch test exists anywhere — the "focused fixture proves GPUI chord dispatch" claim is **not in the tree** (only escape/enter keystrokes are ever simulated). The ctrl-tab/ctrl-1..9/ctrl-alt-* bindings exist in panes.rs and share handlers the socket exercised (tab.cycle/select, pass 4) — reachable, unexercised. Shell-owned strip activation/close (main.rs) is drawn-reachable and untested. Tab overflow/rename/reorder/drag (F-TAB-02/14/18/24), move-tab (12/13/21), close-others/right (17), context menus (09/26), attach/resume (25/27), no-agent fallback (08), and dirty-close prompt (16) are **absent code** — FAILED, not merely "not independently drawn-tested". Geometry/decorations remain visual debt. |
| project duplication + `session.ref` | — | **Closed (P35).** Project identity canonicalised, linked worktrees deduplicated, legacy rows migrated, `session.ref` in SQLite. `projects=1 worktrees=2` stable across three quit/relaunch cycles. |
| `F-PER-01` scrollback | — | **Closed (P35).** Bounded capture on quit, replay on restore; nonce survived two relaunches. This was the critic's named biggest fixable gap. |

### Package tier — worked at 12:55

| Group | Entries | Status |
|---|---|---|
| `F-CORE-ACT` | 26 | 19 PASSED · 0 FAILED · 3 UNREACHABLE · 4 half-proven/blocked |
| rest of `F-CORE-*` | 45 | **36 PASSED** · 0 FAILED · 4 N/A · 4 half-proven · 1 display-blocked |
| `F-GIT` | 16 (not 19) | critic-counted: 8 PASSED · 2 PARTIAL · 6 FAILED for absent behaviour |
| `F-CTRL-*`, `F-AGENT-*` | **54** (34 + 20; the brief said ~57) | **critic-confirmed at pass 6:** F-CTRL — 21 PASSED · 5 PARTIAL · 2 FAILED (WORK-01 comment persistence, CLI-02 shim absent) · 6 N/A (browser, documented unsupported). F-AGENT — 9 PASSED · 2 PARTIAL (API-01 no summarizer in the trait, SAFE-01 skill half) · 3 FAILED (SAFE-02, SESSION-01/02, structural absences) · 6 UNREACHABLE (opencode/omp not installed). No adapter writes outside the worktree — measured against a seeded fake HOME. Biggest finding: `workspace.close` leaves control panes' PTYs and orphaned process groups running (WORK-05). |
| `F-PERSIST-*` | **13** | **P38 builder claim:** 2 PASSED · 5 FAILED for absent Swift-v18 entities · 1 UNREACHABLE (the legacy `terminalTab` path is not present in the Linux rewrite) · 5 half-proven. `tiller_persistence` is claimed; real-file migration, future-schema refusal, non-SQLite/empty/truncated corruption, 64 MiB logical DB bound, session-ref deletion, and two-process disjoint writers are exercised. `tiller` now carries corrupt-store diagnostics through fallback restore. |

### Barely touched

`02-inventory-packages.md` holds **171 entries**. Critic-closed: `F-CORE-ACT` (26), `F-CTRL-*` (34), `F-AGENT-*` (20), `F-GIT-*` (16). The remaining ≈75 — `F-CORE-FILE`, `F-CORE-USG`, `F-CORE-DOM`, `F-CORE-WSP`, `F-PERSIST-*` — are domain logic and therefore **headless by construction**, which makes them the largest reachable body of work while no display exists. codex11 is starting on them (P36).

`F-BRW` (browser, 9) is unimplemented and answers explicit unsupported errors on Linux, which the
entry text permits. `F-WIN` (window shell, 12): **pass 7 correction** — F-WIN-01/07 routing/
restore halves stand (display-verified pass 1 / socket-verified pass 3); F-WIN-02/03/04/05 have **no
bindings** (no ⌘T/⌘O/⌘S/sidebar/right-panel toggles in the tree's full KeyBinding inventory) and
F-WIN-10 has **no toast implementation** (only a theme radius token) — FAILED (absent), not
"headlessly testable". Its geometry and pixel treatment remain visual.

## Pass 7 — the reclamation was audited entry-by-entry (13:35–13:52)

Audited every reclassified entry against the tree and ran the drawn tests in a forced-rebuilt
snapshot of `/tmp/critic-pass7`. The reclassification holds for roughly half the entries and
fails for the rest:

- **Upholdings (35):** F-CHG-09 drawn error/Retry/click recovery (passes); F-CHG-08/12 drawn
  section/row/band clicks (pass); F-CHG-10/11/14 drawn stage/discard buttons (right-kind tests,
  but they fail under the harness pump budget in the snapshot — flaky live, half-proven until
  the pump is hardened); F-EDIT-09 double-click → OpenFile event (flaky, event-level); F-CHG-03/04
  expand/error **state** (handler-direct, not drawn); F-TAB-03/04/05/07 +-menu → action (drawn);
  F-TAB-19/20/22/28 chords (bindings + handlers real, shared with socket-proven tab.cycle/select;
  reachable, still unexercised); F-TERM-03/09 and F-CHG-15/17 state cores; F-AUTO-01 drawn
  toggle/path; F-EDIT-13 model-level missing-file message.
- **Overturned (32) — reclassified as "headlessly reachable" where no code exists:** F-EDIT-01
  (no Code/Preview switch; the builder's own comment calls the split display-bound),
  F-EDIT-10/11 (no context menu), F-EDIT-12 and F-CHG-18 (no drag handlers), F-TAB-02/08/09/12/13/
  14/16/17/18/21/23/24/25/26/27 (no overflow/rename/move/close-others/drag/pane-menus/
  attach/resume/close-prompt code), F-TERM-02/04/05/06/11 (no empty states, no terminal context
  menu), F-CHG-05 (no keyboard handling), F-WIN-02/03/04/05 (no bindings), F-WIN-10 (no toasts).
  These are FAILED (absent), and the harness cannot reach what is not built.
- **F-TERM-08 is the worst case:** pass 6 *measured* the compound-command process-group leak on
  close/quit; the reclamation converted that measured failure into a "mixed, behaviour
  reclaimable" hold.
- **Drawn-test health:** 4 drawn tests fail deterministically in the 13:28 snapshot under the
  pump budget (state lands just after the loop gives up); the failing set differs between stale
  and rebuilt binaries. Every entry resting on them is half-proven until the pump is hardened.
- **Reachable but still marked blocked (the other direction):** F-CHAT-04's Shift+Return half
  (composer binds shift-return), F-SET-03 (Check for Updates click), F-CHG-20 (running count in
  the drawn activity section), F-TAB-15 (strip ✕). F-SET-21's file-icons behaviour half is
  already proven by a drawn test the audit did not list.

Full evidence: `CRITIC-baseline.md` PASS 7.

## Post-pass-7 builder update — P39 finish (13 August, ~14:00)

Builder-claimed facts about the tree since the pass-7 snapshot (13:28); none of these are
critic-confirmed. Pass 7's verdicts stand for the snapshot; this note records what changed after
it, with test names as evidence:

- **F-EDIT-01 is now built and drawn-tested.** The pass-7 snapshot had no Code/Preview switch;
  the current `file_view.rs` has `MarkdownMode`, a drawn Code/Preview switcher, and three hardened
  drawn tests (`markdown_preview_and_code_modes_switch_in_the_drawn_frame`,
  `a_code_file_has_no_mode_switch_and_always_renders_source`,
  `a_large_markdown_file_opens_in_code_with_manual_preview_and_unlocks`). Behaviour claim only;
  typography stays a display debt.
- **Drawn-test pump hardened.** `changes.rs::wait_for_tab`, `right_panel.rs::pump_until` and a new
  `file_view.rs::mounted_file_view` all use the 600-iteration / allow-parking / clock-advance /
  `run_until_parked` pattern, addressing pass 7's "state lands just after the loop gives up".
  `tiller_ui` is 105 green, `tiller_theme` 12 green (cargo test -p tiller_ui -p tiller_theme).
- **F-CHG-04 now has a drawn click test** (`clicking_a_drawn_directory_row_expands_and_collapses_it`);
  pass 7 saw only handler-direct tests.
- **Collapse All / Expand All added** to the Changes toolbar (orca's diff-header affordance) with
  `drawn_expand_all_and_collapse_all_drive_the_whole_list`.
- **Chord dispatch proven** (`tab_bar.rs::modifier_chords_dispatch_actions_through_the_real_key_path`:
  `ctrl-tab`, `ctrl-1`, `ctrl-w` through a scoped binding + key context + real keystroke dispatch),
  superseding pass 7's "no chord-dispatch test exists". The shell's workspace chords remain routed
  to codex12; `ctrl-w` is still unbound.
- **Payload drag proven** (`right_panel.rs::a_payload_drag_reaches_the_drop_target_through_real_mouse_events`):
  F-EDIT-12's harness half exists; the product's drag source/drop target are still absent in the
  shell — routed.
- **Honest partials (not passes):** F-SET-03's Check for Updates button is drawn but its handler
  is empty; F-CHG-11 has no section-level actions or Unstage all; F-CHG-20 has no "No activity"
  or running-count element; F-CHG-03 has no loading/error/Retry tree state. All recorded as
  PARTIAL, not reclaimed.
- **F-SET-21** behaviour half was already drawn-proven (`selecting_the_listed_file_icon_set_changes_the_snapshot`).

## Interaction-tier correction — 13 August

The old display classification bundled two different claims. P39 separates them with evidence,
not capability alone:

- **Builder-claimed drawn behavior:** F-EDIT-09's explorer double-click event (event-level,
  flaky); F-CHG-09's error/Retry/recovery (passes); F-CHG-10's Stage/Unstage, F-CHG-11's Stage
  all and Discard all, F-CHG-14's confirmation path (drawn, but unstable under the harness pump
  budget in the snapshot); F-CHG-12's diff/context-band expansion (passes). **Pass 7 corrects:
  F-EDIT-01's "Code/Preview switch" is not proven and does not exist in code; F-CHG-03/04's
  expand/collapse tests call the handler directly (state, not a drawn click).** These claims
  await an independent critic pass and retain any visual half as a debt.
- **Partial or routed:** the New Tab menu drawn test proves GPUI dispatch to the action level,
  but actual tab/pane creation and shell-owned tab/chord behavior live in `tiller/src/main.rs`
  and are routed to codex12. **No chord-dispatch test exists in the tree** (the chord bindings
  are real but unexercised). F-CHG-03/04 also retain their untested loading/error or
  file-selection portions; F-CHG-11 retains Unstage all.
- **Absent, not display-blocked:** F-EDIT-12/F-CHG-18 drag/drop, Files keyboard navigation, tab
  drag/rename/overflow behavior, and the inventory's Cmd-W close binding are not implemented or
  not present in the exercised surface.
- **Appearance-only:** colors, fonts, glyph/status treatment, exact geometry/spacing, comparison
  against waku, and pixel freeze claims remain `NOT EXERCISED — blocked on display`.

See [INTERACTION-TIER-AUDIT.md](INTERACTION-TIER-AUDIT.md) for the entry-by-entry mapping. A
drawn-frame evidence test moves only the behavior claim; it never promotes its pixels.

## What "blocked on display" covers

Only claims whose substance is pixel appearance: layout and spacing, colour, font rendering,
icon/status treatment, visual comparison against waku, the newly mounted Changes surface's
appearance, and the freeze's pixels. A drawn frame proves presence, bounds, hit-testing, and
interaction; it does not prove what the pixels look like.

No display on this machine presents. `:1` died at 09:56; `:2`'s Xwayland wedged and, once freed,
still produces one-colour frames; displays created after ~09:55 do not present under any driver
combination tried. The cause is a compositor that cannot import an AMD tiling modifier
(`0x0200000000000005`), most likely after a GPU device reset — `/dev/dri` has `card1` and no
`card0`. **The fix is a compositor restart and it belongs to the operator.**

These entries are **debts, not approximations awaiting promotion.** A behaviour half may leave this
display bucket after a real GPUI view test; its appearance half may not be ticked from reading the
source or from a headless frame.

## How to keep this file honest

Update it when a critic pass reports, or when a documented GPUI view-test audit changes the reason
for a hold. Move entries from *builder-claimed* to *critic-confirmed* only on the critic's word.
Move an interaction half out of *blocked on display* only when a drawn-frame input test exists;
never move its appearance half without a real display comparison.

## PASS 8 — per-entry sweep of the unjudged remainder (14:15)

Critic pass 8 judged F-TAB (28), F-WIN (12), F-BRW (9) and F-SID (19) per entry, plus F-SET/
F-USE/F-CHAT spot-checks, against a 13:56 snapshot (full suite green: tiller_ui 105/105 ×3,
tiller 53/53). Corrections to the table above:

- **F-TAB — the exact absent list, not a count: 02, 08, 09, 12, 13, 14, 16, 17, 18, 21, 24, 25,
  26, 27, 28 are FAILED (absent)** (pass 7's "13" was the mega-row count). Half-absent:
  F-TAB-11's disabled-reason display (model unit-tested, never rendered), F-TAB-23's Pane menu
  and left/up directions (right/down splits are real). **PASSED: 03, 04, 10, 15, 19, 20, 22**
  (03/04 drawn action + shell tab creation; 10/19/20/22 exercised live over the socket — pane
  split/focus, tab cycle/select observably moved the active flag). F-TAB-01/05/07 half. F-TAB-28
  has no ⌘W — `ctrl-alt-w` closes a pane, not a tab.
- **F-WIN — 2 PASSED (01 routing, 07 restore) · 5 FAILED absent (02 ⌘T, 03 ⌘O/⌘S, 04 sidebar
  toggle, 05 right-panel toggle, 10 toasts) · 5 N/A-platform.** There is no menu bar in the shell
  at all; the F-WIN command bindings and most F-TAB/F-SID menus hang off it.
- **F-BRW — all 9 FAILED (absent), and the socket's unsupported errors are specific** (each
  `browser.*` method named in its error, distinct from `unknown control method`). No permission
  store exists (F-BRW-06/07/08).
- **F-SID — 03 and 05 are now PASSED** (pass 1's FAILEDs no longer hold: drawn add-project flow
  with real picker; drawn selection event + live socket worktree mount changes the central
  surface). 10/11 PARTIAL, 13/15 PASSED (real git), 16/17 FAILED-absent-by-design, 07/08/09/12/
  14/18/19 FAILED absent.
- **F-SET-03 is worse than pass 7 said:** Check for Updates is a dead no-op button; no update
  states exist. F-SET-02's Escape still unbound.
- **F-CHG-10/11/14 and F-EDIT-09 are no longer half-proven on timing** — the four drawn mutation
  tests pass deterministically in this snapshot (×3); the pump defect pass 7 routed is fixed.
- **F-CHAT surface:** model picker (16) and context ring (18) are drawn-PASSED; absent from the
  surface: 17, 19, 21 (toggle), 22, 26, 27, 28, 31, 32, 34, 35, 36, 14, 09–12; 24/25 reduce to
  the generic Permission card.

Full evidence: `CRITIC-baseline.md` PASS 8. Biggest non-display gap: **the absent menu bar /
shell command layer** — it explains 19 of the 29 absent non-browser features (F-TAB 02/08/09/12/
13/14/17/21/26/27, F-WIN 02/03/04/05, F-SID 07/08/09/12/14).

## P44 — the chat surface, per entry (13 August, ~16:00)

The verdict "the chat surface has not been exercised since 01:00" was the oldest surviving claim;
13 hours and many rebuilds later, every `F-CHAT-*` entry was judged against the tree with the
present-versus-reachable rule, in order: does it exist, can it be exercised, does it satisfy the
VERIFY clause. Full per-entry table and evidence names: `INTERACTION-TIER-AUDIT.md` §P44. All
drawn tests are hardened with the 600-iteration pump and the suite passed **three consecutive
runs** (128 green: tiller_ui 116 + tiller_theme 12).

**Counts (37 entries):** **7 PARTIAL** (05, 07, 08, 16, 18, 23, 25 — the drawn half passes, the
other half is absent) · **5 BEHAVIOUR PASSED** (01, 03, 04, 33's error half is PARTIAL, 37 — the
fully-proven drawn set is 01, 03, 04, 07's Escape half, 25's option half, 37) · **25 FAILED —
absent** (02, 06, 09–15, 17, 19–22, 24, 26–32, 34–36). No entry is display-blocked as a whole:
the blocked remainder of every mixed entry is its appearance half (bubble geometry, coral accent,
type scale, waku comparison), which stays a debt.

**The seam:** a stream that dies mid-reply states an error card with a working Retry (drawn,
reconnect proven); a denied permission records `Answered: deny` on the card (drawn); a cancelled
request previously closed with a plain timestamp — the surface now states the stop reason in the
turn footer (`· cancelled`, `· refused to continue`, `· stopped at the token limit`,
`· stopped at the turn-request limit`) from the protocol's own `stop_reason`, proven over the
wire by the Escape test. **Nothing was needed from the protocol tier.**

**The four missed entries, closed:** `F-SET-03` — version + drawn Check for Updates click proven
(`general_settings_state_the_version_and_the_updates_control_is_clickable`); the button's handler
is still empty, so the checking/result states remain absent (PARTIAL, not passed). `F-CHG-20` —
the two missing elements ("No activity" empty state, `N running` header count) were built and
drawn-tested (`activity_section_states_no_activity_when_empty`,
`activity_section_states_the_running_count`). `F-TAB-15` — the sidebar view of the tab strip's ✕
is drawn, hover-revealed, clicked, and emits `CloseTab` with the real tab id
(`the_drawn_tab_close_control_reports_closeta_tab`); the centre strip stays shell-owned (routed to
codex12). `F-SET-21` — already drawn-proven pre-P44 (`selecting_the_listed_file_icon_set_changes_the_snapshot`);
recorded as closed, appearance half remains a debt.

**Honest remainder:** the 25 absent entries are real gaps (no queue, no slash/mention/attachment
surfaces, no stop control, no plan/question/task cards, no history menu, no scroll-ownership, no
code-block copy, no overflow menu). The chat surface is the centre of the app and most of its
inventory is unbuilt; the drawn harness now reaches everything that exists.
