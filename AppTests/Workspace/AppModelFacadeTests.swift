import Foundation
import Testing
import TillerCore
import TillerTerminal

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

    @Test func autoRenameWritesThroughTheCoordinator() {
        let (model, worktree) = makeModelAndWorktree()
        let tab = model.openChatTab(agentId: "claude-acp", in: worktree)!

        model.applyAutoTitle(tab.id, in: worktree.id, title: "Fix login bug")

        let updated = model.workspaceCoordinator.legacyTabs(for: worktree.id)
            .first { $0.id == tab.id }
        #expect(updated?.title == "Fix login bug")
        #expect(updated?.titleIsAutoNamed == true)
    }

    @Test func fileDropOnTheWorkspaceOpensOrMovesTheDocumentTab() throws {
        let (model, worktree) = makeModelAndWorktree()
        let url = URL(fileURLWithPath: NSTemporaryDirectory())
            .appendingPathComponent("tiller-facade-\(UUID().uuidString).txt")
        try Data("hello".utf8).write(to: url)
        defer { try? FileManager.default.removeItem(at: url) }

        let first = model.openDocument(fileURL: url, in: worktree)
        let second = model.openDocument(fileURL: url, in: worktree)

        #expect(first?.id == second?.id)
        #expect(model.workspaceCoordinator.legacyTabs(for: worktree.id)
            .filter { $0.fileURL?.standardizedFileURL == url.standardizedFileURL }.count == 1)
    }

    @Test func everyMutationRouteReachesTheSameCoordinatorSeam() {
        let (model, worktree) = makeModelAndWorktree()
        let coordinator = model.workspaceCoordinator
        let initialRevision = coordinator.legacyMutationRevision(for: worktree.id)
        let tab = model.openChatTab(agentId: "claude-acp", in: worktree)!
        let afterOpen = coordinator.legacyMutationRevision(for: worktree.id)

        model.renameTab(tab.id, in: worktree.id, to: "Renamed")
        let afterRename = coordinator.legacyMutationRevision(for: worktree.id)
        model.closeTab(tab.id, in: worktree)
        let afterClose = coordinator.legacyMutationRevision(for: worktree.id)

        #expect(afterOpen > initialRevision)
        #expect(afterRename > afterOpen)
        #expect(afterClose > afterRename)
        #expect(coordinator.legacyTabs(for: worktree.id).isEmpty)
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
