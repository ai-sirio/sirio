import Testing
import Foundation
@testable import TillerCore
import TillerPersistence

@Test func saveAndLoadRoundTripsTerminalAndMarkdownTabs() async throws {
    let db = try AppDatabase.inMemory()
    let store = ProjectStore(database: db)
    let project = try await store.addProject(name: "p", rootPath: "/tmp/p")
    let worktree = try await store.addWorktree(projectId: project.id, branch: "main", path: "/tmp/p")

    let terminal = WorkspaceTab(id: UUID(), title: "Terminale 1", tree: .leaf(id: UUID()))
    let markdown = WorkspaceTab(
        id: UUID(), title: "README.md",
        content: .markdown(fileURL: URL(fileURLWithPath: "/tmp/p/README.md"))
    )
    let code = WorkspaceTab(
        id: UUID(), title: "App.swift",
        content: .code(fileURL: URL(fileURLWithPath: "/tmp/p/App.swift")))
    try await store.saveTabs(
        worktreeId: worktree.id,
        tabs: [terminal, markdown, code],
        activeTabId: code.id)

    let loaded = try await store.loadTabs(of: worktree.id)
    #expect(loaded.tabs == [terminal, markdown, code])
    #expect(loaded.activeTabId == code.id)
}

@Test func chatTabRoundTripsItsSessionId() async throws {
    let db = try AppDatabase.inMemory()
    let store = ProjectStore(database: db)
    let project = try await store.addProject(name: "p", rootPath: "/tmp/p")
    let worktree = try await store.addWorktree(projectId: project.id, branch: "main", path: "/tmp/p")

    let bound = WorkspaceTab(id: UUID(), title: "Chat",
                             content: .chat(agentId: "claude-acp", sessionId: "s-1"))
    let legacy = WorkspaceTab(id: UUID(), title: "Chat",
                              content: .chat(agentId: "claude-acp", sessionId: nil))
    try await store.saveTabs(worktreeId: worktree.id, tabs: [bound, legacy],
                             activeTabId: bound.id)

    let loaded = try await store.loadTabs(of: worktree.id)

    #expect(loaded.tabs == [bound, legacy])
    #expect(loaded.tabs.first?.chatSessionId == "s-1")
    #expect(loaded.tabs.last?.chatSessionId == nil)
}

@Test func markdownRecordWithoutFilePathIsSkipped() async throws {
    let db = try AppDatabase.inMemory()
    let store = ProjectStore(database: db)
    let project = try await store.addProject(name: "p", rootPath: "/tmp/p")
    let worktree = try await store.addWorktree(projectId: project.id, branch: "main", path: "/tmp/p")
    // Riga corrotta scritta a mano: kind markdown ma filePath NULL.
    try db.write { dbConn in
        try dbConn.execute(
            sql: """
            INSERT INTO terminalTab (id, worktreeId, title, orderIdx, isActive, treeJSON, updatedAt, kind, filePath)
            VALUES (?, ?, 'x.md', 0, 1, '', ?, 'markdown', NULL)
            """,
            arguments: [UUID().uuidString, worktree.id.uuidString, Date()]
        )
    }
    let loaded = try await store.loadTabs(of: worktree.id)
    #expect(loaded.tabs.isEmpty)
}

@Test func codeRecordWithoutFilePathIsSkipped() async throws {
    let db = try AppDatabase.inMemory()
    let store = ProjectStore(database: db)
    let project = try await store.addProject(name: "p", rootPath: "/tmp/p")
    let worktree = try await store.addWorktree(projectId: project.id, branch: "main", path: "/tmp/p")
    // Corrupt row written by hand: kind code but filePath NULL.
    try db.write { dbConn in
        try dbConn.execute(
            sql: """
            INSERT INTO terminalTab (id, worktreeId, title, orderIdx, isActive, treeJSON, updatedAt, kind, filePath)
            VALUES (?, ?, 'x.swift', 0, 1, '', ?, 'code', NULL)
            """,
            arguments: [UUID().uuidString, worktree.id.uuidString, Date()]
        )
    }
    let loaded = try await store.loadTabs(of: worktree.id)
    #expect(loaded.tabs.isEmpty)
}
