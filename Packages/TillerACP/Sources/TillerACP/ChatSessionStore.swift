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

    /// Most recently active session for a (worktree, agent) pair that has at
    /// least one persisted turn. Sessions created by `start()` but abandoned
    /// before any turn completes never get an on-disk transcript on the
    /// agent side either, so `session/load` would always fail "Session not
    /// found" for them — excluding empty sessions here keeps `latestSession`
    /// pointing at one the agent can actually resume.
    public func latestSession(worktreeId: String, agentId: String) throws -> ChatSessionRecord? {
        try database.read { db in
            try ChatSessionRecord
                .filter(Column("worktreeId") == worktreeId)
                .filter(Column("agentId") == agentId)
                .filter(sql: "EXISTS (SELECT 1 FROM chatItem WHERE chatItem.sessionId = chatSession.id)")
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

    /// Stores the last known context-window usage for the session, so a
    /// worktree remount can restore the ring before the next live update.
    public func setContextUsage(_ usage: ContextUsage?, sessionId: String) throws {
        try database.write { db in
            guard var record = try ChatSessionRecord.fetchOne(db, key: sessionId) else { return }
            record.contextUsageUsed = usage?.used
            record.contextUsageSize = usage?.size
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
        return records.compactMap { record in
            guard let item = try? decoder.decode(TranscriptItem.self, from: record.payload)
            else { return nil }
            // Persisted transcripts are finished turns; normalize agent messages
            // left isComplete=false by the pre-fix reducer so restored chats don't
            // render a perpetual streaming indicator.
            if case .agentMessage(let id, let text, false) = item {
                return .agentMessage(id: id, text: text, isComplete: true)
            }
            return item
        }
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
        case .editSummary: "editSummary"
        }
    }
}
