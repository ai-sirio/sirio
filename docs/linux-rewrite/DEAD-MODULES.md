# DEAD-MODULES — the cluster tier (FABLE-06)

Written by fable, 2026-08-13 ~20:55 CEST, from `Scripts/dead-models.py --modules` against
the **live working tree** at `dbfd776` (builders' uncommitted edits included; the fn-tier
baseline drifted 58 → 52 DEAD since the 19:35 census, so re-run before relying on any
row here). **This document changes no verdict** — only the critic edits
`INVENTORY-LEDGER.md`. It is a separate file, not a section of `DEAD-MODELS.md`, on
purpose: that census is pinned to its 19:35 run and queued for `pireview`'s 5+2
adjudication; a new instrument must not move a pinned target under the critic.

## The instrument

The function census cannot see a subsystem: every member of a mutually-calling group has
`in > 0`, so whole dead features land in `local` among 211 rows. `--modules` therefore
measures **reachability, not counts**: an edge "file B consumes a pub item defined in
file A", BFS from the product binaries (`main.rs`, `tillerctl.rs`; `changes_preview.rs`
is a dev harness and gets its own tier), and whatever no root reaches dies **as a
cluster** — `resolve_file_link`'s shape, made mechanical. On top sits a **strict tier**
(liveness must flow through uniquely-defined names, in comment-stripped text, in
call/path/type syntax position): files alive in the loose graph but not the strict one
print as **weakly live — evidence-free alive**, and are exactly the reading queue.

Three rules were forced by failures during construction, each now a header guard:

1. **Publication is not consumption.** Naively, lib.rs `pub use` lines vivified their own
   crate's dead modules and the sweep found **0 dead files**. Stripping `pub use` from
   the referencing side (imports on the consumer side are kept — cosmic/hex.rs lives as
   `use super::hex::parse as hex;`) took it to **11 dead files in 10 clusters**.
2. **A bare word is not a reference.** `let mut terminal = …` kept the activity
   projection alive through `ActivityTab::terminal`'s name. Syntax anchoring in the
   strict graph (`Name::`, `.name(`, `Name{`, `: Name`) grew the weakly-live queue
   7 → 14, and reading killed half of it.
3. **A zero needs a kill-proof.** The controls plant two synthetic files that reference
   each other and nothing else; the sweep refuses to print unless they die together as
   one cluster, through the same prep, graph and BFS as everything else.

Runtime ~24 s; the fn tier is byte-identical in behaviour.

## Mechanically unreachable: 11 files, 10 clusters

| cluster | file | bears on |
|---|---|---|
| pair | tiller_agents/session_validator.rs + transcript.rs | F-AGENT-SESSION-01 (disclosed); transcript.rs (Claude/Codex `recent_text` readers) has **no row** — SESSION-02's "wiring still needs to consume" names session_sources, not this pair |
| 1 | tiller_activity/bootstrap.rs (`partition`) | ACT-24/25, recorded ↑ |
| 1 | tiller_activity/mount.rs (`ids_to_evict`) | F-SET-07 / ACT-26, recorded ↑ |
| 1 | tiller_agents/hook_migrator.rs | F-AGENT-SAFE-02 (disclosed) |
| 1 | tiller_markdown/editing.rs (`wrap_selection`, `prefix_selected_lines`) | **dead twin #2**: the live ops are tiller_ui editor's `MarkdownFormatOp`; delete-or-unify, do not cite against F-EDIT |
| 1 | tiller_markdown/file_events.rs (`FileSystemEventMonitor::poll`) | follow-external-edits watcher, zero consumers; adjacent to F-CHAT-14 "Follow Edited Files" claim — ~~**no row owns the seam**~~ **CORRECTED 2026-08-14 (see the block at the end of this file): `F-CORE-FILE-06` owns it by name** — its clause requires "auto-reloads external changes… external deletion/rename is surfaced" and its PLATFORM note says *"Linux needs inotify/fanotify or equivalent"*. The row is `FAILED — absent` and its evidence already reads "FileSystemEventMonitor (inotify) has zero callers outside its crate". The *chat* consumer you meant was genuinely unowned, and is now `F-CHAT-14 | FAILED — defective` |
| 1 | tiller_project/file_link.rs (`resolve_file_link`) | the original hand-found cluster, now derived mechanically — the tier's known-answer test |
| 1 | tiller_project/settings.rs (`with_environment_override`) | `TILLER_SOCKET_ENABLE` inert — **queued for codex12, not duplicated here** |
| 1 | tiller_project/skill.rs (`agent_skill_install_command`) | **contradicts the ledger**: F-SET-09 and F-AGENT-SAFE-01 say "no skill code in the port" — the provisioner command exists here (npx skills add …), tested, zero callers. Graders looked in tiller_agents; it lives in tiller_project. Absent → built-and-unwired. **No row owns this correction.** |
| 1 | tiller_usage/ollama.rs (`parse_ollama_cloud_usage`) | 5th-provider usage indicator, dead; ~~**no row**~~ **CORRECTED 2026-08-14: `F-CORE-USG-03` owns the parser** — its clause is *"Ollama Cloud parsing is explicitly best-effort…"* and its VERIFY is *"Feed representative Ollama Cloud payloads"*. That is a parser-level clause, so its `PASSED` on a unit test is **correct and stands**: being unwired to the status bar does not falsify a claim about what the parser returns. The dead *indicator* is a display concern, not this row's (joins status_bar's dead per-provider accessor family) |

Collapse: 5 of 52 fn-DEAD and 13 of 211 fn-local rows sit inside these 11 files.

## Dead by reading: 7 more, masked by collisions and short names

The weakly-live queue (14 files) triaged one at a time, mirror test applied:

- **tiller_activity/rows.rs, sort.rs, activity.rs, session.rs** — the whole
  **activity-panel projection** is unconsumed: `build_activity_rows` has zero production
  callers (xcrate=133, all word-collisions and 216 test refs), `AttentionSort::sorted`
  zero (ACT-22's masked half — **both halves now dead**), `AgentSessionRestorePlan::plan`
  zero. The UI does not import this model: **sidebar.rs:33 uses
  `crate::right_panel::ActivityStatus` — a duplicated enum**, the parallel-model trap as
  architecture. Four files, one dead feature: the panel projection FABLE-05 predicted.
- **tiller_git/side_by_side.rs** — zero mentions in ui/app (`rows`, `rows_from_lines`
  and all 7 surface items); changes.rs builds its own diff rows. Whole-module twin;
  F-GIT-DIFF-03's "the display works" is true of changes.rs, not of this module.
- **tiller_project/file.rs** — `classify_file_drop`, `terminal_file_drop`,
  `load_file_tree`, `FileTreeEntry`: the **file-drop / file-tree data layer, built and
  consumed by nothing**. The fn census binned these as `local` (lib.rs mentions), which
  is precisely the blindness this tier exists for. Bears ↑ on F-BRW ×9 / F-PRJ drop
  rows: their surfaces are absent, but a data layer exists beneath them. ~~**No row.**~~
  **CORRECTED 2026-08-14: split it — the drop half is owned three times over.**
  `F-CORE-FILE-03`'s clause is *"Accepted terminal file drops become one shell-quoted,
  space-separated path string… written to the pane"*, which is `terminal_file_drop` /
  `classify_file_drop` verbatim; `F-TERM-PTY-06` (`UNREACHABLE`) names
  `tiller_project::terminal_file_drop` in its own evidence; `F-EDIT-12` covers the drag
  source. Only `load_file_tree` / `FileTreeEntry` may still be unowned — and that half is
  more likely a **parallel dead model**, since the Files panel renders from `right_panel.rs`
  and does not import this one (the same shape as `sidebar.rs:33`'s duplicated
  `ActivityStatus` noted above).
- **tiller_git/directory_status.rs** — publication-only (lib.rs `mod` + `pub use`);
  STATUS-family pass-12 "zero app callers" precedent, corroborated at module grain.

False weak (alive, evidence hidden from counting — left alive, no action): hex.rs
(import-rename `as hex`), parse.rs (`parse` shared with hex), scan.rs (consumed by
parse.rs:10), error.rs (`, Error>` generics escape the anchors), palette.rs (short
`dark`), chat.rs (type `Chat` under min-len), branches.rs (BRANCH-01 disclosed class).
Unmeasurable-by-construction, hand-checked alive: 6 lib.rs aggregators; id.rs
(macro-minted `ProjectId`/`WorktreeId`/`TabId` — invisible to the item regex);
cosmic/mod.rs; conformance.rs (in-tree proof suite, not product).

## The name-masked tier, resolved by reading

Mirror test first: the call-syntax probe finds `Editor::open` at file_view.rs:76 (known
live), and the bare-import sweep exists because it saved `post` (http.rs) — called bare
at codex.rs:245 via `use crate::http::{get, post}` — from a false death.

| name | verdict |
|---|---|
| `sorted` (sort.rs) | **dead** — ACT-22 complete (with `urgent_first`) |
| `on_refresh` (status_bar.rs) | **dead** — three `StatusBar::new` in main.rs, `.on_refresh(` never chained; F-USE-01 confirmed live-tree |
| `status` ×3 | **all alive**: git free-fn (right_panel.rs:183, changes.rs:1425, bare-import calls), activity model (main.rs:3068 — receiver takes `&format!`, which is what masked it), editor (file_view.rs:287) |
| `rows` (side_by_side.rs) | **dead** with its module |
| `plan`, `fire`/`OnceGate`, `chat` ctor | dead with their modules/families (session.rs; DOM-07's throttle family; rows.rs) |
| `tab` (workspace.rs), `poll` spot-checks `key`/`list`/`tabs`/`raw`/`wait` | alive or crate-local by design |

Bounded out, stated plainly: the 39 `new` definitions were not read one by one — a dead
`new` in a live module is variant surface (fn-tier territory); in a dead module it is
already counted above. Short names with ≥1 outside file (`save`, `load`, `text`, `dark`,
…) were spot-checked as high-traffic and left to the fn tier's rules.

## Does "absent mostly means absent" hold from the module direction?

**Yes, with one bounded correction.** 18 of 118 files are dead subsystems, and they
cluster under families already named: products-on-top-of-detection (6 activity files),
editor parallel twins (2 markdown files + MarkdownDocument's fns), git package tier
(side_by_side, directory_status, branches — pass-12 disclosed class), agents session
tier (3 files), project seams (4). **No new hidden half-built product family exists** —
the 123 `FAILED — absent` rows remain, in bulk, real construction work, and sequencing
builders against construction (not wiring) stays right. The correction: two absences are
misrecorded — F-SET-09/F-AGENT-SAFE-01's "no skill code" is false (skill.rs), and
F-BRW/F-PRJ's file-tree/drop rows have a built data layer beneath them (file.rs). Both
are evidence corrections, not verdict flips; they belong to the critic.

## Owned elsewhere — not duplicated here

DEAD-MODELS' 5+2 zero-consumer PASSED rows → queued for `pireview`.
`TILLER_SOCKET_ENABLE` boot-path wiring → queued for `codex12` (owns main.rs).

## Honest remainder

Trait-object-only dispatch, macro-generated references and `#[cfg]`-gated code remain
hand-check classes (none bit today, all documented in the script header). The anchored
regex misses `, Error>`-shaped generic positions, so the weakly-live queue over-includes
— by design; it is a reading queue, not a verdict. These rows share the census'
half-life: four builders are editing; **re-run `--modules` and diff before adjudicating
any line above.**

---

## Correction — orchestrator, 2026-08-14 12:50: two of the three "no row" labels were wrong

Checked all three against the frozen clause files (`01-inventory-app.md`, `02-inventory-packages.md`),
not against the ledger's row *titles* — which is where the original check went wrong, since a clause
can own a module without naming it in the title.

| module | "no row" | outcome |
|---|---|---|
| `tiller_markdown/file_events.rs` | ✗ wrong | `F-CORE-FILE-06` owns it, PLATFORM note names inotify |
| `tiller_usage/ollama.rs` | ✗ wrong | `F-CORE-USG-03` owns the parser — and its `PASSED` is correct |
| `tiller_project/file.rs` | ✗ wrong (drop half) | `F-CORE-FILE-03` quotes the function's behaviour as its clause; `F-TERM-PTY-06` names the symbol; `F-EDIT-12` owns the drag source. Only `load_file_tree`/`FileTreeEntry` may still be unowned |
| `tiller_agents/transcript.rs` | ✓ **right** | SESSION-02 names `session_sources`, not this pair |

**The one that held is the one that was argued.** The `transcript.rs` entry did not assert "no row";
it named SESSION-02's actual wording and showed the symbol it points at is a different one. The
three that failed were labels. That is a usable rule for this document: *a "no row" claim is only as
good as the clause text quoted next to it.*

The failure mode is specific and worth naming, because it is not carelessness: **the check was run
against row titles and evidence, where these modules are invisible, instead of against the frozen
clause text, where three of them are quoted almost verbatim.** `F-CORE-FILE-03`'s clause *is* a
prose description of `terminal_file_drop`; nothing in the row's title or its ledger evidence
contains the symbol. Searching the summary of a thing for the thing itself returns nothing, reliably
and convincingly.

Not corrected, and correct as it stands: `F-SET-09` at row 51. The census calls
`agent_skill_install_command` "already built" and the ledger says `FAILED — defective`; those agree
— built, tested, and wired to a button whose handler is `|_, _, _| {}`. A cross-check that matches
on "FAILED" flags this pairing as a contradiction when it is the system working.

### Why this is worth correcting rather than shrugging at

**In this document, "no row" is a deletion licence.** The whole point of `DEAD-MODULES.md` is to
identify code nothing needs, and the natural next step for an unowned dead module is to delete it.
`FileSystemEventMonitor` is not unowned — it is the **built half of `F-CORE-FILE-06`**, a row still
marked `FAILED — absent` precisely because that half was never connected to the editor. Deleting it
would destroy the only implementation of a feature the inventory still owes, and the ledger would
not notice: the row is already `FAILED`, so nothing would change colour.

That is the sharpest version of a pattern this project keeps hitting — **a verdict that is already
negative hides any further damage to the same row.** A `FAILED — absent` row cannot get worse, so
regressions inside it are invisible.

### The reframing, which is the useful part

None of these modules is an orphan. Each is the unwired half of a row that is already failing:

- `file_events.rs` → `F-CORE-FILE-06` (`FAILED — absent`): watcher built, editor never subscribes.
- `link_router.rs` → `F-TERM-UI-02` (`FAILED — absent`): router built, `TerminalLinkEvent` has zero
  subscribers.
- the accounts parsers → `F-CORE-AUTH-01` (`UNREACHABLE`) and half of `F-PERSIST-DB-06`: parsers
  built, no caller. `P93` is closing this one.

So the dead-module list and the `FAILED — absent` list are **two views of the same defects** — one
indexed by code, one by clause. Read together they are a work queue of unusually cheap items: the
expensive half already exists and what is missing is a subscription, a call site, or a handler.
Read apart, one looks like garbage to delete and the other like features to build from scratch.
