# P115 — are those three tests actually still red?

**Owner: `pireview`, as critic.** Worktree `/home/enzopalmisano/Scrivania/Progetti/tiller-linux`,
branch `linux/gpui-waku`. Follows your `P113`.

## The contradiction

Your own triage put these in the urgent queue, and it was right to:

| rows | cited test |
|---|---|
| `F-CORE-ACT-06`, `F-CORE-ACT-11` | `panes::tests::process_owned_status_survives_title_and_child_exit_events` |
| `F-CORE-ACT-07` | `real_pty_layer_a_debounce_suppresses_first_title_and_accepts_second` |

Three ledger rows are verdicted `FAILED — defective` on the grounds that a named test **fails
reproducibly**.

But `codex11` ran `cargo test -p tiller` at ~17:05 today, closing `P107`, and reported **134 passed**
with no failures. Those tests live in that crate. Both statements cannot be true of the same tree.

There are four ways out and they lead to different places: the tests were fixed at some point and
nobody carried it back; they were renamed or moved; they do not run under that invocation (ignored,
feature-gated, or in a target `-p tiller` doesn't cover); or they are flaky, which would make
"reproducibly" false and is the most interesting answer of the four.

**Find out which.** Three rows and a whole triage queue rest on it.

## Where to run it

**Not in the live tree.** Five agents are building against `rust/target` right now; a test run there
stalls all of them, and their in-flight edits would make any result unattributable anyway.

```bash
git worktree add /tmp/p115-tree HEAD          # fresh SOURCE tree, no 66 GB target copy
cd /tmp/p115-tree
export CARGO_TARGET_DIR=/tmp/critic-target    # ONE shared target for critic passes, reused
```

A fresh **source** tree is what independence needs — a cold build cache is not, and a per-pass target
directory is how this project once filled a disk. Reuse `/tmp/critic-target` and do not invent a new
one. Record the exact commit you tested (`git rev-parse HEAD`); builders are landing changes while
you run, so a result without a commit is not replayable.

Remove the worktree when you finish (`git worktree remove /tmp/p115-tree`); leave
`/tmp/critic-target` in place for the next pass.

## What to establish

1. **The three cited tests, by name.** Run them explicitly with `--exact` so you are testing the
   named test and not a prefix match. Red or green, and if red, the assertion that fails.
2. **Whether they ran at all** under plain `cargo test -p tiller`. Compare the test names in the full
   output against the names the ledger cites. A test that silently does not run is exactly how a
   "green suite" coexists with a broken feature, and this project has already been bitten by
   evidence that looked green and wasn't.
3. **If green: is it flaky or fixed?** Ten consecutive runs distinguishes them. Do not report "passes
   now" off a single run when the ledger's claim is specifically about reproducibility.
4. **The current true suite baseline** at that commit — which crates, how many tests, what fails.
   Nobody knows whether this tree is green right now, and five builders are landing into it.

## What you may then change

If a test is green and stably so, `F-CORE-ACT-06/07/11` lose their stated grounds — but **green does
not make them `PASSED`.** A passing test is not an exercised feature; that substitution is the
mechanism that produced this project's false passes. The correct move is `NOT EXERCISED` with the
gesture named, and onto the owed list. You may make that edit — you own the ledger.

If a test is red, the rows keep their verdict and you have handed a builder a reproduction, which is
worth more than the verdict.

## What to produce

`docs/linux-rewrite/P115-tests.md`: the commit tested, the exact commands, the raw pass/fail lines
for the three named tests, the flakiness result if relevant, the suite baseline, and any ledger edits
with their justification. Commit path-scoped.

## The rules

- Do not drive the UI; `sonnet` holds the `DISPLAY=:1` lock.
- Do not edit `rust/` — a fix is a builder's job and a separate task.
- Do not idle on an approval gate — `ENVIRONMENT.md` §"Working with the orchestrator".
