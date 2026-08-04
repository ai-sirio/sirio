import CoreGraphics
import Testing
import TillerCore
@testable import TillerWorkspace

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

        #expect(model.tabFrames.count == 1)
        #expect(model.tabFrames[kept] == CGRect(x: 0, y: 0, width: 100, height: 32))
    }
}
