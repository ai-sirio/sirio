# P115 — named test reconciliation

**Tested commit:** `56977f232e313fa74c53f010f7c9319703353466` (`56977f2`), detached fresh source worktree at `/tmp/p115-tree`.

## Method

The source worktree was created from the recorded commit with:

```bash
git worktree add /tmp/p115-tree HEAD
cd /tmp/p115-tree/rust
export PATH="$HOME/.cargo/bin:$PATH"
export CARGO_TARGET_DIR=/tmp/critic-target
```

`/tmp/critic-target` was reused for every pass. No UI was driven and no `rust/` source was edited.

## Cited test results

`F-CORE-ACT-06` and `F-CORE-ACT-11` cite the same test:

```bash
cargo test -p tiller panes::tests::process_owned_status_survives_title_and_child_exit_events -- --exact
```

Raw result:

```text
test panes::tests::process_owned_status_survives_title_and_child_exit_events ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 133 filtered out; finished in 0.00s
```

`F-CORE-ACT-07`'s ledger spelling is unqualified. Running that spelling literally with `--exact` selected no test:

```bash
cargo test -p tiller real_pty_layer_a_debounce_suppresses_first_title_and_accepts_second -- --exact
```

```text
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 134 filtered out; finished in 0.00s
```

Plain `cargo test -p tiller` emitted its actual fully qualified harness name, so the exact replay used that emitted name:

```bash
cargo test -p tiller panes::tests::real_pty_layer_a_debounce_suppresses_first_title_and_accepts_second -- --exact
```

Raw result:

```text
test panes::tests::real_pty_layer_a_debounce_suppresses_first_title_and_accepts_second ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 133 filtered out; finished in 4.34s
```

The full crate invocation ran both cited tests (each appeared once) and was green:

```bash
cargo test -p tiller
```

```text
test result: ok. 134 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 33.19s
```

## Flakiness check

Both executable cited tests were run ten consecutive times with their fully qualified names and `--exact`:

| Test | Runs | Result |
|---|---:|---|
| `panes::tests::process_owned_status_survives_title_and_child_exit_events` | 10 | 10 passed, 0 failed |
| `panes::tests::real_pty_layer_a_debounce_suppresses_first_title_and_accepts_second` | 10 | 10 passed, 0 failed |

This is stable green evidence at `56977f2`; it rules out the previously claimed *reproducible current failure*. It does not establish when the earlier failure was fixed, nor does it exercise either feature through the application.

## Current workspace baseline

```bash
cargo test --workspace
```

The workspace comprises `tiller`, `tiller_acp`, `tiller_persistence`, `tiller_activity`, `tiller_agents`, `tiller_control`, `tiller_git`, `tiller_project`, `tiller_terminal`, `tiller_theme`, `tiller_ui`, `tiller_markdown`, and `tiller_usage`.

The command exited 0. Its target result lines aggregate to **893 passed, 0 failed, 2 ignored** (45 test executables; 891 tests reported by `running N tests` lines). No failure marker was emitted.

## Ledger correction

`F-CORE-ACT-06`, `F-CORE-ACT-07`, and `F-CORE-ACT-11` are now `NOT EXERCISED`, not `PASSED`: their prior `FAILED — defective` verdicts rested solely on tests that are presently stable green, but tests are not live feature exercises. `INVENTORY-LEDGER.md` and `UNPROVEN-ROWS-RECIPES.md` now name the owed gestures:

- ACT-06: distinguish title-owned from process-owned state after replacing a title with unrelated text.
- ACT-07: verify Layer-A debounce with a hook status, an immediate contradictory recognized title, and a second title after 1.5 seconds.
- ACT-11: independently alter/terminate title, process, and spawn signal sources on separate panes, then close each pane and observe ownership-scoped cleanup.
