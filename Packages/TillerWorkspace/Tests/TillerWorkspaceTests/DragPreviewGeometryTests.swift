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
