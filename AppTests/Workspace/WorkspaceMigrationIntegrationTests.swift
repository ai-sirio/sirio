import Foundation
import GRDB
import Testing
import TillerCore
import TillerPersistence
@testable import Tiller

@Suite(.serialized)
struct WorkspaceMigrationIntegrationTests {
    @Test func migrationRunsInOneTransactionAndRollsBackCompletelyOnFailure() throws {
        let fixture = try makeFixture()
        defer { fixture.remove() }
        let before = try legacyRows(fixture.database)

        #expect(throws: (any Error).self) {
            try SQLiteWorkspacePersistence.migrateV15IfNeeded(
                database: fixture.database, backupDirectory: fixture.backupDirectory,
                failAtWriteBoundary: 2)
        }

        #expect(try fixture.database.read { db in
            try String.fetchOne(
                db, sql: "SELECT identifier FROM grdb_migrations ORDER BY rowid DESC LIMIT 1"
            ) == "v16"
        })
        #expect(try fixture.database.read { try $0.tableExists("terminalTab") })
        #expect(!(try fixture.database.read { try $0.tableExists("legacyTerminalTab_v15") }))
        #expect(try legacyRows(fixture.database) == before)
        #expect(try fixture.database.read { try WorkspaceTabRecord.fetchCount($0) } == 0)
    }

    @Test func afterMigrationTerminalTabIsRenamedAndNeverWrittenAgain() throws {
        let fixture = try makeFixture()
        defer { fixture.remove() }

        try SQLiteWorkspacePersistence.migrateV15IfNeeded(
            database: fixture.database, backupDirectory: fixture.backupDirectory)

        #expect(try fixture.database.read { try $0.tableExists("legacyTerminalTab_v15") })
        #expect(!(try fixture.database.read { try $0.tableExists("terminalTab") }))
        #expect(try fixture.database.read { try WorkspaceLayoutRecord.fetchCount($0) } == 1)
        #expect(try fixture.database.read { try WorkspaceTabRecord.fetchCount($0) } == 1)
        #expect(throws: (any Error).self) {
            try fixture.database.write { db in
                try db.execute(sql: "DELETE FROM legacyTerminalTab_v15")
            }
        }
    }

    @Test func interruptingTheMigrationAtEveryWriteBoundaryLeavesTheV15DbIntact() throws {
        for boundary in 0..<SQLiteWorkspacePersistence.v15WriteBoundaryCount {
            let fixture = try makeFixture()
            let before = try legacyRows(fixture.database)
            defer { fixture.remove() }

            #expect(throws: (any Error).self) {
                try SQLiteWorkspacePersistence.migrateV15IfNeeded(
                    database: fixture.database, backupDirectory: fixture.backupDirectory,
                    failAtWriteBoundary: boundary)
            }
            #expect(try legacyRows(fixture.database) == before)
            #expect(try fixture.database.read { try $0.tableExists("terminalTab") })
            #expect(!(try fixture.database.read { try $0.tableExists("legacyTerminalTab_v15") }))
        }
    }

    @Test func aTimestampedBackupExistsBeforeTheMigrationRuns() throws {
        let fixture = try makeFixture()
        defer { fixture.remove() }

        try SQLiteWorkspacePersistence.migrateV15IfNeeded(
            database: fixture.database, backupDirectory: fixture.backupDirectory,
            now: { Date(timeIntervalSince1970: 1_753_000_000) })

        let backups = try FileManager.default.contentsOfDirectory(
            at: fixture.backupDirectory, includingPropertiesForKeys: nil)
        #expect(backups.count == 1)
        #expect(backups[0].lastPathComponent.hasPrefix("tiller-v15-"))
        #expect(backups[0].pathExtension == "sqlite")
        #expect(try Data(contentsOf: backups[0]).isEmpty == false)
    }

    @Test func repeatedLaunchesDoNotMigrateTwice() throws {
        let fixture = try makeFixture()
        defer { fixture.remove() }

        try SQLiteWorkspacePersistence.migrateV15IfNeeded(
            database: fixture.database, backupDirectory: fixture.backupDirectory)
        let firstLayout = try fixture.database.read { try WorkspaceLayoutRecord.fetchAll($0) }
        let firstTabs = try fixture.database.read { try WorkspaceTabRecord.fetchAll($0) }
        let firstBackups = try FileManager.default.contentsOfDirectory(
            at: fixture.backupDirectory, includingPropertiesForKeys: nil)

        let reopened = try AppDatabase(path: fixture.database.databasePath!, upTo: "v16")
        try SQLiteWorkspacePersistence.migrateV15IfNeeded(
            database: reopened, backupDirectory: fixture.backupDirectory)

        #expect(try reopened.read { try WorkspaceLayoutRecord.fetchAll($0) } == firstLayout)
        #expect(try reopened.read { try WorkspaceTabRecord.fetchAll($0) } == firstTabs)
        #expect(try FileManager.default.contentsOfDirectory(
            at: fixture.backupDirectory, includingPropertiesForKeys: nil) == firstBackups)
    }

    private struct Fixture {
        let root: URL
        let database: AppDatabase
        let backupDirectory: URL

        func remove() {
            try? FileManager.default.removeItem(at: root)
        }
    }

    private func makeFixture() throws -> Fixture {
        let root = URL(fileURLWithPath: NSTemporaryDirectory())
            .appendingPathComponent("tiller-v15-migration-\(UUID().uuidString)", isDirectory: true)
        let backupDirectory = root.appendingPathComponent("Application Support/Tiller/backups", isDirectory: true)
        try FileManager.default.createDirectory(at: backupDirectory, withIntermediateDirectories: true)
        let databaseURL = root.appendingPathComponent("tiller.sqlite")
        let database = try AppDatabase(path: databaseURL.path, upTo: "v16")
        let worktreeID = "00000000-0000-4000-8000-000000000099"
        let leafID = "00000000-0000-4000-8000-0000000000d1"
        let tree = "{\"leaf\":{\"id\":\"\(leafID)\"}}"
        try database.write { db in
            try ProjectRecord(id: "project-1", name: "Project", rootPath: root.path, createdAt: Date()).insert(db)
            try WorktreeRecord(
                id: worktreeID, projectId: "project-1", branch: "main", path: root.path,
                createdAt: Date()).insert(db)
            try db.execute(sql: """
                INSERT INTO terminalTab
                    (id, worktreeId, title, orderIdx, isActive, treeJSON, updatedAt,
                     kind, filePath, chatAgentId, chatSessionId, titleIsAutoNamed)
                VALUES ('00000000-0000-4000-8000-0000000000d2', ?, 'Terminale 1', 0, 1, ?, ?,
                        'terminal', NULL, NULL, NULL, 1)
                """, arguments: [worktreeID, tree, Date()])
        }
        return Fixture(root: root, database: database, backupDirectory: backupDirectory)
    }

    private func legacyRows(_ database: AppDatabase) throws -> [[String?]] {
        try database.read { db in
            try Row.fetchAll(db, sql: """
                SELECT id, worktreeId, title, orderIdx, isActive, treeJSON, kind,
                       filePath, chatAgentId, chatSessionId, titleIsAutoNamed
                FROM terminalTab ORDER BY orderIdx
                """).map { row in
                    let id: String? = row["id"]
                    let worktreeID: String? = row["worktreeId"]
                    let title: String? = row["title"]
                    let orderIdx: String? = (row["orderIdx"] as Int64?).map(String.init)
                    let isActive: String? = (row["isActive"] as Int64?).map(String.init)
                    let treeJSON: String? = row["treeJSON"]
                    let kind: String? = row["kind"]
                    let filePath: String? = row["filePath"]
                    let chatAgentID: String? = row["chatAgentId"]
                    let chatSessionID: String? = row["chatSessionId"]
                    let titleIsAutoNamed: String? =
                        (row["titleIsAutoNamed"] as Int64?).map(String.init)
                    return [id, worktreeID, title, orderIdx, isActive, treeJSON, kind,
                            filePath, chatAgentID, chatSessionID, titleIsAutoNamed]
                }
        }
    }
}
