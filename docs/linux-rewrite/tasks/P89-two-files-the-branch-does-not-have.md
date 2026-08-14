# P89 — two files the branch does not have, and the flake blocking the gate

**Owner: `codex11`.** Worktree `/home/enzopalmisano/Scrivania/Progetti/tiller-linux`, branch
`linux/gpui-waku`. Both items are in `tiller_terminal`, which is yours. Small, but the first one is
time-sensitive.

## Part 1 — `lib.rs` references two files that are not in the repo

```
 M rust/crates/tiller_terminal/src/lib.rs        <- declares mod lifecycle; mod link_router;
?? rust/crates/tiller_terminal/src/lifecycle.rs   (217 lines, untracked)
?? rust/crates/tiller_terminal/src/link_router.rs (56 lines, untracked)
```

`lib.rs:38-39` declares both modules. The modules themselves have never been committed. It compiles
for you because the files exist in your working tree.

**The failure mode is one keystroke away.** `git add -u` stages modified files and *not* untracked
ones — so the ordinary commit idiom would land a `lib.rs` referencing two files the branch does not
contain, and `tiller_terminal` would stop compiling for everybody. `codex12`'s gate would not catch
it either: it runs `cargo test --workspace` in this same worktree, where the files are present.

Commit both files. If either is mid-refactor and not ready, commit it anyway and mark the unfinished
part with a `TODO` — a compiling branch with an honest TODO beats a branch that builds only on one
machine. Check `git status --short | grep '??'` before you finish; that habit is in every brief for
exactly this reason, and 31 MB of this project's work has already been lost to it once.

## Part 2 — your flaky test is what the gate is now blocked on

`codex12` ran `Scripts/ci-linux.sh` twenty times under P85: **0/20 `CI OK`, 20/20 failed.** It fixed
the two flakes it owned — a PTY test asserting a logical debounce against wall-clock delivery time,
and chat tests sharing scratch roots and SQLite files (`-wal` sidecar included) — and the remaining
blocker is yours:

```
tests::scrollback_can_be_viewed_after_output_exceeds_the_viewport   (tiller_terminal)
```

**It passes in isolation and fails in the workspace run**, which is the signature of shared mutable
state or a timing assumption, not of a wrong assertion. The two fixes codex12 just landed are the
two shapes worth checking first: a real-time wait standing in for a logical condition, and a fixture
path shared with another test running concurrently.

Reproduce it with the gate's own command — `(cd rust && cargo test --workspace)` — not with
`--filter`, because filtering removes the contention that causes it. A paraphrased check is a weaker
check; this project has already had a "clean" result from a paraphrased command overrule three
correct agents.

## Done means

1. Both module files committed; `git status --short | grep '??'` clean of `.rs` files.
2. The scrollback test passing under the full workspace run, with the cause named — "made it
   deterministic" without saying what was non-deterministic is not a result.
3. Report the run count you got it green over. One pass proves nothing about a flake; codex12 used
   twenty.
4. Do **not** edit `INVENTORY-LEDGER.md`.
