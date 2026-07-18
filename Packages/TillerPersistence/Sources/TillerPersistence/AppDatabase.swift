import Foundation
import GRDB

/// Thin wrapper owning the GRDB queue and the migrator. All schema lives
/// here; domain code never sees SQL.
public final class AppDatabase: Sendable {
    private let dbQueue: DatabaseQueue

    public init(path: String) throws {
        dbQueue = try DatabaseQueue(path: path)
        try Self.migrator.migrate(dbQueue)
    }

    public static func inMemory() throws -> AppDatabase {
        try AppDatabase(queue: DatabaseQueue())
    }

    private init(queue: DatabaseQueue) throws {
        dbQueue = queue
        try Self.migrator.migrate(dbQueue)
    }

    public func read<T>(_ block: (Database) throws -> T) throws -> T {
        try dbQueue.read(block)
    }

    @discardableResult
    public func write<T>(_ block: (Database) throws -> T) throws -> T {
        try dbQueue.write(block)
    }

    static var migrator: DatabaseMigrator {
        var migrator = DatabaseMigrator()
        migrator.registerMigration("v1") { db in
            try db.create(table: "project") { t in
                t.primaryKey("id", .text)
                t.column("name", .text).notNull()
                t.column("rootPath", .text).notNull()
                t.column("createdAt", .datetime).notNull()
            }
            try db.create(table: "worktree") { t in
                t.primaryKey("id", .text)
                t.column("projectId", .text).notNull()
                    .references("project", onDelete: .cascade)
                t.column("branch", .text).notNull()
                t.column("path", .text).notNull()
                t.column("createdAt", .datetime).notNull()
            }
            try db.create(table: "paneScrollback") { t in
                t.primaryKey("paneId", .text)
                t.column("worktreeId", .text).notNull()
                    .references("worktree", onDelete: .cascade)
                t.column("data", .blob).notNull()
                t.column("updatedAt", .datetime).notNull()
            }
        }
        migrator.registerMigration("v2") { db in
            try db.alter(table: "worktree") { t in
                t.add(column: "comment", .text)
                t.add(column: "commentUpdatedAt", .datetime)
                t.add(column: "isPrimary", .boolean).notNull().defaults(to: false)
            }
            try db.alter(table: "project") { t in
                t.add(column: "colorHex", .text)
            }
        }
        migrator.registerMigration("v3") { db in
            try db.create(table: "terminalTab") { t in
                t.primaryKey("id", .text)
                t.column("worktreeId", .text).notNull()
                    .references("worktree", onDelete: .cascade)
                t.column("title", .text).notNull()
                t.column("orderIdx", .integer).notNull()
                t.column("isActive", .boolean).notNull()
                t.column("treeJSON", .text).notNull()
                t.column("updatedAt", .datetime).notNull()
            }
        }
        migrator.registerMigration("v4") { db in
            try db.alter(table: "project") { t in
                t.add(column: "displayName", .text)
                t.add(column: "iconKind", .text).notNull().defaults(to: "icon")
                t.add(column: "iconValue", .text)
                t.add(column: "avatarImage", .blob)
                t.add(column: "defaultWorktreeBase", .text)
                t.add(column: "worktreeLocationOverride", .text)
            }
        }
        migrator.registerMigration("v5") { db in
            try db.create(table: "agentAccount") { t in
                t.primaryKey("id", .text)
                t.column("provider", .text).notNull()
                t.column("configDirPath", .text).notNull()
                t.column("label", .text).notNull()
                t.column("orgName", .text)
                t.column("createdAt", .datetime).notNull()
                t.column("lastAuthenticatedAt", .datetime).notNull()
            }
        }
        migrator.registerMigration("v6") { db in
            try db.create(table: "agentSession") { t in
                t.primaryKey("paneId", .text)
                t.column("worktreeId", .text).notNull()
                    .references("worktree", onDelete: .cascade)
                t.column("agentId", .text).notNull()
                t.column("sessionRef", .text).notNull()
                t.column("capturedAt", .datetime).notNull()
            }
        }
        migrator.registerMigration("v7") { db in
            try db.alter(table: "terminalTab") { t in
                t.add(column: "kind", .text).notNull().defaults(to: "terminal")
                t.add(column: "filePath", .text)
            }
        }
        migrator.registerMigration("v8") { db in
            try db.create(table: "chatSession") { t in
                t.primaryKey("id", .text)
                t.column("worktreeId", .text).notNull()
                    .references("worktree", onDelete: .cascade)
                t.column("agentId", .text).notNull()
                t.column("acpSessionId", .text)
                t.column("createdAt", .datetime).notNull()
                t.column("lastActivityAt", .datetime).notNull()
            }
            try db.create(table: "chatItem") { t in
                t.column("sessionId", .text).notNull()
                    .references("chatSession", onDelete: .cascade)
                t.column("ordinal", .integer).notNull()
                t.column("kind", .text).notNull()
                t.column("payload", .blob).notNull()
                t.primaryKey(["sessionId", "ordinal"])
            }
        }
        migrator.registerMigration("v9") { db in
            try db.alter(table: "terminalTab") { t in
                t.add(column: "chatAgentId", .text)
            }
        }
        return migrator
    }
}
