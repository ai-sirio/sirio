# Wave orchestration scripts

These are the `Workflow` scripts that drive the inventory waves, kept here because **the
orchestrator's scratchpad does not survive a session boundary**. `wave-a.js`, `wave-b.js`,
`triage.js`, `exercise-sweep.js` and `drive-sweep.js` were all lost that way before this
directory existed; what survives starts at wave B's recovery script.

`data/` holds the derived partition inputs, which are expensive to rebuild:

| file | what it is |
| --- | --- |
| `rowfiles.json` | row id → source files, recovered by parsing `../triage/*-plan.md` |
| `wavec.json` | the outstanding rows split into `main` / `other` / `unmapped` |
| `components.json` | connected components of the row↔file graph (see below) |

## How a wave is shaped

The hard constraint is that all agents share **one worktree** — a 69 GB cargo target dir caps
per-agent worktree isolation at about four agents, which is fewer than a wave needs. So file
ownership is a convention, and the scheduling has to make that convention safe.

Two rules do the work:

1. **Slices are connected components of the row↔file graph, not groups by primary file.**
   Union rows that share any file; a component is then the smallest set of rows that cannot be
   split without two agents editing one file. Grouping by "the row's main file" *looks* disjoint
   and isn't — wave C's first cut gave a slice three files while its rows named six.
2. **A component too big for one slice becomes a serialised chain, never a split.**
   Wave C had 60 rows on `main.rs` and 20 glued by `chat.rs`. Those ran as two chains
   (`for … await`, one link at a time) *inside* a `parallel()` alongside the genuinely disjoint
   slices. Concurrency is negotiable; one-writer-per-file is not.

Verification is a separate phase with fresh agents: **the critic is never the agent that built
the piece**, and a builder's `howToExercise` is passed on as a route to find the control, never
as evidence that it works.

## What wave C cost, and what wave D changed

Wave C returned 110 of 117 rows and moved the ledger from 242 to 278 PASSED, but lost two agents
and, through one of them, 45 rows of build work. Three fixes came out of it, all in `wave-d.js`:

1. **A chain link must not abort the links behind it.** `C-MAIN`'s link 1 finished all 15 rows and
   committed each one, then failed the `StructuredOutput` retry cap on its final return — and
   because a bare `await` in a `for` loop propagates, links 2–4 never started. Their 45 rows were
   verified against untouched code. Every link is now wrapped in `try`/`catch`.
2. **Big returns die.** Both casualties were the two 15-row slices; every 6–7-row slice validated
   fine. Links are now 7 rows, `required` fields are minimal, `additionalProperties: false` is gone
   from row objects, and agents are told the committed report is the record and the schema only an
   index.
3. **Normalise file paths before computing ownership.** A recovery step wrote `crates/…/main.rs`
   while triage used `rust/crates/…/main.rs`; the same file under two spellings read as two owners
   and put a parallel slice in collision with the whole `main.rs` chain. The check that caught it
   compares normalised paths, and it is worth running before every launch.

A fourth lesson is about reading the results rather than the process: **19 of the 74 rows still
open are blocked by a missing drive primitive, not by missing app code** — no right-click, no
modifier chords, no button-held drag, no scroll. The lane's vocabulary had quietly become the
ceiling on what could be *proven*, independent of what had been *built*, so wave D extends the
harness in parallel with building.

## Rebuilding the partition

`rowfiles.json` is recoverable — `../triage/*-plan.md` are committed and name the files per row.
The ledger itself (`../INVENTORY-LEDGER.md`) is the source of truth for verdicts; import
`Scripts/ledger-totals.py` via importlib to reuse its `ROW` regex and `split_cells`, rather than
re-parsing the table by hand.
