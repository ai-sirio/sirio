import Foundation
import Testing
import TillerPersistence
@testable import Tiller

@Suite struct ChatHistoryRowsTests {
    private func session(id: String, title: String?, agentId: String = "claude-acp",
                         at seconds: TimeInterval) -> ChatSessionRecord {
        ChatSessionRecord(id: id, worktreeId: "w1", agentId: agentId,
                          createdAt: Date(timeIntervalSince1970: seconds),
                          lastActivityAt: Date(timeIntervalSince1970: seconds),
                          title: title)
    }

    @Test func titledSessionKeepsItsTitle() {
        let rows = ChatHistoryRows.make(
            sessions: [session(id: "s1", title: "Fix the parser", at: 100)],
            displayName: { _ in "Claude Code" },
            timeFormatter: { _ in "14:32" })

        #expect(rows.map(\.title) == ["Fix the parser"])
    }

    @Test func untitledSessionFallsBackToAgentAndTime() {
        let rows = ChatHistoryRows.make(
            sessions: [session(id: "s1", title: nil, at: 100)],
            displayName: { _ in "Claude Code" },
            timeFormatter: { _ in "14:32" })

        #expect(rows.map(\.title) == ["Claude Code · 14:32"])
    }

    @Test func blankTitleIsTreatedAsMissing() {
        let rows = ChatHistoryRows.make(
            sessions: [session(id: "s1", title: "   ", at: 100)],
            displayName: { _ in "Codex" },
            timeFormatter: { _ in "09:05" })

        #expect(rows.map(\.title) == ["Codex · 09:05"])
    }

    @Test func rowsCarrySessionIdentityAndOrder() {
        let rows = ChatHistoryRows.make(
            sessions: [session(id: "s2", title: "b", at: 200),
                       session(id: "s1", title: "a", at: 100)],
            displayName: { _ in "Claude Code" },
            timeFormatter: { _ in "" })

        #expect(rows.map(\.id) == ["s2", "s1"])
        #expect(rows.first?.agentId == "claude-acp")
    }
}
