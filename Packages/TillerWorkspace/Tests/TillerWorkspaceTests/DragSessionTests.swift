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
}
