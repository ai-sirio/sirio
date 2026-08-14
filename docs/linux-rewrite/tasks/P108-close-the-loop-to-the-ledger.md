# P108 — close the loop back to the ledger

**Owner: `pireview`, as critic.** Worktree `/home/enzopalmisano/Scrivania/Progetti/tiller-linux`,
branch `linux/gpui-waku`. **You are the only agent on this board allowed to edit
`INVENTORY-LEDGER.md`, and this task is why.**

## The gap

`fable` named it while finishing `P105`, and it is the most consequential thing found today:

> three of the flipped rows carry code comments that literally name their ledger row id
> (`render_tool_diff` says `F-CHAT-31`, `segment_text` says `F-SET-11`, `render_agents` says
> `F-SET-16`). Builders have been landing row-targeted fixes while the ledger still reads "absent"
> — the census-to-ledger feedback loop is the broken link, not the searching.

Right now the ledger says:

```
PASSED 190 · half-proven 25 · FAILED — absent 82 · FAILED — defective 32
UNREACHABLE 12 · N/A — platform 12 · NOT EXERCISED 35        (389)
```

and **56 rows inside that `FAILED — absent` 82 are known to be wrong** — `P105` re-censused 70 of
them and found 20 BUILT, 36 PARTIAL, only 14 genuinely absent. Nobody has carried that back.

## What you are doing

Reconciling three finished documents into the ledger:

- `RECENSUS-2026-08-14.md` — 70 rows, three-state census with symbol + `file:line` or the needle
  that found nothing.
- `P106-report.md` — 56 of those rows driven, per-row observations. **Read its orchestrator
  correction at the end first**: every `F-CHAT` row whose only route was `surface.chat.*` evidences
  the control API and not the UI, and cannot settle its row.
- `P104-report.md` — 30 different rows driven on `DISPLAY=:1`, groups 1-6, observations only.

## The one rule that governs every edit

**Code plus a green test is `NOT EXERCISED`, never `PASSED`.** The census gives you symbols and line
numbers. That is evidence a row is *not absent*. It is not evidence the feature works, and you must
not let it become one — that substitution is the exact mechanism that produced this project's false
`PASSED`s.

So:

| what the sources show | what the ledger becomes |
|---|---|
| census BUILT, nobody drove it | `NOT EXERCISED` |
| census PARTIAL, nobody drove it | `NOT EXERCISED`, and the clause's missing conjunct named in the evidence |
| census ABSENT, needle quoted | stays `FAILED — absent` |
| a report drove it and it did what the clause says | `PASSED` |
| a report drove one conjunct of two | `half-proven`, saying which half |
| a report drove it and it misbehaved | `FAILED — defective`, with what was seen |
| a report says `owed: gesture` | `NOT EXERCISED`, with the gesture named |

Moving a row from `FAILED — absent` to `NOT EXERCISED` is **removing an unearned claim**, not adding
one — the ledger was asserting the feature does not exist and that assertion is disproven. Treat
that as the safe direction and the common case.

Setting a row to `PASSED` from someone else's report is the direction that needs care. Do it only
where the observation describes an actual exercise with an actual result — the gesture driven and
what the screen did. "The control is present in the capture" is not an exercise. **When a report is
ambiguous about whether the thing was operated or merely seen, it was merely seen.**

## What you may not do

- **Do not drive anything yourself for this task.** You are reconciling records. If a row's sources
  do not settle it, it goes to `NOT EXERCISED` and onto the owed list — that is a correct outcome,
  not a failure to finish.
- **Do not edit `RECENSUS-2026-08-14.md`, `P104-report.md` or `P106-report.md`.** If you believe a
  source is wrong about a row, leave the row unreconciled and list it under "disputed" in your
  summary.
- Do not touch `rust/`.

## What to produce

1. The ledger edits themselves, **committed in batches of no more than 20 rows**, each commit naming
   the source document each row came from.
2. `docs/linux-rewrite/P108-reconciliation.md` — one line per row changed: row id, old verdict, new
   verdict, and the source that justified it (`RECENSUS`, `P104-report`, `P106-report`, plus a
   section reference). Plus three lists at the end: **disputed** rows, rows now **owed a gesture**
   on `DISPLAY=:1`, and rows still **genuinely absent** and needing a builder.
3. Fresh `Scripts/ledger-totals.py` output at the top of that file, before and after.

That owed-gesture list is not an appendix — it is the next drive batch for whoever takes the lock,
and it is worth as much as the verdict changes.

## Sanity checks before you finish

- Total is still **389**. The denominator is frozen; if it moved, you edited structure, not verdicts.
- No row gained `PASSED` whose only source is `RECENSUS-2026-08-14.md`.
- Every `F-CHAT` row whose route was `surface.chat.*` is `NOT EXERCISED`, not `PASSED`.
- Do not idle on an approval gate — `ENVIRONMENT.md` §"Working with the orchestrator".
