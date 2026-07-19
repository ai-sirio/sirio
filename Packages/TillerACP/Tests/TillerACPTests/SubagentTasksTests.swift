import Foundation
import Testing
@testable import TillerACP

@Suite struct SubagentTasksTests {
    func taskToolCall(id: String, status: ToolCallStatus,
                      description: String? = nil) -> TranscriptItem {
        var input: [String: JSONValue] = [
            "subagent_type": .string("Explore"),
            "prompt": .string("look around"),
        ]
        if let description { input["description"] = .string(description) }
        var item = ToolCallItem(toolCallId: id, title: "Task", kind: .other,
                                status: status)
        item.rawInput = .object(input)
        return .toolCall(item)
    }

    @Test func extractsTaskToolCallsWithSubagentType() {
        let items: [TranscriptItem] = [
            .userMessage(id: "u1", blocks: []),
            taskToolCall(id: "t1", status: .inProgress, description: "Explore repo"),
            .agentMessage(id: "a1", text: "hi", isComplete: true),
        ]
        let tasks = SubagentTasks.extract(from: items)
        #expect(tasks.count == 1)
        #expect(tasks[0].toolCallId == "t1")
        #expect(tasks[0].title == "Explore repo")
        #expect(tasks[0].status == .inProgress)
    }

    @Test func ignoresOrdinaryToolCalls() {
        let read = ToolCallItem(toolCallId: "r1", title: "Read file", kind: .read,
                                status: .completed)
        let tasks = SubagentTasks.extract(from: [.toolCall(read)])
        #expect(tasks.isEmpty)
    }

    @Test func fallsBackToToolTitleThenSubagentWhenNoDescription() {
        let tasks = SubagentTasks.extract(from: [
            taskToolCall(id: "t1", status: .completed)
        ])
        #expect(tasks[0].title == "Task")

        var untitled = ToolCallItem(toolCallId: "t2", title: "", kind: .other,
                                    status: .pending)
        untitled.rawInput = .object(["subagent_type": .string("general")])
        let fallback = SubagentTasks.extract(from: [.toolCall(untitled)])
        #expect(fallback[0].title == "Subagent")
    }

    @Test func rawInputSurvivesToolCallItemMergeAndCodableRoundTrip() throws {
        let call = ToolCall(toolCallId: "t1", title: "Task", kind: .other,
                            status: .pending,
                            rawInput: .object(["subagent_type": .string("x")]))
        var item = ToolCallItem(call)
        #expect(item.rawInput == call.rawInput)

        // A later update without rawInput must not erase it.
        item.merge(ToolCallUpdate(toolCallId: "t1", status: .completed))
        #expect(item.rawInput == call.rawInput)
        #expect(item.status == .completed)

        let data = try JSONEncoder().encode(item)
        let decoded = try JSONDecoder().decode(ToolCallItem.self, from: data)
        #expect(decoded.rawInput == call.rawInput)
    }
}
