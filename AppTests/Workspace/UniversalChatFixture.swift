import Foundation
import TillerACP
import TillerCore
import TillerPersistence
import TillerTerminal
import TillerWorkspace

@testable import Tiller

/// Builds an AppModel whose chat tabs live on the universal workspace engine,
/// backed by a real in-memory ChatSessionStore — a chat tab's identity *is* a
/// session id, so a fake store would make the tabs unopenable.
@MainActor
enum UniversalChatFixture {
    struct Handle {
        let model: AppModel
        let worktree: Worktree
        let suiteName: String

        func cleanUp() { UserDefaults().removePersistentDomain(forName: suiteName) }
    }

    static func make(suiteName: String,
                     configureDefaults: (UserDefaults) -> Void = { _ in }) async throws -> Handle {
        let defaults = UserDefaults(suiteName: suiteName)!
        defaults.removePersistentDomain(forName: suiteName)
        configureDefaults(defaults)
        let model = AppModel(
            paneRegistry: PaneRegistry(), registrationTimeoutMs: 100, defaults: defaults,
            workspaceCoordinator: WorkspaceCoordinator(
                persistence: FakeWorkspacePersistence(),
                registry: WorkspaceContentRegistry(),
                adapters: [.chat: ChatContentAdapter()]))
        let worktree = Worktree(
            id: UUID(), projectId: UUID(), branch: "main",
            path: FileManager.default.temporaryDirectory.path)
        // chatSession.worktreeId is a foreign key: without these rows
        // createSession throws and no chat tab can ever be prepared.
        let database = try AppDatabase.inMemory()
        try database.write { db in
            try ProjectRecord(id: worktree.projectId.uuidString, name: "P",
                              rootPath: worktree.path, createdAt: Date()).insert(db)
            try WorktreeRecord(id: worktree.id.uuidString,
                               projectId: worktree.projectId.uuidString,
                               branch: worktree.branch, path: worktree.path,
                               createdAt: Date()).insert(db)
        }
        model.chatStore = ChatSessionStore(database: database)
        model.worktrees = [worktree.projectId: [worktree]]
        // Without a restored layout there is no group to open a tab into.
        await model.workspaceCoordinator.restore(worktree: worktree)
        return Handle(model: model, worktree: worktree, suiteName: suiteName)
    }

    /// Opens a chat tab and waits for it to appear. openChatTab's universal
    /// branch is fire-and-forget (Task { requestNewTab }), so the tab only
    /// exists once the coordinator has committed it.
    static func openChatTab(_ handle: Handle, agentId: String = "claude-acp") async -> WorkspaceTab? {
        let before = tabs(handle).count
        handle.model.openChatTab(agentId: agentId, in: handle.worktree)
        for _ in 0..<200 where tabs(handle).count == before {
            try? await Task.sleep(for: .milliseconds(10))
        }
        return tabs(handle).last
    }

    static func tabs(_ handle: Handle) -> [WorkspaceTab] {
        handle.model.workspaceCoordinator.layouts[handle.worktree.id]?.allTabs ?? []
    }

    static func tab(_ handle: Handle, id: WorkspaceTabID) -> WorkspaceTab? {
        tabs(handle).first { $0.id == id }
    }

    /// Waits for an async coordinator mutation (rename, close) to settle.
    static func settle(_ handle: Handle, until condition: @escaping () -> Bool) async {
        for _ in 0..<200 where !condition() {
            try? await Task.sleep(for: .milliseconds(10))
        }
    }
}
