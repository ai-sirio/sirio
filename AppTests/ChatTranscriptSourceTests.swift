import Foundation
import Testing
import TillerACP

@testable import Tiller

@MainActor
struct ChatTranscriptSourceTests {
    @Test func extractTextReturnsNilForEmptyItems() {
        #expect(ChatTranscriptSource.extractText(from: []) == nil)
    }

    @Test func extractTextJoinsUserAndAgentMessages() {
        let items: [TranscriptItem] = [
            .userMessage(id: "u1", blocks: [.text("fix login bug")]),
            .agentMessage(id: "a1", text: "Looking at auth.ts", isComplete: true)
        ]

        let text = ChatTranscriptSource.extractText(from: items)

        #expect(text?.contains("fix login bug") == true)
        #expect(text?.contains("Looking at auth.ts") == true)
    }

    @Test func recentTextReturnsNilForFreshController() {
        let controller = ChatController(
            tabId: UUID(), agentId: "claude", worktreeId: UUID(),
            worktreePath: "/tmp", store: nil,
            installStore: AgentInstallStore(rootDirectory: URL(fileURLWithPath: "/tmp")))
        let source = ChatTranscriptSource(controller: controller)

        #expect(source.recentText() == nil)
    }
}
