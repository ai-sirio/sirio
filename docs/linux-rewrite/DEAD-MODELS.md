# DEAD-MODELS — the dead-model census (FABLE-05)

Written by fable, 2026-08-13 ~20:05 CEST, from `Scripts/dead-models.py` against the
**live working tree** at `dbfd776` (the scan reads the filesystem, builders' uncommitted
edits included). **This document changes no verdict** — only the critic edits
`INVENTORY-LEDGER.md`. Each entry names the rows it bears on and the direction:
**↓ downward** = a PASSED row's function is dead; **↑ upward** = a FAILED row's feature
is built and unwired.

## The sweep is a moving target — pin before adjudicating

Three runs inside thirty minutes gave **58 → 65 → 58** DEAD. The 65 was a transient
(pi mid-refactor in `tiller_ui/settings.rs` — callers momentarily gone, line numbers
moved 614→488 between runs). Between the first and last run, **`failure_message`
(tiller_terminal) left the list** — a builder wired or removed it *during this census* —
and `render_tab_context_menu` entered it (born mid-work, plainly about to be wired).
The census below is pinned to the 19:35 run (58 DEAD / 21 flagged / 37 unflagged — the
brief's 38 had already drifted by one). Whoever applies this: **re-run the script and
diff against this list first**; single rows have a half-life of hours here.

## Scan blindness corrected by hand (read before trusting any number)

1. **Crate-level `tests/` dirs were outside the scan** (src/ only), so `test=0`
   under-reported: all 8 unflagged `tiller_activity` names are exercised in
   `tests/activity_*integration.rs`; `is_likely_valid` in `tests/session_sources.rs`;
   the four persistence names in `tests/persistence_integration.rs`. The cluster reads
   as *untested*; it is **tested and unwired** — the stronger claim. (Fixed in the
   script; TEST now includes `tests/`.)
2. **`examples/` too**: `new_for_demo`, `new_with_default_context` are demo-wired
   (sidebar_demo.rs, chrome_demo.rs). (Now flagged `~example=` mechanically.)
3. **`pub(crate)` makes `out=0` vacuous** (`process_signal_interval`, `raster_size`).
   (Now marked `[crate-vis]`.)
4. **Masking creates false NEGATIVES, never false positives**: common-word names
   (`sorted`, `rows`, `status`) and methods sharing a field's name (`on_refresh`,
   status_bar.rs — F-USE-01's known dead API, invisible here) never appear. Every DEAD
   row is textually airtight; absence from the list proves nothing.

## Disposition of all 58

**Deliberate, out (6):** `in_memory` (doc'd fixture ctor — note: **zero references
anywhere**, even tests use real files; delete candidate; `F-PERSIST-DB-01`'s evidence
still cites it), `new_for_demo`, `new_with_default_context` (example-wired),
`with_account_states`, `with_availability` (doc'd test/preview seams),
`process_signal_interval` (carries `#[allow(dead_code)]`; the 500 ms const is live).

**True redundant variants, out (7):** `save_projects`, `save_worktrees`,
`stage_entries`, `unstage_entries`, `discard_change_entries`,
`discard_untracked_entries` (trap #2, adjudicated in the brief), `decode_or_empty`
(prod restore uses `decode` + explicit fallback). These seven are now exactly the
script's flagged set — see the heuristic results at the bottom.

**Superseded parallel model, out (4) — correction to the brief's F-EDIT hypothesis:**
`set_text`, `refresh_from_disk`, `has_conflict`, `is_deleted` (`MarkdownDocument`,
tiller_markdown). The UI's `use tiller_markdown::{Document, parse}` imports the
**parser AST**, not this type; `MarkdownDocument` has zero callers as a type. The
editing feature lives in **`tiller_ui::editor::Editor`**, which production FileView
drives (`Editor::open` file_view.rs:76; dirty/conflict read :119-124; header comment:
"an editor, not a viewer (P34)"). The 11 F-EDIT FAILED rows are therefore neither
"absent" nor "three lines from `MarkdownDocument`": their substance is being built on
the *other* model (P34/P48 claims, unverified). `MarkdownDocument` is a dead twin —
delete-or-unify; do not cite it to reclassify F-EDIT rows. This trap — parallel model
in a *different crate* under *different names* — is invisible to any name heuristic;
`expand_project`/`collapse_project` (below) are its miniature: workspace-side expand
state dead beside the sidebar's live local state.

**Trivial dead convenience surface, delete-or-wire-cheap (15):** `is_clean`,
`activity_tab_id`, `activity_pane_ids`, `reads_local_state` (the "callers can decide
what to show" that never materialized — F-SET-10's dead visibility toggle, adjacent),
`raster_size`, `id_for`, `database_limit_bytes` (the 64 MiB cap itself is installed and
live; only the reporting accessor is dead), `claude_state`/`codex_state`/
`opencode_go_state` (status_bar per-provider accessors, same family as F-USE-01's dead
`on_refresh`), `effective_name`, `persistence_enabled`, `surface_opacity`,
`collapse_project`/`expand_project` (unwired variants beside a live `toggle_project`).

**Survivors — genuinely built-and-unwired (26, one already re-wired mid-census):**

| function | site | bears on |
|---|---|---|
| `requires_close_confirmation` | activity.rs:30 | **F-TERM-08 ↑ confirmed**; ACT-23 ↓ |
| `ids_to_evict` | mount.rs:9 | **F-SET-07/ACT-26 ↑ confirmed** |
| `partition` | bootstrap.rs:16 | ACT-24/25 (already recorded ↑) |
| `build_payload` / `should_notify` | model.rs:420 / notification.rs:19 | ACT-19/20 ↓ (FABLE-04, pending); F-USE-06, F-AUTO-06 |
| `agent_id_for_panes` | model.rs:377 | **ACT-17 ↓ new** |
| `running_agent_ids` | model.rs:399 | **ACT-18 ↓ new**; F-SID-11 badge half |
| `urgent_first` | sort.rs:20 | **ACT-22 ↓ new (partial** — `sorted` half unresolvable, common word) |
| `register_agent_id` | model.rs:147 | no row — restore-identity seam, below |
| `pane_closed` | model.rs:338 | no row — close-cleanup leak, below |
| `with_environment_override` | project settings.rs:48 | **F-CORE-SET-01 ↑ new** |
| `should_request` + `record_request` | domain.rs:100/:106 | **F-CORE-DOM-07 ↓ new** (auto-naming throttle, zero consumers; coheres with F-SET-05 "report-only") |
| `needs_refresh` | usage codex.rs:51 | **F-CORE-USG-05 ↓ new (partial** — the 8-day-refresh half is never consulted; merge-save half masked) |
| `parse_claude_json` | account.rs:50 | **F-CORE-AUTH-01 ↓ new**; F-SET-14/15 ↑ |
| `format_markdown` | file_view.rs:177 | **F-EDIT-02 ↑ new** (below) |
| `from_buffer` | editor.rs:375 | F-EDIT fresh-file leg ↑ (below) |
| `is_plain_text` | editor.rs:200 | F-EDIT-01 mode-gating adjacency |
| `is_likely_valid` | session_validator.rs:11 | F-AGENT-SESSION-01 (row already states wiring owed) |
| `migrate_file` | hook_migrator.rs:50 | F-AGENT-SAFE-02 (row already states wiring owed; old `~sibling=migrate` flag was cross-crate noise) |
| `rows_from_lines` | side_by_side.rs:74 | F-GIT-DIFF-03 (disclosed ↑); appearance debt now that the display works |
| `split_disabled_reason` | panes.rs:207 | F-TAB-11 (already recorded ↑) |
| `worktree_by_path`, `delete_session_ref`, `quarantined_records` | db.rs:288/:615/:569 | F-PERSIST-DB-08/06/07 claims: capability without a consumer — P56 cites their proofs, no caller owns them |
| `failure_message` | tiller_terminal lib.rs:501 | **left the DEAD list during this census** — builder drift resolving a row in real time |

## Downward — PASSED rows the census bears against

FABLE-04's four-plus-partial (ACT-19, ACT-20, CTRL-NOTIFY-03, FILE-04, ACT-02-partial)
are **still awaiting critic application** — pass 12's "10 re-marked" were pireview's own
UI-row audit. The census reproduces ACT-19/20 mechanically; not recounted.

**New undisclosed zero-consumer PASSEDs: 5 full + 2 partial, all judged pass 10.**

- **F-CORE-ACT-17** — `agent_id_for_panes` (the ported `agentIdForWorktree`): zero
  callers; the app's `agent_id_for_action` (main.rs:2074) is a different function.
- **F-CORE-ACT-18** — `running_agent_ids`, whose docstring names its purpose ("the
  worktree row's trailing running-agents badge"): zero callers, no badge renders.
- **F-CORE-ACT-23** — `requires_close_confirmation` (also upward, F-TERM-08).
- **F-CORE-AUTH-01** — `parse_claude_json`: no surface consumes account identity
  (F-SET-14/15 absent agree).
- **F-CORE-DOM-07** — both halves of the auto-naming throttle (`should_request`,
  `record_request`) are dead; nothing ever asks permission to auto-name. Coheres with
  F-SET-05 (auto_naming report-only). *Surfaced only after the sibling-flag fix — the
  old `~should` flag was hiding it as a "redundant variant".*
- **F-CORE-ACT-22 (partial)** — `urgent_first` dead; `sorted` half stays with the critic.
- **F-CORE-USG-05 (partial)** — `needs_refresh` dead: the tested 8-day refresh decision
  has no consumer; the token merge-save half is name-masked and stays with the critic.

Disposition note: pass 12 set a precedent in F-GIT (RUN-02, BRANCH-01, CLONE-01,
REMOTE-01, STATUS-02, DIFF-03) — package rows may stay PASSED **with "zero app callers —
wiring owed" disclosed in evidence**. The seven above have no such disclosure; today
they are indistinguishable from delivered rows, which is the defect class FABLE-04
measured at 21% on UI rows. Re-mark or annotate — either restores honesty.

---

## Resolution, orchestrator, 2026-08-14 08:00 — the precedent was the defect

**This census was right and went unapplied for twelve hours.** Every `↓ new` above named a
row that was still counted as delivered this morning. Applied now: fourteen rows
overturned, `PASSED` 210 → 196.

**The pass-12 precedent quoted above is rejected.** "Package rows may stay PASSED with the
gap disclosed" is a category error in *this* ledger, which counts app behaviour and not
library coverage. Disclosure makes the evidence honest and leaves the verdict wrong — and
the verdict is what the totals are computed from, so an annotated false PASSED still spends
the same credit as an unannotated one. The disposition note's "re-mark **or** annotate —
either restores honesty" is the one line to discard: annotating restored the honesty of the
*sentence* and not of the *count*.

The replacement rule, now in the ledger: **a row whose evidence names a gap cannot hold a
verdict that denies it.** Zero app callers ⇒ `UNREACHABLE`, and the wiring becomes queue
work. Same disposition critic2 reached independently on `F-TERM-02` and `F-TERM-PTY-06`
("complete-and-unreachable is not PASSED").

### The two partials are resolved — and the technique generalises

Both were handed to the critic as unresolvable because of the name-masking limitation this
document declares up front ("common-word names never appear… absence from the list proves
nothing"). **Read them as open questions; the ledger rows had read them as findings** —
`F-CORE-ACT-22` said "the sorted half stays live" and `F-CORE-USG-05` said "the merge-save
half stays live". Neither was true.

The way past the mask is to stop grepping the common word and **check the module's export
surface instead**:

- **`F-CORE-ACT-22`** — never grep `sorted`. `sort.rs` exports exactly one name,
  `AttentionSort`, which has **0 app references**. The whole module is dead; there is no
  live half.
- **`F-CORE-USG-05`** — never grep merge-save. Enumerate `tiller_usage/src/codex.rs`'s
  exports and check each: `needs_refresh`, `classify_token_refresh_failure`,
  `codex_auth_file_path`, `codex_has_credentials_at`, `load_codex_credentials`,
  `CodexOAuthCredentials` — **all 0**. No live half either.

A module's export list is short, unambiguous and always greppable, whatever its function
names are called. Where the scan says "masked", enumerate the exports.

## Upward — the FAILED side

**The two named candidates: both confirmed.**

1. **F-TERM-08** (defective, "no confirmation prompt"): the confirmation *policy* is
   built and integration-tested for all five states (Running/NeedsInput/Error → true);
   prompt infrastructure exists elsewhere (F-SID-10's remove-project confirmation).
   Missing: the call. Wiring, not construction.
2. **F-SET-07 / F-CORE-ACT-26**: `ids_to_evict` dead — the machine count independently
   reproduces the critic's hand finding ("eviction has zero callers"). The eviction
   policy (cap; selected/running/unsaved exemptions) is complete and tested.

**New upward items no ledger row records:**

3. **`TILLER_SOCKET_ENABLE` is inert in production** — the strongest new fact here.
   `with_environment_override` is called only from its own `#[cfg(test)]` block; the
   boot path reads saved settings and calls
   `control_socket.set_enabled(saved_settings.control_socket_enabled)` (main.rs:6167,
   :6209) — the env var is never consulted. Documented (mirrors macOS), tested, does
   nothing. One call in the boot path makes it true. Bears on **F-CORE-SET-01** (which
   counts the policy as tested) and the F-AUTO-01 disable story.
4. **F-EDIT-02** ("no formatting toolbar", FAILED — absent): the *operation tier*
   exists — `FileView::format_markdown` applies `MarkdownFormatOp` to the live editor
   through selections, and its own docstring says "until a textarea exists this is the
   shell's entry point" (file_view.rs:177). `from_buffer`'s twelve tests are exactly
   "the formatting-operation tests". Absent at the toolbar, built beneath it.
5. **`register_agent_id`** — restored panes never get their identity re-registered
   (the docstring exists for precisely the restore case). Restore works (F-PER-05,
   F-CTRL-SESSION-01); restored agent panes are identity-less until a hook or title
   arrives. No row owns this seam.
6. **`pane_closed`** — nothing removes a closed pane's state from the activity model:
   `agent_status`, `pane_agents`, ownership sets grow monotonically. A leak, not a
   feature gap (pane ids are unique, so nothing visible corrupts). No row owns it.
7. **`from_buffer`** (editor.rs:375, **out 0, in 0, test=12** — verbatim: twelve tests
   exercise a constructor no production code calls). Precisely: the editor *is* wired
   via `Editor::open`; `from_buffer` is the documented fresh-file leg ("flows that
   create a fresh file") — a flow that does not exist. Still the clearest single
   illustration of the project's defect, one notch less damning than the brief assumed.

**Already recorded by passes 11/12 — census corroborates mechanically (7):**
ACT-24/25, ACT-26, F-TAB-11, F-USE-06, F-CORE-FILE-06, F-AGENT-SESSION-01,
F-GIT-DIFF-03. Honest accounting: the critic's pass-11 idiom ("zero app callers")
already harvested much of the upward direction row by row. Of 124 `FAILED — absent`,
the census reclassifies far fewer than the hoped dozen — the biggest absent blocks
(F-PRJ ×18, F-BRW ×9, most F-SET/F-TAB) have no dead function under them. **Absent
mostly means absent.** The dead functions cluster under rows already defective, already
disclosed, or under no row at all.

## The tiller_activity cluster — holds up, with sharper wording

**10 of its 48 `pub fn` are dead.** One correction to the brief's phrasing: the
**detection core is wired and live** — the app calls `notify` (Layer A),
`agent_spawned`, `apply_exit_result`, `process_gone`, and status reaches the sidebar
dot (FABLE-04 exonerated that chain). What is dead is precisely **every product feature
built on top of detection**: identity badges (`agent_id_for_panes`,
`running_agent_ids`), urgency ordering (`urgent_first`), close confirmation
(`requires_close_confirmation`), restore identity (`register_agent_id`), close cleanup
(`pane_closed`), launch-restore ordering (`partition`), eviction (`ids_to_evict`),
notifications (`build_payload`, `should_notify`). The engine runs; nothing downstream
consumes it beyond the dot. That is still the single largest structural fact about this
rewrite, and it is now an enumeration a machine can re-derive instead of an impression.

## Script changes (Scripts/dead-models.py) — and their measured effect

1. `tests/` dirs feed TEST; `examples/` references print `~example=` (mechanically
   triages demo constructors).
2. `pub(crate)`/`pub(super)` rows print `[crate-vis]` — `out=0` is vacuous there.
3. Sibling heuristic: first-segment candidate added (`discard_change_entries` →
   `discard`), and the sibling must be **defined as a fn in the same crate**.
4. Header documents the two new trap classes (parallel model in another crate;
   same-name field masking).

Measured: flags went **21 → 8**, and the stable seven are *exactly* the hand-adjudicated
true variants — including the two the old rule missed (`discard_*_entries`) — while the
fourteen noise flags (`~should`, `~build`, `~needs`, `~claude`, cross-crate `~migrate`…)
died. Two of those noise flags were actively hiding real findings (DOM-07, USG-05
above): a false "redundant variant" is worse than no flag, because it reads as
"feature live elsewhere" and ends triage early. Not fixable by counting: common-word
functions stay invisible either way; those remain reading work.
