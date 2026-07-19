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
        let latest = try store.latestSession(worktreeId: worktreeId, agentId: "claude")
        #expect(latest?.id == created.id)
        #expect(try store.latestSession(worktreeId: worktreeId, agentId: "opencode") == nil)
    }

    @Test func latestPicksMostRecentActivity() throws {
        let (store, worktreeId) = try makeStore()
        _ = try store.createSession(worktreeId: worktreeId, agentId: "claude",
                                    now: Date(timeIntervalSince1970: 100))
        let newer = try store.createSession(worktreeId: worktreeId, agentId: "claude",
                                            now: Date(timeIntervalSince1970: 200))
        #expect(try store.latestSession(worktreeId: worktreeId, agentId: "claude")?.id
                == newer.id)
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
        let record = try store.latestSession(worktreeId: worktreeId, agentId: "claude")
        #expect(record?.lastActivityAt == Date(timeIntervalSince1970: 200))
    }

    @Test func acpSessionIdIsStoredForResume() throws {
        let (store, worktreeId) = try makeStore()
        let session = try store.createSession(worktreeId: worktreeId, agentId: "opencode",
                                              now: .init())
        try store.setACPSessionId("acp-42", sessionId: session.id)
        #expect(try store.latestSession(worktreeId: worktreeId, agentId: "opencode")?
            .acpSessionId == "acp-42")
    }

    @Test func kindLabelsAreDistinct() {
        let labels = sampleItems.map(\.kindLabel)
        #expect(labels == ["userMessage", "toolCall", "agentMessage"])
    }
}
