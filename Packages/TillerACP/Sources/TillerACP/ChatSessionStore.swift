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

    /// Most recently active session for a worktree that has at
    /// least one persisted turn. The `agentId` on the record is the
    /// last-used agent, not an owner. Sessions created by `start()` but
    /// abandoned before any turn completes never get an on-disk transcript
    /// on the agent side either, so `session/load` would always fail "Session
    /// not found" for them — excluding empty sessions here keeps
    /// `latestSession` pointing at one the agent can actually resume.
    public func latestSession(worktreeId: String) throws -> ChatSessionRecord? {
        try database.read { db in
            try ChatSessionRecord
                .filter(Column("worktreeId") == worktreeId)
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

    /// Records the (new) last-used agent for a session after a switch.
    public func setAgentId(_ agentId: String, sessionId: String) throws {
        try database.write { db in
            guard var record = try ChatSessionRecord.fetchOne(db, key: sessionId) else { return }
            record.agentId = agentId
            try record.update(db)
        }
    }

    /// Severs same-agent resume: after an agent switch the stored ACP session
    /// belongs to the previous agent and must never be replayed into the new one.
    public func clearACPSessionId(sessionId: String) throws {
        try database.write { db in
            guard var record = try ChatSessionRecord.fetchOne(db, key: sessionId) else { return }
            record.acpSessionId = nil
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

    /// Stores the session's selected permission mode, model, and effort.
    public func setSessionSettings(permissionMode: String?, selectedModel: String?,
                                   selectedEffort: String?, sessionId: String) throws {
        try database.write { db in
            guard var record = try ChatSessionRecord.fetchOne(db, key: sessionId) else { return }
            record.permissionMode = permissionMode
            record.selectedModel = selectedModel
            record.selectedEffort = selectedEffort
            try record.update(db)
        }
    }

    /// Stores which transport should be used to resume the session.
    public func setTransportKind(_ transportKind: String, sessionId: String) throws {
        try database.write { db in
            guard var record = try ChatSessionRecord.fetchOne(db, key: sessionId) else { return }
            record.transportKind = transportKind
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
        let decoded: [TranscriptItem] = records.compactMap { record in
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
        return decoded.normalizedTranscriptIDs()
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
        case .systemNotice: "systemNotice"
        }
    }
}

private extension Array where Element == TranscriptItem {
    func normalizedTranscriptIDs() -> [TranscriptItem] {
        var usedIDs: Set<String> = []
        return map { item in
            guard usedIDs.contains(item.id) else {
                usedIDs.insert(item.id)
                return item
            }

            var duplicateOrdinal = 1
            var normalizedID = "\(item.id)-duplicate-\(duplicateOrdinal)"
            while usedIDs.contains(normalizedID) {
                duplicateOrdinal += 1
                normalizedID = "\(item.id)-duplicate-\(duplicateOrdinal)"
            }
            usedIDs.insert(normalizedID)
            return item.withTranscriptID(normalizedID)
        }
    }
}

private extension TranscriptItem {
    func withTranscriptID(_ id: String) -> TranscriptItem {
        switch self {
        case .userMessage(_, let blocks): return .userMessage(id: id, blocks: blocks)
        case .agentMessage(_, let text, let isComplete):
            return .agentMessage(id: id, text: text, isComplete: isComplete)
        case .thought(_, let text): return .thought(id: id, text: text)
        case .toolCall(var item):
            item.transcriptID = id
            return .toolCall(item)
        case .plan(_, let entries): return .plan(id: id, entries: entries)
        case .turnDivider(_, let at): return .turnDivider(id: id, at: at)
        case .editSummary(_, let paths): return .editSummary(id: id, paths: paths)
        case .systemNotice(_, let text): return .systemNotice(id: id, text: text)
        }
    }
}
