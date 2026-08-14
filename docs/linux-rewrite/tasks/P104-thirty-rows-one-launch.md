# P104 — thirty rows, one launch

**Owner: `sonnet`, as critic.** Worktree `/home/enzopalmisano/Scrivania/Progetti/tiller-linux`,
branch `linux/gpui-waku`. **The display drive lock is free as of this dispatch — claim it.**

## What this is

`STALE-FAILED-RECIPES.md` is a finished route doc for 39 rows that the ledger calls
`FAILED — absent` and that `FABLE-08` found already built. Session 1 of that doc — groups 1
through 6 — is **30 rows reachable in a single app launch** with nothing but git fixtures. Every
route is already written: where the app starts, the gesture that reaches the function, the
on-screen element to look at, and the `debug_selector` id when one exists.

**This is the largest single conversion available on the board.** You are not investigating and
not building. You are driving 30 pre-written routes and reporting what the screen did.

Read `STALE-FAILED-RECIPES.md` first, then `ENVIRONMENT.md`, then this file's warnings.

## You did not build any of these

That is the point, and it is the one property that makes your report worth anything. Do not read
the builders' notes, do not read `STALE-FAILED-CENSUS.md`'s reasoning about *why* a row is built.
The recipe tells you what to do; the tree tells you what happened. If a recipe's gesture does not
exist on screen, **that is a finding, not a recipe bug** — report the row as absent and say what
you found instead.

## The discipline

- **A recipe says what to DO, never what to CONCLUDE.** "Right-click a tab, report the menu's
  items top to bottom" is the instruction. If you catch yourself confirming an expectation rather
  than probing, you have stopped being an independent control.
- **Report verbatim where wording matters** — dialog text, menu labels, greyed entries and their
  reason text, banner copy. A paraphrase of a label is not evidence of the label.
- **A function you did not successfully exercise does not exist.** No inference from adjacent
  behaviour, no "the menu item is there so it must work". Click it.
- **Code plus a green test is `NOT EXERCISED`, never `PASSED`.** You are here precisely because
  these 30 rows have code and, in many cases, tests.
- **Conjunctions split.** Where a clause says "and" or "then", drive and record each conjunct
  separately. Half a clause proven is `half-proven`, not `PASSED`.

## Order, and why

Groups 1-6 are ordered by cost and share one launch. Do them in order — group 2 opens the files
that group 3 inspects, and group 6's fixtures (two modified files, one `git add`ed) are cheapest
to make once at the start. Group 5 needs `sleep 8` and a real agent CLI in a pane; if no
ACP-capable agent is on PATH, report `F-CHG-22` unreachable today rather than improvising one.

## The display lane

`Scripts/linux-drive.sh` is launch → act → capture → kill. Most of Session 1 needs the app
**alive across many gestures**, so hold the lock yourself — `ENVIRONMENT.md`
§"Holding the lock yourself" has the exact `mkdir` snippet, including the `trap` that releases it.
Export `TILLER_DRIVE_LABEL=sonnet` so the holder file names you.

Three traps that have each cost a false result on this machine:

1. **`mkdir` lock, not flock.** Pointing `TILLER_DRIVE_LOCK` at a path where a regular file
   already exists fails EEXIST forever (exit 6). Use the default `/tmp/tiller-drive-1.lockd`.
2. **Right-click is `rclick`.** `linux-drive.sh`'s `click()` has no button-3 path; a menu that
   "did not open" was for a while blamed on GPUI. Group 1 is almost entirely right-clicks.
3. **The Files panel paints over the menu** in the same frame in at least one recorded case —
   if a menu looks empty, check whether something is drawn on top of it before reporting absence.

**Release the lock when you finish**, and say so in your pane — `pi` (`P102`) and others are
waiting on it.

## What to produce

Append to `docs/linux-rewrite/P104-report.md`, **committing after each group.** Three sessions
were interrupted mid-work today; a report held in an uncommitted buffer is a report that gets lost.

Per row: the gesture you drove, what the screen showed (verbatim where it matters), and the
capture filename if you took one. **No verdict column** — you report observations; the ledger is
not yours to edit. Where an observation plainly settles a row, say what you saw and let it settle
itself.

## The rules

- **Do not edit `INVENTORY-LEDGER.md`.** Do not edit any `rust/` source: if you find a bug, that
  is a finding, not a task.
- Commit path-scoped, never `git add -A`.
- Do not idle on an approval gate — `ENVIRONMENT.md` §"Working with the orchestrator".
