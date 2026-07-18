import Foundation
import GRDB
import TillerPersistence

/// Persistence facade for chat conversations: session lookup/creation and
/// whole-transcript snapshots. Callers invoke `saveTranscript` at settle
/// points only (turn end, permission resolution, tool-call terminal state,
/// pane close) — never per streaming chunk.
public struct ChatSessionStore: Sendable {
    private let database: AppDatabase

    public init(database: AppDatabase) {
        self.database = database
    }

    /// Most recently active session for a (worktree, agent) pair.
    public func latestSession(worktreeId: String, agentId: String) throws -> ChatSessionRecord? {
        try database.read { db in
            try ChatSessionRecord
                .filter(Column("worktreeId") == worktreeId)
                .filter(Column("agentId") == agentId)
                .order(Column("lastActivityAt").desc)
                .fetchOne(db)
        }
    }

    public func createSession(worktreeId: String, agentId: String,
                              now: Date = Date()) throws -> ChatSessionRecord {
        let record = ChatSessionRecord(
            id: UUID().uuidString, worktreeId: worktreeId, agentId: agentId,
            createdAt: now, lastActivityAt: now)
        try database.write { db in try record.insert(db) }
        return record
    }

    /// Stores the agent-side session reference for later `session/load`.
    public func setACPSessionId(_ acpSessionId: String, sessionId: String) throws {
        try database.write { db in
            guard var record = try ChatSessionRecord.fetchOne(db, key: sessionId) else { return }
            record.acpSessionId = acpSessionId
            try record.update(db)
        }
    }

    /// Atomically replaces the session's transcript and bumps activity.
    public func saveTranscript(sessionId: String, items: [TranscriptItem],
                               now: Date = Date()) throws {
        let encoder = JSONEncoder()
        let records = try items.enumerated().map { ordinal, item in
            ChatItemRecord(sessionId: sessionId, ordinal: ordinal,
                           kind: item.kindLabel, payload: try encoder.encode(item))
        }
        try database.write { db in
            try ChatItemRecord.filter(Column("sessionId") == sessionId).deleteAll(db)
            for record in records { try record.insert(db) }
            if var session = try ChatSessionRecord.fetchOne(db, key: sessionId) {
                session.lastActivityAt = now
                try session.update(db)
            }
        }
    }

    /// Loads the transcript in order; items that no longer decode (schema
    /// drift across app versions) are skipped rather than failing the load.
    public func loadTranscript(sessionId: String) throws -> [TranscriptItem] {
        let records = try database.read { db in
            try ChatItemRecord
                .filter(Column("sessionId") == sessionId)
                .order(Column("ordinal"))
                .fetchAll(db)
        }
        let decoder = JSONDecoder()
        return records.compactMap { try? decoder.decode(TranscriptItem.self, from: $0.payload) }
    }
}

extension TranscriptItem {
    /// Stable discriminator stored in `chatItem.kind` for future queries.
    var kindLabel: String {
        switch self {
        case .userMessage: "userMessage"
        case .agentMessage: "agentMessage"
        case .thought: "thought"
        case .toolCall: "toolCall"
        case .plan: "plan"
        case .turnDivider: "turnDivider"
        }
    }
}
