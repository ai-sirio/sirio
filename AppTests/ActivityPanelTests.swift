import Foundation
import Testing
import TillerCore

@testable import Tiller

@Suite("ActivityPanelTests", .serialized)
@MainActor
struct ActivityPanelTests {
    /// The list follows the mounted worktrees, not the sidebar selection: with
    /// nothing selected, an open worktree's chat still shows up.
    @Test func openWorktreesDriveTheListWithoutASelection() async throws {
        let fixture = try await UniversalChatFixture.make(
            suiteName: "dev.tiller.Tiller.ActivityPanelTests.openWorktrees")
        defer { fixture.cleanUp() }
        _ = try #require(await UniversalChatFixture.openChatTab(fixture))

        fixture.model.projects = [Project(
            id: fixture.worktree.projectId, name: "P", rootPath: fixture.worktree.path)]
        fixture.model.openWorktreeIds = [fixture.worktree.id]
        fixture.model.selectedWorktree = nil

        let rows = ActivityPanelModel.rows(appModel: fixture.model)

        #expect(rows.count == 1)
        #expect(rows[0].kind == .chat)
        #expect(rows[0].worktreeLabel == "P/main")
    }

    /// A worktree that is not mounted contributes nothing, even though its
    /// layout is still in the coordinator.
    @Test func unmountedWorktreesContributeNothing() async throws {
        let fixture = try await UniversalChatFixture.make(
            suiteName: "dev.tiller.Tiller.ActivityPanelTests.unmounted")
        defer { fixture.cleanUp() }
        _ = try #require(await UniversalChatFixture.openChatTab(fixture))

        fixture.model.openWorktreeIds = []

        #expect(ActivityPanelModel.rows(appModel: fixture.model).isEmpty)
    }

    /// The list is derived, never cached: closing the tab empties it.
    @Test func closingTheTabRemovesItsRow() async throws {
        let fixture = try await UniversalChatFixture.make(
            suiteName: "dev.tiller.Tiller.ActivityPanelTests.close")
        defer { fixture.cleanUp() }
        let tab = try #require(await UniversalChatFixture.openChatTab(fixture))
        fixture.model.openWorktreeIds = [fixture.worktree.id]

        fixture.model.closeTab(tab.id.rawValue, in: fixture.worktree)
        await UniversalChatFixture.settle(fixture) {
            UniversalChatFixture.tabs(fixture).isEmpty
        }

        #expect(ActivityPanelModel.rows(appModel: fixture.model).isEmpty)
    }
}
