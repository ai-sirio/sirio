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

## Rebuilding the partition

`rowfiles.json` is recoverable — `../triage/*-plan.md` are committed and name the files per row.
The ledger itself (`../INVENTORY-LEDGER.md`) is the source of truth for verdicts; import
`Scripts/ledger-totals.py` via importlib to reuse its `ROW` regex and `split_cells`, rather than
re-parsing the table by hand.
