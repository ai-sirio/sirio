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
