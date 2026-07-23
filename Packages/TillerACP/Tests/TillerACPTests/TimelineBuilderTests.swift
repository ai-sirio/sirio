import Testing
import Foundation
@testable import TillerACP

@Suite("TimelineBuilder")
struct TimelineBuilderTests {
    func user(_ id: String, _ text: String) -> TranscriptItem {
        .userMessage(id: id, blocks: [.text(text)])
    }
    func agent(_ id: String, _ text: String, complete: Bool = true) -> TranscriptItem {
        .agentMessage(id: id, text: text, isComplete: complete)
    }
    func tool(_ id: String, title: String = "Read file",
              kind: ToolKind = .read,
              status: ToolCallStatus = .completed,
              permission: PermissionState? = nil) -> TranscriptItem {
        .toolCall(ToolCallItem(toolCallId: id, title: title, kind: kind,
                               status: status, permission: permission))
    }
    func divider(_ id: String, at: Date = Date(timeIntervalSince1970: 100)) -> TranscriptItem {
        .turnDivider(id: id, at: at)
    }

    @Test func mapsPlainMessagesToMessageRows() {
        let rows = TimelineBuilder.rows(
            items: [user("u1", "hi"), agent("a1", "hello")],
            state: TimelineState())
        #expect(rows.count == 2)
        guard case .message(let first, _) = rows[0] else { Issue.record("expected message"); return }
        #expect(first.id == "u1")
        guard case .message(let second, _) = rows[1] else { Issue.record("expected message"); return }
        #expect(second.id == "a1")
    }

    @Test func closedTurnKeepsItsDividerRow() {
        let rows = TimelineBuilder.rows(
            items: [user("u1", "hi"), agent("a1", "done"), divider("d1")],
            state: TimelineState())
        guard case .turnDivider(let id, _) = rows.last else {
            Issue.record("expected trailing divider"); return
        }
        #expect(id == "d1")
    }

    @Test func streamingAppendsWorkingRow() {
        let rows = TimelineBuilder.rows(
            items: [user("u1", "hi")],
            state: TimelineState(isStreaming: true))
        #expect(rows.last == .working)
    }

    @Test func notStreamingHasNoWorkingRow() {
        let rows = TimelineBuilder.rows(items: [user("u1", "hi")], state: TimelineState())
        #expect(!rows.contains(.working))
    }

    @Test func rowIdsAreStableAndUnique() {
        let rows = TimelineBuilder.rows(
            items: [user("u1", "hi"), agent("a1", "hello"), divider("d1")],
            state: TimelineState())
        #expect(Set(rows.map(\.id)).count == rows.count)
    }
}
