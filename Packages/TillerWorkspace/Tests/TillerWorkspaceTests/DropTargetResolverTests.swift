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
    func anExternalDragUsesEveryTargetWithoutApplyingTheSoleTabRule() {
        let groupID = PaneGroupID()
        let frames = [
            CGRect(x: 0, y: 0, width: 100, height: 30),
            CGRect(x: 100, y: 0, width: 100, height: 30)
        ]

        #expect(
            DropTargetResolver.resolve(
                pointInGroup: CGPoint(x: 500, y: 250), groupBounds: bounds,
                tabStripHeight: 40, tabFrames: [], draggedTab: nil,
                sourceGroup: nil, hoveredGroup: groupID, groupTabCount: 1
            ) == .center(groupID))
        #expect(
            DropTargetResolver.resolve(
                pointInGroup: CGPoint(x: 995, y: 250), groupBounds: bounds,
                tabStripHeight: 40, tabFrames: [], draggedTab: nil,
                sourceGroup: nil, hoveredGroup: groupID, groupTabCount: 1
            ) == .edge(groupID, placement: .right))
        #expect(
            DropTargetResolver.resolve(
                pointInGroup: CGPoint(x: 110, y: 10), groupBounds: bounds,
                tabStripHeight: 40, tabFrames: frames, draggedTab: nil,
                sourceGroup: nil, hoveredGroup: groupID, groupTabCount: 1
            ) == .tabStrip(groupID, insertionIndex: 1))
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
        #expect(reason == .insufficientWidth(available: 195, required: 240))
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
        #expect(heightReason == .insufficientHeight(available: 145, required: 160))
    }
}
