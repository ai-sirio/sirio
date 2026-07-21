import Foundation
import Testing
@testable import TillerACP

@Suite struct TranscriptHandoffTests {
    @Test func emptyTranscriptProducesNoPreamble() {
        #expect(TranscriptHandoff.preamble(items: []) == nil)
        #expect(TranscriptHandoff.preamble(items: [
            .turnDivider(id: "d", at: Date())]) == nil)
    }

    @Test func formatsMessagesAndCompressesToolCalls() throws {
        let items: [TranscriptItem] = [
            .userMessage(id: "u1", blocks: [.text("Fix the bug")]),
            .toolCall(ToolCallItem(toolCallId: "t1", title: "Read main.swift",
                                   kind: .read, status: .completed)),
            .agentMessage(id: "a1", text: "Done, the bug was X.", isComplete: true),
            .thought(id: "th1", text: "hidden reasoning"),
            .systemNotice(id: "s1", text: "Agent changed"),
        ]
        let preamble = try #require(TranscriptHandoff.preamble(items: items))
        #expect(preamble.contains("User: Fix the bug"))
        #expect(preamble.contains("- [tool] Read main.swift → completed"))
        #expect(preamble.contains("Agent: Done, the bug was X."))
        #expect(!preamble.contains("hidden reasoning"))   // thoughts excluded
        #expect(!preamble.contains("Agent changed"))      // notices excluded
        #expect(preamble.hasPrefix("Previous conversation with another agent"))
    }

    @Test func capDropsOldestFirstWithTruncationNote() throws {
        let big = String(repeating: "x", count: 60_000)
        let items: [TranscriptItem] = [
            .agentMessage(id: "old", text: "OLDEST-" + big, isComplete: true),
            .agentMessage(id: "mid", text: "MIDDLE-" + big, isComplete: true),
            .agentMessage(id: "new", text: "NEWEST-" + big, isComplete: true),
        ]
        let preamble = try #require(TranscriptHandoff.preamble(items: items))
        #expect(preamble.utf8.count < TranscriptHandoff.maxBytes + 1_000)
        #expect(preamble.contains("NEWEST-"))
        #expect(!preamble.contains("OLDEST-"))
        #expect(preamble.contains("(older messages truncated)"))
    }

    @Test func systemNoticeRoundTripsThroughCodable() throws {
        let item = TranscriptItem.systemNotice(id: "s1", text: "Agent changed: A → B.")
        let data = try JSONEncoder().encode(item)
        let decoded = try JSONDecoder().decode(TranscriptItem.self, from: data)
        #expect(decoded == item)
        #expect(item.kindLabel == "systemNotice")
    }
}
