import Testing
import TillerCore
@testable import Tiller

@Suite @MainActor
struct BrowserSurfaceBudgetTests {
    @Test
    func visibleSurfacesAreNeverEvictedEvenWhenFiveBrowsersAreOpen() {
        let budget = BrowserSurfaceBudget()
        let surfaces = makeSurfaces(count: 5)

        for (index, surface) in surfaces.enumerated() {
            budget.register(surface, persistedURL: "http://127.0.0.1:\(index + 1)")
            #expect(budget.activate(surface) == .existing)
        }

        #expect(budget.liveCount == 5)
        #expect(budget.hiddenLiveIDs.isEmpty)
    }

    @Test
    func fourthHiddenSurfaceIsEvictedLeastRecentlyVisibleFirst() {
        let budget = BrowserSurfaceBudget()
        let surfaces = makeSurfaces(count: 4)

        for (index, surface) in surfaces.enumerated() {
            budget.register(surface, persistedURL: "http://127.0.0.1:\(index + 1)")
            #expect(budget.activate(surface) == .existing)
        }

        for surface in surfaces {
            _ = budget.deactivate(surface)
        }

        #expect(budget.liveCount == 3)
        #expect(!budget.isLive(surfaces[0]))
        #expect(budget.isLive(surfaces[1]))
        #expect(budget.isLive(surfaces[2]))
        #expect(budget.isLive(surfaces[3]))
    }

    @Test
    func anEvictedSurfaceReloadsItsPersistedURLWhenShownAgain() {
        let budget = BrowserSurfaceBudget()
        let surfaces = makeSurfaces(count: 4)
        let urls = surfaces.enumerated().map {
            "http://127.0.0.1:\($0.offset)/persisted"
        }

        for (surface, url) in zip(surfaces, urls) {
            budget.register(surface, persistedURL: url)
            #expect(budget.activate(surface) == .existing)
        }
        for surface in surfaces { _ = budget.deactivate(surface) }

        #expect(budget.activate(surfaces[0]) == .reload(url: urls[0]))
        #expect(budget.isLive(surfaces[0]))
    }

    /// An agent working in a worktree the human is not looking at is the normal
    /// case, not an edge one: reclaiming its surface destroys refs it still holds
    /// and cannot recover.
    @Test
    func aSurfaceAnAgentIsDrivingIsNeverEvictedWhileHidden() {
        let budget = BrowserSurfaceBudget()
        let surfaces = makeSurfaces(count: 5)
        for (index, surface) in surfaces.enumerated() {
            budget.register(surface, persistedURL: "http://127.0.0.1:\(index + 1)")
            #expect(budget.activate(surface) == .existing)
        }
        #expect(budget.attachAgent(surfaces[0], persistedURL: "http://127.0.0.1:1") == .existing)

        for surface in surfaces { _ = budget.deactivate(surface) }

        #expect(budget.isLive(surfaces[0]))
        // Four unattached hidden surfaces remain, so one of those is the victim.
        #expect(budget.liveCount == 4)
    }

    @Test
    func anAgentReachingAReclaimedSurfaceIsToldToReloadItsURL() {
        let budget = BrowserSurfaceBudget()
        let surfaces = makeSurfaces(count: 4)
        let urls = surfaces.indices.map { "http://127.0.0.1:\($0)/persisted" }
        for (surface, url) in zip(surfaces, urls) {
            budget.register(surface, persistedURL: url)
            #expect(budget.activate(surface) == .existing)
        }
        for surface in surfaces { _ = budget.deactivate(surface) }
        #expect(!budget.isLive(surfaces[0]))

        #expect(budget.attachAgent(surfaces[0], persistedURL: urls[0]) == .reload(url: urls[0]))
        #expect(budget.isLive(surfaces[0]))
    }

    private func makeSurfaces(count: Int) -> [BrowserContentID] {
        (0..<count).map { _ in BrowserContentID() }
    }
}
