import Foundation
import Testing
@testable import TillerACP

@Suite struct TranscriptReducerEditSummaryTests {
    private func editCall(_ id: String, path: String,
                          status: ToolCallStatus = .completed) -> SessionUpdate {
        .toolCall(ToolCall(
            toolCallId: id, title: "Edit \(path)", kind: .edit, status: .pending,
            locations: [ToolCallLocation(path: path, line: nil)]))
    }

    @Test func completedEditEmitsSummaryAtTurnEnd() {
        var reducer = TranscriptReducer()
        reducer.userPrompted([.text("fix it")])
        reducer.apply(editCall("t1", path: "/repo/a.swift"))
        reducer.apply(.toolCallUpdate(ToolCallUpdate(toolCallId: "t1",
                                                     status: .completed)))
        reducer.turnEnded(.endTurn)
        let summaries = reducer.items.compactMap { item -> [String]? in
            if case .editSummary(_, let paths) = item { return paths }
            return nil
        }
        #expect(summaries == [["/repo/a.swift"]])
        // Emesso prima del divider di fine turno.
        if case .turnDivider = reducer.items.last {} else {
            Issue.record("expected turnDivider as last item")
        }
    }

    @Test func duplicatePathsAreDeduplicatedPreservingOrder() {
        var reducer = TranscriptReducer()
        reducer.userPrompted([.text("go")])
        reducer.apply(editCall("t1", path: "/repo/a.swift"))
        reducer.apply(.toolCallUpdate(ToolCallUpdate(toolCallId: "t1",
                                                     status: .completed)))
        reducer.apply(editCall("t2", path: "/repo/b.swift"))
        reducer.apply(.toolCallUpdate(ToolCallUpdate(toolCallId: "t2",
                                                     status: .completed)))
        reducer.apply(editCall("t3", path: "/repo/a.swift"))
        reducer.apply(.toolCallUpdate(ToolCallUpdate(toolCallId: "t3",
                                                     status: .completed)))
        reducer.turnEnded(.endTurn)
        guard case .editSummary(_, let paths)? = reducer.items.first(where: {
            if case .editSummary = $0 { return true } else { return false }
        }) else { Issue.record("missing editSummary"); return }
        #expect(paths == ["/repo/a.swift", "/repo/b.swift"])
    }

    @Test func nonEditOrFailedCallsEmitNoSummary() {
        var reducer = TranscriptReducer()
        reducer.userPrompted([.text("read")])
        reducer.apply(.toolCall(ToolCall(
            toolCallId: "r1", title: "Read", kind: .read, status: .pending,
            locations: [ToolCallLocation(path: "/repo/a.swift", line: nil)])))
        reducer.apply(.toolCallUpdate(ToolCallUpdate(toolCallId: "r1",
                                                     status: .completed)))
        reducer.apply(editCall("e1", path: "/repo/b.swift"))
        reducer.apply(.toolCallUpdate(ToolCallUpdate(toolCallId: "e1",
                                                     status: .failed)))
        reducer.turnEnded(.endTurn)
        #expect(!reducer.items.contains {
            if case .editSummary = $0 { return true } else { return false }
        })
    }

    @Test func diffContentPathCountsAsEditedPath() {
        var reducer = TranscriptReducer()
        reducer.userPrompted([.text("go")])
        reducer.apply(.toolCall(ToolCall(
            toolCallId: "t1", title: "Edit", kind: .edit, status: .pending,
            content: [.diff(path: "/repo/c.swift", oldText: "a", newText: "b")])))
        reducer.apply(.toolCallUpdate(ToolCallUpdate(toolCallId: "t1",
                                                     status: .completed)))
        reducer.turnEnded(.endTurn)
        guard case .editSummary(_, let paths)? = reducer.items.first(where: {
            if case .editSummary = $0 { return true } else { return false }
        }) else { Issue.record("missing editSummary"); return }
        #expect(paths == ["/repo/c.swift"])
    }

    @Test func newPromptResetsAccumulatedPaths() {
        var reducer = TranscriptReducer()
        // Replay di session/load: edit completati arrivano SENZA turnEnded.
        reducer.apply(editCall("old", path: "/repo/stale.swift"))
        reducer.apply(.toolCallUpdate(ToolCallUpdate(toolCallId: "old",
                                                     status: .completed)))
        // Il turno live che parte dopo il replay non deve ereditarli.
        reducer.userPrompted([.text("new turn")])
        reducer.apply(editCall("t1", path: "/repo/fresh.swift"))
        reducer.apply(.toolCallUpdate(ToolCallUpdate(toolCallId: "t1",
                                                     status: .completed)))
        reducer.turnEnded(.endTurn)
        guard case .editSummary(_, let paths)? = reducer.items.first(where: {
            if case .editSummary = $0 { return true } else { return false }
        }) else { Issue.record("missing editSummary"); return }
        #expect(paths == ["/repo/fresh.swift"])
    }

    @Test func editSummaryRoundTripsThroughCodable() throws {
        let item = TranscriptItem.editSummary(id: "s-1",
                                              paths: ["/repo/a.swift"])
        let data = try JSONEncoder().encode(item)
        let decoded = try JSONDecoder().decode(TranscriptItem.self, from: data)
        #expect(decoded == item)
        #expect(item.id == "s-1")
    }
}
