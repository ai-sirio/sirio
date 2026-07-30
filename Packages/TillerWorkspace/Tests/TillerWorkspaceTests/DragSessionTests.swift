import CoreGraphics
import Testing
import TillerCore
import TillerWorkspace

@Suite @MainActor
struct DragSessionTests {
    @Test
    func aPressUnderFourPointsActivatesInsteadOfDragging() {
        let session = DragSession()
        let tab = WorkspaceTabID()
        let frame = CGRect(x: 0, y: 0, width: 120, height: 32)
        session.pressBegan(tab: tab, at: CGPoint(x: 10, y: 10), inTabFrame: frame)
        session.pointerMoved(to: CGPoint(x: 13, y: 10)) { _ in .center(PaneGroupID()) }

        #expect(!session.isActive)
        #expect(session.release() == .activateTab(tab))
    }

    @Test
    func theGrabOffsetIsPreservedForTheWholeDrag() {
        let session = DragSession()
        let tab = WorkspaceTabID()
        let group = PaneGroupID()
        let frame = CGRect(x: 20, y: 30, width: 120, height: 32)
        session.pressBegan(tab: tab, at: CGPoint(x: 47, y: 39), inTabFrame: frame)
        session.pointerMoved(to: CGPoint(x: 51, y: 39)) { _ in .center(group) }

        #expect(session.isActive)
        #expect(session.grabOffset == CGPoint(x: 27, y: 9))
        #expect(session.currentTarget == .center(group))
    }

    @Test
    func onlyTheHoveredGroupHasATarget() {
        let session = DragSession()
        let tab = WorkspaceTabID()
        let hoveredGroup = PaneGroupID()
        let otherGroup = PaneGroupID()
        session.pressBegan(tab: tab, at: CGPoint(x: 10, y: 10), inTabFrame: CGRect(x: 0, y: 0, width: 100, height: 30))
        session.pointerMoved(to: CGPoint(x: 14, y: 10)) { _ in .center(hoveredGroup) }
        #expect(session.currentTarget == .center(hoveredGroup))

        session.pointerMoved(to: CGPoint(x: 18, y: 10)) { _ in .center(otherGroup) }
        #expect(session.currentTarget == .center(otherGroup))
        #expect(session.currentTarget != .center(hoveredGroup))
    }

    @Test
    func escapeCancelsWithoutEmittingAnyIntent() {
        let session = DragSession()
        let tab = WorkspaceTabID()
        session.pressBegan(tab: tab, at: CGPoint(x: 10, y: 10), inTabFrame: CGRect(x: 0, y: 0, width: 100, height: 30))
        session.pointerMoved(to: CGPoint(x: 20, y: 10)) { _ in .center(PaneGroupID()) }
        session.cancel(reason: .escape)

        #expect(session.release() == nil)
        #expect(session.currentTarget == .none)
        #expect(!session.isActive)
    }

    @Test
    func focusLossAndPointerCancellationAreAlsoMutationFree() {
        for reason in [DragSession.CancelReason.focusLost, .pointerCancelled, .staleIdentity] {
            let session = DragSession()
            session.pressBegan(tab: WorkspaceTabID(), at: CGPoint(x: 1, y: 1), inTabFrame: CGRect(x: 0, y: 0, width: 40, height: 20))
            session.pointerMoved(to: CGPoint(x: 8, y: 1)) { _ in .tabStrip(PaneGroupID(), insertionIndex: 0) }
            session.cancel(reason: reason)

            #expect(session.release() == nil)
            #expect(session.currentTarget == .none)
        }
    }

    @Test
    func releasingOutsideALegalTargetEmitsNothing() {
        let session = DragSession()
        session.pressBegan(tab: WorkspaceTabID(), at: CGPoint(x: 10, y: 10), inTabFrame: CGRect(x: 0, y: 0, width: 100, height: 30))
        session.pointerMoved(to: CGPoint(x: 20, y: 10)) { _ in .none }

        #expect(session.isActive)
        #expect(session.release() == nil)
        #expect(session.currentTarget == .none)
    }

    /// Regression: `release()` used to collapse left into right and top into
    /// bottom, so a left-edge drag would split to the anchor's RIGHT instead
    /// of its left. That is a real interaction bug, not a naming detail — the
    /// #3 Edge Preview contract requires the exact edge the pointer hovers,
    /// not "the same axis, arbitrary side".
    @Test
    func eachOfTheFourEdgesProducesItsOwnDistinctSplitPlacement() {
        let group = PaneGroupID()
        var placements: [EdgePlacement: SplitPlacementSide] = [:]
        for edge in [EdgePlacement.left, .right, .top, .bottom] {
            let session = DragSession()
            session.pressBegan(
                tab: WorkspaceTabID(), at: CGPoint(x: 10, y: 10),
                inTabFrame: CGRect(x: 0, y: 0, width: 100, height: 30))
            session.pointerMoved(to: CGPoint(x: 20, y: 10)) { _ in .edge(group, placement: edge) }
            guard case .requestMove(_, to: .edgeSplit(_, let placement)) = session.release() else {
                Issue.record("expected an edgeSplit intent for \(edge)")
                continue
            }
            placements[edge] = placement
        }
        #expect(placements[.left] == .left)
        #expect(placements[.right] == .right)
        #expect(placements[.top] == .above)
        #expect(placements[.bottom] == .below)
        #expect(Set(placements.values).count == 4)
    }
}
