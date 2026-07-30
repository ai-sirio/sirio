import CoreGraphics
import Testing
import TillerCore
import TillerWorkspace

@Suite
struct DropTargetResolverTests {
    @Test
    func pointerInsideTheOuterTwentyTwoPercentPicksTheNearestEdge() {
        let groupID = PaneGroupID()
        let bounds = CGRect(x: 0, y: 0, width: 1_000, height: 500)

        #expect(
            DropTargetResolver.resolve(
                pointInGroup: CGPoint(x: 950, y: 250), groupBounds: bounds,
                tabStripHeight: 40, tabFrames: [], draggedTab: WorkspaceTabID(),
                sourceGroup: groupID, groupTabCount: 2
            ) == .edge(groupID, placement: .right)
        )
        #expect(
            DropTargetResolver.resolve(
                pointInGroup: CGPoint(x: 500, y: 250), groupBounds: bounds,
                tabStripHeight: 40, tabFrames: [], draggedTab: WorkspaceTabID(),
                sourceGroup: groupID, groupTabCount: 2
            ) == .center(groupID)
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

        #expect(
            DropTargetResolver.resolve(
                pointInGroup: CGPoint(x: 110, y: 10), groupBounds: CGRect(x: 0, y: 0, width: 400, height: 300),
                tabStripHeight: 40, tabFrames: frames, draggedTab: tab,
                sourceGroup: groupID, groupTabCount: 3
            ) == .tabStrip(groupID, insertionIndex: 1)
        )
        #expect(
            DropTargetResolver.resolve(
                pointInGroup: CGPoint(x: 380, y: 10), groupBounds: CGRect(x: 0, y: 0, width: 400, height: 300),
                tabStripHeight: 40, tabFrames: frames, draggedTab: tab,
                sourceGroup: groupID, groupTabCount: 3
            ) == .tabStrip(groupID, insertionIndex: 3)
        )
    }

    @Test
    func theSoleTabOfAGroupCannotEdgeSplitIntoItsOwnGroup() {
        let groupID = PaneGroupID()
        let result = DropTargetResolver.resolve(
            pointInGroup: CGPoint(x: 995, y: 250),
            groupBounds: CGRect(x: 0, y: 0, width: 1_000, height: 500),
            tabStripHeight: 40, tabFrames: [], draggedTab: WorkspaceTabID(),
            sourceGroup: groupID, groupTabCount: 1
        )

        #expect(result == .center(groupID))
    }

    @Test
    func splitIsIneligibleWhenEitherHalfWouldFallBelowTwoFortyPoints() {
        let result = SplitEligibility.check(
            groupSize: CGSize(width: 400, height: 300),
            placement: .right, isSoleTabOfSourceGroup: false
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
            placement: .down, isSoleTabOfSourceGroup: true
        )
        guard case .failure(let soleTabReason) = soleTabResult else {
            Issue.record("a sole tab should make an edge split ineligible")
            return
        }
        #expect(soleTabReason == .soleTabOfItsOwnGroup)

        let heightResult = SplitEligibility.check(
            groupSize: CGSize(width: 600, height: 300),
            placement: .down, isSoleTabOfSourceGroup: false
        )
        guard case .failure(let heightReason) = heightResult else {
            Issue.record("a 300-point vertical group should reject this split")
            return
        }
        #expect(heightReason == .insufficientHeight(available: 147, required: 160))
    }
}
