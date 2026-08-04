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
