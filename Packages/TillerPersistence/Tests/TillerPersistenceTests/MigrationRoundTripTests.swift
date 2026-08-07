import Testing
import Foundation
import GRDB
@testable import TillerPersistence

/// A v1 database must survive full migration to head with all rows intact and
/// correct defaults applied for columns added in v2–v6.
@Test func v1DataSurvivesFullMigration() throws {
    let queue = try DatabaseQueue()
    try AppDatabase.migrator.migrate(queue, upTo: "v1")

    // Insert v1-shaped rows with raw SQL — the record structs describe the v4
    // shape so we must not use them for v1 inserts.
    let blob = Data([0xDE, 0xAD, 0xBE, 0xEF])
    try queue.write { db in
        try db.execute(sql: """
            INSERT INTO project (id, name, rootPath, createdAt)
            VALUES ('p1', 'migration-test', '/tmp/test', '2026-07-07 12:00:00')
            """)
        try db.execute(sql: """
            INSERT INTO worktree (id, projectId, branch, path, createdAt)
            VALUES ('w1', 'p1', 'main', '/tmp/test/src', '2026-07-07 12:00:00')
            """)
        try db.execute(sql: """
            INSERT INTO paneScrollback (paneId, worktreeId, data, updatedAt)
            VALUES ('sp1', 'w1', ?, '2026-07-07 12:00:00')
            """, arguments: [blob])
    }

    // Migrate to head
    try AppDatabase.migrator.migrate(queue)

    // Assert all v1 data survived and v2/v4 defaults are correct
    try queue.read { db in
        // 1) project row still present with original name
        let projectRow = try Row.fetchOne(db, sql: "SELECT * FROM project WHERE id = ?", arguments: ["p1"])
        #expect(projectRow?["name"] as? String == "migration-test")

        // 2) iconKind defaulted to "icon" (v4 default)
        #expect(projectRow?["iconKind"] as? String == "icon")

        // 3) worktree.isPrimary defaulted to false (v2 default)
        let worktreeRow = try Row.fetchOne(db, sql: "SELECT * FROM worktree WHERE id = ?", arguments: ["w1"])
        // GRDB stores booleans as INTEGER — cast through Int64
        #expect(worktreeRow?["isPrimary"] as? Int64 == 0)

        // 4) paneScrollback.data blob byte-identical
        let scrollbackRow = try Row.fetchOne(db, sql: "SELECT * FROM paneScrollback WHERE terminalContentId = ?", arguments: ["sp1"])
        #expect(scrollbackRow?["data"] as? Data == blob)
    }
}

/// v14 backfills the manual sidebar order from rowid, so rows that predate the
/// feature keep the order the user already sees instead of tying at 0 and
/// coming back in whatever order SQLite happens to pick.
@Test func v14BackfillsManualOrderFromInsertionOrder() throws {
    // Arrange: three projects and two worktrees inserted before v14 exists.
    let queue = try DatabaseQueue()
    try AppDatabase.migrator.migrate(queue, upTo: "v13")
    try queue.write { db in
        for (id, name) in [("p1", "first"), ("p2", "second"), ("p3", "third")] {
            try db.execute(sql: """
                INSERT INTO project (id, name, rootPath, createdAt)
                VALUES (?, ?, '/tmp/test', '2026-07-07 12:00:00')
                """, arguments: [id, name])
        }
        for (id, branch) in [("w1", "main"), ("w2", "feat")] {
            try db.execute(sql: """
                INSERT INTO worktree (id, projectId, branch, path, createdAt)
                VALUES (?, 'p1', ?, '/tmp/test/src', '2026-07-07 12:00:00')
                """, arguments: [id, branch])
        }
    }

    // Act
    try AppDatabase.migrator.migrate(queue, upTo: "v14")

    // Assert: distinct, strictly increasing indices in insertion order.
    try queue.read { db in
        let projectIds = try String.fetchAll(db, sql: "SELECT id FROM project ORDER BY orderIdx")
        #expect(projectIds == ["p1", "p2", "p3"])
        let indices = try Int.fetchAll(db, sql: "SELECT orderIdx FROM project ORDER BY orderIdx")
        #expect(Set(indices).count == indices.count)

        let worktreeIds = try String.fetchAll(db, sql: "SELECT id FROM worktree ORDER BY orderIdx")
        #expect(worktreeIds == ["w1", "w2"])
    }
}

/// The migrator must apply all migrations in registration order so that a
/// database created today can be migrated from any intermediate version.
@Test func migrationsAreOrderedAndComplete() throws {
    let queue = try DatabaseQueue()
    try AppDatabase.migrator.migrate(queue)

    let identifiers = try queue.read { db in
        try AppDatabase.migrator.appliedMigrations(db)
    }
    #expect(identifiers == ["v1", "v2", "v3", "v4", "v5", "v6", "v7", "v8", "v9", "v10", "v11", "v12", "v13", "v14", "v15", "v16", "v17", "v18"])
}

/// Applying migrations one at a time (stepwise) must produce the same final
/// schema as a single head migration.  If any increment is not self-contained,
/// the schemas will diverge.
@Test func stepwiseMigrationMatchesDirectMigration() throws {
    // Queue A: stepwise — one identifier at a time
    let queueA = try DatabaseQueue()
    try AppDatabase.migrator.migrate(queueA, upTo: "v1")
    try AppDatabase.migrator.migrate(queueA, upTo: "v2")
    try AppDatabase.migrator.migrate(queueA, upTo: "v3")
    try AppDatabase.migrator.migrate(queueA, upTo: "v4")
    try AppDatabase.migrator.migrate(queueA, upTo: "v5")
    try AppDatabase.migrator.migrate(queueA, upTo: "v6")
    try AppDatabase.migrator.migrate(queueA, upTo: "v7")
    try AppDatabase.migrator.migrate(queueA, upTo: "v8")
    try AppDatabase.migrator.migrate(queueA, upTo: "v9")
    try AppDatabase.migrator.migrate(queueA, upTo: "v10")
    try AppDatabase.migrator.migrate(queueA, upTo: "v11")
    try AppDatabase.migrator.migrate(queueA, upTo: "v12")
    try AppDatabase.migrator.migrate(queueA, upTo: "v13")
    try AppDatabase.migrator.migrate(queueA, upTo: "v14")
    try AppDatabase.migrator.migrate(queueA, upTo: "v15")
    try AppDatabase.migrator.migrate(queueA, upTo: "v16")
    try AppDatabase.migrator.migrate(queueA, upTo: "v17")
    try AppDatabase.migrator.migrate(queueA, upTo: "v18")

    // Queue B: direct to head
    let queueB = try DatabaseQueue()
    try AppDatabase.migrator.migrate(queueB)

    // Compare schema SQL for the user tables
    let schemaSQL = { (queue: DatabaseQueue) -> [String] in
        try queue.read { db in
            let rows = try Row.fetchAll(db, sql: """
                SELECT sql FROM sqlite_master
                WHERE type = 'table'
                  AND name NOT LIKE 'sqlite_%'
                  AND name != 'grdb_migrations'
                ORDER BY name
                """)
            return rows.compactMap { $0["sql"] as? String }
        }
    }

    let sqlA = try schemaSQL(queueA).map(normalizeSQL)
    let sqlB = try schemaSQL(queueB).map(normalizeSQL)

    #expect(sqlA.count == sqlB.count)
    for (a, b) in zip(sqlA, sqlB) {
        #expect(a == b)
    }
}

/// Normalize CREATE TABLE SQL for comparison: collapse runs of whitespace,
/// strip leading/trailing space.
private func normalizeSQL(_ sql: String) -> String {
    sql
        .components(separatedBy: .whitespacesAndNewlines)
        .filter { !$0.isEmpty }
        .joined(separator: " ")
}

/// v15 binds each worktree's active chat tab to that worktree's most recent
/// non-empty session, so a restored tab loads its own transcript instead of
/// guessing at runtime. Sessions with no items are skipped: the agent side
/// cannot resume them either.
@Test func v15BindsActiveChatTabToLatestNonEmptySession() throws {
    let queue = try DatabaseQueue()
    try AppDatabase.migrator.migrate(queue, upTo: "v14")
    try queue.write { db in
        try db.execute(sql: """
            INSERT INTO project (id, name, rootPath, createdAt)
            VALUES ('p1', 'proj', '/tmp/p', '2026-07-01 10:00:00')
            """)
        try db.execute(sql: """
            INSERT INTO worktree (id, projectId, branch, path, createdAt)
            VALUES ('w1', 'p1', 'main', '/tmp/p', '2026-07-01 10:00:00')
            """)
        // Older session WITH items — the one a resume can actually use.
        try db.execute(sql: """
            INSERT INTO chatSession (id, worktreeId, agentId, createdAt, lastActivityAt)
            VALUES ('s-used', 'w1', 'claude-acp', '2026-07-01 10:00:00', '2026-07-01 11:00:00')
            """)
        try db.execute(sql: """
            INSERT INTO chatItem (sessionId, ordinal, kind, payload)
            VALUES ('s-used', 0, 'agentMessage', X'7B7D')
            """)
        // Newer session with NO items — must be ignored.
        try db.execute(sql: """
            INSERT INTO chatSession (id, worktreeId, agentId, createdAt, lastActivityAt)
            VALUES ('s-empty', 'w1', 'claude-acp', '2026-07-01 12:00:00', '2026-07-01 12:00:00')
            """)
        try db.execute(sql: """
            INSERT INTO terminalTab (id, worktreeId, title, orderIdx, isActive, treeJSON,
                                     updatedAt, kind, chatAgentId)
            VALUES ('t-active', 'w1', 'Chat', 0, 1, '', '2026-07-01 12:00:00', 'chat', 'claude-acp')
            """)
        try db.execute(sql: """
            INSERT INTO terminalTab (id, worktreeId, title, orderIdx, isActive, treeJSON,
                                     updatedAt, kind, chatAgentId)
            VALUES ('t-other', 'w1', 'Chat', 1, 0, '', '2026-07-01 12:00:00', 'chat', 'claude-acp')
            """)
    }

    try AppDatabase.migrator.migrate(queue)

    try queue.read { db in
        let active = try Row.fetchOne(db, sql: "SELECT * FROM legacyTerminalTab_v15 WHERE id = 't-active'")
        #expect(active?["chatSessionId"] as? String == "s-used")
        let other = try Row.fetchOne(db, sql: "SELECT * FROM legacyTerminalTab_v15 WHERE id = 't-other'")
        #expect(other?["chatSessionId"] as? String == nil)
        let session = try Row.fetchOne(db, sql: "SELECT * FROM chatSession WHERE id = 's-used'")
        #expect(session?["title"] as? String == nil)
    }
}
