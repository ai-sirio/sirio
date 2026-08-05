import CoreGraphics
import Testing
import TillerCore
@testable import TillerWorkspace

@Suite @MainActor
struct PaneTabStripLayoutTests {
    @Test func activeTabsKeepTheLargerMinimumWidth() {
        #expect(PaneTabStripLayout.minimumWidth(isActive: false) == 72)
        #expect(PaneTabStripLayout.minimumWidth(isActive: true) == 118)
        #expect(PaneTabStripLayout.maximumWidth == 160)
        #expect(PaneTabStripLayout.closeControlWidth == 16)
    }

    @Test func overflowUsesAToleranceForFractionalLayoutNoise() {
        #expect(!PaneTabStripLayout.isOverflowing(contentWidth: 200.5, viewportWidth: 200))
        #expect(PaneTabStripLayout.isOverflowing(contentWidth: 202, viewportWidth: 200))
    }

    @Test func theModelPublishesOverflowAndTheActiveEntry() {
        let active = WorkspaceTabID()
        let model = PaneTabStripModel()
        model.entries = [TabMenuEntry(tabID: active, title: "active", isActive: true)]

        model.updateContentWidth(240)
        model.updateViewportWidth(160)

        #expect(model.activeTabID == active)
        #expect(model.isOverflowing)
    }

    @Test func theOverflowControlAppearsOnlyWhenTheSequenceDoesNotFit() {
        let model = PaneTabStripModel()

        model.updateContentWidth(180)
        model.updateViewportWidth(200)
        #expect(!model.showsOverflowMenu)

        model.updateContentWidth(240)
        #expect(model.showsOverflowMenu)
    }

    @Test func negativeMeasurementsClampToZero() {
        let model = PaneTabStripModel()

        model.updateContentWidth(-10)
        model.updateViewportWidth(-20)

        #expect(model.contentWidth == 0)
        #expect(model.viewportWidth == 0)
        #expect(!model.isOverflowing)
    }
}
