# Pane Drag & Drop Wiring Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make pane tabs actually draggable in the shipping app — reorder inside a pane, move between panes, and create edge splits — by wiring the already-tested `TillerWorkspace` interaction model to the real SwiftUI/AppKit pane chrome.

**Architecture:** Phase 5 of `2026-07-29-multi-pane-workspace.md` produced pure model types (`DragSession`, `DropTargetResolver`, `DividerTracking`, `SplitEligibility`) that no production code path ever calls: the app mounts `App/PaneTabStripBar.swift` (SwiftUI) through `stripFactory`, not the package's `PaneTabStripView`. This plan keeps every existing model type and adds the four missing seams: a hit tester that maps a workspace-root point to a `DropTarget` across *all* panes, tab-frame publishing from the SwiftUI strip back into `PaneTabStripModel`, a drag coordinator that owns the session and the feedback overlay, and a divider commit policy. Two semantic defects in the existing model — source/target confusion and inverted top/bottom edges — are fixed first, because the wiring makes them reachable.

**Tech Stack:** Swift 6, SwiftUI + AppKit (macOS 15+), swift-testing (`@Test`/`#expect`), SwiftPM packages, XcodeGen.

## Global Constraints

- **Coordinate space:** every rectangle and point handed to `DropTargetResolver`, `WorkspaceHitTester`, `DragPreviewGeometry`, or `WorkspaceDragCoordinator` is in **workspace-root flipped coordinates** — origin at the top-left of the workspace root view, `y` growing downward. AppKit views are not flipped by default, so the collector converts. This is the single convention; no function takes "whatever space the caller had".
- **Mutation-free cancellation:** Escape, pointer cancellation, window focus loss, and release outside a legal target must emit no `WorkspaceIntent` at all (contract rule 8).
- **One intent per gesture:** a divider drag emits exactly one `.setPreferredFraction`, on release (contract rule 9).
- **Package boundaries** (enforced by `Scripts/check-module-boundaries.sh` in CI): `TillerWorkspace` never imports app/content/persistence packages; workspace domain files in `TillerCore` import Foundation only. All new package code lives in `TillerWorkspace` and may import `AppKit`, `CoreGraphics`, `TillerCore` only.
- **Tests are swift-testing**, never XCTest. Structs with `@Suite @MainActor` where AppKit types are touched.
- **Value types for models**; classes only for real identity, and then `@MainActor`-isolated.
- **UI strings are English.**
- **Verification gate:** `Scripts/ci.sh` must print `CI OK`. Run it once per phase boundary, not per task (per-task verification is the package-level `swift test --filter`).
- **Commit messages:** Conventional Commits, lower-case imperative subject.
- **Reduced Motion:** the drag overlay never animates topology; it draws the preview directly (contract rule 10). No `NSAnimationContext` in any new code.

## File structure

**Package — `Packages/TillerWorkspace/Sources/TillerWorkspace/`**

| File | Responsibility |
|---|---|
| `DropTargetResolver.swift` (modify) | Resolve a point *inside one group* to a `DropTarget`. Gains an explicit hovered-group parameter; top/bottom fixed. |
| `SplitEligibility.swift` (modify) | Unchanged logic; parameter renamed to say what it actually means. |
| `WorkspaceHitTester.swift` (create) | Pick which group a root-space point is over, then delegate to `DropTargetResolver`. |
| `PaneTabStripModel.swift` (modify) | Carries tab frames published by the view, plus the drag callbacks the chrome invokes. |
| `PaneGroupController.swift` (modify) | Publishes its group's hit frame; forwards strip drag callbacks to the coordinator. |
| `DragPreviewGeometry.swift` (create) | Pure: `DropTarget` + frames → the rectangle to highlight. |
| `WorkspaceDragOverlay.swift` (create) | Transparent `NSView` that paints preview rect and drag ghost. Never hit-tests. |
| `WorkspaceDragCoordinator.swift` (create) | Owns `DragSession`, drives the overlay, emits `WorkspaceIntent`. |
| `WorkspaceReconciler.swift` (modify) | Builds the coordinator, collects hit frames, hosts the overlay. |
| `WorkspaceViewController.swift` (modify) | Exposes the coordinator so the app target can reach it. |
| `DividerCommitPolicy.swift` (create) | Pure: `NSEvent.EventType?` → move / commit / ignore. |
| `WorkspaceSplitController.swift` (modify) | Feeds `DividerTracking` from split-view resize callbacks. |

**App — `App/`**

| File | Responsibility |
|---|---|
| `PaneTabStripBar.swift` (modify) | Replaces `onTapGesture` with a single `DragGesture(minimumDistance: 0)`; publishes tab frames. |

**Tests — `Packages/TillerWorkspace/Tests/TillerWorkspaceTests/`**

`DropTargetResolverTests.swift` (modify), `WorkspaceHitTesterTests.swift`, `DragPreviewGeometryTests.swift`, `WorkspaceDragCoordinatorTests.swift`, `DividerCommitPolicyTests.swift`, `PaneTabStripDragTests.swift` (create).

## Out of scope

`OverflowCanvas` and `PaneTabStripView` stay unwired. The overflow work (CO-1…6) is a separate gap and is not part of the Edge Preview contract. `PaneTabStripView` is now dead: this plan does not delete it (deletion belongs to Phase 14 of the parent plan), but it also does not revive it.

---

# Task 1: Fix the coordinate contract and source/target semantics

The resolver currently takes one `sourceGroup` and uses it both as "the group being dragged from" and "the group under the pointer". That makes rule 7 ("the sole tab of a group cannot edge-split into *its own* group") fire against every one-tab pane, including other panes, which blocks the most common split of all. Separately it places the tab strip at `minY` — top, in flipped space — but computes `.bottom` as the distance from `minY`, so the vertical edges are labelled backwards. No test covers top or bottom, which is why this survived.

**Files:**
- Modify: `Packages/TillerWorkspace/Sources/TillerWorkspace/DropTargetResolver.swift`
- Modify: `Packages/TillerWorkspace/Sources/TillerWorkspace/SplitEligibility.swift:12-16`
- Test: `Packages/TillerWorkspace/Tests/TillerWorkspaceTests/DropTargetResolverTests.swift`

**Interfaces:**
- Consumes: `PaneGroupID`, `WorkspaceTabID`, `SplitPlacementSide` from `TillerCore`; `WorkspaceMetrics.edgeBandFraction`.
- Produces:

```swift
public enum DropTargetResolver {
    /// All geometry is in workspace-root flipped coordinates: the origin is the
    /// top-left of the workspace, `y` grows downward, so the tab strip occupies
    /// the band starting at `groupBounds.minY`.
    public static func resolve(
        pointInGroup: CGPoint,
        groupBounds: CGRect,
        tabStripHeight: CGFloat,
        tabFrames: [CGRect],
        draggedTab: WorkspaceTabID,
        sourceGroup: PaneGroupID,
        hoveredGroup: PaneGroupID,
        groupTabCount: Int
    ) -> DropTarget
}

public enum SplitEligibility {
    public static func check(
        groupSize: CGSize,
        placement: SplitPlacementSide,
        isSplittingItsOwnSoleGroup: Bool
    ) -> Result<Void, Reason>
}
```

- [ ] **Step 1: Write the failing tests**

Replace the body of `DropTargetResolverTests.swift` with the version below. The three pre-existing cases keep their assertions and gain `hoveredGroup:`; four cases are new.

```swift
import CoreGraphics
import Testing
import TillerCore
import TillerWorkspace

@Suite
struct DropTargetResolverTests {
    private let bounds = CGRect(x: 0, y: 0, width: 1_000, height: 500)

    @Test
    func pointerInsideTheOuterTwentyTwoPercentPicksTheNearestEdge() {
        let groupID = PaneGroupID()

        #expect(
            DropTargetResolver.resolve(
                pointInGroup: CGPoint(x: 950, y: 250), groupBounds: bounds,
                tabStripHeight: 40, tabFrames: [], draggedTab: WorkspaceTabID(),
                sourceGroup: PaneGroupID(), hoveredGroup: groupID, groupTabCount: 2
            ) == .edge(groupID, placement: .right)
        )
        #expect(
            DropTargetResolver.resolve(
                pointInGroup: CGPoint(x: 500, y: 250), groupBounds: bounds,
                tabStripHeight: 40, tabFrames: [], draggedTab: WorkspaceTabID(),
                sourceGroup: PaneGroupID(), hoveredGroup: groupID, groupTabCount: 2
            ) == .center(groupID)
        )
    }

    /// In flipped space the tab strip is at the top, so a point just below the
    /// strip is near the *top* edge. Getting this backwards sends a tab to the
    /// opposite half of the pane from the one the preview highlighted.
    @Test
    func theEdgeNearestTheTabStripIsTheTopEdge() {
        let groupID = PaneGroupID()

        #expect(
            DropTargetResolver.resolve(
                pointInGroup: CGPoint(x: 500, y: 60), groupBounds: bounds,
                tabStripHeight: 40, tabFrames: [], draggedTab: WorkspaceTabID(),
                sourceGroup: PaneGroupID(), hoveredGroup: groupID, groupTabCount: 2
            ) == .edge(groupID, placement: .top)
        )
        #expect(
            DropTargetResolver.resolve(
                pointInGroup: CGPoint(x: 500, y: 480), groupBounds: bounds,
                tabStripHeight: 40, tabFrames: [], draggedTab: WorkspaceTabID(),
                sourceGroup: PaneGroupID(), hoveredGroup: groupID, groupTabCount: 2
            ) == .edge(groupID, placement: .bottom)
        )
    }

    @Test
    func pointerOverTheTabStripAlwaysPicksCenterWithAnExactInsertionIndex() {
        let groupID = PaneGroupID()
        let tab = WorkspaceTabID()
        let frames = [
            CGRect(x: 0, y: 0, width: 100, height: 30),
            CGRect(x: 100, y: 0, width: 120, height: 30),
            CGRect(x: 220, y: 0, width: 80, height: 30)
        ]
        let groupBounds = CGRect(x: 0, y: 0, width: 400, height: 300)

        #expect(
            DropTargetResolver.resolve(
                pointInGroup: CGPoint(x: 110, y: 10), groupBounds: groupBounds,
                tabStripHeight: 40, tabFrames: frames, draggedTab: tab,
                sourceGroup: groupID, hoveredGroup: groupID, groupTabCount: 3
            ) == .tabStrip(groupID, insertionIndex: 1)
        )
        #expect(
            DropTargetResolver.resolve(
                pointInGroup: CGPoint(x: 380, y: 10), groupBounds: groupBounds,
                tabStripHeight: 40, tabFrames: frames, draggedTab: tab,
                sourceGroup: groupID, hoveredGroup: groupID, groupTabCount: 3
            ) == .tabStrip(groupID, insertionIndex: 3)
        )
    }

    @Test
    func theSoleTabOfAGroupCannotEdgeSplitIntoItsOwnGroup() {
        let groupID = PaneGroupID()

        #expect(
            DropTargetResolver.resolve(
                pointInGroup: CGPoint(x: 995, y: 250), groupBounds: bounds,
                tabStripHeight: 40, tabFrames: [], draggedTab: WorkspaceTabID(),
                sourceGroup: groupID, hoveredGroup: groupID, groupTabCount: 1
            ) == .center(groupID)
        )
    }

    /// The regression this whole task exists for: a one-tab pane is still a
    /// legal split target for a tab dragged out of a *different* pane.
    @Test
    func aOneTabPaneIsStillASplitTargetForATabFromAnotherPane() {
        let hovered = PaneGroupID()

        #expect(
            DropTargetResolver.resolve(
                pointInGroup: CGPoint(x: 995, y: 250), groupBounds: bounds,
                tabStripHeight: 40, tabFrames: [], draggedTab: WorkspaceTabID(),
                sourceGroup: PaneGroupID(), hoveredGroup: hovered, groupTabCount: 1
            ) == .edge(hovered, placement: .right)
        )
    }

    @Test
    func aPointOutsideTheGroupHasNoTarget() {
        #expect(
            DropTargetResolver.resolve(
                pointInGroup: CGPoint(x: 1_200, y: 250), groupBounds: bounds,
                tabStripHeight: 40, tabFrames: [], draggedTab: WorkspaceTabID(),
                sourceGroup: PaneGroupID(), hoveredGroup: PaneGroupID(), groupTabCount: 2
            ) == .none
        )
    }

    @Test
    func splitIsIneligibleWhenEitherHalfWouldFallBelowTwoFortyPoints() {
        let result = SplitEligibility.check(
            groupSize: CGSize(width: 400, height: 300),
            placement: .right, isSplittingItsOwnSoleGroup: false
        )
        guard case .failure(let reason) = result else {
            Issue.record("a 400-point horizontal group should reject this split")
            return
        }
        #expect(reason == .insufficientWidth(available: 197, required: 240))
    }

    @Test
    func eligibilityReasonsAreExposedForAccessibilityAndControlCallers() {
        let soleTabResult = SplitEligibility.check(
            groupSize: CGSize(width: 600, height: 300),
            placement: .below, isSplittingItsOwnSoleGroup: true
        )
        guard case .failure(let soleTabReason) = soleTabResult else {
            Issue.record("a sole tab should make an edge split ineligible")
            return
        }
        #expect(soleTabReason == .soleTabOfItsOwnGroup)

        let heightResult = SplitEligibility.check(
            groupSize: CGSize(width: 600, height: 300),
            placement: .below, isSplittingItsOwnSoleGroup: false
        )
        guard case .failure(let heightReason) = heightResult else {
            Issue.record("a 300-point vertical group should reject this split")
            return
        }
        #expect(heightReason == .insufficientHeight(available: 147, required: 160))
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd Packages/TillerWorkspace && swift test --filter DropTargetResolverTests`
Expected: compile error — `resolve` has no parameter named `hoveredGroup`, `check` has no parameter named `isSplittingItsOwnSoleGroup`.

- [ ] **Step 3: Write the implementation**

Replace `resolve` in `DropTargetResolver.swift`:

```swift
public enum DropTargetResolver {
    /// All geometry is in workspace-root flipped coordinates: the origin is the
    /// top-left of the workspace and `y` grows downward, so the tab strip
    /// occupies the band starting at `groupBounds.minY` and the edge nearest
    /// the strip is `.top`.
    ///
    /// `sourceGroup` is where the dragged tab came from; `hoveredGroup` is the
    /// group under the pointer. They differ on every cross-pane drag, and
    /// conflating them makes a one-tab pane reject splits it should accept.
    public static func resolve(
        pointInGroup: CGPoint,
        groupBounds: CGRect,
        tabStripHeight: CGFloat,
        tabFrames: [CGRect],
        draggedTab: WorkspaceTabID,
        sourceGroup: PaneGroupID,
        hoveredGroup: PaneGroupID,
        groupTabCount: Int
    ) -> DropTarget {
        _ = draggedTab

        guard groupBounds.contains(pointInGroup) else { return .none }

        let strip = CGRect(
            x: groupBounds.minX,
            y: groupBounds.minY,
            width: groupBounds.width,
            height: min(max(0, tabStripHeight), groupBounds.height)
        )
        if strip.contains(pointInGroup) {
            return .tabStrip(hoveredGroup, insertionIndex: insertionIndex(
                for: pointInGroup.x, in: tabFrames
            ))
        }

        // Rule 7: only the tab's *own* group is barred, and only when taking
        // that tab out would leave the group empty.
        if hoveredGroup == sourceGroup, groupTabCount == 1 {
            return .center(hoveredGroup)
        }

        let horizontalBand = groupBounds.width * WorkspaceMetrics.edgeBandFraction
        let verticalBand = groupBounds.height * WorkspaceMetrics.edgeBandFraction
        let distances: [(EdgePlacement, CGFloat, CGFloat)] = [
            (.left, pointInGroup.x - groupBounds.minX, horizontalBand),
            (.right, groupBounds.maxX - pointInGroup.x, horizontalBand),
            (.top, pointInGroup.y - groupBounds.minY, verticalBand),
            (.bottom, groupBounds.maxY - pointInGroup.y, verticalBand)
        ]
        guard let nearest = distances.min(by: { $0.1 < $1.1 }), nearest.1 <= nearest.2 else {
            return .center(hoveredGroup)
        }
        return .edge(hoveredGroup, placement: nearest.0)
    }

    private static func insertionIndex(for x: CGFloat, in frames: [CGRect]) -> Int {
        for (index, frame) in frames.enumerated() where x < frame.midX {
            return index
        }
        return frames.count
    }
}
```

In `SplitEligibility.swift`, rename the parameter and the guard — the logic is unchanged, the name now states the rule:

```swift
    public static func check(
        groupSize: CGSize,
        placement: SplitPlacementSide,
        isSplittingItsOwnSoleGroup: Bool
    ) -> Result<Void, Reason> {
        if isSplittingItsOwnSoleGroup {
            return .failure(.soleTabOfItsOwnGroup)
        }
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd Packages/TillerWorkspace && swift test --filter DropTargetResolverTests`
Expected: PASS, 7 tests.

- [ ] **Step 5: Commit**

```bash
git add Packages/TillerWorkspace/Sources/TillerWorkspace/DropTargetResolver.swift \
        Packages/TillerWorkspace/Sources/TillerWorkspace/SplitEligibility.swift \
        Packages/TillerWorkspace/Tests/TillerWorkspaceTests/DropTargetResolverTests.swift
git commit -m "fix: separate the hovered pane from the drag source when resolving drops

The resolver used one identifier for both, so a pane holding a single tab
rejected every edge split, not just a split into its own group. The vertical
edges were also labelled against the flipped coordinate space the tab strip
band already assumed.

refs OB-D-4, OB-D-6"
```

---

# Task 2: Hit-test a workspace-root point across every pane

`DropTargetResolver` answers "where inside *this* group", but a drag crosses panes. This adds the layer above it: which group is under the pointer at all.

**Files:**
- Create: `Packages/TillerWorkspace/Sources/TillerWorkspace/WorkspaceHitTester.swift`
- Test: `Packages/TillerWorkspace/Tests/TillerWorkspaceTests/WorkspaceHitTesterTests.swift`

**Interfaces:**
- Consumes: `DropTargetResolver.resolve` (Task 1), `WorkspaceMetrics.tabStripHeight`.
- Produces:

```swift
public struct PaneGroupHitFrame: Equatable, Sendable {
    public let id: PaneGroupID
    /// The whole pane, tab strip included, in workspace-root flipped coordinates.
    public let bounds: CGRect
    /// Tab rectangles in the same space, in visual order.
    public let tabFrames: [CGRect]
    public let tabCount: Int

    public init(id: PaneGroupID, bounds: CGRect, tabFrames: [CGRect], tabCount: Int)
}

public enum WorkspaceHitTester {
    public static func target(
        at point: CGPoint,
        in frames: [PaneGroupHitFrame],
        draggedTab: WorkspaceTabID,
        sourceGroup: PaneGroupID,
        tabStripHeight: CGFloat = WorkspaceMetrics.tabStripHeight
    ) -> DropTarget
}
```

- [ ] **Step 1: Write the failing tests**

Create `WorkspaceHitTesterTests.swift`:

```swift
import CoreGraphics
import Testing
import TillerCore
import TillerWorkspace

@Suite
struct WorkspaceHitTesterTests {
    private let left = PaneGroupID()
    private let right = PaneGroupID()

    /// Two panes side by side, 500 points wide each, 400 tall, 32-point strips.
    private func sideBySide(leftTabs: Int = 2, rightTabs: Int = 2) -> [PaneGroupHitFrame] {
        [
            PaneGroupHitFrame(
                id: left,
                bounds: CGRect(x: 0, y: 0, width: 500, height: 400),
                tabFrames: (0..<leftTabs).map {
                    CGRect(x: CGFloat($0) * 100, y: 0, width: 100, height: 32)
                },
                tabCount: leftTabs
            ),
            PaneGroupHitFrame(
                id: right,
                bounds: CGRect(x: 500, y: 0, width: 500, height: 400),
                tabFrames: (0..<rightTabs).map {
                    CGRect(x: 500 + CGFloat($0) * 100, y: 0, width: 100, height: 32)
                },
                tabCount: rightTabs
            )
        ]
    }

    @Test
    func theTargetIsResolvedInsideWhicheverPaneHoldsThePoint() {
        let target = WorkspaceHitTester.target(
            at: CGPoint(x: 750, y: 200), in: sideBySide(),
            draggedTab: WorkspaceTabID(), sourceGroup: left
        )

        #expect(target == .center(right))
    }

    @Test
    func aPointInTheOtherPanesEdgeBandSplitsThatPane() {
        let target = WorkspaceHitTester.target(
            at: CGPoint(x: 980, y: 200), in: sideBySide(),
            draggedTab: WorkspaceTabID(), sourceGroup: left
        )

        #expect(target == .edge(right, placement: .right))
    }

    @Test
    func aPointOverTheOtherPanesStripInsertsAtThatIndex() {
        let target = WorkspaceHitTester.target(
            at: CGPoint(x: 610, y: 12), in: sideBySide(),
            draggedTab: WorkspaceTabID(), sourceGroup: left
        )

        #expect(target == .tabStrip(right, insertionIndex: 1))
    }

    @Test
    func aPointInNoPaneHasNoTarget() {
        let target = WorkspaceHitTester.target(
            at: CGPoint(x: 40, y: 900), in: sideBySide(),
            draggedTab: WorkspaceTabID(), sourceGroup: left
        )

        #expect(target == .none)
    }

    @Test
    func theSourcePanesOwnSoleTabStillResolvesToItsCenter() {
        let target = WorkspaceHitTester.target(
            at: CGPoint(x: 480, y: 200), in: sideBySide(leftTabs: 1),
            draggedTab: WorkspaceTabID(), sourceGroup: left
        )

        #expect(target == .center(left))
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd Packages/TillerWorkspace && swift test --filter WorkspaceHitTesterTests`
Expected: compile error — `cannot find 'WorkspaceHitTester' in scope`.

- [ ] **Step 3: Write the implementation**

Create `WorkspaceHitTester.swift`:

```swift
import CoreGraphics
import TillerCore

/// One pane's geometry, in workspace-root flipped coordinates, as the drag
/// machinery sees it. The collector builds these from live view frames; tests
/// build them by hand, which is why the drag logic never needs a window.
public struct PaneGroupHitFrame: Equatable, Sendable {
    public let id: PaneGroupID
    public let bounds: CGRect
    public let tabFrames: [CGRect]
    public let tabCount: Int

    public init(id: PaneGroupID, bounds: CGRect, tabFrames: [CGRect], tabCount: Int) {
        self.id = id
        self.bounds = bounds
        self.tabFrames = tabFrames
        self.tabCount = tabCount
    }
}

public enum WorkspaceHitTester {
    /// Panes tile without overlapping, so the first frame containing the point
    /// is the only one that can.
    public static func target(
        at point: CGPoint,
        in frames: [PaneGroupHitFrame],
        draggedTab: WorkspaceTabID,
        sourceGroup: PaneGroupID,
        tabStripHeight: CGFloat = WorkspaceMetrics.tabStripHeight
    ) -> DropTarget {
        guard let frame = frames.first(where: { $0.bounds.contains(point) }) else {
            return .none
        }

        return DropTargetResolver.resolve(
            pointInGroup: point,
            groupBounds: frame.bounds,
            tabStripHeight: tabStripHeight,
            tabFrames: frame.tabFrames,
            draggedTab: draggedTab,
            sourceGroup: sourceGroup,
            hoveredGroup: frame.id,
            groupTabCount: frame.tabCount
        )
    }
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd Packages/TillerWorkspace && swift test --filter WorkspaceHitTesterTests`
Expected: PASS, 5 tests.

- [ ] **Step 5: Commit**

```bash
git add Packages/TillerWorkspace/Sources/TillerWorkspace/WorkspaceHitTester.swift \
        Packages/TillerWorkspace/Tests/TillerWorkspaceTests/WorkspaceHitTesterTests.swift
git commit -m "feat: hit-test drop targets across every pane in the workspace

refs OB-D-2"
```

---

# Task 3: Publish tab frames and drag callbacks through the strip model

The tab rectangles only exist inside the SwiftUI strip; the drag machinery lives in the package. `PaneTabStripModel` is already the bridge between them, so it carries the frames up and the drag callbacks down.

**Files:**
- Modify: `Packages/TillerWorkspace/Sources/TillerWorkspace/PaneTabStripModel.swift`
- Test: `Packages/TillerWorkspace/Tests/TillerWorkspaceTests/PaneTabStripDragTests.swift`

**Interfaces:**
- Consumes: `WorkspaceTabID` from `TillerCore`.
- Produces (added to `PaneTabStripModel`):

```swift
/// Tab rectangles in the strip view's own coordinate space, top-left origin.
public internal(set) var tabFrames: [WorkspaceTabID: CGRect] { get }
public func setTabFrame(_ frame: CGRect, for tab: WorkspaceTabID)
public func removeTabFrames(notIn tabs: Set<WorkspaceTabID>)
/// Frames in `entries` order — the order `PaneGroupHitFrame.tabFrames` needs.
public var orderedTabFrames: [CGRect] { get }

public var onDragChanged: (WorkspaceTabID, CGPoint) -> Void
public var onDragEnded: (WorkspaceTabID) -> Void
public var onDragCancelled: () -> Void
```

`onDragChanged` receives screen coordinates: SwiftUI's local drag translation cannot address other panes, and `NSEvent.mouseLocation` is the one space both worlds agree on.

- [ ] **Step 1: Write the failing tests**

Create `PaneTabStripDragTests.swift`:

```swift
import CoreGraphics
import Testing
import TillerCore
import TillerWorkspace

@Suite @MainActor
struct PaneTabStripDragTests {
    @Test
    func orderedFramesFollowTheEntryOrderNotInsertionOrder() {
        let model = PaneTabStripModel()
        let first = WorkspaceTabID()
        let second = WorkspaceTabID()
        model.entries = [
            TabMenuEntry(tabID: first, title: "one", isActive: true),
            TabMenuEntry(tabID: second, title: "two", isActive: false)
        ]

        model.setTabFrame(CGRect(x: 100, y: 0, width: 100, height: 32), for: second)
        model.setTabFrame(CGRect(x: 0, y: 0, width: 100, height: 32), for: first)

        #expect(model.orderedTabFrames == [
            CGRect(x: 0, y: 0, width: 100, height: 32),
            CGRect(x: 100, y: 0, width: 100, height: 32)
        ])
    }

    @Test
    func aTabWithNoMeasuredFrameIsSkippedRatherThanFakedAtZero() {
        let model = PaneTabStripModel()
        let measured = WorkspaceTabID()
        let unmeasured = WorkspaceTabID()
        model.entries = [
            TabMenuEntry(tabID: measured, title: "one", isActive: true),
            TabMenuEntry(tabID: unmeasured, title: "two", isActive: false)
        ]
        model.setTabFrame(CGRect(x: 0, y: 0, width: 100, height: 32), for: measured)

        #expect(model.orderedTabFrames == [CGRect(x: 0, y: 0, width: 100, height: 32)])
    }

    @Test
    func framesForClosedTabsAreDropped() {
        let model = PaneTabStripModel()
        let kept = WorkspaceTabID()
        let closed = WorkspaceTabID()
        model.setTabFrame(CGRect(x: 0, y: 0, width: 100, height: 32), for: kept)
        model.setTabFrame(CGRect(x: 100, y: 0, width: 100, height: 32), for: closed)

        model.removeTabFrames(notIn: [kept])

        #expect(model.tabFrames.keys.sorted { $0.rawValue.uuidString < $1.rawValue.uuidString }
                == [kept])
    }
}
```

**Note on the last assertion:** `WorkspaceTabID` wraps a `UUID` in `rawValue`. If the concrete shape differs, assert `model.tabFrames.count == 1 && model.tabFrames[kept] != nil` instead — the behaviour under test is the eviction, not the key ordering.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd Packages/TillerWorkspace && swift test --filter PaneTabStripDragTests`
Expected: compile error — `value of type 'PaneTabStripModel' has no member 'setTabFrame'`.

- [ ] **Step 3: Write the implementation**

Add to `PaneTabStripModel`, keeping the existing `entries`, `onActivate`, `onClose`, `onNewTab` untouched:

```swift
    public private(set) var tabFrames: [WorkspaceTabID: CGRect] = [:]

    /// Called continuously while a tab is pressed. The point is in screen
    /// coordinates: a SwiftUI drag's local translation cannot address a pane it
    /// does not belong to, and screen space is what both worlds share.
    public var onDragChanged: (WorkspaceTabID, CGPoint) -> Void = { _, _ in }
    public var onDragEnded: (WorkspaceTabID) -> Void = { _ in }
    public var onDragCancelled: () -> Void = {}

    public func setTabFrame(_ frame: CGRect, for tab: WorkspaceTabID) {
        tabFrames[tab] = frame
    }

    public func removeTabFrames(notIn tabs: Set<WorkspaceTabID>) {
        tabFrames = tabFrames.filter { tabs.contains($0.key) }
    }

    /// Only measured tabs appear. A missing frame means the view has not laid
    /// that tab out yet; inventing a zero rectangle would put a phantom
    /// insertion point at the left edge of the strip.
    public var orderedTabFrames: [CGRect] {
        entries.compactMap { tabFrames[$0.tabID] }
    }
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd Packages/TillerWorkspace && swift test --filter PaneTabStripDragTests`
Expected: PASS, 3 tests.

- [ ] **Step 5: Commit**

```bash
git add Packages/TillerWorkspace/Sources/TillerWorkspace/PaneTabStripModel.swift \
        Packages/TillerWorkspace/Tests/TillerWorkspaceTests/PaneTabStripDragTests.swift
git commit -m "feat: carry tab frames and drag callbacks through the strip model

refs OB-D-1, OB-D-3"
```

---

# Task 4: Compute the preview rectangle for a drop target

What the overlay paints is pure geometry, so it is decided and tested without any view.

**Files:**
- Create: `Packages/TillerWorkspace/Sources/TillerWorkspace/DragPreviewGeometry.swift`
- Test: `Packages/TillerWorkspace/Tests/TillerWorkspaceTests/DragPreviewGeometryTests.swift`

**Interfaces:**
- Consumes: `DropTarget` (Task 1), `PaneGroupHitFrame` (Task 2), `WorkspaceMetrics.tabStripHeight`.
- Produces:

```swift
public enum DragPreviewGeometry {
    /// The region the drop would occupy, in workspace-root flipped coordinates,
    /// or nil when the target is not a legal drop.
    public static func previewRect(
        for target: DropTarget,
        in frames: [PaneGroupHitFrame],
        tabStripHeight: CGFloat = WorkspaceMetrics.tabStripHeight
    ) -> CGRect?

    /// The insertion caret drawn between two tabs.
    public static let insertionCaretWidth: CGFloat = 2
}
```

- [ ] **Step 1: Write the failing tests**

Create `DragPreviewGeometryTests.swift`:

```swift
import CoreGraphics
import Testing
import TillerCore
import TillerWorkspace

@Suite
struct DragPreviewGeometryTests {
    private let group = PaneGroupID()

    private var frames: [PaneGroupHitFrame] {
        [PaneGroupHitFrame(
            id: group,
            bounds: CGRect(x: 100, y: 0, width: 400, height: 300),
            tabFrames: [
                CGRect(x: 100, y: 0, width: 100, height: 32),
                CGRect(x: 200, y: 0, width: 100, height: 32)
            ],
            tabCount: 2
        )]
    }

    @Test
    func aCenterDropPreviewsTheContentAreaBelowTheStrip() {
        #expect(
            DragPreviewGeometry.previewRect(for: .center(group), in: frames)
                == CGRect(x: 100, y: 32, width: 400, height: 268)
        )
    }

    @Test
    func anEdgeDropPreviewsTheHalfItWouldCreate() {
        #expect(
            DragPreviewGeometry.previewRect(for: .edge(group, placement: .right), in: frames)
                == CGRect(x: 300, y: 32, width: 200, height: 268)
        )
        #expect(
            DragPreviewGeometry.previewRect(for: .edge(group, placement: .top), in: frames)
                == CGRect(x: 100, y: 32, width: 400, height: 134)
        )
        #expect(
            DragPreviewGeometry.previewRect(for: .edge(group, placement: .bottom), in: frames)
                == CGRect(x: 100, y: 166, width: 400, height: 134)
        )
    }

    @Test
    func aStripDropPreviewsACaretAtTheInsertionPoint() {
        #expect(
            DragPreviewGeometry.previewRect(for: .tabStrip(group, insertionIndex: 1), in: frames)
                == CGRect(x: 199, y: 0, width: 2, height: 32)
        )
    }

    @Test
    func theCaretForTheLastPositionSitsAfterTheFinalTab() {
        #expect(
            DragPreviewGeometry.previewRect(for: .tabStrip(group, insertionIndex: 2), in: frames)
                == CGRect(x: 299, y: 0, width: 2, height: 32)
        )
    }

    @Test
    func noTargetAndUnknownGroupsPreviewNothing() {
        #expect(DragPreviewGeometry.previewRect(for: .none, in: frames) == nil)
        #expect(DragPreviewGeometry.previewRect(for: .center(PaneGroupID()), in: frames) == nil)
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd Packages/TillerWorkspace && swift test --filter DragPreviewGeometryTests`
Expected: compile error — `cannot find 'DragPreviewGeometry' in scope`.

- [ ] **Step 3: Write the implementation**

Create `DragPreviewGeometry.swift`:

```swift
import CoreGraphics
import TillerCore

public enum DragPreviewGeometry {
    public static let insertionCaretWidth: CGFloat = 2

    public static func previewRect(
        for target: DropTarget,
        in frames: [PaneGroupHitFrame],
        tabStripHeight: CGFloat = WorkspaceMetrics.tabStripHeight
    ) -> CGRect? {
        switch target {
        case .none:
            return nil

        case .center(let id):
            guard let frame = frames.first(where: { $0.id == id }) else { return nil }
            return content(of: frame, tabStripHeight: tabStripHeight)

        case .edge(let id, let placement):
            guard let frame = frames.first(where: { $0.id == id }) else { return nil }
            return half(of: content(of: frame, tabStripHeight: tabStripHeight), on: placement)

        case .tabStrip(let id, let insertionIndex):
            guard let frame = frames.first(where: { $0.id == id }) else { return nil }
            return caret(at: insertionIndex, in: frame, tabStripHeight: tabStripHeight)
        }
    }

    /// The strip is chrome, not a drop region: previewing over it would cover
    /// the very tabs the caret is meant to sit between.
    private static func content(of frame: PaneGroupHitFrame, tabStripHeight: CGFloat) -> CGRect {
        let strip = min(max(0, tabStripHeight), frame.bounds.height)
        return CGRect(
            x: frame.bounds.minX,
            y: frame.bounds.minY + strip,
            width: frame.bounds.width,
            height: frame.bounds.height - strip
        )
    }

    private static func half(of rect: CGRect, on placement: EdgePlacement) -> CGRect {
        switch placement {
        case .left:
            return CGRect(x: rect.minX, y: rect.minY, width: rect.width / 2, height: rect.height)
        case .right:
            return CGRect(x: rect.midX, y: rect.minY, width: rect.width / 2, height: rect.height)
        case .top:
            return CGRect(x: rect.minX, y: rect.minY, width: rect.width, height: rect.height / 2)
        case .bottom:
            return CGRect(x: rect.minX, y: rect.midY, width: rect.width, height: rect.height / 2)
        }
    }

    private static func caret(
        at index: Int,
        in frame: PaneGroupHitFrame,
        tabStripHeight: CGFloat
    ) -> CGRect {
        let strip = min(max(0, tabStripHeight), frame.bounds.height)
        let x: CGFloat
        if index <= 0 {
            x = frame.tabFrames.first?.minX ?? frame.bounds.minX
        } else if index >= frame.tabFrames.count {
            x = frame.tabFrames.last?.maxX ?? frame.bounds.minX
        } else {
            x = frame.tabFrames[index].minX
        }
        return CGRect(
            x: x - insertionCaretWidth / 2,
            y: frame.bounds.minY,
            width: insertionCaretWidth,
            height: strip
        )
    }
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd Packages/TillerWorkspace && swift test --filter DragPreviewGeometryTests`
Expected: PASS, 5 tests.

- [ ] **Step 5: Commit**

```bash
git add Packages/TillerWorkspace/Sources/TillerWorkspace/DragPreviewGeometry.swift \
        Packages/TillerWorkspace/Tests/TillerWorkspaceTests/DragPreviewGeometryTests.swift
git commit -m "feat: compute the edge-preview rectangle for a drop target

refs OB-D-4, OB-D-5"
```

---

# Task 5: The drag overlay view

A transparent sibling of the pane tree that paints feedback and never intercepts a click. It is deliberately dumb: rectangles in, drawing out.

**Files:**
- Create: `Packages/TillerWorkspace/Sources/TillerWorkspace/WorkspaceDragOverlay.swift`
- Test: `Packages/TillerWorkspace/Tests/TillerWorkspaceTests/WorkspaceDragOverlayTests.swift`

**Interfaces:**
- Consumes: `DragPreviewGeometry.insertionCaretWidth`.
- Produces:

```swift
@MainActor
public final class WorkspaceDragOverlay: NSView {
    public var previewRect: CGRect?      // workspace-root flipped coordinates
    public var ghostRect: CGRect?
    public var ghostTitle: String
    public func clear()
    public var isShowingFeedback: Bool { get }
}
```

- [ ] **Step 1: Write the failing tests**

Create `WorkspaceDragOverlayTests.swift`:

```swift
import AppKit
import Testing
import TillerWorkspace

@Suite @MainActor
struct WorkspaceDragOverlayTests {
    @Test
    func theOverlayNeverSwallowsAClick() {
        let overlay = WorkspaceDragOverlay(frame: CGRect(x: 0, y: 0, width: 400, height: 300))
        overlay.previewRect = CGRect(x: 0, y: 0, width: 200, height: 300)

        #expect(overlay.hitTest(CGPoint(x: 100, y: 100)) == nil)
    }

    @Test
    func theOverlayIsFlippedSoRectanglesNeedNoConversion() {
        #expect(WorkspaceDragOverlay(frame: .zero).isFlipped)
    }

    @Test
    func clearingRemovesEveryPieceOfFeedback() {
        let overlay = WorkspaceDragOverlay(frame: CGRect(x: 0, y: 0, width: 400, height: 300))
        overlay.previewRect = CGRect(x: 0, y: 0, width: 200, height: 300)
        overlay.ghostRect = CGRect(x: 10, y: 10, width: 100, height: 32)
        overlay.ghostTitle = "agent"
        #expect(overlay.isShowingFeedback)

        overlay.clear()

        #expect(overlay.previewRect == nil)
        #expect(overlay.ghostRect == nil)
        #expect(overlay.ghostTitle.isEmpty)
        #expect(!overlay.isShowingFeedback)
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd Packages/TillerWorkspace && swift test --filter WorkspaceDragOverlayTests`
Expected: compile error — `cannot find 'WorkspaceDragOverlay' in scope`.

- [ ] **Step 3: Write the implementation**

Create `WorkspaceDragOverlay.swift`:

```swift
import AppKit

/// Paints drag feedback above the pane tree. Flipped, so it shares the drag
/// machinery's coordinate space, and transparent to hit-testing, so the panes
/// underneath keep receiving events during and after a drag.
@MainActor
public final class WorkspaceDragOverlay: NSView {
    public var previewRect: CGRect? {
        didSet { needsDisplay = true }
    }

    public var ghostRect: CGRect? {
        didSet { needsDisplay = true }
    }

    public var ghostTitle: String = "" {
        didSet { needsDisplay = true }
    }

    public override var isFlipped: Bool { true }

    public var isShowingFeedback: Bool {
        previewRect != nil || ghostRect != nil
    }

    public func clear() {
        previewRect = nil
        ghostRect = nil
        ghostTitle = ""
    }

    public override func hitTest(_ point: NSPoint) -> NSView? { nil }

    public override func draw(_ dirtyRect: NSRect) {
        if let previewRect {
            NSColor.controlAccentColor.withAlphaComponent(0.18).setFill()
            previewRect.fill()
            NSColor.controlAccentColor.setStroke()
            let border = NSBezierPath(rect: previewRect.insetBy(dx: 0.5, dy: 0.5))
            border.lineWidth = 1
            border.stroke()
        }

        guard let ghostRect else { return }
        NSColor.windowBackgroundColor.withAlphaComponent(0.9).setFill()
        let ghost = NSBezierPath(roundedRect: ghostRect, xRadius: 7, yRadius: 7)
        ghost.fill()
        NSColor.separatorColor.setStroke()
        ghost.lineWidth = 1
        ghost.stroke()

        guard !ghostTitle.isEmpty else { return }
        let attributes: [NSAttributedString.Key: Any] = [
            .font: NSFont.systemFont(ofSize: 12),
            .foregroundColor: NSColor.labelColor
        ]
        let size = ghostTitle.size(withAttributes: attributes)
        let origin = CGPoint(
            x: ghostRect.minX + 9,
            y: ghostRect.midY - size.height / 2
        )
        ghostTitle.draw(at: origin, withAttributes: attributes)
    }
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd Packages/TillerWorkspace && swift test --filter WorkspaceDragOverlayTests`
Expected: PASS, 3 tests.

- [ ] **Step 5: Commit**

```bash
git add Packages/TillerWorkspace/Sources/TillerWorkspace/WorkspaceDragOverlay.swift \
        Packages/TillerWorkspace/Tests/TillerWorkspaceTests/WorkspaceDragOverlayTests.swift
git commit -m "feat: paint edge-preview drag feedback in a click-through overlay

refs OB-D-4, AX-13"
```

---

# Task 6: The drag coordinator

Owns the one `DragSession`, converts pointer positions, drives the overlay, and emits exactly one intent per legal drop. Everything it needs from the outside arrives as a closure, so it is fully testable without a window.

**Files:**
- Create: `Packages/TillerWorkspace/Sources/TillerWorkspace/WorkspaceDragCoordinator.swift`
- Test: `Packages/TillerWorkspace/Tests/TillerWorkspaceTests/WorkspaceDragCoordinatorTests.swift`

**Interfaces:**
- Consumes: `DragSession` (unchanged), `WorkspaceHitTester.target` (Task 2), `DragPreviewGeometry.previewRect` (Task 4), `WorkspaceDragOverlay` (Task 5), `WorkspaceIntentSink`.
- Produces:

```swift
@MainActor
public final class WorkspaceDragCoordinator {
    public init(
        sink: WorkspaceIntentSink?,
        frames: @escaping @MainActor () -> [PaneGroupHitFrame],
        overlay: @escaping @MainActor () -> WorkspaceDragOverlay?,
        convertToRoot: @escaping @MainActor (CGPoint) -> CGPoint = { $0 }
    )

    public private(set) var isDragging: Bool
    public func pressBegan(tab: WorkspaceTabID, in group: PaneGroupID, atScreenPoint: CGPoint, tabFrame: CGRect, title: String)
    public func pointerMoved(toScreenPoint: CGPoint)
    public func released()
    public func cancel(reason: DragSession.CancelReason)
}
```

`convertToRoot` maps a screen point into workspace-root flipped coordinates. Its default is the identity so tests can speak root coordinates directly; the reconciler supplies the real conversion in Task 7.

- [ ] **Step 1: Write the failing tests**

Create `WorkspaceDragCoordinatorTests.swift`:

```swift
import AppKit
import Testing
import TillerCore
import TillerWorkspace

@MainActor
private final class RecordingSink: WorkspaceIntentSink {
    var intents: [WorkspaceIntent] = []
    func send(_ intent: WorkspaceIntent) { intents.append(intent) }
}

@Suite @MainActor
struct WorkspaceDragCoordinatorTests {
    private let left = PaneGroupID()
    private let right = PaneGroupID()
    private let tab = WorkspaceTabID()

    private func frames() -> [PaneGroupHitFrame] {
        [
            PaneGroupHitFrame(
                id: left, bounds: CGRect(x: 0, y: 0, width: 500, height: 400),
                tabFrames: [CGRect(x: 0, y: 0, width: 100, height: 32)], tabCount: 2
            ),
            PaneGroupHitFrame(
                id: right, bounds: CGRect(x: 500, y: 0, width: 500, height: 400),
                tabFrames: [CGRect(x: 500, y: 0, width: 100, height: 32)], tabCount: 2
            )
        ]
    }

    private func makeCoordinator(
        _ sink: RecordingSink,
        overlay: WorkspaceDragOverlay? = nil
    ) -> WorkspaceDragCoordinator {
        WorkspaceDragCoordinator(
            sink: sink, frames: { self.frames() }, overlay: { overlay }
        )
    }

    private func press(_ coordinator: WorkspaceDragCoordinator) {
        coordinator.pressBegan(
            tab: tab, in: left, atScreenPoint: CGPoint(x: 40, y: 16),
            tabFrame: CGRect(x: 0, y: 0, width: 100, height: 32), title: "agent"
        )
    }

    @Test
    func aPressBelowTheThresholdActivatesTheTabInsteadOfMovingIt() {
        let sink = RecordingSink()
        let coordinator = makeCoordinator(sink)

        press(coordinator)
        coordinator.pointerMoved(toScreenPoint: CGPoint(x: 42, y: 16))
        coordinator.released()

        #expect(sink.intents == [.activateTab(tab)])
        #expect(!coordinator.isDragging)
    }

    @Test
    func draggingIntoAnotherPanesEdgeRequestsThatSplit() {
        let sink = RecordingSink()
        let coordinator = makeCoordinator(sink)

        press(coordinator)
        coordinator.pointerMoved(toScreenPoint: CGPoint(x: 980, y: 200))
        coordinator.released()

        #expect(sink.intents == [
            .requestMove(tab, to: .edgeSplit(anchor: right, placement: .right))
        ])
    }

    @Test
    func draggingOntoAnotherPanesStripRequestsAMoveAtThatIndex() {
        let sink = RecordingSink()
        let coordinator = makeCoordinator(sink)

        press(coordinator)
        coordinator.pointerMoved(toScreenPoint: CGPoint(x: 590, y: 12))
        coordinator.released()

        #expect(sink.intents == [.requestMove(tab, to: .group(right, index: 1))])
    }

    @Test
    func escapeLeavesTheLayoutUntouchedAndClearsTheOverlay() {
        let sink = RecordingSink()
        let overlay = WorkspaceDragOverlay(frame: CGRect(x: 0, y: 0, width: 1_000, height: 400))
        let coordinator = makeCoordinator(sink, overlay: overlay)

        press(coordinator)
        coordinator.pointerMoved(toScreenPoint: CGPoint(x: 980, y: 200))
        #expect(overlay.isShowingFeedback)

        coordinator.cancel(reason: .escape)
        coordinator.released()

        #expect(sink.intents.isEmpty)
        #expect(!overlay.isShowingFeedback)
    }

    @Test
    func releasingOutsideEveryPaneEmitsNothing() {
        let sink = RecordingSink()
        let coordinator = makeCoordinator(sink)

        press(coordinator)
        coordinator.pointerMoved(toScreenPoint: CGPoint(x: 40, y: 900))
        coordinator.released()

        #expect(sink.intents.isEmpty)
    }

    @Test
    func theOverlayShowsThePreviewAndTheGhostWhileDragging() {
        let sink = RecordingSink()
        let overlay = WorkspaceDragOverlay(frame: CGRect(x: 0, y: 0, width: 1_000, height: 400))
        let coordinator = makeCoordinator(sink, overlay: overlay)

        press(coordinator)
        coordinator.pointerMoved(toScreenPoint: CGPoint(x: 700, y: 200))

        #expect(overlay.previewRect == CGRect(x: 500, y: 32, width: 500, height: 368))
        // The ghost keeps the grab offset: pressed 40 points into a tab that
        // started at x = 0, so the ghost's left edge trails 40 points behind.
        #expect(overlay.ghostRect == CGRect(x: 660, y: 184, width: 100, height: 32))
        #expect(overlay.ghostTitle == "agent")
    }

    @Test
    func aSecondReleaseAfterACompletedDragEmitsNothingMore() {
        let sink = RecordingSink()
        let coordinator = makeCoordinator(sink)

        press(coordinator)
        coordinator.pointerMoved(toScreenPoint: CGPoint(x: 980, y: 200))
        coordinator.released()
        coordinator.released()

        #expect(sink.intents.count == 1)
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd Packages/TillerWorkspace && swift test --filter WorkspaceDragCoordinatorTests`
Expected: compile error — `cannot find 'WorkspaceDragCoordinator' in scope`.

- [ ] **Step 3: Write the implementation**

Create `WorkspaceDragCoordinator.swift`:

```swift
import AppKit
import TillerCore

/// Drives one tab drag: it owns the session, keeps the overlay in step with the
/// resolved target, and forwards the single resulting intent. Every outside
/// dependency arrives as a closure so the whole gesture is testable without a
/// window or a live view tree.
@MainActor
public final class WorkspaceDragCoordinator {
    private let session = DragSession()
    private weak var sink: WorkspaceIntentSink?
    private let frames: @MainActor () -> [PaneGroupHitFrame]
    private let overlay: @MainActor () -> WorkspaceDragOverlay?
    private let convertToRoot: @MainActor (CGPoint) -> CGPoint

    private var tab: WorkspaceTabID?
    private var sourceGroup: PaneGroupID?
    private var tabSize = CGSize.zero
    private var title = ""

    public init(
        sink: WorkspaceIntentSink?,
        frames: @escaping @MainActor () -> [PaneGroupHitFrame],
        overlay: @escaping @MainActor () -> WorkspaceDragOverlay?,
        convertToRoot: @escaping @MainActor (CGPoint) -> CGPoint = { $0 }
    ) {
        self.sink = sink
        self.frames = frames
        self.overlay = overlay
        self.convertToRoot = convertToRoot
    }

    public var isDragging: Bool { session.isActive }

    public func pressBegan(
        tab: WorkspaceTabID,
        in group: PaneGroupID,
        atScreenPoint screenPoint: CGPoint,
        tabFrame: CGRect,
        title: String
    ) {
        self.tab = tab
        sourceGroup = group
        tabSize = tabFrame.size
        self.title = title
        session.pressBegan(
            tab: tab, at: convertToRoot(screenPoint), inTabFrame: tabFrame
        )
    }

    public func pointerMoved(toScreenPoint screenPoint: CGPoint) {
        guard let tab, let sourceGroup else { return }
        let point = convertToRoot(screenPoint)
        let snapshot = frames()

        session.pointerMoved(to: point) { candidate in
            WorkspaceHitTester.target(
                at: candidate, in: snapshot, draggedTab: tab, sourceGroup: sourceGroup
            )
        }

        guard session.isActive, let overlay = overlay() else { return }
        overlay.previewRect = DragPreviewGeometry.previewRect(
            for: session.currentTarget, in: snapshot
        )
        overlay.ghostRect = CGRect(
            origin: CGPoint(
                x: point.x - session.grabOffset.x,
                y: point.y - session.grabOffset.y
            ),
            size: tabSize
        )
        overlay.ghostTitle = title
    }

    public func released() {
        defer { finish() }
        guard let intent = session.release() else { return }
        sink?.send(intent)
    }

    public func cancel(reason: DragSession.CancelReason) {
        session.cancel(reason: reason)
        overlay()?.clear()
    }

    private func finish() {
        overlay()?.clear()
        tab = nil
        sourceGroup = nil
        tabSize = .zero
        title = ""
    }
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd Packages/TillerWorkspace && swift test --filter WorkspaceDragCoordinatorTests`
Expected: PASS, 7 tests.

- [ ] **Step 5: Commit**

```bash
git add Packages/TillerWorkspace/Sources/TillerWorkspace/WorkspaceDragCoordinator.swift \
        Packages/TillerWorkspace/Tests/TillerWorkspaceTests/WorkspaceDragCoordinatorTests.swift
git commit -m "feat: drive tab drags from one coordinator over the pane tree

refs OB-D-1, OB-D-2, OB-D-5, OB-D-7"
```

---

# Task 7: Mount the overlay and collect live pane frames

Everything so far runs on hand-built rectangles. This is the seam where real view geometry enters, and the one place the flipped-coordinate conversion happens.

**Files:**
- Modify: `Packages/TillerWorkspace/Sources/TillerWorkspace/PaneGroupController.swift`
- Modify: `Packages/TillerWorkspace/Sources/TillerWorkspace/WorkspaceReconciler.swift`
- Modify: `Packages/TillerWorkspace/Sources/TillerWorkspace/WorkspaceViewController.swift`
- Test: `Packages/TillerWorkspace/Tests/TillerWorkspaceTests/WorkspaceHitFrameCollectorTests.swift`

**Interfaces:**
- Consumes: `PaneGroupHitFrame` (Task 2), `WorkspaceDragOverlay` (Task 5), `WorkspaceDragCoordinator` (Task 6), `PaneTabStripModel.orderedTabFrames` (Task 3).
- Produces:

```swift
extension PaneGroupController {
    /// This group's geometry in `space`'s coordinates, already flipped.
    public func hitFrame(in space: NSView) -> PaneGroupHitFrame?
}

extension WorkspaceReconciler {
    public var dragCoordinator: WorkspaceDragCoordinator { get }
    public func hitFrames() -> [PaneGroupHitFrame]
}

extension WorkspaceViewController {
    public var dragCoordinator: WorkspaceDragCoordinator { get }
}
```

- [ ] **Step 1: Write the failing test**

Create `WorkspaceHitFrameCollectorTests.swift`:

```swift
import AppKit
import Testing
import TillerCore
import TillerWorkspace

@Suite @MainActor
struct WorkspaceHitFrameCollectorTests {
    /// AppKit views are not flipped, so a pane sitting at the top of a 400-point
    /// root has `frame.minY == 200`, not 0. The collector must report 0 — every
    /// consumer downstream assumes a top-left origin.
    @Test
    func aPaneAtTheTopOfTheRootReportsAZeroTopEdge() {
        let root = NSView(frame: CGRect(x: 0, y: 0, width: 600, height: 400))
        let group = PaneGroupController(id: PaneGroupID())
        group.view.frame = CGRect(x: 0, y: 200, width: 600, height: 200)
        root.addSubview(group.view)

        let frame = group.hitFrame(in: root)

        #expect(frame?.bounds == CGRect(x: 0, y: 0, width: 600, height: 200))
    }

    @Test
    func aPaneOutsideAnyViewTreeReportsNothing() {
        let root = NSView(frame: CGRect(x: 0, y: 0, width: 600, height: 400))
        let group = PaneGroupController(id: PaneGroupID())

        #expect(group.hitFrame(in: root) == nil)
    }
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cd Packages/TillerWorkspace && swift test --filter WorkspaceHitFrameCollectorTests`
Expected: compile error — `value of type 'PaneGroupController' has no member 'hitFrame'`.

- [ ] **Step 3: Write the implementation**

Add to `PaneGroupController` (it already holds `stripModel`, so it can supply the tab rectangles):

```swift
    /// This group's geometry in `space`'s coordinates with a top-left origin.
    /// AppKit's default is bottom-left; the flip happens here, once, so nothing
    /// downstream has to think about it.
    public func hitFrame(in space: NSView) -> PaneGroupHitFrame? {
        guard isViewLoaded, view.superview != nil else { return nil }
        let converted = view.convert(view.bounds, to: space)
        let flipped = CGRect(
            x: converted.minX,
            y: space.bounds.height - converted.maxY,
            width: converted.width,
            height: converted.height
        )
        let tabFrames = stripModel.orderedTabFrames.map { frame in
            frame.offsetBy(dx: flipped.minX, dy: flipped.minY)
        }
        return PaneGroupHitFrame(
            id: id, bounds: flipped, tabFrames: tabFrames, tabCount: tabEntries.count
        )
    }
```

Note: `stripModel` is currently `private let`. Change it to `private let` → keep it private and add the accessor above inside the same file, so no new API leaks.

In `WorkspaceReconciler`, build the coordinator and host the overlay. Add the stored properties:

```swift
    public private(set) lazy var dragCoordinator = WorkspaceDragCoordinator(
        sink: intentSink,
        frames: { [weak self] in self?.hitFrames() ?? [] },
        overlay: { [weak self] in self?.overlay },
        convertToRoot: { [weak self] screenPoint in
            guard let self, let root = self.rootViewController.viewIfLoaded,
                  let window = root.window else { return screenPoint }
            let inWindow = window.convertPoint(fromScreen: screenPoint)
            let inRoot = root.convert(inWindow, from: nil)
            return CGPoint(x: inRoot.x, y: root.bounds.height - inRoot.y)
        }
    )

    private let overlay = WorkspaceDragOverlay(frame: .zero)

    public func hitFrames() -> [PaneGroupHitFrame] {
        guard let root = rootViewController.viewIfLoaded else { return [] }
        return groupControllers.values.compactMap { $0.hitFrame(in: root) }
    }
```

Mount the overlay above the pane tree in `WorkspaceRootController.setContent`, so every reconcile keeps it on top:

```swift
    func setContent(_ child: NSViewController) {
        children.forEach { $0.removeFromParent() }
        view.subviews.forEach { $0.removeFromSuperview() }
        child.removeFromParent()
        child.viewIfLoaded?.removeFromSuperview()
        addChild(child)
        child.view.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(child.view)
        NSLayoutConstraint.activate([
            child.view.leadingAnchor.constraint(equalTo: view.leadingAnchor),
            child.view.trailingAnchor.constraint(equalTo: view.trailingAnchor),
            child.view.topAnchor.constraint(equalTo: view.topAnchor),
            child.view.bottomAnchor.constraint(equalTo: view.bottomAnchor)
        ])

        guard let overlay else { return }
        overlay.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(overlay, positioned: .above, relativeTo: child.view)
        NSLayoutConstraint.activate([
            overlay.leadingAnchor.constraint(equalTo: view.leadingAnchor),
            overlay.trailingAnchor.constraint(equalTo: view.trailingAnchor),
            overlay.topAnchor.constraint(equalTo: view.topAnchor),
            overlay.bottomAnchor.constraint(equalTo: view.bottomAnchor)
        ])
    }
```

`WorkspaceRootController` gains `var overlay: WorkspaceDragOverlay?`, set by the reconciler in `init` right after `rootViewController` is created.

Wire each group's strip callbacks to the coordinator inside `build(_:in:)`, where the controller is created:

```swift
                let created = PaneGroupController(
                    id: id, intentSink: intentSink, stripFactory: stripFactory)
                created.connectDrag(to: dragCoordinator)
```

and in `PaneGroupController`:

```swift
    /// The strip lives in the app target and speaks in screen points; the
    /// coordinator owns the rest of the gesture.
    public func connectDrag(to coordinator: WorkspaceDragCoordinator) {
        stripModel.onDragChanged = { [weak self, weak coordinator] tab, screenPoint in
            guard let self, let coordinator else { return }
            if coordinator.isDragging {
                coordinator.pointerMoved(toScreenPoint: screenPoint)
            } else {
                let entry = self.tabEntries.first { $0.tabID == tab }
                coordinator.pressBegan(
                    tab: tab, in: self.id, atScreenPoint: screenPoint,
                    tabFrame: self.stripModel.tabFrames[tab] ?? .zero,
                    title: entry?.title ?? ""
                )
                coordinator.pointerMoved(toScreenPoint: screenPoint)
            }
        }
        stripModel.onDragEnded = { [weak coordinator] _ in coordinator?.released() }
        stripModel.onDragCancelled = { [weak coordinator] in
            coordinator?.cancel(reason: .pointerCancelled)
        }
    }
```

Finally expose the coordinator from `WorkspaceViewController`:

```swift
    public var dragCoordinator: WorkspaceDragCoordinator { reconciler.dragCoordinator }
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd Packages/TillerWorkspace && swift test`
Expected: PASS — the two new tests plus every existing suite. The reconciler suites exercise `setContent`, so an overlay that breaks layout shows up here.

- [ ] **Step 5: Commit**

```bash
git add Packages/TillerWorkspace/Sources/TillerWorkspace/ \
        Packages/TillerWorkspace/Tests/TillerWorkspaceTests/WorkspaceHitFrameCollectorTests.swift
git commit -m "feat: collect live pane frames and host the drag overlay

refs OB-D-2, OB-D-4"
```

---

# Task 8: Make the app's tab strip draggable

The last seam. `PaneTabStripItem` currently uses `onTapGesture`; a `DragGesture` added alongside it would swallow those taps — the hover-yes/click-no symptom this codebase has hit before. One `DragGesture(minimumDistance: 0)` handles both, because `DragSession` already returns `.activateTab` for a press that never crossed the threshold.

**Files:**
- Modify: `App/PaneTabStripBar.swift`
- Test: manual (`Scripts/ci.sh` covers compilation and the package logic; SwiftUI gesture plumbing has no unit seam)

**Interfaces:**
- Consumes: `PaneTabStripModel.setTabFrame`, `.onDragChanged`, `.onDragEnded`, `.onDragCancelled` (Task 3).

- [ ] **Step 1: Publish each tab's frame**

In `PaneTabStripItem`, add a geometry reporter. The frame must be in the strip's space, so the enclosing `HStack` in `PaneTabStripBar` declares a named coordinate space:

```swift
struct PaneTabStripBar: View {
    @Bindable var model: PaneTabStripModel

    private static let stripSpace = "paneTabStrip"

    var body: some View {
        HStack(spacing: 4) {
            ScrollView(.horizontal, showsIndicators: false) {
                HStack(spacing: 2) {
                    ForEach(model.entries, id: \.tabID) { entry in
                        PaneTabStripItem(
                            entry: entry,
                            onActivate: { model.onActivate(entry.tabID) },
                            onClose: { model.onClose(entry.tabID) },
                            onFrameChange: { model.setTabFrame($0, for: entry.tabID) },
                            onDragChanged: { model.onDragChanged(entry.tabID, $0) },
                            onDragEnded: { model.onDragEnded(entry.tabID) })
                    }
                }
                .padding(.leading, 6)
            }

            Button(action: model.onNewTab) {
                Image(systemName: "plus")
                    .font(.system(size: 11))
                    .foregroundStyle(AppTheme.meta)
            }
            .buttonStyle(.plain)
            .help("New tab (⌘T)")
            .accessibilityLabel("New tab")
            .padding(.trailing, 8)
        }
        .frame(height: 32)
        .coordinateSpace(name: Self.stripSpace)
        .background { MainSurfaceMaterial(tint: AppTheme.chatSurface) }
        .onChange(of: model.entries.map(\.tabID)) { _, ids in
            model.removeTabFrames(notIn: Set(ids))
        }
    }
}
```

- [ ] **Step 2: Replace the tap with one drag gesture**

In `PaneTabStripItem`, add the three closures and swap the gesture. `NSEvent.mouseLocation` is the pointer in screen space — SwiftUI's local `value.location` cannot address a neighbouring pane:

```swift
private struct PaneTabStripItem: View {
    let entry: TabMenuEntry
    let onActivate: () -> Void
    let onClose: () -> Void
    let onFrameChange: (CGRect) -> Void
    let onDragChanged: (CGPoint) -> Void
    let onDragEnded: () -> Void

    @State private var hovering = false

    var body: some View {
        HStack(spacing: 6) {
            // … unchanged label and close button …
        }
        .padding(.horizontal, 9)
        .padding(.vertical, 5)
        .contentShape(Rectangle())
        .background {
            // … unchanged background …
        }
        .onGeometryChange(for: CGRect.self) { proxy in
            proxy.frame(in: .named("paneTabStrip"))
        } action: { frame in
            onFrameChange(frame)
        }
        .onHover { hovering = $0 }
        // One gesture, not a tap plus a drag: a simultaneous DragGesture eats
        // the taps, and DragSession already reports a sub-threshold press as an
        // activation.
        .gesture(
            DragGesture(minimumDistance: 0)
                .onChanged { _ in onDragChanged(NSEvent.mouseLocation) }
                .onEnded { _ in onDragEnded() }
        )
        .help(entry.title)
    }
}
```

`onActivate` is now reached through the coordinator's `.activateTab` intent rather than directly, so the parameter stays in the initializer but is no longer called from the gesture. Delete it from `PaneTabStripItem` and from the call site in `PaneTabStripBar` — leaving an unused closure invites the next reader to wire a second activation path.

- [ ] **Step 3: Cancel on Escape**

Add an event monitor for the duration of the drag. Put it in `PaneTabStripBar` so it is scoped to the visible chrome:

```swift
    @State private var escapeMonitor: Any?
```

and, on the same `HStack`:

```swift
        .onAppear {
            escapeMonitor = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { event in
                // 53 is Escape. Cancelling a drag consumes the key; with no
                // drag in flight the event must pass through untouched.
                guard event.keyCode == 53 else { return event }
                model.onDragCancelled()
                return event
            }
        }
        .onDisappear {
            if let escapeMonitor { NSEvent.removeMonitor(escapeMonitor) }
            escapeMonitor = nil
        }
```

- [ ] **Step 4: Build and verify the gate**

Run: `Scripts/ci.sh`
Expected: `CI OK`. If `PtyProcessTests` or `spawnCapturesOutput` fails, that is the known flake — re-run the gate; do not change PTY code.

- [ ] **Step 5: Commit**

```bash
git add App/PaneTabStripBar.swift
git commit -m "feat: drag pane tabs to reorder, move, and split

refs OB-D-1, OB-D-3, OB-D-5, OB-D-7"
```

---

# Task 9: Commit divider drags as one preferred fraction

`DividerTracking` exists and is tested, and nothing calls it. `NSSplitView` already moves the divider; what is missing is turning the end of that gesture into exactly one `.setPreferredFraction`.

**Files:**
- Create: `Packages/TillerWorkspace/Sources/TillerWorkspace/DividerCommitPolicy.swift`
- Modify: `Packages/TillerWorkspace/Sources/TillerWorkspace/WorkspaceSplitController.swift`
- Test: `Packages/TillerWorkspace/Tests/TillerWorkspaceTests/DividerCommitPolicyTests.swift`

**Interfaces:**
- Consumes: `DividerTracking` (unchanged), `WorkspaceIntentSink`.
- Produces:

```swift
public enum DividerCommitPolicy {
    public enum Step: Equatable, Sendable { case move, commit, ignore }
    /// `nil` means AppKit had no current event — a programmatic resize.
    public static func step(for eventType: NSEvent.EventType?) -> Step
}
```

- [ ] **Step 1: Write the failing tests**

Create `DividerCommitPolicyTests.swift`:

```swift
import AppKit
import Testing
import TillerWorkspace

@Suite
struct DividerCommitPolicyTests {
    @Test
    func draggingTracksWithoutCommitting() {
        #expect(DividerCommitPolicy.step(for: .leftMouseDragged) == .move)
    }

    @Test
    func releasingCommits() {
        #expect(DividerCommitPolicy.step(for: .leftMouseUp) == .commit)
    }

    /// A window resize or a reconcile also resizes subviews. Committing there
    /// would write a preferred fraction the user never chose.
    @Test
    func programmaticAndUnrelatedResizesAreIgnored() {
        #expect(DividerCommitPolicy.step(for: nil) == .ignore)
        #expect(DividerCommitPolicy.step(for: .keyDown) == .ignore)
        #expect(DividerCommitPolicy.step(for: .applicationDefined) == .ignore)
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd Packages/TillerWorkspace && swift test --filter DividerCommitPolicyTests`
Expected: compile error — `cannot find 'DividerCommitPolicy' in scope`.

- [ ] **Step 3: Write the implementation**

Create `DividerCommitPolicy.swift`:

```swift
import AppKit

/// `NSSplitView` reports subview resizes without telling you whether a pointer
/// gesture is still in flight. The current event does: a drag is mid-gesture, a
/// mouse-up is the release, anything else is layout doing its job.
public enum DividerCommitPolicy {
    public enum Step: Equatable, Sendable {
        case move
        case commit
        case ignore
    }

    public static func step(for eventType: NSEvent.EventType?) -> Step {
        switch eventType {
        case .leftMouseDragged: return .move
        case .leftMouseUp: return .commit
        default: return .ignore
        }
    }
}
```

In `WorkspaceSplitController`, hold a `DividerTracking` and feed it from the split view's resize callback. Add to the class:

```swift
    private var tracking: DividerTracking?

    /// The sink arrives after construction because the reconciler builds split
    /// controllers before it knows the layout they belong to.
    public func connectDivider(sink: WorkspaceIntentSink) {
        tracking = DividerTracking(sink: sink)
    }

    override public func splitViewDidResizeSubviews(_ notification: Notification) {
        super.splitViewDidResizeSubviews(notification)
        guard let tracking else { return }

        let isVertical = splitView.isVertical
        let total = isVertical ? splitView.bounds.width : splitView.bounds.height
        let position = isVertical
            ? (splitView.arrangedSubviews.first?.frame.width ?? 0)
            : (splitView.arrangedSubviews.first?.frame.height ?? 0)

        switch DividerCommitPolicy.step(for: NSApp.currentEvent?.type) {
        case .move:
            tracking.began(
                split: id,
                total: total,
                minimum: isVertical
                    ? WorkspaceMetrics.preferredGroupSize.width
                    : WorkspaceMetrics.preferredGroupSize.height
            )
            tracking.moved(to: position)
        case .commit:
            tracking.moved(to: position)
            tracking.ended()
        case .ignore:
            break
        }
    }
```

**Note:** `DividerTracking.began` resets the gesture, so calling it on every `.move` is safe and removes the need to track gesture start separately. If `WorkspaceSplitController` is not already an `NSSplitViewDelegate`, it inherits the callback from `NSSplitViewController`, which is — no extra conformance is needed.

Call `connectDivider(sink:)` from the reconciler where split controllers are created, next to `created.connectDrag`:

```swift
                if let intentSink { created.connectDivider(sink: intentSink) }
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd Packages/TillerWorkspace && swift test`
Expected: PASS, including `DividerTrackingTests` and `WorkspaceDividerTests`.

- [ ] **Step 5: Commit**

```bash
git add Packages/TillerWorkspace/Sources/TillerWorkspace/DividerCommitPolicy.swift \
        Packages/TillerWorkspace/Sources/TillerWorkspace/WorkspaceSplitController.swift \
        Packages/TillerWorkspace/Sources/TillerWorkspace/WorkspaceReconciler.swift \
        Packages/TillerWorkspace/Tests/TillerWorkspaceTests/DividerCommitPolicyTests.swift
git commit -m "feat: commit one preferred fraction when a divider drag ends

refs OB-D-8, CO-4"
```

---

# Task 10: Gate and manual acceptance

The unit suites prove the geometry and the intents. They cannot prove the gesture reaches the geometry — that is exactly the failure mode this whole plan exists to repair, so it gets a human check.

**Files:**
- Modify: `docs/superpowers/plans/2026-08-04-pane-drag-drop-wiring.md` (tick the boxes)

- [ ] **Step 1: Run the full gate**

Run: `Scripts/ci.sh`
Expected: `CI OK`. Known flake: `TillerTerminal`'s `spawnCapturesOutput` may need several re-runs; re-run the gate rather than touching PTY code.

- [ ] **Step 2: Walk the manual checklist**

Build and run (⌘R), open a worktree with at least two tabs, and confirm each line. A failure here means a seam is still unwired — record which one before changing anything.

- [ ] Pressing a tab and releasing without moving activates it (no move, no flicker).
- [ ] Dragging a tab 4 points or more shows a ghost that tracks the pointer and keeps the grab offset.
- [ ] Dragging within the same strip shows a caret at the insertion point; releasing reorders.
- [ ] Dragging onto another pane's strip shows a caret there; releasing moves the tab into that pane.
- [ ] Dragging into another pane's left/right/top/bottom band previews the correct half — the highlighted half is the half the tab lands in.
- [ ] Dropping on an edge creates one split and keeps the target pane's content alive.
- [ ] A pane holding a single tab is a legal edge-split target for a tab from another pane.
- [ ] Dragging the sole tab of a pane onto its own pane's edge previews the centre, not a split.
- [ ] Moving the last tab out of a non-root pane collapses that pane and its parent split.
- [ ] Escape mid-drag leaves the layout unchanged and removes the overlay.
- [ ] Releasing outside every pane leaves the layout unchanged.
- [ ] Dragging a divider updates live and, on release, the new proportion survives a restart.
- [ ] Resizing the window does not change any pane's proportion.
- [ ] With Reduce Motion on, the topology changes without animation.
- [ ] The terminal in a pane still takes keyboard focus and mouse selection after any drag.

- [ ] **Step 3: Commit the completed plan**

```bash
git add docs/superpowers/plans/2026-08-04-pane-drag-drop-wiring.md
git commit -m "docs: record the pane drag wiring manual acceptance run"
```

---

## Risk register

| Risk | Where it bites | Mitigation |
|---|---|---|
| `NSEvent.mouseLocation` is screen-space; the workspace root may be scrolled or offset | Task 6/7 conversion | The conversion lives in exactly one closure (`convertToRoot`); the flip lives in exactly one method (`hitFrame(in:)`). A wrong preview position means one of those two, not a hunt. |
| SwiftUI `DragGesture` still competing with the close button | Task 8 | The close button is a `Button`, which claims its own hit region; verify the manual checklist line for closing a tab mid-hover. |
| Ghostty surfaces mis-reporting occlusion under a new sibling overlay | Task 7 | The overlay returns `nil` from `hitTest` and never covers a mounted host's visibility state — visibility is mount state, not z-order. Confirm the last manual checklist line. |
| `splitViewDidResizeSubviews` firing during reconcile with a stale current event | Task 9 | `DividerCommitPolicy` ignores everything that is not an explicit drag or mouse-up. |
| The overlay stealing first responder from a terminal | Task 7 | It is an `NSView` with no `acceptsFirstResponder` override, so it never becomes one. |
