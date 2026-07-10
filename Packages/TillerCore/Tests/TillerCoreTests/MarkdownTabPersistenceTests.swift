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
    try await store.saveTabs(worktreeId: worktree.id, tabs: [terminal, markdown],
                             activeTabId: markdown.id)

    let loaded = try await store.loadTabs(of: worktree.id)
    #expect(loaded.tabs == [terminal, markdown])
    #expect(loaded.activeTabId == markdown.id)
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
