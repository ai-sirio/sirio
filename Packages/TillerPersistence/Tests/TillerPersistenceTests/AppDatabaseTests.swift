import Testing
import Foundation
import GRDB
@testable import TillerPersistence

@Test func migratorCreatesSchemaAndRoundTripsRecords() throws {
    let db = try AppDatabase.inMemory()
    let project = ProjectRecord(
        id: UUID().uuidString, name: "demo",
        rootPath: "/tmp/demo", createdAt: Date()
    )
    let worktree = WorktreeRecord(
        id: UUID().uuidString, projectId: project.id,
        branch: "main", path: "/tmp/demo", createdAt: Date()
    )
    try db.write { txn in
        try project.insert(txn)
        try worktree.insert(txn)
    }
    let loaded = try db.read { try WorktreeRecord.fetchAll($0) }
    #expect(loaded.count == 1)
    #expect(loaded[0].projectId == project.id)
}

@Test func deletingProjectCascadesToWorktrees() throws {
    let db = try AppDatabase.inMemory()
    let project = ProjectRecord(id: "p1", name: "x", rootPath: "/tmp/x", createdAt: Date())
    let worktree = WorktreeRecord(id: "w1", projectId: "p1", branch: "main", path: "/tmp/x", createdAt: Date())
    try db.write { txn in
        try project.insert(txn)
        try worktree.insert(txn)
        _ = try project.delete(txn)
    }
    #expect(try db.read { try WorktreeRecord.fetchCount($0) } == 0)
}

@Test func v18CreatesBrowserContentTableWithTheExpectedColumns() throws {
    let queue = try DatabaseQueue()
    try AppDatabase.migrator.migrate(queue)
    let columns = try queue.read { database in
        try database.columns(in: "browserContent").map(\.name)
    }

    #expect(columns == ["id", "worktreeId", "url", "title", "createdAt"])
}

@Test func v3CreatesTerminalTabTableAndRoundTripsRecord() throws {
    let db = try AppDatabase.inMemory()
    try db.write { database in
        try ProjectRecord(
            id: "p1", name: "demo", rootPath: "/tmp/demo", createdAt: Date()
        ).insert(database)
        try WorktreeRecord(
            id: "w1", projectId: "p1", branch: "main",
            path: "/tmp/demo", createdAt: Date()
        ).insert(database)
        try TerminalTabRecord(
            id: "t1", worktreeId: "w1", title: "Terminale 1", orderIdx: 0,
            isActive: true, treeJSON: "{}", updatedAt: Date()
        ).insert(database)
    }
    let fetched = try db.read { try TerminalTabRecord.fetchOne($0, key: "t1") }
    #expect(fetched?.title == "Terminale 1")
    #expect(fetched?.isActive == true)
}

// MARK: - Project settings (v4)

@Test func v4AddsProjectIconAndWorktreeDefaultColumns() throws {
    let db = try AppDatabase.inMemory()
    try db.write { database in
        try ProjectRecord(
            id: "p1", name: "demo", rootPath: "/tmp/demo", createdAt: Date(),
            displayName: "Demo Project", iconKind: "emoji", iconValue: "🚀",
            avatarImage: nil, defaultWorktreeBase: "develop",
            worktreeLocationOverride: "/tmp/worktrees"
        ).insert(database)
    }
    let fetched = try db.read { try ProjectRecord.fetchOne($0, key: "p1") }
    #expect(fetched?.displayName == "Demo Project")
    #expect(fetched?.iconKind == "emoji")
    #expect(fetched?.iconValue == "🚀")
    #expect(fetched?.defaultWorktreeBase == "develop")
    #expect(fetched?.worktreeLocationOverride == "/tmp/worktrees")
}

// MARK: - Agent accounts (v5)

@Test func v5CreatesAgentAccountTableAndRoundTripsRecord() throws {
    let db = try AppDatabase.inMemory()
    let account = AgentAccountRecord(
        id: "acct1", provider: "claude", configDirPath: "/tmp/acct1",
        label: "e.palmisano@reply.it", orgName: "e.palmisano@reply.it's Organization",
        createdAt: Date(), lastAuthenticatedAt: Date()
    )
    try db.write { try account.insert($0) }
    let fetched = try db.read { try AgentAccountRecord.fetchOne($0, key: "acct1") }
    #expect(fetched?.provider == "claude")
    #expect(fetched?.label == "e.palmisano@reply.it")
    #expect(fetched?.orgName == "e.palmisano@reply.it's Organization")
}

@Test func agentAccountsFilterByProvider() throws {
    let db = try AppDatabase.inMemory()
    try db.write { database in
        try AgentAccountRecord(
            id: "c1", provider: "claude", configDirPath: "/tmp/c1", label: "a@x.com",
            createdAt: Date(), lastAuthenticatedAt: Date()
        ).insert(database)
        try AgentAccountRecord(
            id: "x1", provider: "codex", configDirPath: "/tmp/x1", label: "b@x.com",
            createdAt: Date(), lastAuthenticatedAt: Date()
        ).insert(database)
    }
    let claudeOnly = try db.read {
        try AgentAccountRecord.filter(Column("provider") == "claude").fetchAll($0)
    }
    #expect(claudeOnly.count == 1)
    #expect(claudeOnly[0].id == "c1")
}

// MARK: - Agent sessions (v6)

@Test func v6CreatesAgentSessionTableAndRoundTripsRecord() throws {
    let db = try AppDatabase.inMemory()
    try db.write { database in
        try ProjectRecord(
            id: "p1", name: "demo", rootPath: "/tmp/demo", createdAt: Date()
        ).insert(database)
        try WorktreeRecord(
            id: "w1", projectId: "p1", branch: "main",
            path: "/tmp/demo", createdAt: Date()
        ).insert(database)
        try AgentSessionRecord(
            paneId: "pane1", worktreeId: "w1", agentId: "claude",
            sessionRef: "abc-123", capturedAt: Date()
        ).insert(database)
    }
    let fetched = try db.read { try AgentSessionRecord.fetchOne($0, key: "pane1") }
    #expect(fetched?.agentId == "claude")
    #expect(fetched?.sessionRef == "abc-123")
}

@Test func agentSessionUpsertReplacesRefForSamePane() throws {
    let db = try AppDatabase.inMemory()
    try db.write { database in
        try ProjectRecord(id: "p1", name: "x", rootPath: "/tmp/x", createdAt: Date()).insert(database)
        try WorktreeRecord(id: "w1", projectId: "p1", branch: "main", path: "/tmp/x", createdAt: Date()).insert(database)
        try AgentSessionRecord(paneId: "pane1", worktreeId: "w1", agentId: "claude",
                               sessionRef: "old", capturedAt: Date()).save(database)
        try AgentSessionRecord(paneId: "pane1", worktreeId: "w1", agentId: "claude",
                               sessionRef: "new", capturedAt: Date()).save(database)
    }
    let all = try db.read { try AgentSessionRecord.fetchAll($0) }
    #expect(all.count == 1)
    #expect(all[0].sessionRef == "new")
}

@Test func deletingWorktreeCascadesToAgentSessions() throws {
    let db = try AppDatabase.inMemory()
    try db.write { database in
        try ProjectRecord(id: "p1", name: "x", rootPath: "/tmp/x", createdAt: Date()).insert(database)
        try WorktreeRecord(id: "w1", projectId: "p1", branch: "main", path: "/tmp/x", createdAt: Date()).insert(database)
        try AgentSessionRecord(paneId: "pane1", worktreeId: "w1", agentId: "claude",
                               sessionRef: "abc", capturedAt: Date()).insert(database)
        _ = try WorktreeRecord.deleteOne(database, key: "w1")
    }
    #expect(try db.read { try AgentSessionRecord.fetchCount($0) } == 0)
}
@Test func migrationV11AddsTitleIsAutoNamedDefaultingFalse() throws {
    let db = try AppDatabase.inMemory()
    try db.write { database in
        try database.execute(sql: "INSERT INTO project (id, name, rootPath, createdAt, iconKind) VALUES ('p1','demo','/tmp',?,'icon')", arguments: [Date()])
        try database.execute(sql: "INSERT INTO worktree (id, projectId, branch, path, createdAt) VALUES ('w1','p1','main','/tmp',?)", arguments: [Date()])
        try database.execute(
            sql: """
            INSERT INTO terminalTab (id, worktreeId, title, orderIdx, isActive, treeJSON, updatedAt, kind)
            VALUES ('t1','w1','Terminale 1',0,1,'{}',?,'terminal')
            """, arguments: [Date()])
    }
    let value = try db.read { database in
        try Bool.fetchOne(database, sql: "SELECT titleIsAutoNamed FROM terminalTab WHERE id = 't1'")
    }
    #expect(value == false)
}

@Test func migrationV12FlipsExistingTabsToAutoNamed() throws {
    let queue = try DatabaseQueue()
    let migrator = AppDatabase.migrator
    try migrator.migrate(queue, upTo: "v11")
    try queue.write { database in
        try database.execute(sql: "INSERT INTO project (id, name, rootPath, createdAt, iconKind) VALUES ('p1','demo','/tmp',?,'icon')", arguments: [Date()])
        try database.execute(sql: "INSERT INTO worktree (id, projectId, branch, path, createdAt) VALUES ('w1','p1','main','/tmp',?)", arguments: [Date()])
        try database.execute(
            sql: """
            INSERT INTO terminalTab (id, worktreeId, title, orderIdx, isActive, treeJSON, updatedAt, kind)
            VALUES ('t1','w1','Terminale 1',0,1,'{}',?,'terminal')
            """, arguments: [Date()])
    }
    try migrator.migrate(queue)
    let value = try queue.read { database in
        try Bool.fetchOne(
            database, sql: "SELECT titleIsAutoNamed FROM legacyTerminalTab_v15 WHERE id = 't1'"
        )
    }
    #expect(value == true)
}

@Test func v13AddsSessionSettingsColumns() throws {
    let db = try AppDatabase.inMemory()
    try db.read { conn in
        let columns = try conn.columns(in: "chatSession").map(\.name)
        #expect(columns.contains("permissionMode"))
        #expect(columns.contains("selectedModel"))
        #expect(columns.contains("selectedEffort"))
        #expect(columns.contains("transportKind"))
    }
}

@Test func migrationV13DefaultsAndBackfillsTransportKind() throws {
    let queue = try DatabaseQueue()
    try AppDatabase.migrator.migrate(queue, upTo: "v12")
    try queue.write { database in
        try database.execute(sql: "INSERT INTO project (id, name, rootPath, createdAt, iconKind) VALUES ('p1','demo','/tmp',?,'icon')", arguments: [Date()])
        try database.execute(sql: "INSERT INTO worktree (id, projectId, branch, path, createdAt) VALUES ('w1','p1','main','/tmp',?)", arguments: [Date()])
        try database.execute(sql: """
            INSERT INTO chatSession (id, worktreeId, agentId, createdAt, lastActivityAt)
            VALUES ('s1', 'w1', 'claude-acp', ?, ?), ('s2', 'w1', 'other-agent', ?, ?)
            """, arguments: [Date(), Date(), Date(), Date()])
    }
    try AppDatabase.migrator.migrate(queue)
    let transportKinds = try queue.read { database in
        try String.fetchAll(database, sql: "SELECT transportKind FROM chatSession ORDER BY id")
    }
    #expect(transportKinds == ["native", "acp"])
}
