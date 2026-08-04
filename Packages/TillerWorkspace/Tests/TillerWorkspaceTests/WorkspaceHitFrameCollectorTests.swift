import AppKit
import Testing
import TillerCore
@testable import TillerWorkspace

@MainActor
private final class RecordingWiringSink: WorkspaceIntentSink {
    var intents: [WorkspaceIntent] = []

    func send(_ intent: WorkspaceIntent) {
        intents.append(intent)
    }
}

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

    /// The strip reports a stream of pointer positions, and the first ones are
    /// always sub-threshold. Treating each of those as a fresh press would move
    /// the origin the 4-point threshold is measured from, and no drag would
    /// ever start.
    @Test
    func aStreamOfSmallMovesStillCrossesTheDragThreshold() {
        let sink = RecordingWiringSink()
        let group = PaneGroupController(id: PaneGroupID())
        let hovered = PaneGroupID()
        let coordinator = WorkspaceDragCoordinator(
            sink: sink,
            frames: {
                [PaneGroupHitFrame(
                    id: hovered, bounds: CGRect(x: 0, y: 0, width: 500, height: 400),
                    tabFrames: [], tabCount: 2
                )]
            },
            overlay: { nil }
        )
        group.connectDrag(to: coordinator)
        let tab = WorkspaceTabID()
        group.strip.entries = [TabMenuEntry(tabID: tab, title: "one", isActive: true)]
        group.strip.setTabFrame(CGRect(x: 0, y: 0, width: 100, height: 32), for: tab)

        for x in stride(from: 40.0, through: 48.0, by: 2.0) {
            group.strip.onDragChanged(tab, CGPoint(x: x, y: 200))
        }

        #expect(coordinator.isDragging)
    }

    /// Tab rectangles are measured inside the strip; the collector offsets them
    /// into root space so the caret lands on the tab the pointer is actually
    /// over, not on the matching offset in the leftmost pane.
    @Test
    func tabFramesAreOffsetIntoTheSameSpaceAsTheBounds() {
        let root = NSView(frame: CGRect(x: 0, y: 0, width: 600, height: 400))
        let group = PaneGroupController(id: PaneGroupID())
        group.view.frame = CGRect(x: 300, y: 200, width: 300, height: 200)
        root.addSubview(group.view)
        let tab = WorkspaceTabID()
        group.strip.entries = [TabMenuEntry(tabID: tab, title: "one", isActive: true)]
        group.strip.setTabFrame(CGRect(x: 6, y: 0, width: 100, height: 32), for: tab)

        let frame = group.hitFrame(in: root)

        #expect(frame?.bounds == CGRect(x: 300, y: 0, width: 300, height: 200))
        #expect(frame?.tabFrames == [CGRect(x: 306, y: 0, width: 100, height: 32)])
    }
}
