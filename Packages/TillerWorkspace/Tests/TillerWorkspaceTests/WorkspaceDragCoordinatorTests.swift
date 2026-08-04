import AppKit
import Testing
import TillerCore
import TillerWorkspace

@MainActor
private final class RecordingSink: WorkspaceIntentSink {
    var intents: [WorkspaceIntent] = []

    func send(_ intent: WorkspaceIntent) {
        intents.append(intent)
    }
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
        WorkspaceDragCoordinator(sink: sink, frames: { self.frames() }, overlay: { overlay })
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
