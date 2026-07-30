# Workspace Performance Baseline — Release build

**Date:** TODO (Step 2)
**Build:** Release, commit TODO, `dev.tiller.Tiller`
**Scenario:** Two existing worktrees selected alternately for 5 warm passes via `tillerctl select-workspace --workspace <uuid-or-path>`; TODO: record the actual fixture/worktree shape used
**Signposts:** `debug.signpostMetrics = YES`

## PF-1 — Workspace command apply (budget: p95 ≤ 1 ms)

| | |
|---|---|
| Method | Five captured pass logs; select the median run by `workspaceCommandApply` p95, then compute that run's sample median and p95 from begin/end pairs |
| Result | TODO |

## PF-2 — Workspace reconcile (budget: p95 ≤ 8 ms)

| | |
|---|---|
| Method | Five captured pass logs; select the median run by `workspaceReconcile` p95, then compute that run's sample median and p95 from begin/end pairs |
| Result | TODO |

## PF-3 — Drag preview + divider tracking (budget: p95 ≤ 16.7 ms; no two consecutive samples > 33.3 ms)

| | |
|---|---|
| Method | Five captured pass logs; combine `workspaceDragFrame` samples from drag moves and divider moves, select the median run by p95, then check p95 and consecutive-sample budget |
| Result | TODO |

## PF-4 — Workspace structural commit (budget: p95 ≤ 100 ms; p99 ≤ 250 ms)

| | |
|---|---|
| Method | Five captured pass logs; select the median run by `workspaceStructuralCommit` p95, then compute that run's sample median, p95, and p99 from begin/end pairs |
| Result | TODO |

## PF-5 — Workspace restore (budget: p95 ≤ 250 ms)

| | |
|---|---|
| Method | Five captured pass logs; select the median run by `workspaceRestore` p95, excluding the terminal PTY-hydration loop as defined by the instrumentation boundary |
| Result | TODO |

## Notes

- This is a template only. Step 2 must fill in the date, build/commit, fixture details, sample counts, medians, percentiles, and PASS/FAIL results after a manual run on real hardware.
- The UserDefaults gate belongs to the app bundle domain (`dev.tiller.Tiller`), not the OSLog subsystem (`dev.tiller`): `defaults write dev.tiller.Tiller debug.signpostMetrics -bool YES`. Relaunch Tiller after changing it because `SignpostMetrics.enabled` is a lazily evaluated static value read once per process.
- Signpost capture requires `log show --predicate 'subsystem == "dev.tiller"' --last <window> --signpost --debug --info --style compact`. The `--signpost` flag is required for these debug-level signpost events to appear.
- `tillerctl select-workspace --workspace <uuid-or-path>` already exists and is used by `Scripts/bench-workspace.sh`; no new production CLI surface was added for benchmarking.
- The existing CLI has no verb for pointer drag, divider tracking, or direct workspace-layout commands. The script automates only worktree selection; PF-1, PF-3, and PF-4 require manual GUI interaction during a real-hardware capture if they are to produce samples.
