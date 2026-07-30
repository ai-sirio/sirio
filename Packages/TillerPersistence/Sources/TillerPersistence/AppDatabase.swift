import Foundation
import GRDB

/// Thin wrapper owning the GRDB queue and the migrator. All schema lives
/// here; domain code never sees SQL.
public final class AppDatabase: Sendable {
    private let dbQueue: DatabaseQueue
    public let databasePath: String?

    public init(path: String, upTo: String? = nil) throws {
        dbQueue = try DatabaseQueue(path: path)
        databasePath = path
        if let upTo {
            let applied = try dbQueue.read { db in
                try Self.migrator.appliedMigrations(db)
            }
            if !applied.contains(upTo) {
                try Self.migrator.migrate(dbQueue, upTo: upTo)
            }
        } else {
            try Self.migrator.migrate(dbQueue)
        }
    }

    public static func inMemory() throws -> AppDatabase {
        try AppDatabase(queue: DatabaseQueue())
    }

    private init(queue: DatabaseQueue) throws {
        dbQueue = queue
        databasePath = nil
        // In-memory databases remain a v16 fixture surface for legacy-store
        // tests. Persistent application databases opt into v17 explicitly so
        // the App-layer callback can read terminalTab before it is renamed.
        try Self.migrator.migrate(dbQueue, upTo: "v16")
    }

    /// Runs v17 with the App-layer data callback inside the same SQLite
    /// transaction as the schema rename. The callback is never invoked again
    /// after v17 has been recorded.
    public func migrateV17(using migration: @escaping @Sendable (Database) throws -> Void) throws {
        try Self.migrator(v17Migration: migration).migrate(dbQueue)
    }

    public func read<T>(_ block: (Database) throws -> T) throws -> T {
        try dbQueue.read(block)
    }

    @discardableResult
    public func write<T>(_ block: (Database) throws -> T) throws -> T {
        try dbQueue.write(block)
    }

    static var migrator: DatabaseMigrator {
        migrator(v17Migration: nil)
    }

    private static func migrator(
        v17Migration: (@Sendable (Database) throws -> Void)?
    ) -> DatabaseMigrator {
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
        migrator.registerMigration("v10") { db in
            try db.alter(table: "chatSession") { t in
                t.add(column: "contextUsageUsed", .integer)
                t.add(column: "contextUsageSize", .integer)
            }
        }
        migrator.registerMigration("v11") { db in
            try db.alter(table: "terminalTab") { t in
                t.add(column: "titleIsAutoNamed", .boolean).notNull().defaults(to: false)
            }
        }
        migrator.registerMigration("v12") { db in
            // v11 backfilled pre-feature tabs with false, excluding them from
            // auto-naming forever. Flip them: most had default titles, and a
            // manual rename done from now on re-protects a tab via renameTab.
            try db.execute(sql: "UPDATE terminalTab SET titleIsAutoNamed = 1")
        }
        migrator.registerMigration("v13") { db in
            try db.alter(table: "chatSession") { t in
                t.add(column: "permissionMode", .text)
                t.add(column: "selectedModel", .text)
                t.add(column: "selectedEffort", .text)
                t.add(column: "transportKind", .text).notNull().defaults(to: "acp")
            }
            // Existing sessions for the three native harnesses flip to native;
            // ids here are the canonical registry ids (AgentIdMigration).
            try db.execute(sql: """
                UPDATE chatSession SET transportKind = 'native'
                WHERE agentId IN ('claude-acp', 'codex-acp', 'opencode', 'claude', 'codex')
                """)
        }
        migrator.registerMigration("v14") { db in
            // Manual sidebar order for projects and worktrees. Backfilled from
            // rowid so every existing row keeps the order the user sees today
            // instead of collapsing into an arbitrary tie at 0.
            try db.alter(table: "project") { t in
                t.add(column: "orderIdx", .integer).notNull().defaults(to: 0)
            }
            try db.alter(table: "worktree") { t in
                t.add(column: "orderIdx", .integer).notNull().defaults(to: 0)
            }
            try db.execute(sql: "UPDATE project SET orderIdx = rowid")
            try db.execute(sql: "UPDATE worktree SET orderIdx = rowid")
        }
        migrator.registerMigration("v15") { db in
            try db.alter(table: "chatSession") { t in
                t.add(column: "title", .text)
            }
            try db.alter(table: "terminalTab") { t in
                t.add(column: "chatSessionId", .text)
            }
            // Bind each worktree's active chat tab to its most recent non-empty
            // session. Empty sessions are skipped for the same reason
            // latestSession skipped them: the agent cannot resume one either.
            // Other legacy chat tabs stay NULL and open a fresh session — the
            // ambiguity is settled once here rather than re-guessed on every launch.
            try db.execute(sql: """
                UPDATE terminalTab SET chatSessionId = (
                    SELECT s.id FROM chatSession s
                    WHERE s.worktreeId = terminalTab.worktreeId
                      AND EXISTS (SELECT 1 FROM chatItem i WHERE i.sessionId = s.id)
                    ORDER BY s.lastActivityAt DESC
                    LIMIT 1
                )
                WHERE kind = 'chat' AND isActive = 1
                """)
        }
        migrator.registerMigration("v16") { db in
            try db.create(table: "workspaceLayout") { t in
                t.primaryKey("worktreeId", .text)
                    .references("worktree", onDelete: .cascade)
                t.column("schemaVersion", .integer).notNull()
                t.column("revision", .integer).notNull()
                t.column("payload", .text).notNull()
                t.column("checksum", .text).notNull()
                t.column("updatedAt", .datetime).notNull()
            }
            try db.create(table: "workspaceTab") { t in
                t.primaryKey("id", .text)
                t.column("worktreeId", .text).notNull()
                    .references("worktree", onDelete: .cascade)
                t.column("title", .text).notNull()
                t.column("titleIsAutoNamed", .boolean).notNull()
                t.column("contentKind", .text).notNull()
                t.column("contentId", .text).notNull()
                t.column("viewStateJSON", .text)
                t.column("viewStateVersion", .integer).notNull()
                t.column("createdAt", .datetime).notNull()
                t.uniqueKey(["worktreeId", "contentKind", "contentId"])
            }
            try db.create(table: "terminalContent") { t in
                t.primaryKey("id", .text)
                t.column("worktreeId", .text).notNull()
                    .references("worktree", onDelete: .cascade)
                t.column("launchKind", .text).notNull()
                t.column("agentId", .text)
                t.column("commandJSON", .text)
                t.column("createdAt", .datetime).notNull()
            }
            try db.create(table: "workspaceLayoutQuarantine") { t in
                t.primaryKey("id", .text)
                t.column("worktreeId", .text).notNull()
                    .references("worktree", onDelete: .cascade)
                t.column("payload", .text).notNull()
                t.column("reason", .text).notNull()
                t.column("createdAt", .datetime).notNull()
            }

            try db.alter(table: "paneScrollback") { t in
                t.rename(column: "paneId", to: "terminalContentId")
            }
            try db.alter(table: "agentSession") { t in
                t.rename(column: "paneId", to: "terminalContentId")
            }
        }
        migrator.registerMigration("v17") { db in
            if let v17Migration {
                try v17Migration(db)
            }
            guard try db.tableExists("terminalTab") else { return }
            try db.rename(table: "terminalTab", to: "legacyTerminalTab_v15")
            try db.execute(sql: """
                CREATE TRIGGER legacyTerminalTab_v15_read_only_insert
                BEFORE INSERT ON legacyTerminalTab_v15
                BEGIN SELECT RAISE(ABORT, 'legacyTerminalTab_v15 is read-only'); END
                """)
            try db.execute(sql: """
                CREATE TRIGGER legacyTerminalTab_v15_read_only_update
                BEFORE UPDATE ON legacyTerminalTab_v15
                BEGIN SELECT RAISE(ABORT, 'legacyTerminalTab_v15 is read-only'); END
                """)
            try db.execute(sql: """
                CREATE TRIGGER legacyTerminalTab_v15_read_only_delete
                BEFORE DELETE ON legacyTerminalTab_v15
                BEGIN SELECT RAISE(ABORT, 'legacyTerminalTab_v15 is read-only'); END
                """)
        }
        return migrator
    }
}
