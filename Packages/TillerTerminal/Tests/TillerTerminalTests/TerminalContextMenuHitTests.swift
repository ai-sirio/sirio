import Testing
import AppKit
@testable import TillerTerminal

@MainActor
struct TerminalContextMenuHitTests {
    /// The clicked view is a deep subview of the pane container; the handler
    /// must still resolve it to that pane. Regression for the inverted
    /// `isDescendant(of:)` direction that suppressed the context menu.
    @Test func resolvesPaneIdFromNestedHitView() {
        let controller = NSViewController()
        let root = NSView(frame: NSRect(x: 0, y: 0, width: 100, height: 100))
        let child = NSView(frame: NSRect(x: 10, y: 10, width: 50, height: 50))
        root.addSubview(child)
        controller.view = root
        let id = UUID()

        let hit = TerminalContextMenuHandler.paneId(forHit: child, in: [id: controller])

        #expect(hit == id)
    }

    @Test func returnsNilForUnrelatedHitView() {
        let controller = NSViewController()
        controller.view = NSView(frame: NSRect(x: 0, y: 0, width: 100, height: 100))
        let unrelated = NSView(frame: NSRect(x: 0, y: 0, width: 10, height: 10))

        let hit = TerminalContextMenuHandler.paneId(forHit: unrelated, in: [UUID(): controller])

        #expect(hit == nil)
    }

    /// Regression: production always supplies a worktree-shared paneCache
    /// (TerminalSplitHost stores leaf controllers there instead of in
    /// coordinator.leafControllers), so the handler must prefer it — else
    /// hit testing always misses and the context menu never appears.
    @Test func controllersPrefersPaneCacheOverCoordinatorWhenBothPresent() {
        let coordinator = TerminalSplitHost.Coordinator()
        let cache = TerminalPaneCache()
        let cachedId = UUID()
        cache.controllers[cachedId] = NSViewController()

        let resolved = TerminalContextMenuHandler.controllers(coordinator: coordinator, paneCache: cache)

        #expect(resolved[cachedId] != nil)
    }

    @Test func controllersFallsBackToCoordinatorWhenNoPaneCache() {
        let coordinator = TerminalSplitHost.Coordinator()
        let id = UUID()
        coordinator.leafControllers[id] = NSViewController()

        let resolved = TerminalContextMenuHandler.controllers(coordinator: coordinator, paneCache: nil)

        #expect(resolved[id] != nil)
    }
}
