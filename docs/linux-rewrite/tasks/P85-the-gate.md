# P85 — the gate

**Owner: `codex12`.** Two parts: kill the nondeterminism that is holding `CI OK`, then take
possession of the files you inherited tonight.

Worktree `/home/enzopalmisano/Scrivania/Progetti/tiller-linux`, branch `linux/gpui-waku`.

---

## First — you own more files than you did an hour ago

`sonnet` was promoted to **second critic** and now owns no source files. Its files were split; see
`OWNERSHIP.md`, section *"`sonnet`'s files, reassigned"*. **What is now yours:**

`status_bar.rs` · `titlebar.rs` · `controls.rs` · `icons.rs` · `sfsymbol.rs` ·
`project_identity.rs` · `tiller_theme/**`

Two consequences you should not have to discover the hard way:

- **`F-BRW-08` is no longer `sonnet`'s, and it is not yours either.** `settings.rs` went to
  `codex11`, which already owns `browser.rs`, so that seam is closed by ownership. Your P83 report
  named it as still open against `sonnet`; that is now stale. Do not dispatch or wait on it.
- **`status_bar.rs` + `tiller_usage/**` are now one owner — you.** The `ProviderUsage`/`UsageWindow`
  imports that were the last thing blocking the gate are no longer a cross-owner problem. If they
  are still broken, just fix them.

---

## Part 1 — the two flaky tests (this is the actual piece)

You reported: *"ci-linux.sh resta non verde per due test flakey esterni, entrambi passati
isolatamente."* That is the whole problem, and it is worth more than it looks.

**A test that passes in isolation and fails in the suite is not noise — it is a real defect that has
not been localised yet.** In this project specifically it is worse than a normal bug, because this
codebase's central failure mode is *false verdicts*: a suite that is red for reasons nobody can
reproduce trains everyone to ignore red, and the next genuine regression rides in behind it. We have
already lost passes to a test that pinned a stub in place and to a gate that was reproduced with a
paraphrased command instead of its own.

**Do not fix this by retrying, by `#[ignore]`, by `--test-threads=1`, or by widening a timeout until
it passes.** Every one of those hides the defect and leaves the gate lying.

What to do:

1. **Name them.** Write the two test names into this brief's Done report. "Two flaky tests" is not a
   finding; `foo::bar_baz` is.
2. **Reproduce the failure, not the pass.** Run the full suite the way `Scripts/ci-linux.sh` runs it —
   *its own command line*, not a paraphrase of it — in a loop until each fails. If you cannot make it
   fail, say so plainly and stop; do not report a fix you could not first reproduce.
3. **Find the shared state.** Suite-only failures come from something two tests both touch. In this
   tree the usual suspects, in order: a real filesystem path or temp dir reused across tests; the
   shared `tiller.sqlite` under `~/.local/state/TillerRust/` (**and its `-wal` sidecar** — copying or
   deleting the `.sqlite` alone has produced a false verdict here before); the control socket at
   `$TILLER_SOCKET`; a global in `tiller_activity`; an `env::set_var` that leaks into a sibling test;
   or a real clock/sleep where a deterministic one belongs.
4. **Fix the cause.** Isolate the state per test — own temp dir, own DB path, own socket — or make the
   ordering explicit. The test should be unable to observe another test's state at all.
5. **Prove it.** Run the full gate command **20 times** and report the pass count. 20/20 or it is not
   fixed.

**Goal: `Scripts/ci-linux.sh` prints `CI OK`.** If something outside these two tests also blocks it,
fix that too if it is yours, and name it with its owner if it is not.

`Scripts/ci.sh` (macOS) stopping on missing `xcodegen` is **expected on Linux** and is not a blocker —
do not try to make it pass here.

---

## Part 2 — only if Part 1 lands with time to spare

`status_bar.rs` is now yours together with `tiller_usage/**`. `F-SET-11` and the `F-USE-*` rows rest
on it and **`sonnet` may no longer judge them**, because it built that surface. Read those rows and
report which are genuinely absent versus merely never exercised — **do not build yet.**

Before you believe any row that reads `FAILED — absent`, read the last section of `QUEUE.md`. Three
independent audits tonight found that verdict to be the least trustworthy in the ledger: four
`F-CHAT` rows marked absent since pass 8 turned out to be fully implemented, two of them with tests
named after the row. One `grep` of the owning file for the row's own vocabulary settles it.

---

## Done means

1. Both flaky tests **named**, their shared state **identified**, and the cause fixed — not masked.
2. The gate command run **20 times**, with the pass count reported.
3. `Scripts/ci-linux.sh` prints `CI OK`, or a precise statement of what else blocks it and who owns it.
4. `cargo fmt`, `clippy -D warnings` green on what you touched.
5. `git status --short | grep '??'` before you finish — explicit-path commits never catch new files,
   and that habit already left 31 MB of this project untracked once.
