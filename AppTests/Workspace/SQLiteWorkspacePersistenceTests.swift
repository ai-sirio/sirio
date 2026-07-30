import Foundation
import GRDB
import Testing
import TillerCore
import TillerPersistence
@testable import Tiller

@Suite(.serialized)
struct SQLiteWorkspacePersistenceTests {
    @Test func structuralCommitWritesSnapshotAndTabsInOneTransaction() async throws {
        let (database, worktreeID) = try makeDatabase()
        let persistence = SQLiteWorkspacePersistence(database: database)
        let fixture = makeWorkspace()

        try await persistence.commitStructural(
            worktreeID: worktreeID, revision: 7, snapshot: fixture.snapshot,
            tabs: [fixture.tab], terminalContents: [fixture.content])

        let restored = await persistence.restore(worktreeID: worktreeID)
        #expect(restored.revision == 7)
        #expect(restored.layout == fixture.layout)
        #expect(restored.tabs[fixture.tab.id] == fixture.tab)
        #expect(try database.read { try WorkspaceTabRecord.fetchCount($0) } == 1)
        #expect(try database.read { try TerminalContentRecord.fetchCount($0) } == 1)
    }

    @Test func aFailedStructuralCommitLeavesTheStoredRevisionUnchanged() async throws {
        let (database, worktreeID) = try makeDatabase()
        let persistence = SQLiteWorkspacePersistence(database: database)
        let fixture = makeWorkspace()
        try await persistence.commitStructural(
            worktreeID: worktreeID, revision: 1, snapshot: fixture.snapshot,
            tabs: [fixture.tab], terminalContents: [fixture.content])

        await #expect(throws: (any Error).self) {
            try await persistence.commitStructural(
                worktreeID: worktreeID, revision: 2, snapshot: fixture.snapshot,
                tabs: [fixture.tab], terminalContents: [])
        }

        let row = try database.read { try WorkspaceLayoutRecord.fetchOne($0, key: worktreeID.uuidString) }
        #expect(row?.revision == 1)
        #expect(try database.read { try WorkspaceTabRecord.fetchCount($0) } == 1)
        #expect(try database.read { try TerminalContentRecord.fetchCount($0) } == 1)
    }

    @Test func checksumMismatchOnRestoreQuarantinesAndSalvages() async throws {
        let (database, worktreeID) = try makeDatabase()
        let fixture = makeWorkspace()
        let payload = String(decoding: try fixture.snapshot.canonicalPayload(), as: UTF8.self)
        try database.write { db in
            try WorkspaceLayoutRecord(
                worktreeId: worktreeID.uuidString, schemaVersion: 1, revision: 4,
                payload: payload, checksum: "wrong", updatedAt: Date()).save(db)
        }
        let persistence = SQLiteWorkspacePersistence(database: database)

        let restored = await persistence.restore(worktreeID: worktreeID)
        #expect(restored.layout.groups.count == 1)
        #expect(restored.layout.allTabs.isEmpty)
        #expect(restored.diagnostics.contains(.quarantinedSnapshot(reason: "checksum")))
        #expect(try database.read {
            try WorkspaceLayoutQuarantineRecord
                .filter(Column("worktreeId") == worktreeID.uuidString)
                .fetchCount($0)
        } == 1)
        #expect(try database.read {
            try WorkspaceLayoutRecord.fetchOne($0, key: worktreeID.uuidString)?.revision == 5
        })
    }

    @Test func oneMissingTabRowIsRemovedWithoutFallingBackToASinglePane() async throws {
        let (database, worktreeID) = try makeDatabase()
        let persistence = SQLiteWorkspacePersistence(database: database)
        let fixture = makeTwoTabWorkspace()
        try await persistence.commitStructural(
            worktreeID: worktreeID, revision: 3, snapshot: fixture.snapshot,
            tabs: fixture.tabs, terminalContents: fixture.contents)
        try database.write { db in
            try db.execute(sql: "DELETE FROM workspaceTab WHERE id = ?", arguments: [fixture.tabs[1].id.rawValue.uuidString])
        }

        let restored = await persistence.restore(worktreeID: worktreeID)
        #expect(restored.layout.groups.count == 1)
        #expect(restored.layout.allTabs.map(\.id) == [fixture.tabs[0].id])
        #expect(restored.diagnostics.contains(.removedMissingTab(fixture.tabs[1].id)))
    }

    @Test func quarantineKeepsAtMostThreePayloadsPerWorktree() async throws {
        let (database, worktreeID) = try makeDatabase()
        let fixture = makeWorkspace()
        let payload = String(decoding: try fixture.snapshot.canonicalPayload(), as: UTF8.self)
        let persistence = SQLiteWorkspacePersistence(database: database)

        for revision in 1...4 {
            try database.write { db in
                try WorkspaceLayoutRecord(
                    worktreeId: worktreeID.uuidString, schemaVersion: 1, revision: revision,
                    payload: payload, checksum: "bad-\(revision)", updatedAt: Date()).save(db)
            }
            _ = await persistence.restore(worktreeID: worktreeID)
        }

        #expect(try database.read {
            try WorkspaceLayoutQuarantineRecord
                .filter(Column("worktreeId") == worktreeID.uuidString)
                .fetchCount($0)
        } == 3)
    }

    @Test func anOlderRevisionCompletingLateNeverOverwritesANewerOne() async throws {
        let (database, worktreeID) = try makeDatabase()
        let persistence = SQLiteWorkspacePersistence(database: database)
        let old = makeWorkspace(title: "old")
        let new = makeWorkspace(title: "new")
        try await persistence.commitStructural(
            worktreeID: worktreeID, revision: 9, snapshot: new.snapshot,
            tabs: [new.tab], terminalContents: [new.content])
        try await persistence.commitStructural(
            worktreeID: worktreeID, revision: 8, snapshot: old.snapshot,
            tabs: [old.tab], terminalContents: [old.content])

        let restored = await persistence.restore(worktreeID: worktreeID)
        #expect(restored.revision == 9)
        #expect(restored.tabs.values.first?.title == "new")
    }

    @Test func fullEnvelopeRoundTripsExactly() async throws {
        let (database, worktreeID) = try makeDatabase()
        let persistence = SQLiteWorkspacePersistence(database: database)
        let fixture = makeWorkspace()
        let payload = try fixture.snapshot.canonicalPayload()
        try await persistence.commitStructural(
            worktreeID: worktreeID, revision: 12, snapshot: fixture.snapshot,
            tabs: [fixture.tab], terminalContents: [fixture.content])

        let stored = try database.read { try WorkspaceLayoutRecord.fetchOne($0, key: worktreeID.uuidString) }
        #expect(stored?.payload == String(decoding: payload, as: UTF8.self))
        #expect(stored?.checksum == SHA256Hex.digest(payload))
        let restored = await persistence.restore(worktreeID: worktreeID)
        #expect(restored.layout == fixture.layout)
        #expect(restored.tabs == [fixture.tab.id: fixture.tab])
    }

    private typealias Fixture = (layout: WorkspaceLayout, snapshot: WorkspaceSnapshot,
                                  tab: WorkspaceTab, content: TerminalContentRecordValue)

    private func makeWorkspace(title: String = "Terminal") -> Fixture {
        let groupID = PaneGroupID(UUID(uuidString: "00000000-0000-4000-8000-000000000001")!)
        let tabID = WorkspaceTabID(UUID(uuidString: "00000000-0000-4000-8000-000000000002")!)
        let contentID = TerminalContentID(UUID(uuidString: "00000000-0000-4000-8000-000000000003")!)
        let tab = WorkspaceTab(id: tabID, title: title, titleIsAutoNamed: true, content: .terminal(contentID))
        let layout = try! WorkspaceLayout.make(
            root: .group(groupID), groups: [groupID: PaneGroup(id: groupID, tabs: [tab], activeTabID: tabID)],
            activeGroupID: groupID).get()
        return (layout, WorkspaceSnapshot(layout: layout), tab,
                TerminalContentRecordValue(id: contentID, worktreeID: fixedWorktreeID, launchKind: .shell, commandJSON: nil))
    }

    private func makeTwoTabWorkspace() -> (layout: WorkspaceLayout, snapshot: WorkspaceSnapshot,
                                            tabs: [WorkspaceTab], contents: [TerminalContentRecordValue]) {
        let groupID = PaneGroupID(UUID(uuidString: "00000000-0000-4000-8000-000000000010")!)
        let tabs = (0..<2).map { index in
            let tabID = WorkspaceTabID(UUID(uuidString: String(format: "00000000-0000-4000-8000-%012d", 20 + index))!)
            let contentID = TerminalContentID(UUID(uuidString: String(format: "00000000-0000-4000-8000-%012d", 30 + index))!)
            return WorkspaceTab(id: tabID, title: "Tab \(index)", titleIsAutoNamed: true, content: .terminal(contentID))
        }
        let layout = try! WorkspaceLayout.make(
            root: .group(groupID), groups: [groupID: PaneGroup(id: groupID, tabs: tabs, activeTabID: tabs[0].id)],
            activeGroupID: groupID).get()
        let contents = tabs.compactMap { tab -> TerminalContentRecordValue? in
            guard case .terminal(let id) = tab.content else { return nil }
            return TerminalContentRecordValue(id: id, worktreeID: fixedWorktreeID, launchKind: .shell, commandJSON: nil)
        }
        return (layout, WorkspaceSnapshot(layout: layout), tabs, contents)
    }

    private func makeDatabase() throws -> (AppDatabase, UUID) {
        let database = try AppDatabase.inMemory()
        let worktreeID = UUID(uuidString: "00000000-0000-4000-8000-000000000099")!
        try database.write { db in
            try ProjectRecord(id: "project-1", name: "Project", rootPath: "/tmp/project", createdAt: Date()).insert(db)
            try WorktreeRecord(id: worktreeID.uuidString, projectId: "project-1", branch: "main", path: "/tmp/project", createdAt: Date()).insert(db)
        }
        return (database, worktreeID)
    }

    private var fixedWorktreeID: UUID {
        UUID(uuidString: "00000000-0000-4000-8000-000000000099")!
    }
}
