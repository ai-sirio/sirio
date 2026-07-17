# Performance and Memory Optimization — Master Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (- [ ]) syntax for tracking.

## Goal

Reduce CPU, memory, and rendering overhead in Tiller under the representative load of 10 worktrees and 20 active panes, targeting the 12 acceptance criteria defined in the approved design spec. No agent or terminal is ever automatically terminated. No new dependencies unless native and explicitly justified. Swift 6 / macOS 15+ only.

## Architecture

The work is decomposed into five sequential phases plus one investigation spike. Each phase produces independently testable, reviewable changes. The master plan owns the dependency graph, shared invariants, measurement protocol, and cross-plan AC mapping. Each sub-plan owns its code steps.

```
Phase 1: Empty Worktree Lifecycle (AC9–AC11)
    ↓
Phase 2: Measurement + Baseline (AC1–AC6 measurement infrastructure)
    ↓
Phase 3: Terminal Pipeline (AC2, AC6, AC7)
    ↓
Phase 4: Background/UI Coalescing (AC3, AC4)
    ↓
Phase 5: SwiftUI + Service Optimization (AC1, AC3, AC5)
    ↓
Spike: Hidden-Pane Rendering (AC7 gate)
```

## Tech Stack

- Swift 6, macOS 15+, Xcode 16+
- `swift-testing` (`@Test` / `#expect`) — no XCTest
- `os_signpost` via `OSSignposter` (gated by `UserDefaults`)
- `Scripts/ci.sh` as the single verification gate
- `project.yml` for target/config changes; never hand-edit `Tiller.xcodeproj`

## Global Constraints

1. **Never automatically terminate active agents or terminals.** Only explicit user action or the opt-in `maxMountedWorktrees` cap (evicts only idle worktrees — never the selected worktree, one with a running or input-waiting agent, or one with unsaved tabs) may terminate a PTY.
2. **Hidden panes continue execution.** A pane whose worktree is not selected, or whose tab is not active, continues to receive PTY output, accumulate scrollback, and run agent detection. Only rendering is throttled/coalesced.
3. **Full synchronization on visibility.** When a hidden pane becomes visible, its scrollback, agent state, and terminal surface must reflect all output produced while hidden.
4. **Empty worktrees have no PTY.** A worktree with zero tabs has no `PtyProcess`, no `ScrollbackBuffer`, no `TerminalPaneCache`, and no `PaneRegistry` entry. Its resource cost is bounded by the model-layer entries it owns.
5. **No data loss.** Closing the final tab of a worktree must not discard scrollback or agent state. The empty worktree persists and can receive a new terminal.
6. **No sensitive content in metrics.** `os_signpost` payloads contain only fixed labels and numeric counts — never terminal content, paths, commands, or identities.
7. **`Scripts/ci.sh` must print `CI OK`** at every phase gate.

## Shared Invariants

| Invariant | Enforced by |
|-----------|-------------|
| `openWorktreeIds` includes empty worktrees | Phase 1 bootstrap change |
| `activeTab(for:)` returns nil for empty tab list | Already safe (Phase 1 precondition) |
| `liveLeafIds(for:)` returns empty set for empty tabs | Already safe |
| `persistTabs` with empty list deletes all rows | Already safe (`saveTabs` DELETE + INSERT) |
| `scrollbackFlushTargets` returns empty for empty tabs | Already safe |
| `statusForWorktree` returns nil for empty worktrees | Already safe |
| `WorktreeMountPolicy.idsToEvict` treats nil status as idle | Already safe |
| `RightPanelModel.scheduleRefresh` marks dirty for inactive worktrees | Phase 4 |
| `UsageStore` timer is absent when all providers disabled | Phase 5 |
| Sidebar animations are per-row, not collection-wide | Phase 5 |
| Agent status is computed once per render pass | Phase 5 (scoped computed property, not global cache) |

## Measurement Protocol

### Reference Mac Configuration

| Property | Value |
|----------|-------|
| Model | Apple M-series (M1, M2, M3, or M4) |
| RAM | 16 GB |
| macOS | 15.x |
| Xcode | 16.x |

### Baseline Capture (Phase 2)

1. Launch Tiller with 10 worktrees, 20 panes (2 per worktree).
2. Let all panes idle for 60 seconds.
3. Record: CPU (Activity Monitor / `top`), resident RAM (`vmmap`), `os_signpost` metrics.
4. Run heavy output in 4 panes simultaneously (`git clone` of a large repo, `npm install`).
5. Record same metrics during and after.
6. Switch between worktrees rapidly (5 switches in 2 seconds).
7. Record main-thread stall duration (Instruments Time Profiler, HID template).

### After Each Change

Repeat the same scenario. Compare against baseline.

| Metric | Target |
|--------|--------|
| Idle CPU (10 wt / 20 panes) | < 1% |
| Terminal pipeline CPU (heavy output) | ≥ 20% reduction |
| Main-thread stall (worktree switch) | < 100 ms |
| Resident RAM | ≥ 15% reduction |
| RAM after 10× pane open/close | No growth |

Percentage gates are evaluated only after the Phase-2 baseline is recorded on the documented reference Mac configuration and workload.

## Cross-Plan AC Mapping

| AC | Description | Primary Plan | Verification |
|----|-------------|-------------|--------------|
| AC1 | Idle CPU < 1% on reference Mac | Phase 5 (background/UI) + Phase 3 (pipeline) | `top` / Activity Monitor, 60 s sample |
| AC2 | ≥ 20% terminal-pipeline CPU reduction | Phase 3 (terminal pipeline) | `os_signpost` `ptyIngest` + `contentSignal` intervals |
| AC3 | No main-thread stalls > 100 ms | Phase 4 (dirty coalescing) + Phase 5 (animations) | Instruments HID template, 5 switches |
| AC4 | No file tree or Git refresh for invisible worktrees | Phase 4 (dirty marker) | `os_signpost` `panelRefresh` — zero intervals while panel hidden |
| AC5 | ≥ 15% lower resident RAM | Phase 1 (empty lifecycle) + Phase 3 (ring buffer) | `vmmap` / Activity Monitor before and after |
| AC6 | No progressive RAM growth after 10× pane open/close | Phase 3 (ring buffer) + Phase 1 (empty lifecycle) | `vmmap` before and after cycle |
| AC7 | No lost bytes — hidden pane shows all accumulated output | Phase 3 (ring buffer tests) + Spike (hidden-pane) | Automated `ScrollbackBuffer` byte-level tests; B4 gate |
| AC8 | Correct agent state and notifications for hidden panes | Phase 3 (content signal) + Phase 4 | Control socket `notify` handler test; sidebar badge verification |
| AC9 | Empty worktree persists and restores | Phase 1 | Close all tabs, quit, relaunch — worktree present with empty state |
| AC10 | "New Terminal" button and keyboard shortcut create first tab | Phase 1 | Click button / press ⌘T → shell tab appears |
| AC11 | Empty mounted worktree has no PTY process | Phase 1 | `ps aux | grep tiller` — no shell process for empty worktree |
| AC12 | `Scripts/ci.sh` prints `CI OK` | All phases | Run `Scripts/ci.sh` |

## Phase Dependencies

```
Phase 1 (empty lifecycle) — no dependencies
    ↓
Phase 2 (measurement) — no code dependencies on Phase 1
    ↓
Phase 3 (terminal pipeline) — depends on Phase 2 for baseline
    ↓
Phase 4 (background/UI) — depends on Phase 1 (empty lifecycle changes affect RightPanelModel activation)
    ↓
Phase 5 (SwiftUI) — depends on Phase 1 (empty state UI) + Phase 4 (dirty marker)
    ↓
Spike (hidden-pane) — independent, can run in parallel with any phase
```

## Definition of Done

1. All checkbox steps in all sub-plans are complete.
2. `Scripts/ci.sh` prints `CI OK`.
3. All 12 ACs are verified per their verification column.
4. No `TBD`, `TODO`, `FIXME`, `XXX`, "similar to", "add tests", "handle edge cases", or other placeholders remain in any plan or implementation.
5. No sensitive content in `os_signpost` payloads.
6. No hard-coded uncalibrated performance thresholds in unit tests.
7. No fragile global caches with manual invalidation at many mutation sites.
8. No invented libghostty APIs or undocumented rendering workarounds.

## Files Created

| File | Purpose |
|------|---------|
| `docs/superpowers/plans/2026-07-17-performance-memory-optimization-plan.md` | Master orchestration (this file) |
| `docs/superpowers/plans/2026-07-17-empty-worktree-lifecycle-plan.md` | Phase 1: empty worktree lifecycle |
| `docs/superpowers/plans/2026-07-17-performance-measurement-plan.md` | Phase 2: measurement + baseline |
| `docs/superpowers/plans/2026-07-17-terminal-pipeline-plan.md` | Phase 3: terminal pipeline |
| `docs/superpowers/plans/2026-07-17-background-swiftui-plan.md` | Phase 4 + 5: background/UI coalescing + SwiftUI |
| `docs/superpowers/plans/2026-07-17-hidden-pane-rendering-spike-plan.md` | Spike: hidden-pane rendering investigation |

---

## Assumptions Requiring Implementation-Time Confirmation

1. **`RightPanelModel` FSEvent lifecycle:** The current `scheduleRefresh` checks `token == generation` and silently drops events for inactive worktrees. The plan assumes this is the only path for FS events. Confirm by tracing `FileSystemEventMonitor` event delivery to `RightPanelModel` — if events are delivered through a different path, the dirty-marker insertion point changes.
2. **`persistTabs` scrollback dependency:** The spec states `persistTabs` consumes only `snapshot()` for scrollback data. Confirm by reading `PaneScrollbackRecord` write sites — if scrollback is persisted through a different path, the empty-worktree persistence precondition is already met.
3. **`ContentView.terminalStack` empty-state insertion point:** The plan adds `EmptyWorktreeView` before the `ForEach(model.openWorktreeIds)`. Confirm the exact ZStack ordering in `terminalStack` — the empty-state view must be mutually exclusive with the terminal `ForEach` to avoid rendering both.
4. **`UsageStore.timer` visibility for testing:** The plan makes `timer` `internal` for test access. Confirm no other code path depends on `timer` being `private` (e.g., a `deinit` that cancels it — already handled by `deinit { timer?.cancel() }`).
5. **`os_signpost` import availability:** `OSSignposter` is available in `os` module on macOS 15+. Confirm the import compiles in all target packages (`TillerTerminal`, `TillerCore`, `App`).
6. **`SignpostMetrics.swift` dependency chain:** This file lives in `TillerTerminal` but instruments `App` types (e.g., `RightPanelModel`, `SidebarView`, `AppModel`). It uses `AppSettings.signpostMetricsKey` from `TillerCore`. The dependency chain `App -> TillerTerminal -> TillerCore` makes the import valid — `App` depends on `TillerTerminal`, which depends on `TillerCore`. No circular dependency is introduced.
