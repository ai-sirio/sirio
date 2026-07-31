# Workspace Performance Baseline — Release build

**Date:** 2026-07-31
**Build:** Release, commit 1c213cf, `dev.tiller.Tiller`
**Scenario:** Two real worktrees on this machine (both this `tiller` checkout: `main`
and `fix/chat-card-ui`). Five passes, each: `select-workspace A` → `panel create --cmd
"sleep 20"` → `panel split --from <id> right --cmd "sleep 20"` → `panel close` both →
`select-workspace B`. Driven entirely through a Release `tillerctl` (no GUI automation
tool was available this session — see gotcha below). Each pass's `log show` window was
captured immediately after driving it.
**Signposts:** `debug.signpostMetrics = YES`

## Two real bugs found and fixed while setting up this capture

Driving the scenario above surfaced two production bugs that were blocking it, both
fixed and committed before this baseline was captured (not benchmarking artifacts —
verified against this machine's real, already-migrated `~/Library/Application
Support/Tiller/tiller.sqlite`):

1. **`AppModel.restoreWorktree` never reached `workspaceCoordinator.restore()`** on any
   database that had completed the v17 migration — `ProjectStore.loadTabs(of:)` queries
   `terminalTab`, which v17 renames to `legacyTerminalTab_v15`, so the call threw and the
   function returned early before the line that populates the universal engine's layout.
   Confirmed via `log show`: every worktree logged `restore: loadTabs failed ... no such
   table: terminalTab`, `coordinator.layouts` stayed empty for every worktree, and
   `panel.create` failed with `workspace layout unavailable` for all of them. Fixed in
   commit `e40baaa`.
2. **`panel.split` still resolved its `--from` source panel through the legacy
   `workspaceTabContaining(paneId:)` lookup** — the one gate-enabled control operation
   Task 12.1 missed. Any panel actually created through the (now-fixed) universal-engine
   `panel.create` failed to split with `unknown source panel`. Fixed in commit `1c213cf`.

## PF-1 — Workspace command apply (budget: p95 ≤ 1 ms)

| | |
|---|---|
| Method | Five captured pass logs; select the median run by `workspaceCommandApply` p95, then compute that run's sample median and p95 from begin/end pairs |
| Result | samples=3, median=0.000 ms, p95=0.000 ms (median run: pass 5) — durations under `log show`'s effective timestamp resolution round to 0 ms |
| Verdict | **PASS**, with a caveat: only 3 samples across the whole capture (one `workspaceCommandApply` per `panel.create`/`panel.split`/close-driven mutation), and durations this small are at the edge of what `log show`'s timestamp precision can resolve. Real, but a thin sample — a signposted GUI session with dozens of ordinary tab operations would be a stronger confirmation |

## PF-2 — Workspace reconcile (budget: p95 ≤ 8 ms)

| | |
|---|---|
| Method | Five captured pass logs; select the median run by `workspaceReconcile` p95, then compute that run's sample median and p95 from begin/end pairs |
| Result | samples=14, median=1.500 ms, p95=10.650 ms, p99=20.530 ms (median run: pass 1) |
| Verdict | **FAIL (borderline)** — p95 is ~33% over budget. Only 14 samples total across 5 passes (worktree selection + the reconcile that follows each structural mutation), so this is a noisy estimate, not a confirmed regression. Not fixed in this session: doing so responsibly needs a larger, GUI-driven sample (this scenario is CLI-only, exercising fewer/heavier reconciles than ordinary interactive use) before attributing the overage to a real hot path rather than sample noise. Tracked as a follow-up, not silently dropped |

## PF-3 — Drag preview + divider tracking (budget: p95 ≤ 16.7 ms; no two consecutive samples > 33.3 ms)

| | |
|---|---|
| Method | Combine `workspaceDragFrame` samples from drag moves and divider moves |
| Result | 0 samples |
| Verdict | **NO DATA** — expected, not a gap in the instrumentation. `DragSession.pointerMoved`/`DividerTracking.moved` only fire from real pointer events; no `tillerctl` verb exists for simulated drag or divider tracking, and no GUI automation tool (computer-use, Chrome extension, etc.) was available in this session to drive real mouse events. Requires a manual mouse-driven capture on a future run |

## PF-4 — Workspace structural commit (budget: p95 ≤ 100 ms; p99 ≤ 250 ms)

| | |
|---|---|
| Method | Five captured pass logs; select the median run by `workspaceStructuralCommit` p95, then compute that run's sample median, p95, and p99 from begin/end pairs |
| Result | samples=3, median=6.000 ms, p95=6.000 ms, p99=6.000 ms (median run: pass 4) |
| Verdict | **PASS**, comfortably (94 ms of headroom against p95, 244 ms against p99). Same thin-sample caveat as PF-1 |

## PF-5 — Workspace restore (budget: p95 ≤ 250 ms)

| | |
|---|---|
| Method | Select the median run by `workspaceRestore` p95 |
| Result | 0 samples |
| Verdict | **NO DATA** — not a gap in the instrumentation, a gap in this scenario's design. `WorkspaceCoordinator.restore(worktree:)` runs once per worktree during `AppModel.bootstrap()`, before this capture's window started (each pass only captured its last ~12s, well after launch); ordinary worktree selection afterward does not call it again. Needs a scenario that quits and relaunches the app (or opens a worktree for the first time) inside the capture window |

## Notes / gotchas from this run

- The UserDefaults gate belongs to the app bundle domain (`dev.tiller.Tiller`), not the
  OSLog subsystem (`dev.tiller`): `defaults write dev.tiller.Tiller
  debug.signpostMetrics -bool YES`. Relaunch Tiller after changing it —
  `SignpostMetrics.enabled` is a lazily evaluated `static let`, read once per process.
- Signpost capture requires `log show --predicate 'subsystem == "dev.tiller"' --last
  <window> --signpost --debug --info --style compact`. `--signpost` is mandatory; without
  it these `OS_LOG_TYPE_DEBUG` events are silently absent, not errored.
- A pane created via `panel.split` needs noticeably longer to become closable than one
  created via plain `panel.create` — a new split spawns a new nested split-view
  hierarchy that must lay out and settle before `PtyRuntime` spawns (it spawns "on first
  settled resize"), and `panel.close`/`panel.read`/etc. resolve the pane's id through the
  live `PaneRegistry` registration, not just through the layout. A ~1.5s gap after split
  was not reliably enough on this machine; ~3s was. This is latency, not a correctness
  bug — the id returned by `panel.split` does resolve, just not instantly.
- `Scripts/bench-workspace.sh` (written in the prior commit) assumes an interactive
  terminal for its "freshly launched?" confirmation prompt and drives only
  `select-workspace`. This session drove `panel create`/`panel split`/`panel close`
  directly instead — no GUI automation tool was available, so PF-3 could not be captured
  either way, but doing so headlessly via `tillerctl` reached real `workspaceCommandApply`/
  `workspaceStructuralCommit` samples that plain worktree selection alone would not have.
- `tillerctl`'s actual subcommand is `ping`, not `system ping`, and `panel split`'s
  direction is a positional argument (`panel split --from <id> right`), not `--direction
  right` — both differ from what the plan's illustrative examples implied.
