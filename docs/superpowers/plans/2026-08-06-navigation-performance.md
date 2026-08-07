# Navigation Performance Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make project/worktree/tab navigation feel immediate while preserving every mounted PTY, scrollback byte, agent signal, and workspace state.

**Architecture:** Route every worktree selection through one idempotent funnel, pass a revisioned render state to the AppKit workspace host, and separate visibility changes from structural reconciliation. Build immutable sidebar/activity projections once per render pass instead of repeatedly deriving legacy tabs and agent state. Keep right-panel work off the switch critical path unless profiling proves it is material.

**Tech Stack:** Swift 6, SwiftUI + AppKit, Observation, libghostty, `OSSignposter`, swift-testing, Xcode 16+, macOS 15+.

## Audit Basis

- Audited commit: `47cb15a` on 2026-08-06.
- Historical workspace baseline: Release commit `1c213cf` from 2026-07-31.
- The historical baseline is 218 commits and 264 changed files behind the audited commit; it is evidence about the benchmark method, not a valid baseline for current HEAD.
- Existing working-tree change at audit time: `App/TillerApp.swift`. Do not edit, stage, or revert that file as part of this plan.
- No current-HEAD build, CI run, or Instruments capture was performed during this read-only audit.

## Global Constraints

1. Never terminate or restart a PTY because its worktree or tab becomes hidden.
2. Hidden panes continue PTY reads, scrollback capture, agent detection, control-socket handling, and notifications.
3. A hidden terminal surface may stop its display link only through the existing tested Ghostty occlusion path; selecting it must restore a fully synchronized surface.
4. Do not replace the mounted-worktree `ZStack` with conditional creation/destruction.
5. Do not force refreshes with `.id(...)` or recreate AppKit controllers during selection.
6. Do not add a global cache with mutation-site invalidation. Derived navigation data must be a value snapshot or keyed by an existing monotonic workspace revision.
7. Do not put hard-coded wall-clock performance assertions in unit tests. Unit tests verify operation counts and state transitions; Release benchmarks enforce latency budgets.
8. All tests use swift-testing (`@Test`, `#expect`), not XCTest.
9. Every task ends with its focused tests; the final gate is `Scripts/ci.sh` printing `CI OK`.
10. Commit subjects follow Conventional Commits and remain lower-case imperative.

---

## Findings Driving the Plan

### Proven from the current source

1. **Universal-workspace terminal visibility is not propagated.** `ContentView.workspaceStack` hides mounted worktrees with `.opacity(0)`, while `TerminalContentAdapter.makeHost` constructs `WorkspaceContentHostAdapter` without a visibility closure. Its default closure is a no-op. The tested `SurfaceVisibility`/Ghostty occlusion path therefore used by `TerminalSplitHost` is not reached by universal workspace hosts.
2. **Every `WorkspaceViewController.update` reconciles and requests layout.** There is no equality or revision guard before `WorkspaceReconciler.reconcile`, and `view.needsLayout = true` is unconditional.
3. **Reconcile still walks the whole pane tree.** Cached controllers are reused, but every walk rebuilds tab menu entries, assigns observable strip state, recomputes accessibility, prunes maps, and evaluates focus. Existing tests assert zero new controller allocation; they do not assert zero redundant controller updates.
4. **Semantic delta is global while layouts/revisions are per worktree.** `WorkspaceCoordinator.lastSemanticDelta` can be passed to whichever worktree is selected next, even when the delta came from another worktree.
5. **Sidebar projection contains a quadratic lookup.** `SidebarTabProjection.rows` iterates `layout.allTabs`; for every terminal it calls `liveControlPaneId`, which scans `layout.allTabs` again.
6. **Attention sorting re-evaluates status inside the comparator.** `AttentionSort.urgentFirst` invokes `statusOf` repeatedly during sort; `statusForWorktree` rebuilds the tab projection each time.
7. **Activity rows repeat the same work.** `ActivitySectionView.runningCount` reads computed `rows`, then expanded content reads `rows` again. `ActivityPanelModel` also resolves live pane IDs with the repeated full-tab scan.
8. **Selection is not idempotent.** `selectedWorktree.didSet` persists defaults and evaluates mount eviction even when the same worktree is assigned again. There are 25 direct assignments in `App/`.
9. **Engine tab navigation still crosses the legacy seam.** Sidebar tab selection, numbered selection, and tab cycling call `AppModel.activateTab`, which writes `LegacyWorkspaceStore`; the visible universal layout is sourced from `WorkspaceCoordinator.layouts`.
10. **Right-panel activation starts fresh work on every selected-worktree change.** It clears state, loads root directory + full Git status, then prefetches every root directory at user-initiated priority.
11. **The current signpost is not end-to-end.** `worktreeSwitch` begins and ends inside `selectedWorktree.didSet`, before SwiftUI invalidation, AppKit reconcile, layout, focus, and frame presentation.
12. **The navigation state hub is unusually broad.** `AppModel` spans 2,943 lines and has file fan-out 72. This magnifies invalidation and change risk, but size alone is not proof of a frame hitch.

### Existing measured signal

The 2026-07-31 Release run recorded `workspaceReconcile` median 1.5 ms and p95 10.65 ms against an 8 ms budget, with only 14 samples. Treat this as a reason to re-measure, not as a current regression verdict.

### Hypotheses requiring Instruments

- How many mounted `WorkspaceView`s receive `updateNSViewController` for one worktree switch.
- Whether hidden universal terminal display links materially consume CPU/GPU during navigation.
- Whether right-panel root/status loading overlaps the click-to-present critical path.
- Which observed `AppModel` mutations cause the widest SwiftUI update groups.

---

## Target Interfaces and File Map

### New files

- `App/Navigation/WorktreeSwitchTracker.swift` — owns one pending end-to-end switch interval and rejects stale completions.
- `App/Navigation/SidebarPresentation.swift` — immutable, per-render sidebar value model and pure builder.
- `AppTests/Navigation/WorktreeSwitchTrackerTests.swift` — stale/current completion and idempotence tests.
- `AppTests/Navigation/SidebarPresentationTests.swift` — one-pass projection and state tests.
- `Packages/TillerWorkspace/Tests/TillerWorkspaceTests/WorkspaceVisibilityTests.swift` — visibility changes without host recreation or structural reconcile.
- `Scripts/bench-navigation.sh` — repeatable Release workload and signpost extraction.
- `docs/superpowers/notes/navigation-performance-baseline.md` — reference machine, workload, raw summary, before/after table.

### Modified files

- `App/AppModel.swift` — private selection storage, one selection method, engine-aware tab navigation, snapshot production.
- `App/AppModel+Control.swift`, `App/SidebarView.swift`, `App/AgentRosterView.swift` — replace direct selection assignments with the selection funnel.
- `App/ContentView.swift` — pass revision, per-worktree delta, visibility, and presentation generation into each mounted workspace.
- `App/Workspace/WorkspaceCoordinator.swift` — per-worktree deltas and one-pass presentation index.
- `App/Workspace/TerminalContentAdapter.swift` — connect workspace visibility to `TerminalSurfaceHost`.
- `App/RightPanel/ActivityPanelModel.swift`, `App/RightPanel/ActivitySectionView.swift` — consume one activity snapshot per body pass.
- `App/RightPanel/RightPanelModel.swift` — activation signpost and, only if the profiling gate fires, bounded warm snapshots/deferred prefetch.
- `Packages/TillerCore/Sources/TillerCore/AttentionSort.swift` — decorate once, then stable-sort cached urgency ranks.
- `Packages/TillerCore/Tests/TillerCoreTests/AttentionSortTests.swift` — exactly-one status lookup per item.
- `Packages/TillerTerminal/Sources/TillerTerminal/TerminalSurfaceHost.swift` — public idempotent visibility operation backed by existing occlusion implementation.
- `Packages/TillerTerminal/Tests/TillerTerminalTests/TerminalSurfaceHostTests.swift` — visibility before/after mount and relaunch.
- `Packages/TillerWorkspace/Sources/TillerWorkspace/WorkspaceView.swift` — consume one render-state value.
- `Packages/TillerWorkspace/Sources/TillerWorkspace/WorkspaceViewController.swift` — separate visibility/presentation from revisioned reconcile.
- `Packages/TillerWorkspace/Sources/TillerWorkspace/WorkspaceReconciler.swift` — workspace visibility propagation and operation counters for tests.
- `Packages/TillerWorkspace/Sources/TillerWorkspace/PaneGroupController.swift` — idempotent strip assignment and mounted-host visibility.
- `Packages/TillerWorkspace/Tests/TillerWorkspaceTests/ReconcilerPerformanceTests.swift` — operation-count assertions, not only allocation-count assertions.

---

### Task 1: Centralize selection and route tab navigation to the active engine

**Files:**

- Create: `App/Navigation/WorktreeSwitchTracker.swift`
- Create: `AppTests/Navigation/WorktreeSwitchTrackerTests.swift`
- Modify: `App/AppModel.swift:27-75, 1295-1326, 1423-1426, 1596-1616, 2029-2049`
- Modify: `App/AppModel+Control.swift:76-135`
- Modify: `App/SidebarView.swift:341-425, 500-680`
- Modify: `App/AgentRosterView.swift:55-65`
- Test: `AppTests/Workspace/SidebarEngineParityTests.swift`

**Interfaces:**

- Produces: `AppModel.selectWorktree(_ worktree: Worktree?)` and read-only `worktreeSelectionGeneration: UInt64`.
- Produces: `WorktreeSwitchTracker.begin(worktreeID:) -> UInt64`, `complete(generation:worktreeID:)`, and `cancel()`.
- Produces: test initializer `WorktreeSwitchTracker(onCompletion:)`, where `onCompletion` is `(UInt64, UUID) -> Void`; the live initializer owns the `worktreeSwitchToLayout` signpost.
- Preserves: `AppModel.selectedWorktree` as readable observed state, but makes external writes impossible.

- [ ] **Step 1: Write tracker tests before the tracker**

```swift
@Test func onlyTheCurrentSelectionCanComplete() {
    var completed: [(UInt64, UUID)] = []
    let tracker = WorktreeSwitchTracker {
        completed.append(($0, $1))
    }
    let firstID = UUID()
    let secondID = UUID()

    let first = tracker.begin(worktreeID: firstID)
    let second = tracker.begin(worktreeID: secondID)
    tracker.complete(generation: first, worktreeID: firstID)
    tracker.complete(generation: second, worktreeID: secondID)

    #expect(completed.count == 1)
    #expect(completed[0].0 == second)
    #expect(completed[0].1 == secondID)
}

@Test func selectingTheSameWorktreeDoesNotAdvanceGeneration() {
    let fixture = makeNavigationFixture()
    fixture.model.selectWorktree(fixture.first)
    let generation = fixture.model.worktreeSelectionGeneration
    fixture.model.selectWorktree(fixture.first)
    #expect(fixture.model.worktreeSelectionGeneration == generation)
}
```

- [ ] **Step 2: Run the focused tests and confirm RED**

Run:

```bash
xcodegen generate
xcodebuild test -project Tiller.xcodeproj -scheme Tiller \
  -destination 'platform=macOS' \
  -only-testing:TillerTests/WorktreeSwitchTrackerTests
```

Expected: compilation fails because `WorktreeSwitchTracker` and `selectWorktree` do not exist.

- [ ] **Step 3: Implement one idempotent selection funnel**

Move the current `didSet` effects behind this shape:

```swift
private(set) var selectedWorktree: Worktree?
private(set) var worktreeSelectionGeneration: UInt64 = 0

func selectWorktree(_ worktree: Worktree?) {
    let previousID = selectedWorktree?.id
    guard selectedWorktree != worktree else { return }

    // A same-ID value refresh keeps selected metadata current without
    // creating a navigation transaction.
    selectedWorktree = worktree
    selectedProjectId = worktree?.projectId
    guard previousID != worktree?.id else { return }

    let generation = worktree.map { switchTracker.begin(worktreeID: $0.id) }
    worktreeSelectionGeneration = generation ?? worktreeSelectionGeneration
    UserDefaults.standard.set(
        worktree?.id.uuidString,
        forKey: AppSettings.selectedWorktreeIdKey)
    guard let worktree else {
        switchTracker.cancel()
        return
    }
    if !openWorktreeIds.contains(worktree.id) {
        openWorktreeIds.append(worktree.id)
    }
    evictIdleWorktreesIfNeeded()
}
```

Replace all 25 direct assignments under `App/` with this method. Keep bootstrap/removal semantics unchanged; nil still clears selection. Add a regression where a same-ID `Worktree` with a changed comment refreshes `selectedWorktree` without advancing the selection generation.

- [ ] **Step 4: Make tab activation use the same source of truth as rendering**

```swift
func activateTab(_ tabID: UUID, in worktreeID: UUID) {
    if WorkspaceEngineGate.isEnabled {
        workspaceCoordinator.activateTabDirectly(
            WorkspaceTabID(tabID), in: worktreeID)
        return
    }
    workspaceCoordinator.setLegacyActiveTabID(tabID, for: worktreeID)
    workspacePersistTabs(for: worktreeID)
}
```

When the engine is enabled, make `workspaceSelectTab` and `workspaceCycleTab` derive their list from `layout.group(layout.activeGroupID)?.tabs`; numbered/cyclic navigation is local to the focused pane group. Keep `legacyTabs(for:)` only for the legacy path. Return before publishing when the requested tab is already active.

- [ ] **Step 5: Add engine navigation regressions**

```swift
@Test func activatingProjectedSidebarTabUpdatesEngineLayout() async throws {
    let fixture = makeFixture(suffix: #function)
    let tabs = try await openTwoEngineTabs(fixture)
    fixture.model.activateTab(tabs[0].id.rawValue, in: fixture.worktree.id)
    #expect(fixture.model.workspaceActiveTabID(for: fixture.worktree.id) == tabs[0].id.rawValue)
}

@Test func cyclingTabsUsesEngineTabsWhenTheGateIsEnabled() async throws {
    let fixture = makeFixture(suffix: #function)
    let tabs = try await openTwoEngineTabs(fixture)
    fixture.model.selectWorktree(fixture.worktree)
    fixture.model.activateTab(tabs[0].id.rawValue, in: fixture.worktree.id)
    fixture.model.workspaceCycleTab(forward: true)
    #expect(fixture.model.workspaceActiveTabID(for: fixture.worktree.id) == tabs[1].id.rawValue)
}
```

- [ ] **Step 6: Run focused tests and commit**

```bash
xcodebuild test -project Tiller.xcodeproj -scheme Tiller \
  -destination 'platform=macOS' \
  -only-testing:TillerTests/WorktreeSwitchTrackerTests \
  -only-testing:TillerTests/SidebarEngineParityTests
git add App/Navigation AppTests/Navigation App/AppModel.swift App/AppModel+Control.swift \
  App/SidebarView.swift App/AgentRosterView.swift AppTests/Workspace/SidebarEngineParityTests.swift
git commit -m "fix: centralize workspace navigation"
```

---

### Task 2: Propagate mounted-worktree visibility to terminal occlusion

**Files:**

- Modify: `Packages/TillerTerminal/Sources/TillerTerminal/TerminalSurfaceHost.swift:53-138`
- Modify: `Packages/TillerTerminal/Tests/TillerTerminalTests/TerminalSurfaceHostTests.swift`
- Modify: `Packages/TillerWorkspace/Sources/TillerWorkspace/WorkspaceView.swift:4-43`
- Modify: `Packages/TillerWorkspace/Sources/TillerWorkspace/WorkspaceViewController.swift:4-69`
- Modify: `Packages/TillerWorkspace/Sources/TillerWorkspace/WorkspaceReconciler.swift:4-218`
- Modify: `Packages/TillerWorkspace/Sources/TillerWorkspace/PaneGroupController.swift:176-234`
- Create: `Packages/TillerWorkspace/Tests/TillerWorkspaceTests/WorkspaceVisibilityTests.swift`
- Modify: `App/Workspace/TerminalContentAdapter.swift:69-94`
- Modify: `App/ContentView.swift:577-639`

**Interfaces:**

- Produces: `WorkspaceRenderState(layout:revision:delta:isVisible:presentationGeneration:)`.
- Produces: `TerminalSurfaceHost.setVisible(_:)`, idempotent and independent from PTY lifecycle.

```swift
public struct WorkspaceRenderState: Equatable, Sendable {
    public let layout: WorkspaceLayout
    public let revision: Int
    public let delta: WorkspaceLayoutDelta?
    public let isVisible: Bool
    public let presentationGeneration: UInt64?
}
```

- Produces: `WorkspaceReconciler.setVisible(_:)`; it affects mounted hosts only and never detaches them.

- [ ] **Step 1: Write workspace visibility tests**

```swift
@Test func hidingAWorkspaceOccludesEveryMountedHostWithoutRecreation() throws {
    let fixture = try makeTwoGroupVisibilityFixture()
    fixture.controller.update(state: fixture.visibleState)
    let created = fixture.provider.hostCreationCount

    fixture.controller.update(state: fixture.hiddenState)

    #expect(fixture.provider.hostCreationCount == created)
    #expect(fixture.hosts.allSatisfy { $0.visibilityEvents.last == false })
}

@Test func showingAWorkspaceRevealsOnlyTheMountedActiveHostInEachGroup() throws {
    let fixture = try makeTwoGroupVisibilityFixture()
    fixture.controller.update(state: fixture.hiddenState)
    fixture.controller.update(state: fixture.visibleState)
    #expect(fixture.hosts.filter(\.isMounted).allSatisfy {
        $0.visibilityEvents.last == true
    })
}
```

- [ ] **Step 2: Run the package test and confirm RED**

```bash
cd Packages/TillerWorkspace
swift test --filter WorkspaceVisibilityTests
```

Expected: compilation fails because render state and workspace visibility do not exist.

- [ ] **Step 3: Add an idempotent visibility seam to `TerminalSurfaceHost`**

The public interface is deliberately one method; recursive Ghostty-view discovery remains hidden inside `TillerTerminal`:

```swift
public func setVisible(_ visible: Bool) {
    guard surfaceVisible != visible else { return }
    surfaceVisible = visible
    applyCurrentVisibility()
}
```

Store the current value and reapply it after `relaunch()` and after a late surface mount. Reuse `SurfaceVisibility.apply`; do not duplicate or expose Ghostty internals to `App/`.

- [ ] **Step 4: Wire the terminal adapter to that seam**

```swift
return WorkspaceContentHostAdapter(
    tabID: tab.id,
    viewController: surface.viewController,
    visibility: { [weak surface] visible in surface?.setVisible(visible) },
    focus: { [weak surface] _ in surface?.focusTerminal() == true },
    release: { [weak surface] in await surface?.teardown() })
```

- [ ] **Step 5: Carry visibility through the workspace renderer**

`ContentView.workspaceStack` supplies `isVisible: isSelected` for every mounted worktree. `WorkspaceViewController` sends visibility changes to `WorkspaceReconciler`, which forwards them to each `PaneGroupController`. The unchanged-host fast path must still call `mountedHost.setVisible(workspaceIsVisible)` before returning.

- [ ] **Step 6: Verify no-loss behavior**

Add a package-level visibility test that writes deterministic terminal bytes while hidden, reveals the host, and checks the complete scrollback sequence. Then run:

```bash
cd Packages/TillerTerminal
swift test --filter TerminalSurfaceHostTests
swift test --filter SurfaceVisibilityTests
cd ../TillerWorkspace
swift test --filter WorkspaceVisibilityTests
```

Expected: all focused tests pass; visibility events do not stop the runtime or clear scrollback.

- [ ] **Step 7: Commit**

```bash
git add Packages/TillerTerminal Packages/TillerWorkspace \
  App/Workspace/TerminalContentAdapter.swift App/ContentView.swift
git commit -m "perf: occlude hidden workspace terminals"
```

---

### Task 3: Reconcile only the worktree revision that changed

**Files:**

- Modify: `App/Workspace/WorkspaceCoordinator.swift:20-30, 501-510`
- Modify: `App/ContentView.swift:577-639`
- Modify: `Packages/TillerWorkspace/Sources/TillerWorkspace/WorkspaceView.swift`
- Modify: `Packages/TillerWorkspace/Sources/TillerWorkspace/WorkspaceViewController.swift:35-43`
- Modify: `Packages/TillerWorkspace/Sources/TillerWorkspace/WorkspaceReconciler.swift:62-89`
- Modify: `Packages/TillerWorkspace/Sources/TillerWorkspace/PaneGroupController.swift:176-210`
- Modify: `Packages/TillerWorkspace/Tests/TillerWorkspaceTests/ReconcilerPerformanceTests.swift`

**Interfaces:**

- Produces: `WorkspaceCoordinator.semanticDelta(for worktreeID: UUID) -> WorkspaceLayoutDelta?`.
- Consumes: `WorkspaceRenderState.revision` and `.isVisible` from Task 2.
- Invariant: a visibility-only change updates host visibility/focus but does not traverse the layout tree.

- [ ] **Step 1: Extend performance tests to count work, not allocations**

```swift
@Test func oneHundredUnchangedUpdatesPerformNoAdditionalReconcile() throws {
    let fixture = try makeControllerFixture(groupCount: 8, tabsPerGroup: 4)
    fixture.controller.update(state: fixture.state(revision: 7, visible: true))
    let baseline = fixture.controller.reconcileCount

    for _ in 0..<100 {
        fixture.controller.update(state: fixture.state(revision: 7, visible: true))
    }

    #expect(fixture.controller.reconcileCount == baseline)
}

@Test func changingVisibilityDoesNotTraverseTheLayoutTree() throws {
    let fixture = try makeControllerFixture(groupCount: 8, tabsPerGroup: 4)
    fixture.controller.update(state: fixture.state(revision: 7, visible: true))
    let baseline = fixture.controller.reconcileCount
    fixture.controller.update(state: fixture.state(revision: 7, visible: false))
    #expect(fixture.controller.reconcileCount == baseline)
}
```

- [ ] **Step 2: Run RED**

```bash
cd Packages/TillerWorkspace
swift test --filter ReconcilerPerformanceTests
```

Expected: new operation-count assertions fail because `update` always reconciles.

- [ ] **Step 3: Store deltas per worktree**

Replace the single delta with locality at the coordinator seam:

```swift
private var semanticDeltas: [UUID: WorkspaceLayoutDelta] = [:]

func semanticDelta(for worktreeID: UUID) -> WorkspaceLayoutDelta? {
    semanticDeltas[worktreeID]
}

private func publish(_ transition: WorkspaceLayoutTransition, worktreeID: UUID) {
    layouts[worktreeID] = transition.layout
    revisions[worktreeID, default: 0] += 1
    semanticDeltas[worktreeID] = transition.delta
    dirtyWorktreeIDs.remove(worktreeID)
    lastFocusIntent = .none
}
```

Remove `lastSemanticDelta` after all callers use the keyed method.

- [ ] **Step 4: Guard reconcile by revision and value**

Use optional initial state so revision zero remains valid:

```swift
private var appliedRevision: Int?

public func update(state: WorkspaceRenderState) {
    _ = view
    attachRootIfNeeded()
    reconciler.setVisible(state.isVisible)

    let layoutChanged = appliedRevision != state.revision || currentLayout != state.layout
    guard layoutChanged else {
        completePresentationIfNeeded(state.presentationGeneration)
        return
    }

    currentLayout = state.layout
    appliedRevision = state.revision
    reconciler.reconcile(to: state.layout, delta: state.delta)
    view.needsLayout = true
    completePresentationAfterLayout(state.presentationGeneration)
}
```

Assign `PaneTabStripModel.entries` and `isFocusedGroup` only when the new values differ.

- [ ] **Step 5: End action-to-layout instrumentation at the selected controller**

`ContentView` passes `worktreeSelectionGeneration` only to the selected render state. After reconcile/focus, `WorkspaceViewController` calls `view.layoutSubtreeIfNeeded()` and reports that generation exactly once. `AppModel` forwards generation + worktree ID to `WorktreeSwitchTracker.complete`; stale completions are ignored by Task 1.

Keep the old synchronous interval under the accurate name `worktreeSelectionMutation`; use `worktreeSwitchToLayout` for this action-to-layout interval. Do not label it click-to-present: actual HID-to-present latency and animation hitches are measured with Instruments in Task 6.

- [ ] **Step 6: Run focused tests and commit**

```bash
cd Packages/TillerWorkspace
swift test --filter ReconcilerPerformanceTests
swift test --filter WorkspaceVisibilityTests
cd ../..
xcodebuild test -project Tiller.xcodeproj -scheme Tiller \
  -destination 'platform=macOS' \
  -only-testing:TillerTests/WorktreeSwitchTrackerTests
git add App/Workspace/WorkspaceCoordinator.swift App/ContentView.swift \
  Packages/TillerWorkspace App/Navigation/WorktreeSwitchTracker.swift
git commit -m "perf: skip unchanged workspace reconciliation"
```

---

### Task 4: Build sidebar and activity presentation in one linear pass

**Files:**

- Create: `App/Navigation/SidebarPresentation.swift`
- Create: `AppTests/Navigation/SidebarPresentationTests.swift`
- Modify: `App/Workspace/WorkspaceCoordinator.swift:142-164`
- Modify: `App/Workspace/SidebarTabProjection.swift:16-29`
- Modify: `App/AppModel.swift:140-184, 1303-1318`
- Modify: `App/SidebarView.swift:15-145, 341-461, 500-700`
- Modify: `App/RightPanel/ActivityPanelModel.swift`
- Modify: `App/RightPanel/ActivitySectionView.swift:22-100`
- Modify: `Packages/TillerCore/Sources/TillerCore/AttentionSort.swift:13-45`
- Modify: `Packages/TillerCore/Tests/TillerCoreTests/AttentionSortTests.swift`

**Interfaces:**

- Produces: `WorkspacePresentationIndex` containing tabs, projected rows, active tab ID, live pane IDs, and activity pane IDs for one worktree.
- Produces: immutable `SidebarPresentation`/`SidebarWorktreePresentation` values.
- Removes: row-level observation of the entire `AppModel`; row actions remain closures.

- [ ] **Step 1: Prove status decoration happens once**

```swift
@Test func urgentSortReadsEachStatusExactlyOnce() {
    let rows = (0..<20).map { Row(id: $0) }
    var calls: [Int: Int] = [:]
    _ = AttentionSort.urgentFirst(rows) { row in
        calls[row.id, default: 0] += 1
        return row.id == 4 ? .needsInput : nil
    }
    #expect(calls == Dictionary(uniqueKeysWithValues: rows.map { ($0.id, 1) }))
}
```

- [ ] **Step 2: Prove terminal resolution is linear**

```swift
@Test func onePassIndexResolvesEachTerminalOnce() throws {
    let layout = try makeLayout(tabCount: 40)
    var resolutions = 0
    _ = WorkspacePresentationIndex.build(layout: layout) { _ in
        resolutions += 1
        return UUID()
    }
    #expect(resolutions == 40)
}
```

- [ ] **Step 3: Run both RED suites**

```bash
cd Packages/TillerCore
swift test --filter AttentionSortTests
cd ../..
xcodebuild test -project Tiller.xcodeproj -scheme Tiller \
  -destination 'platform=macOS' \
  -only-testing:TillerTests/SidebarPresentationTests
```

- [ ] **Step 4: Decorate before sorting**

```swift
let decorated = items.enumerated().map { offset, item in
    (offset: offset, item: item, rank: urgencyRank(statusOf(item)))
}
return decorated.sorted {
    $0.rank == $1.rank ? $0.offset < $1.offset : $0.rank < $1.rank
}.map(\.item)
```

Apply the same one-read rule to `sorted(_:statusOf:)`.

- [ ] **Step 5: Replace per-tab reverse scans with one index**

Build `layout.allTabs` once, resolve each terminal generation once, and pass dictionaries into both sidebar and activity projection. `liveControlPaneId(contentID:in:)` may remain for control-plane callers, but render paths must not call it from inside another `allTabs` iteration.

- [ ] **Step 6: Build one immutable sidebar snapshot per body pass**

The snapshot holds all values read by a row:

```swift
struct SidebarWorktreePresentation: Identifiable, Equatable {
    let worktree: Worktree
    let tabs: [LegacyWorkspaceTab]
    let activeTabID: UUID?
    let status: AgentStatus?
    let agentID: String?
    let runningAgentIDs: [String]
    var id: UUID { worktree.id }
}
```

`WorktreeRow`, `TabRow`, and `PaneRow` receive this value plus action closures. Remove their `@Bindable AppModel`; do not read model state from their bodies.

- [ ] **Step 7: Compute activity rows once**

```swift
var body: some View {
    let rows = ActivityPanelModel.rows(appModel: appModel)
    let runningCount = rows.count { $0.status == .running }
    return content(rows: rows, runningCount: runningCount)
}
```

Pass `rows` into header/content builders rather than using a recomputing property.

- [ ] **Step 8: Run focused tests and commit**

```bash
cd Packages/TillerCore
swift test --filter AttentionSortTests
cd ../..
xcodebuild test -project Tiller.xcodeproj -scheme Tiller \
  -destination 'platform=macOS' \
  -only-testing:TillerTests/SidebarPresentationTests \
  -only-testing:TillerTests/ActivityPanelTests \
  -only-testing:TillerTests/SidebarEngineParityTests
git add App/Navigation AppTests/Navigation App/SidebarView.swift \
  App/RightPanel/ActivityPanelModel.swift App/RightPanel/ActivitySectionView.swift \
  App/Workspace/WorkspaceCoordinator.swift App/Workspace/SidebarTabProjection.swift \
  App/AppModel.swift Packages/TillerCore
git commit -m "perf: snapshot navigation presentation state"
```

---

### Task 5: Profile and conditionally move right-panel work out of the switch path

**Files:**

- Modify: `App/RightPanel/RightPanelModel.swift:136-333`
- Modify: `App/ContentView.swift:8-100`
- Modify: `AppTests/RightPanelDirectoryStatusTests.swift`
- Modify: `AppTests/RightPanelPrefetchTests.swift`
- Modify: `docs/superpowers/notes/navigation-performance-baseline.md`

**Interfaces:**

- Produces immediately: `rightPanelActivation` and `rightPanelPrefetch` signposts.
- Conditional implementation: bounded `RightPanelSnapshotCache(capacity: 3)` keyed by worktree ID.
- Decision rule: implement warm snapshots only when right-panel activation contributes at least 10% of `worktreeSwitchToLayout` p95 or its own p95 exceeds 8 ms in the reference workload.

- [ ] **Step 1: Add activation and prefetch intervals without changing behavior**

Instrument activation, root directory load, Git status load, and one-level prefetch separately. Payloads contain only counts and booleans, never paths, branch names, or repository names.

- [ ] **Step 2: Capture 50 switches with the right panel open and 50 closed**

Run the Task 6 benchmark in both modes. Record p50/p95 and overlap in `docs/superpowers/notes/navigation-performance-baseline.md`.

- [ ] **Step 3: Apply the decision rule**

If the gate does not fire, retain current activation behavior and commit only instrumentation plus measurements. If it fires, write the following failing tests before cache code:

```swift
@Test func returningToAWorktreePublishesItsWarmSnapshotBeforeReloadCompletes() async throws {
    let probe = RightPanelProbe(blockingStatusCall: 3)
    let model = makeModel(probe: probe, cacheCapacity: 3)
    await model.activate(worktree: first, isGitRepository: true)
    await model.activate(worktree: second, isGitRepository: true)
    let task = Task { await model.activate(worktree: first, isGitRepository: true) }
    await probe.waitUntilStatusIsBlocked()
    #expect(model.worktree?.id == first.id)
    #expect(!model.childrenByDirectory["", default: []].isEmpty)
    task.cancel()
}

@Test func snapshotCacheEvictsLeastRecentlyUsedEntryAtCapacityThree() {
    var cache = RightPanelSnapshotCache(capacity: 3)
    cache.insert(snapshotA, for: a)
    cache.insert(snapshotB, for: b)
    cache.insert(snapshotC, for: c)
    _ = cache.value(for: a)
    cache.insert(snapshotD, for: d)
    #expect(cache.value(for: b) == nil)
    #expect(cache.value(for: a) == snapshotA)
}
```

- [ ] **Step 4: Implement the bounded warm path only when gated in**

Publish a cached value snapshot synchronously, mark it stale, and refresh in the background. Move one-level prefetch to utility priority and begin it only after root rows have been published and one main-actor yield has completed. Generation checks continue rejecting stale results.

- [ ] **Step 5: Run focused tests and commit the measured outcome**

```bash
xcodebuild test -project Tiller.xcodeproj -scheme Tiller \
  -destination 'platform=macOS' \
  -only-testing:TillerTests/RightPanelDirectoryStatusTests \
  -only-testing:TillerTests/RightPanelPrefetchTests
git add App/RightPanel/RightPanelModel.swift App/ContentView.swift \
  AppTests/RightPanelDirectoryStatusTests.swift AppTests/RightPanelPrefetchTests.swift \
  docs/superpowers/notes/navigation-performance-baseline.md
git commit -m "perf: keep right panel off navigation path"
```

---

### Task 6: Establish the current Release baseline and enforce acceptance gates

**Files:**

- Create: `Scripts/bench-navigation.sh`
- Create: `docs/superpowers/notes/navigation-performance-baseline.md`
- Modify: `Scripts/ci.sh` only if the benchmark script has deterministic parser tests; do not run hardware latency gates in CI.

**Interfaces:**

- Consumes signposts: `worktreeSelectionMutation`, `worktreeSwitchToLayout`, `workspaceReconcile`, `sidebarBody`, `rightPanelActivation`, `rightPanelPrefetch`.
- Produces a stable Markdown/JSON summary with sample count, p50, p95, p99, and maximum.

- [ ] **Step 1: Write parser fixture tests in the shell script**

The script supports `--self-test` with a fixed signpost fixture and verifies percentile calculation, unmatched begin/end rejection, and minimum-sample rejection.

```bash
Scripts/bench-navigation.sh --self-test
```

Expected before implementation: command or option is missing.

- [ ] **Step 2: Implement the repeatable workload**

Reference scenario:

1. Release build at audited HEAD.
2. 10 mounted worktrees and 20 terminal panes total.
3. Sidebar expanded, Activity expanded, right panel tested both open and closed.
4. 60-second idle sample.
5. Four hidden terminals producing deterministic sustained output.
6. 50 alternating worktree switches after five warm-up switches.
7. 50 active-group tab switches in one four-group workspace.
8. Time Profiler + SwiftUI Update Groups + Core Animation/GPU capture.

The script must print the exact commit, hardware model, macOS version, display refresh rate, build configuration, scenario, and sample counts.

- [ ] **Step 3: Record pre-change and post-change runs**

Use at least 50 valid samples per scenario. Store raw capture paths outside git; commit only the summarized table and reproduction command.

- [ ] **Step 4: Apply acceptance gates**

| Metric | Gate |
| --- | --- |
| Idle action-to-layout `worktreeSwitchToLayout` | p95 <= 8 ms; p99 <= 16.7 ms |
| HID-to-present in Instruments | p95 <= 16.7 ms; p99 <= 33.3 ms |
| Four hidden output producers, HID-to-present | p95 <= 33.3 ms; no event > 100 ms |
| `workspaceReconcile` when revision changed | p95 <= 8 ms |
| Unchanged mounted worktrees during switch | zero structural reconcile traversals |
| Hidden universal terminal surfaces | zero active display links/render passes after settle |
| Sidebar status lookup | exactly one lookup per worktree per snapshot |
| Terminal live-ID projection | exactly one generation lookup per terminal tab |
| Hidden-output correctness | first/last sentinel, byte count, and order all exact after reveal |
| Memory after 100 switches | no monotonic growth after warm-up |
| Right panel open vs closed | result documented; Task 5 decision rule applied |

- [ ] **Step 5: Run proactive diagnostics before the full build**

```text
Run `lsp_diagnostics` for every changed Swift file, then `lens_diagnostics` with mode=all. Resolve blocking errors before invoking the repository gate.
```

- [ ] **Step 6: Run the repository gate**

```bash
Scripts/ci.sh
```

Expected: exit code 0 and final line `CI OK`.

- [ ] **Step 7: Run the Release benchmark and commit evidence**

```bash
Scripts/bench-navigation.sh --configuration release --samples 50
git add Scripts/bench-navigation.sh docs/superpowers/notes/navigation-performance-baseline.md
git commit -m "perf: validate navigation frame budget"
```

---

## Rollout Order

1. Task 1 first: establish one correct navigation funnel.
2. Task 2 second: stop hidden universal surfaces from competing for rendering time.
3. Task 3 third: skip unchanged workspace reconcile/layout work and make the end-to-end signpost truthful.
4. Task 4 fourth: remove deterministic repeated sidebar/activity derivation.
5. Run the first full Release comparison.
6. Task 5 only according to its recorded decision rule.
7. Task 6 finalizes evidence and the CI gate.

Tasks 2 and 4 can be developed independently after Task 1, but integrate Task 2 before Task 3 because they share `WorkspaceRenderState`.

## Explicitly Rejected Approaches

- Unmounting hidden worktrees: terminates live agents and violates product semantics.
- Pausing PTY reads: risks backpressure, lost output, stale scrollback, and incorrect agent signals.
- Using opacity as a rendering optimization: the current source already demonstrates that compositing opacity does not occlude Ghostty surfaces.
- Rebuilding workspace controllers on every selection: trades CPU for lifecycle/focus bugs.
- A broad `AppModel` rewrite before measurement: too much risk and no causal evidence. Extract only the selection tracker and presentation builder at clean seams.
- Cross-cutting manual memoization: invalidation would be distributed across dozens of mutations. Use immutable snapshots and existing workspace revisions.
- More animations to mask latency: motion must explain state change, not hide missed frames; reduced-motion behavior must remain correct.

## Completion Checklist

- [ ] All direct worktree selection writes route through `selectWorktree`.
- [ ] Sidebar/keyboard/activity tab navigation updates the universal layout when enabled.
- [ ] Hidden universal terminal hosts receive `setVisible(false)` without runtime teardown.
- [ ] Re-selecting a hidden worktree restores full terminal output and visibility.
- [ ] Unchanged workspace revisions do not reconcile or request layout.
- [ ] Deltas are keyed by worktree.
- [ ] Sidebar/activity projections are linear and built once per render pass.
- [ ] Current-HEAD Release baseline contains at least 50 samples per scenario.
- [ ] Right-panel optimization follows its measured decision rule.
- [ ] `App/TillerApp.swift` user change remains untouched.
- [ ] `Scripts/ci.sh` prints `CI OK`.
