# P78 — the seven functions the app never calls

**Owner: `codex12`.** Seven rows. The logic exists and is tested. What is missing is the call.

Read `../ENVIRONMENT.md` and `../OWNERSHIP.md` first; this brief does not restate them.

## The rows, and the one sentence they share

| row | the ledger's evidence |
|---|---|
| `F-CORE-ACT-24` | planner resumable/prunable split tested; **zero app callers** of `bootstrap::partition` — dead code |
| `F-CORE-ACT-25` | partition order (selected/open/deferred) tested; **zero app callers** — launch remount planning never invoked |
| `F-CORE-ACT-26` | `ids_to_evict` tested; **zero app callers** — no eviction side effect exists |
| `F-CORE-DOM-03` | `default_project_base` code-verified (`TILLER_PROJECTS_DIR` → XDG → `HOME/Tiller/projects`); no test and **zero app callers** |
| `F-CORE-WSP-04` | all 8 commands + `classify` tested; `LayoutCommand` has **zero app callers** (dead enum) |
| `F-CORE-WSP-08` | `WorkspaceTabViewState` has all fields; **session store never persists `view_state`** |
| `F-CORE-FILE-06` | atomic save/reload/conflict/deletion tested; `FileSystemEventMonitor` (inotify) has **zero callers outside its crate** |

I verified this rather than inheriting it from the ledger, because the ledger has been wrong in this
exact way before. Reference counts across the whole Rust workspace, against references inside
`crates/tiller/src/` — the app:

```
partition                total 2   app 0
ids_to_evict             total 2   app 0
LayoutCommand            total 14  app 0      <- fully tested, fully dead
WorkspaceTabViewState    total 5   app 0
default_project_base     total 2   app 0
FileSystemEventMonitor   total 4   app 0
```

**Every one of these is real.** `LayoutCommand` is the sharpest: fourteen references, eight commands,
a `classify` function, a green test suite, and nothing in the application has ever constructed one.

## This is the thirteenth instance, not the first

`F-PRJ-04`, `F-SET-09`, `F-SET-16`, `F-SET-22`, `split_disabled_reason` behind `F-TAB-11`,
`clone_repository` behind six `F-PRJ` rows, and now these seven. **It is the defect this project
produces more than any other**, and it has a mechanism: a builder asked to "implement X" writes X,
writes a test for X, watches both go green, and reports done. *Compiles*, *is tested*, and *is
reachable* are three independent properties, and only the first two were ever checked.

Which is why these rows are also the cheapest on the board. **You are not building seven features.
You are writing seven call sites** to code that already works.

## The trap, and it is not the obvious one

The obvious reading is "add the call, row passes." That reading produces a second dead control, and
the critic will catch it.

**Before writing any call, establish what the app does today instead.** For each row there are three
possibilities and they need different work:

1. **The behaviour is genuinely absent.** Wire it up. This is the easy case.
2. **The app already does it by an ad-hoc path** that grew up beside the tested one. Then the job is
   to *replace* the ad-hoc path with the tested function and delete the duplicate — not to call both.
   Two code paths computing eviction differently is worse than one dead one.
3. **The row is already satisfied by another route**, and the tested function is genuinely
   redundant. Then the honest outcome is to **say so and name the route**, so the critic can verify
   the row against the path that actually runs. Do not wire something up merely to justify its
   existence.

Say which of the three each row was. That sentence is the most useful thing in your handoff.

## What "wired" has to mean here

A row is satisfied when the behaviour is **observable to somebody using the app**, because that is
how the critic will test it. Concretely, for the ones where this is easy to get wrong:

- **`F-CORE-ACT-26` / `ids_to_evict`** — calling it and discarding the result passes no row. Something
  must actually be evicted, and the eviction must be observable.
- **`F-CORE-WSP-08` / `WorkspaceTabViewState`** — "persists" means it survives a restart. The proof
  is: set state, quit, relaunch, state is there. A field written to a struct that is never
  serialised is exactly the defect the row already describes.
- **`F-CORE-ACT-24/25`** — the partition drives *launch remount planning*. The observable consequence
  is which worktrees come back mounted after a relaunch, in which order.
- **`F-CORE-DOM-03`** — this one has **no test at all**, unlike the others. Write one. The precedence
  chain `TILLER_PROJECTS_DIR` → XDG → `HOME/Tiller/projects` is three branches and env-var precedence
  is where this kind of function is usually wrong.

## Ownership

Every call site is in `crates/tiller/src/` — `main.rs`, `session.rs`, `panes.rs`. All yours.

Some of the callees live in crates that are **`codex11`'s** (`tiller_project`, and the inotify
monitor). **Calling them is fine; changing them is not** — consuming a crate is not owning it. If a
callee needs a signature change to be callable, stop and write that up as a named two-half seam
instead of reaching across. One line saying what `codex11` must change is worth more than a
collision.

`sidebar.rs` is yours too, so `F-CORE-FILE-06`'s consumer side may land there — but check
`file_view.rs` first, which is `codex11`'s and already consumes that monitor.

## Done means

1. Each of the seven says which of the three cases it was.
2. Where wired, a test that would fail if the wiring were removed — not merely a test that the
   function works, which already exists.
3. `F-CORE-DOM-03` has its missing test.
4. The binary builds. It is currently red on `main.rs:2654` (`reorder_sidebar`), which is your own
   in-flight seam — clear that before handing off.
5. `Scripts/transplant-check.py` run and its result stated.

**The critic will exercise these from the app, not from the test suite.** A green test on a function
nobody calls is precisely what put these seven rows here.
