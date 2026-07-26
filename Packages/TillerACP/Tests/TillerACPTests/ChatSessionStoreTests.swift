import Testing
import Foundation
import TillerPersistence
@testable import TillerACP

@Suite struct ChatSessionStoreTests {
    private func makeStore() throws -> (ChatSessionStore, worktreeId: String) {
        let database = try AppDatabase.inMemory()
        let worktreeId = UUID().uuidString
        try database.write { db in
            try ProjectRecord(id: "p1", name: "P", rootPath: "/p", createdAt: .init())
                .insert(db)
            try WorktreeRecord(id: worktreeId, projectId: "p1", branch: "main",
                               path: "/p", createdAt: .init()).insert(db)
        }
        return (ChatSessionStore(database: database), worktreeId)
    }

    private let sampleItems: [TranscriptItem] = [
        .userMessage(id: "user-0", blocks: [.text("fix the bug")]),
        .toolCall(ToolCallItem(
            toolCallId: "tc1", title: "Edit F.swift", kind: .edit, status: .completed,
            content: [.diff(path: "/w/F.swift", oldText: "a", newText: "b")],
            locations: [ToolCallLocation(path: "/w/F.swift", line: 3)],
            permission: PermissionState(
                requestId: .number(9),
                options: [PermissionOption(optionId: "y", name: "Allow", kind: .allowOnce)],
                resolution: .selected(optionId: "y")))),
        .agentMessage(id: "agent-1", text: "Done.", isComplete: true),
    ]

    @Test func createThenLatestFindsSession() throws {
        let (store, worktreeId) = try makeStore()
        let created = try store.createSession(worktreeId: worktreeId, agentId: "claude",
                                              now: Date(timeIntervalSince1970: 100))
        try store.saveTranscript(sessionId: created.id, items: sampleItems,
                                 now: Date(timeIntervalSince1970: 100))
        let latest = try store.latestSession(worktreeId: worktreeId)
        #expect(latest?.id == created.id)
        #expect(try store.latestSession(worktreeId: worktreeId)?.agentId == "claude")
    }

    @Test func latestPicksMostRecentActivity() throws {
        let (store, worktreeId) = try makeStore()
        let older = try store.createSession(worktreeId: worktreeId, agentId: "claude",
                                            now: Date(timeIntervalSince1970: 100))
        try store.saveTranscript(sessionId: older.id, items: sampleItems,
                                 now: Date(timeIntervalSince1970: 100))
        let newer = try store.createSession(worktreeId: worktreeId, agentId: "claude",
                                            now: Date(timeIntervalSince1970: 200))
        try store.saveTranscript(sessionId: newer.id, items: sampleItems,
                                 now: Date(timeIntervalSince1970: 200))
        #expect(try store.latestSession(worktreeId: worktreeId)?.id
                == newer.id)
    }

    /// Reproduces the "Session not found" resume failure: a session created
    /// by `ChatController.start()` but abandoned before any turn completes
    /// never gets an on-disk transcript on the agent side either, so it must
    /// never be selected as resumable even though it is the most recent row.
    @Test func emptySessionIsExcludedEvenIfMostRecent() throws {
        let (store, worktreeId) = try makeStore()
        let withContent = try store.createSession(worktreeId: worktreeId, agentId: "claude",
                                                   now: Date(timeIntervalSince1970: 100))
        try store.saveTranscript(sessionId: withContent.id, items: sampleItems,
                                 now: Date(timeIntervalSince1970: 100))
        _ = try store.createSession(worktreeId: worktreeId, agentId: "claude",
                                    now: Date(timeIntervalSince1970: 200))
        #expect(try store.latestSession(worktreeId: worktreeId)?.id
                == withContent.id)
    }

    @Test func transcriptRoundTripsIncludingPermissions() throws {
        let (store, worktreeId) = try makeStore()
        let session = try store.createSession(worktreeId: worktreeId, agentId: "claude",
                                              now: .init())
        try store.saveTranscript(sessionId: session.id, items: sampleItems, now: .init())
        #expect(try store.loadTranscript(sessionId: session.id) == sampleItems)
    }

    /// I transcript persistiti sono di turni finiti: un agentMessage salvato
    /// isComplete=false (bug pre-fix) deve tornare completo al load, altrimenti
    /// la UI mostra RunningDots su conversazioni ripristinate.
    @Test func loadTranscriptNormalizesIncompleteAgentMessages() throws {
        let (store, worktreeId) = try makeStore()
        let session = try store.createSession(worktreeId: worktreeId, agentId: "claude",
                                              now: .init())
        let dirty: [TranscriptItem] = [
            .agentMessage(id: "agent-0", text: "Risposta", isComplete: false),
        ]
        try store.saveTranscript(sessionId: session.id, items: dirty, now: .init())
        let loaded = try store.loadTranscript(sessionId: session.id)
        #expect(loaded == [.agentMessage(id: "agent-0", text: "Risposta", isComplete: true)])
    }

    @Test func loadTranscriptNormalizesDuplicateIDsWithoutChangingOrder() throws {
        let (store, worktreeId) = try makeStore()
        let session = try store.createSession(worktreeId: worktreeId, agentId: "claude",
                                              now: .init())
        try store.saveTranscript(sessionId: session.id, items: [
            .userMessage(id: "user-0", blocks: [.text("first")]),
            .userMessage(id: "user-0", blocks: [.text("second")]),
        ], now: .init())

        let loaded = try store.loadTranscript(sessionId: session.id)
        #expect(Set(loaded.map(\.id)).count == loaded.count)
        #expect(loaded.compactMap { item in
            if case .userMessage(_, let blocks) = item {
                return blocks.compactMap { block in
                    if case .text(let text) = block { return text }
                    return nil
                }
            }
            return nil
        } == [["first"], ["second"]])
    }

    @Test func loadSaveLoadPreservesNormalizedIDsAndRawToolCallIDs() throws {
        let (store, worktreeId) = try makeStore()
        let session = try store.createSession(worktreeId: worktreeId, agentId: "claude",
                                              now: .init())
        let duplicateToolCalls = [
            TranscriptItem.toolCall(ToolCallItem(
                toolCallId: "tc1", title: "first", kind: .read, status: .completed)),
            TranscriptItem.toolCall(ToolCallItem(
                toolCallId: "tc1", title: "second", kind: .read, status: .completed)),
        ]
        try store.saveTranscript(sessionId: session.id, items: duplicateToolCalls, now: .init())
        let firstLoad = try store.loadTranscript(sessionId: session.id)
        try store.saveTranscript(sessionId: session.id, items: firstLoad, now: .init())
        let secondLoad = try store.loadTranscript(sessionId: session.id)

        #expect(secondLoad.map(\.id) == firstLoad.map(\.id))
        #expect(secondLoad.compactMap { item in
            if case .toolCall(let call) = item { return call.toolCallId }
            return nil
        } == ["tc1", "tc1"])
    }

    @Test func legacyToolCallPayloadFallsBackToRawPresentationID() throws {
        let payload = Data(#"{"toolCallId":"tc1","title":"Read","kind":"read","status":"completed","content":[],"locations":[]}"#.utf8)
        let item = try JSONDecoder().decode(ToolCallItem.self, from: payload)

        #expect(item.transcriptID == nil)
        #expect(item.id == "tc1")
        #expect(item.toolCallId == "tc1")
    }

    @Test func saveTranscriptReplacesAndBumpsActivity() throws {
        let (store, worktreeId) = try makeStore()
        let session = try store.createSession(worktreeId: worktreeId, agentId: "claude",
                                              now: Date(timeIntervalSince1970: 100))
        try store.saveTranscript(sessionId: session.id, items: sampleItems,
                                 now: Date(timeIntervalSince1970: 150))
        let shorter: [TranscriptItem] = [.userMessage(id: "user-0", blocks: [.text("hi")])]
        try store.saveTranscript(sessionId: session.id, items: shorter,
                                 now: Date(timeIntervalSince1970: 200))
        #expect(try store.loadTranscript(sessionId: session.id) == shorter)
        let record = try store.latestSession(worktreeId: worktreeId)
        #expect(record?.lastActivityAt == Date(timeIntervalSince1970: 200))
    }

    @Test func acpSessionIdIsStoredForResume() throws {
        let (store, worktreeId) = try makeStore()
        let session = try store.createSession(worktreeId: worktreeId, agentId: "opencode",
                                              now: .init())
        try store.setACPSessionId("acp-42", sessionId: session.id)
        try store.saveTranscript(sessionId: session.id, items: sampleItems, now: .init())
        #expect(try store.latestSession(worktreeId: worktreeId)?
            .acpSessionId == "acp-42")
    }

    @Test func contextUsageRoundTripsThroughLatestSession() throws {
        let (store, worktreeId) = try makeStore()
        let session = try store.createSession(worktreeId: worktreeId, agentId: "claude",
                                              now: .init())
        try store.setContextUsage(ContextUsage(used: 1200, size: 200_000), sessionId: session.id)
        try store.saveTranscript(sessionId: session.id, items: sampleItems, now: .init())
        let latest = try store.latestSession(worktreeId: worktreeId)
        #expect(latest?.contextUsageUsed == 1200)
        #expect(latest?.contextUsageSize == 200_000)
    }

    @Test func contextUsageClearsWhenSetToNil() throws {
        let (store, worktreeId) = try makeStore()
        let session = try store.createSession(worktreeId: worktreeId, agentId: "claude",
                                              now: .init())
        try store.setContextUsage(ContextUsage(used: 500, size: 100_000), sessionId: session.id)
        try store.setContextUsage(nil, sessionId: session.id)
        try store.saveTranscript(sessionId: session.id, items: sampleItems, now: .init())
        let latest = try store.latestSession(worktreeId: worktreeId)
        #expect(latest?.contextUsageUsed == nil)
        #expect(latest?.contextUsageSize == nil)
    }

    @Test func sessionSettingsRoundTrip() throws {
        let (store, worktreeId) = try makeStore()
        let record = try store.createSession(worktreeId: worktreeId, agentId: "claude-acp")
        try store.setSessionSettings(permissionMode: "acceptEdits",
                                     selectedModel: "claude-sonnet-5",
                                     selectedEffort: "high",
                                     sessionId: record.id)
        try store.setTransportKind("native", sessionId: record.id)
        try store.saveTranscript(sessionId: record.id, items: sampleItems)
        let reloaded = try store.latestSession(worktreeId: worktreeId)
        #expect(reloaded?.permissionMode == "acceptEdits")
        #expect(reloaded?.selectedModel == "claude-sonnet-5")
        #expect(reloaded?.selectedEffort == "high")
        #expect(reloaded?.transportKind == "native")
    }

    @Test func latestSessionIgnoresAgent() throws {
        let (store, worktreeId) = try makeStore()
        let a = try store.createSession(worktreeId: worktreeId, agentId: "claude-acp")
        try store.saveTranscript(sessionId: a.id, items: [
            .agentMessage(id: "m1", text: "hi", isComplete: true)])
        let b = try store.createSession(worktreeId: worktreeId, agentId: "codex-acp",
                                        now: Date().addingTimeInterval(10))
        try store.saveTranscript(sessionId: b.id, items: [
            .agentMessage(id: "m2", text: "yo", isComplete: true)],
            now: Date().addingTimeInterval(20))
        #expect(try store.latestSession(worktreeId: worktreeId)?.id == b.id)
        #expect(try store.latestSession(worktreeId: worktreeId)?.agentId == "codex-acp")
    }

    @Test func setAgentIdUpdatesRecord() throws {
        let (store, worktreeId) = try makeStore()
        let session = try store.createSession(worktreeId: worktreeId, agentId: "claude-acp")
        try store.saveTranscript(sessionId: session.id, items: [
            .agentMessage(id: "m", text: "x", isComplete: true)])
        try store.setAgentId("codex-acp", sessionId: session.id)
        #expect(try store.latestSession(worktreeId: worktreeId)?.agentId == "codex-acp")
    }

    @Test func clearACPSessionIdRemovesResumeHandle() throws {
        let (store, worktreeId) = try makeStore()
        let session = try store.createSession(worktreeId: worktreeId, agentId: "claude-acp")
        try store.setACPSessionId("acp-123", sessionId: session.id)
        try store.saveTranscript(sessionId: session.id, items: [
            .agentMessage(id: "m", text: "x", isComplete: true)])
        try store.clearACPSessionId(sessionId: session.id)
        #expect(try store.latestSession(worktreeId: worktreeId)?.acpSessionId == nil)
    }


    @Test func kindLabelsAreDistinct() {
        let labels = sampleItems.map(\.kindLabel)
        #expect(labels == ["userMessage", "toolCall", "agentMessage"])
    }
}
