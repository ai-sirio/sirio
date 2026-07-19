import Testing
import Foundation
import GRDB
@testable import TillerPersistence

@Suite struct ChatRecordsTests {
    /// Inserts the project + worktree rows chatSession's FK chain needs.
    private func makeDatabaseWithWorktree() throws -> (AppDatabase, worktreeId: String) {
        let database = try AppDatabase.inMemory()
        let worktreeId = UUID().uuidString
        try database.write { db in
            try ProjectRecord(id: "p1", name: "P", rootPath: "/p", createdAt: .init())
                .insert(db)
            try WorktreeRecord(id: worktreeId, projectId: "p1", branch: "main",
                               path: "/p", createdAt: .init()).insert(db)
        }
        return (database, worktreeId)
    }

    @Test func sessionAndItemsRoundTrip() throws {
        let (database, worktreeId) = try makeDatabaseWithWorktree()
        let session = ChatSessionRecord(
            id: "s1", worktreeId: worktreeId, agentId: "claude",
            acpSessionId: "acp-1", createdAt: .init(), lastActivityAt: .init())
        let item = ChatItemRecord(sessionId: "s1", ordinal: 0,
                                  kind: "userMessage", payload: Data("x".utf8))
        try database.write { db in
            try session.insert(db)
            try item.insert(db)
        }
        let fetchedSession = try database.read { db in
            try ChatSessionRecord.fetchOne(db, key: "s1")
        }
        let fetchedItems = try database.read { db in
            try ChatItemRecord.fetchAll(db)
        }
        #expect(fetchedSession?.agentId == "claude")
        #expect(fetchedSession?.acpSessionId == "acp-1")
        #expect(fetchedItems == [item])
    }

    @Test func deletingWorktreeCascadesToSessionsAndItems() throws {
        let (database, worktreeId) = try makeDatabaseWithWorktree()
        try database.write { db in
            try ChatSessionRecord(id: "s1", worktreeId: worktreeId, agentId: "claude",
                                  createdAt: .init(), lastActivityAt: .init()).insert(db)
            try ChatItemRecord(sessionId: "s1", ordinal: 0, kind: "plan",
                               payload: Data()).insert(db)
            _ = try WorktreeRecord.deleteOne(db, key: worktreeId)
        }
        let sessions = try database.read { try ChatSessionRecord.fetchCount($0) }
        let items = try database.read { try ChatItemRecord.fetchCount($0) }
        #expect(sessions == 0)
        #expect(items == 0)
    }

    @Test func duplicateOrdinalInSameSessionIsRejected() throws {
        let (database, worktreeId) = try makeDatabaseWithWorktree()
        try database.write { db in
            try ChatSessionRecord(id: "s1", worktreeId: worktreeId, agentId: "claude",
                                  createdAt: .init(), lastActivityAt: .init()).insert(db)
            try ChatItemRecord(sessionId: "s1", ordinal: 0, kind: "a",
                               payload: Data()).insert(db)
        }
        #expect(throws: (any Error).self) {
            try database.write { db in
                try ChatItemRecord(sessionId: "s1", ordinal: 0, kind: "b",
                                   payload: Data()).insert(db)
            }
        }
    }
}
