import Testing
import AppKit
@testable import TillerTerminal

@MainActor
struct TerminalPaneCacheTests {
    @Test func pruneDropsControllersOutsideKeepSet() {
        // Arrange
        let cache = TerminalPaneCache()
        let keep = UUID()
        let drop = UUID()
        cache.controllers[keep] = NSViewController()
        cache.controllers[drop] = NSViewController()

        // Act
        cache.prune(keeping: [keep])

        // Assert
        #expect(cache.controllers[keep] != nil)
        #expect(cache.controllers[drop] == nil)
    }

    @Test func pruneWithEmptyKeepSetClearsCache() {
        let cache = TerminalPaneCache()
        cache.controllers[UUID()] = NSViewController()
        cache.prune(keeping: [])
        #expect(cache.controllers.isEmpty)
    }

    @Test func focusTargetsOnlyRequestedPane() {
        let cache = TerminalPaneCache()
        let targetId = UUID(), otherId = UUID()
        let targetController = NSViewController()
        let otherController = NSViewController()
        let targetView = FocusablePaneView()
        let otherView = FocusablePaneView()
        targetController.view = targetView
        otherController.view = otherView
        cache.controllers[targetId] = targetController
        cache.controllers[otherId] = otherController

        let host = NSView(frame: NSRect(x: 0, y: 0, width: 200, height: 100))
        host.addSubview(targetView)
        host.addSubview(otherView)
        let window = NSWindow(
            contentRect: host.bounds,
            styleMask: .borderless,
            backing: .buffered,
            defer: false
        )
        window.contentView = host
        #expect(window.makeFirstResponder(otherView))

        #expect(cache.focus(paneId: targetId))
        #expect(window.firstResponder === targetView)
        #expect(cache.focus(paneId: UUID()) == false)
        #expect(window.firstResponder === targetView)
    }
}

private final class FocusablePaneView: NSView {
    override var acceptsFirstResponder: Bool { true }
}
