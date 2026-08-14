# P99 — eleven rows, no display lock

**Owner: `fable`, as critic, after `P92`.** Worktree
`/home/enzopalmisano/Scrivania/Progetti/tiller-linux`, branch `linux/gpui-waku`.

**None of this needs the drive lock.** Every row here is a library API, so `sonnet` can hold the
display for `P94` while you work. That is the point of the piece.

## What happened, so you can challenge it

On 2026-08-14 I swept a block of rows carrying `PASSED` whose own evidence read *"zero app callers —
wiring owed"* and demoted them. Demoting was right: a green test is not an exercise. **The
destination was wrong for eight of them.** I sent them to `UNREACHABLE`, which means *the host never
mounts it, wiring is owed* — the expensive build queue. But their own `VERIFY` lines ask to *inspect
the parse*, *inspect the validator result*, *compare returned values*, *confirm progress arrives*.
Those name the **API**, not a surface. App callers were never required by the clause, so nothing is
owed and nothing is blocked.

I refiled them `NOT EXERCISED`. That is the mirror error warned about in `P94` — *a consumerless
function is not a defect unless the clause requires a consumer* — committed at scale by the same
pass that wrote the warning.

**Check that reasoning before you accept it.** If a clause actually does name a surface, say so and
send the row back to `UNREACHABLE`. I would rather you overturn me than inherit me.

## The eight to exercise

Each row's own `VERIFY` line is your script — it says exactly what to feed in and what to inspect.

| row | its VERIFY, in short |
|---|---|
| `F-CORE-DOM-07` | transcript growth below each threshold and above both; observe generated-name requests |
| `F-CORE-USG-05` | valid, missing and expired credential files at **both** supported locations |
| `F-CORE-AUTH-01` | representative Claude account JSON and Codex credential text, **including empty fields** |
| `F-AGENT-SESSION-01` | create and remove each expected session file; Claude, Codex, **and an uncheckable agent** |
| `F-AGENT-SESSION-02` | transcript files at each path, **string and block content**, compare returned recents |
| `F-GIT-RUN-02` | a command that emits progress; confirm it arrives **incrementally, before completion** |
| `F-AGENT-SAFE-02` | migration against matching, stale, malformed **and unrelated** hook files |
| `F-GIT-BRANCH-01` | branches **with spaces** and ordinary names; compare exact names |

Note how many are conjunctive, and how many name an awkward input on purpose — *empty fields*, *an
uncheckable agent*, *unrelated hook files*, *branches with spaces*. **The awkward input is the
clause.** A run that only covers the happy path has not exercised the row, and that is precisely how
these came to be `PASSED` in the first place.

## Three rows that need a subject ruling before a verdict

I could not decide these from the clause alone, and guessing would have manufactured a verdict:

- **`F-CORE-ACT-22`** — *"create rows in mixed statuses and inspect normal sorted order and
  urgent-first order, including ties"*. "Rows" sounds like the sidebar; the thing under test is a
  sort function. Which is the subject?
- **`F-GIT-STATUS-02`** — *"inspect status markers on every ancestor directory"*. A "marker" sounds
  visual; `DirectoryStatusAggregator` returns a map.
- **`F-GIT-DIFF-03`** — *"**open** a diff … and compare left/right rows"*. "Open" sounds like a
  gesture; the comparison is over the diff model.

**Rule the subject first, in writing, then judge.** If a row is API-subject, exercise it here. If it
is surface-subject, it stays `UNREACHABLE` and goes to the build queue — say which and why.

## How to exercise, and what does not count

**Replaying the existing suite does not count.** These rows already had green tests; that is exactly
why they were wrongly `PASSED`. What is owed is the exercise the `VERIFY` describes, against real
fixtures, including the awkward inputs — which the existing tests demonstrably do not cover, or the
rows would not read "zero app callers" as their whole evidence.

**Your tests must land in the repo.** A proof that lives in `/tmp` is deleted by the next pass and
the verdict it supports silently becomes unreplayable. Commit them path-scoped.

Verdicts available to you: works as the clause describes → `PASSED`. Acts on the wrong thing, or the
effect never happens → `FAILED — defective`. Genuinely not built → `FAILED — absent`. One conjunct
proven and the other untouched → `half-proven`, **naming which is which**. The harness truly cannot
perform it → `NOT EXERCISED` **with the instrument reason**, never `FAILED`.

## The rules

- **Do not edit `INVENTORY-LEDGER.md` totals by hand** — recount with the one-liner in its Totals
  block. Total is frozen at **389**; if your recount disagrees, you created or destroyed a row.
- Commit path-scoped, never `git add -A`.
- `git status --short | grep '??'` before you finish.
- Prefer `cargo test -p <crate>`; a red `--workspace` gate is usually another agent's intermediate
  state, and **a red gate needs its cause attributed before it is reported**.
- Do not idle on an approval gate — `ENVIRONMENT.md` §"Working with the orchestrator".

## Done means

1. Eight rows exercised against real fixtures including the awkward inputs, each with a verdict and
   the command that produced it.
2. Three subject rulings, in writing, each followed by either an exercise or a return to the build
   queue.
3. Tests committed to the repo, not left in `/tmp`.
4. Report the verdict deltas.
