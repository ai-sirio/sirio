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
}
