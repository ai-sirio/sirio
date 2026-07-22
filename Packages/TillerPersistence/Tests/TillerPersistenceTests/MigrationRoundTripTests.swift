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
        let scrollbackRow = try Row.fetchOne(db, sql: "SELECT * FROM paneScrollback WHERE paneId = ?", arguments: ["sp1"])
        #expect(scrollbackRow?["data"] as? Data == blob)
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
    #expect(identifiers == ["v1", "v2", "v3", "v4", "v5", "v6", "v7", "v8", "v9", "v10", "v11", "v12", "v13"])
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
