import Foundation
import Testing
import TillerACP
import TillerPersistence
@testable import Tiller

@Suite(.serialized)
@MainActor
struct ChatControllerTests {
    @Test func newChatDoesNotResumeLatestWorktreeSession() async throws {
        let database = try AppDatabase.inMemory()
        let projectId = UUID().uuidString
        let worktreeId = UUID()
        try database.write { db in
            try ProjectRecord(id: projectId, name: "P", rootPath: "/tmp/chat-controller",
                              createdAt: Date()).insert(db)
            try WorktreeRecord(id: worktreeId.uuidString, projectId: projectId,
                               branch: "main", path: "/tmp/chat-controller",
                               createdAt: Date()).insert(db)
        }
        let store = ChatSessionStore(database: database)
        let previous = try store.createSession(
            worktreeId: worktreeId.uuidString, agentId: "claude-acp")
        try store.setACPSessionId("old-session", sessionId: previous.id)
        try store.saveTranscript(sessionId: previous.id, items: [
            .agentMessage(id: "old-message", text: "old chat", isComplete: true)
        ])

        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let installStore = AgentInstallStore(rootDirectory: root)

        let controller = ChatController(
            tabId: UUID(), agentId: "claude-acp", worktreeId: worktreeId,
            worktreePath: root.path, store: store, installStore: installStore,
            startNewConversation: true)
        await controller.start()

        #expect(controller.state == ChatController.ChatState.disconnected(message: "Agent not installed. Install it from Settings → Agents."))
        #expect(controller.items.isEmpty)
    }
}