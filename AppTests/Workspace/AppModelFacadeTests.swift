import Foundation
import Testing
import TillerCore
import TillerTerminal
import TillerWorkspace

@testable import Tiller

@Suite("AppModelFacadeTests", .serialized)
@MainActor
struct AppModelFacadeTests {
    @Test func appModelKeepsNoTabStateOfItsOwn() {
        let model = makeModel()
        let storedLabels = Mirror(reflecting: model).children.compactMap(\.label)

        #expect(!storedLabels.contains("tabs"))
        #expect(!storedLabels.contains("activeTabId"))
        #expect(!storedLabels.contains("paneCaches"))
    }

    @Test func autoRenameWritesThroughTheCoordinator() async throws {
        let fixture = try await UniversalChatFixture.make(
            suiteName: "dev.tiller.Tiller.AppModelFacadeTests.autoRename")
        defer { fixture.cleanUp() }
        let tab = try #require(await UniversalChatFixture.openChatTab(fixture))

        fixture.model.applyAutoTitle(
            tab.id.rawValue, in: fixture.worktree.id, title: "Fix login bug")
        await UniversalChatFixture.settle(fixture) {
            UniversalChatFixture.tab(fixture, id: tab.id)?.title == "Fix login bug"
        }

        let updated = UniversalChatFixture.tab(fixture, id: tab.id)
        #expect(updated?.title == "Fix login bug")
        #expect(updated?.titleIsAutoNamed == true)
    }

    @Test func fileDropOnTheWorkspaceOpensOrMovesTheDocumentTab() async throws {
        let suiteName = "dev.tiller.Tiller.AppModelFacadeTests.fileDrop"
        let defaults = UserDefaults(suiteName: suiteName)!
        defaults.removePersistentDomain(forName: suiteName)
        let model = AppModel(
            paneRegistry: PaneRegistry(), registrationTimeoutMs: 100, defaults: defaults,
            workspaceCoordinator: WorkspaceCoordinator(
                persistence: AppModelFacadeWorkspacePersistence(),
                registry: WorkspaceContentRegistry(),
                adapters: [.document: DocumentContentAdapter()]))
        let worktree = Worktree(
            id: UUID(), projectId: UUID(), branch: "main", path: "/tmp/facade-tests")
        model.worktrees = [worktree.projectId: [worktree]]
        await model.workspaceCoordinator.restore(worktree: worktree)

        let url = URL(fileURLWithPath: NSTemporaryDirectory())
            .appendingPathComponent("tiller-facade-\(UUID().uuidString).txt")
        try Data("hello".utf8).write(to: url)
        defer { try? FileManager.default.removeItem(at: url) }

        model.openDocument(fileURL: url, in: worktree)
        var tabs: [WorkspaceTab] = []
        for _ in 0..<200 where tabs.isEmpty {
            tabs = model.workspaceCoordinator.layouts[worktree.id]?.allTabs ?? []
            if tabs.isEmpty { try await Task.sleep(for: .milliseconds(10)) }
        }
        model.openDocument(fileURL: url, in: worktree)
        try await Task.sleep(for: .milliseconds(50))

        #expect(model.workspaceCoordinator.layouts[worktree.id]?.allTabs.count == 1)
    }

    /// Open / rename / close all land in the coordinator's layout: AppModel
    /// keeps no tab state of its own to diverge from it.
    @Test func everyMutationRouteReachesTheSameCoordinatorSeam() async throws {
        let fixture = try await UniversalChatFixture.make(
            suiteName: "dev.tiller.Tiller.AppModelFacadeTests.seam")
        defer { fixture.cleanUp() }
        let tab = try #require(await UniversalChatFixture.openChatTab(fixture))

        fixture.model.renameTab(tab.id.rawValue, in: fixture.worktree.id, to: "Renamed")
        await UniversalChatFixture.settle(fixture) {
            UniversalChatFixture.tab(fixture, id: tab.id)?.title == "Renamed"
        }
        #expect(UniversalChatFixture.tab(fixture, id: tab.id)?.title == "Renamed")

        fixture.model.closeTab(tab.id.rawValue, in: fixture.worktree)
        await UniversalChatFixture.settle(fixture) {
            UniversalChatFixture.tabs(fixture).isEmpty
        }
        #expect(UniversalChatFixture.tabs(fixture).isEmpty)
    }

    private func makeModel() -> AppModel {
        let suiteName = "dev.tiller.Tiller.AppModelFacadeTests"
        let defaults = UserDefaults(suiteName: suiteName)!
        defaults.removePersistentDomain(forName: suiteName)
        return AppModel(
            paneRegistry: PaneRegistry(), registrationTimeoutMs: 100, defaults: defaults)
    }

    private func makeModelAndWorktree() -> (AppModel, Worktree) {
        let model = makeModel()
        let worktree = Worktree(
            id: UUID(), projectId: UUID(), branch: "main", path: "/tmp/facade-tests")
        model.worktrees = [worktree.projectId: [worktree]]
        return (model, worktree)
    }
}

private actor AppModelFacadeWorkspacePersistence: WorkspaceLayoutPersistence {
    func restore(worktreeID: UUID) async -> RestoredWorkspace {
        RestoredWorkspace(
            layout: .empty(groupID: PaneGroupID(worktreeID)),
            tabs: [:], revision: 0, diagnostics: [])
    }

    func commitStructural(worktreeID: UUID, revision: Int, snapshot: WorkspaceSnapshot,
                          tabs: [WorkspaceTab],
                          terminalContents: [TerminalContentRecordValue],
                          browserContents: [BrowserContentRecordValue]) async throws {}

    func checkpoint(worktreeID: UUID, revision: Int, snapshot: WorkspaceSnapshot) async {}
    func flush(worktreeID: UUID) async throws {}
    nonisolated func writeRecoverySidecar(
        worktreeID: UUID, revision: Int, snapshot: WorkspaceSnapshot) throws {}
    func purge(worktreeID: UUID) async throws {}
}
