import Foundation
import GRDB
import Testing
@testable import TillerPersistence

@Suite
struct WorkspaceSchemaTests {
    @Test func v16CreatesTheWorkspaceTablesWithTheOwnershipConstraint() throws {
        let database = try makeDatabase()
        let first = WorkspaceTabRecord(
            id: "tab-1", worktreeId: "worktree-1", title: "Terminal",
            titleIsAutoNamed: true, contentKind: "terminal", contentId: "c1",
            viewStateJSON: nil, viewStateVersion: 1, createdAt: Date())
        try database.write { db in try first.insert(db) }

        #expect(throws: (any Error).self) {
            try database.write { db in
                try WorkspaceTabRecord(
                    id: "tab-2", worktreeId: "worktree-1", title: "Duplicate",
                    titleIsAutoNamed: true, contentKind: "terminal", contentId: "c1",
                    viewStateJSON: nil, viewStateVersion: 1, createdAt: Date()
                ).insert(db)
            }
        }
        let tables = try database.read { db in
            try String.fetchAll(db, sql: """
                SELECT name FROM sqlite_master
                WHERE type = 'table' AND name IN
                    ('workspaceLayout', 'workspaceTab', 'terminalContent', 'workspaceLayoutQuarantine')
                ORDER BY name
                """)
        }
        #expect(tables == ["terminalContent", "workspaceLayout", "workspaceLayoutQuarantine", "workspaceTab"])
    }

    @Test func scrollbackAndAgentSessionKeepTheirValuesUnderTheNewColumnName() throws {
        let database = try makeDatabase()
        try database.write { db in
            try PaneScrollbackRecord(
                paneId: "content-1", worktreeId: "worktree-1", data: Data("scrollback".utf8),
                updatedAt: Date(timeIntervalSince1970: 100)
            ).insert(db)
            try AgentSessionRecord(
                paneId: "content-2", worktreeId: "worktree-1", agentId: "codex",
                sessionRef: "session-1", capturedAt: Date(timeIntervalSince1970: 200)
            ).insert(db)
        }

        try database.read { db in
            let scrollback = try Row.fetchOne(db, sql: "SELECT * FROM paneScrollback")
            #expect(scrollback?["terminalContentId"] as? String == "content-1")
            #expect(scrollback?["data"] as? Data == Data("scrollback".utf8))
            let session = try Row.fetchOne(db, sql: "SELECT * FROM agentSession")
            #expect(session?["terminalContentId"] as? String == "content-2")
            #expect(session?["sessionRef"] as? String == "session-1")
        }
    }

    @Test func deletingAWorktreeCascadesEveryWorkspaceRow() throws {
        let database = try makeDatabase()
        try database.write { db in
            try WorkspaceLayoutRecord(
                worktreeId: "worktree-1", schemaVersion: 1, revision: 1,
                payload: "{}", checksum: "checksum", updatedAt: Date()).insert(db)
            try WorkspaceTabRecord(
                id: "tab-1", worktreeId: "worktree-1", title: "Terminal",
                titleIsAutoNamed: true, contentKind: "terminal", contentId: "content-1",
                viewStateJSON: nil, viewStateVersion: 1, createdAt: Date()).insert(db)
            try TerminalContentRecord(
                id: "content-1", worktreeId: "worktree-1", launchKind: "shell",
                agentId: nil, commandJSON: nil, createdAt: Date()).insert(db)
            try WorkspaceLayoutQuarantineRecord(
                id: "quarantine-1", worktreeId: "worktree-1", payload: "{}",
                reason: "checksum", createdAt: Date()).insert(db)
            _ = try WorktreeRecord.deleteOne(db, key: "worktree-1")
        }

        let counts = try database.read { db in
            try [
                WorkspaceLayoutRecord.fetchCount(db),
                WorkspaceTabRecord.fetchCount(db),
                TerminalContentRecord.fetchCount(db),
                WorkspaceLayoutQuarantineRecord.fetchCount(db)
            ]
        }
        #expect(counts == [0, 0, 0, 0])
    }

    @Test func migratingAV15DatabaseTwiceIsIdempotent() throws {
        let queue = try DatabaseQueue()
        try AppDatabase.migrator.migrate(queue, upTo: "v15")
        try queue.write { db in
            try db.execute(sql: """
                INSERT INTO project (id, name, rootPath, createdAt)
                VALUES ('project-1', 'Project', '/tmp/project', ?)
                """, arguments: [Date()])
            try db.execute(sql: """
                INSERT INTO worktree (id, projectId, branch, path, createdAt)
                VALUES ('worktree-1', 'project-1', 'main', '/tmp/project', ?)
                """, arguments: [Date()])
            try db.execute(sql: """
                INSERT INTO paneScrollback (paneId, worktreeId, data, updatedAt)
                VALUES ('content-1', 'worktree-1', ?, ?)
                """, arguments: [Data([1, 2, 3]), Date()])
        }

        try AppDatabase.migrator.migrate(queue)
        try AppDatabase.migrator.migrate(queue)

        try queue.read { db in
            #expect(try AppDatabase.migrator.appliedMigrations(db).last == "v16")
            #expect(try db.tableExists("terminalTab"))
            #expect(!(try db.tableExists("legacyTerminalTab_v15")))
            let value = try String.fetchOne(
                db, sql: "SELECT terminalContentId FROM paneScrollback")
            #expect(value == "content-1")
        }
    }

    private func makeDatabase() throws -> AppDatabase {
        let database = try AppDatabase.inMemory()
        try database.write { db in
            try ProjectRecord(
                id: "project-1", name: "Project", rootPath: "/tmp/project", createdAt: Date()
            ).insert(db)
            try WorktreeRecord(
                id: "worktree-1", projectId: "project-1", branch: "main",
                path: "/tmp/project", createdAt: Date()
            ).insert(db)
        }
        return database
    }
}
