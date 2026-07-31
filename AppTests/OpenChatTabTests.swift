import Foundation
import Testing
import TillerCore
import TillerTerminal
import TillerWorkspace

@testable import Tiller

@MainActor
struct OpenChatTabTests {
    /// The tab carries only a ChatContentID, which *is* the session id — the
    /// agent is recovered from the session row, so this asserts the whole
    /// identity chain the universal engine relies on.
    @Test func openChatTabBacksTheTabWithASessionForThatAgent() async throws {
        let fixture = try await UniversalChatFixture.make(
            suiteName: "OpenChatTabTests-\(UUID().uuidString)")
        defer { fixture.cleanUp() }

        let tab = try #require(await UniversalChatFixture.openChatTab(fixture, agentId: "codex-acp"))

        guard case .chat(let contentID) = tab.content else {
            Issue.record("expected a chat tab")
            return
        }
        let record = try #require(fixture.model.chatSession(id: contentID.rawValue))
        #expect(record.agentId == "codex-acp")
        #expect(record.worktreeId == fixture.worktree.id.uuidString)
    }

    /// Without a store there is no session id, so no tab may be created —
    /// a chat tab that cannot find its transcript is worse than none.
    @Test func openChatTabCreatesNoTabWithoutAStore() async throws {
        let fixture = try await UniversalChatFixture.make(
            suiteName: "OpenChatTabTests-nostore-\(UUID().uuidString)")
        defer { fixture.cleanUp() }
        fixture.model.chatStore = nil

        fixture.model.openChatTab(agentId: "codex-acp", in: fixture.worktree)
        try await Task.sleep(for: .milliseconds(100))

        #expect(UniversalChatFixture.tabs(fixture).isEmpty)
    }
}
