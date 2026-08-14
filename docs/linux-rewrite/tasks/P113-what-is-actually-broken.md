# P113 — forty-seven defective rows, and far fewer than forty-seven bugs

**Owner: `pireview`, as critic.** Worktree `/home/enzopalmisano/Scrivania/Progetti/tiller-linux`,
branch `linux/gpui-waku`. Follows your `P108`, which produced this pile.

## What you made visible

Reconciling the ledger moved `FAILED — absent` from 82 to 14 and `FAILED — defective` from 32 to
**47**. That second number is now the largest actionable bucket in the project and **nobody owns a
single row of it.** Fourteen absent rows are already dispatched to builders (`P110`, `P111`). These
47 are not.

They are also, on their face, not 47 independent defects. Several read like one broken thing
reported once per row that depends on it. Nobody has ever grouped them, so the roster cannot be
pointed at causes — only at symptoms, one at a time, which is how six rows get six tasks and one
fix.

## What you are producing

`docs/linux-rewrite/P113-triage.md`: **the 47 rows grouped by root cause**, ranked by how many rows
each cause unblocks.

Per cause: a name, the rows it accounts for, the evidence in each row that puts it in that group,
where in `rust/` you believe it lives, and **what a builder would have to do**. Where two rows look
like one cause but the evidence doesn't actually say so, keep them apart and say why — an overlarge
cluster sends a builder to fix a thing that isn't there.

Also separate out, explicitly:

- **Rows that need no build at all**, only a re-drive. Several carry a `N/A — platform` excuse whose
  premise was "the browser is out of scope" — the browser now exists and works, so the excuse is
  void and the row simply needs exercising. That is a drive batch, not builder work, and it belongs
  in a different queue.
- **Rows whose defect is a failing test.** At least three cite a named test as *failing
  reproducibly*. If a test in this repo is red right now, that is the cheapest and most urgent thing
  on the board, and it should not be buried among UI defects.
- **Rows superseded by later work**, where the evidence itself says "pass 13 is superseded". Those
  may not be defective any more; if the row's own evidence retracts its verdict, say so and mark it
  for re-drive.

I have my own read of where the clusters are, deliberately not written here — I want yours first so
the two can be compared. If they agree the grouping is trustworthy; where they differ, one of us has
missed something and that is worth knowing.

## How to work

**This is a reading task.** The evidence column of every one of these rows was written by someone who
drove the thing, and it is unusually detailed — that is your source.

- **Do not drive anything.** `sonnet` holds the `DISPLAY=:1` lock for `P109`.
- **Do not run `cargo build` or `cargo test` in the live tree.** Five agents are building right now
  and they share one target directory; a test run here stalls all of them. For the failing-test
  group, report the test names and the rows that cite them — verifying them needs its own snapshot
  tree and is a separate task.
- **Do not edit `rust/`.** You are the critic.
- You may edit `INVENTORY-LEDGER.md` — you are the only one who may — but only where a row's own
  evidence retracts its verdict. **Every other row keeps its verdict until someone exercises it.**

## Why the ranking matters more than the grouping

The output I need is an ordering: which single fix returns the most rows to a drivable state. A
cause that unblocks six rows and a cause that unblocks one are not the same task, and right now
nothing in this project distinguishes them. Put the count next to each cause and sort by it.

If a cluster is large but the fix is speculative, say that too — six rows behind a cause nobody knows
how to fix is worth less than two rows behind a known one, and I would rather you tell me that than
give me a clean list that hides it.

## The rules

- Commit path-scoped, never `git add -A`. `grep '??'` before calling it done.
- Do not idle on an approval gate — `ENVIRONMENT.md` §"Working with the orchestrator".
