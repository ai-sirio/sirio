import Testing
import Foundation
@testable import TillerACP

@Suite struct SessionUpdateTests {
    private func decode(_ json: String) throws -> SessionNotification {
        try JSONDecoder().decode(SessionNotification.self, from: Data(json.utf8))
    }

    @Test func decodesAgentMessageChunk() throws {
        let note = try decode("""
        {"sessionId":"s1","update":{"sessionUpdate":"agent_message_chunk",
         "content":{"type":"text","text":"Hello"}}}
        """)
        #expect(note.sessionId == "s1")
        #expect(note.update == .agentMessageChunk(.text("Hello")))
    }

    @Test func decodesToolCallWithDiffAndLocation() throws {
        let note = try decode("""
        {"sessionId":"s1","update":{"sessionUpdate":"tool_call","toolCallId":"tc1",
         "title":"Edit Fetcher.swift","kind":"edit","status":"pending",
         "content":[{"type":"diff","path":"/w/Fetcher.swift","oldText":"a","newText":"b"}],
         "locations":[{"path":"/w/Fetcher.swift","line":12}]}}
        """)
        guard case .toolCall(let call) = note.update else {
            Issue.record("expected toolCall"); return
        }
        #expect(call.toolCallId == "tc1")
        #expect(call.kind == .edit)
        #expect(call.status == .pending)
        #expect(call.content == [.diff(path: "/w/Fetcher.swift", oldText: "a", newText: "b")])
        #expect(call.locations == [ToolCallLocation(path: "/w/Fetcher.swift", line: 12)])
    }

    @Test func decodesToolCallUpdateWithPartialFields() throws {
        let note = try decode("""
        {"sessionId":"s1","update":{"sessionUpdate":"tool_call_update",
         "toolCallId":"tc1","status":"completed"}}
        """)
        guard case .toolCallUpdate(let update) = note.update else {
            Issue.record("expected toolCallUpdate"); return
        }
        #expect(update.toolCallId == "tc1")
        #expect(update.status == .completed)
        #expect(update.title == nil)
        #expect(update.content == nil)
    }

    @Test func decodesPlanCommandsAndMode() throws {
        let plan = try decode("""
        {"sessionId":"s1","update":{"sessionUpdate":"plan","entries":
         [{"content":"Add retry","priority":"high","status":"in_progress"}]}}
        """)
        #expect(plan.update == .plan([PlanEntry(content: "Add retry", priority: "high",
                                                status: "in_progress")]))

        let commands = try decode("""
        {"sessionId":"s1","update":{"sessionUpdate":"available_commands_update",
         "availableCommands":[{"name":"init","description":"Set up project"}]}}
        """)
        #expect(commands.update == .availableCommandsUpdate(
            [AvailableCommand(name: "init", description: "Set up project")]))

        let mode = try decode("""
        {"sessionId":"s1","update":{"sessionUpdate":"current_mode_update","currentModeId":"plan"}}
        """)
        #expect(mode.update == .currentModeUpdate("plan"))
    }

    @Test func unknownUpdateAndKindAreTolerated() throws {
        let note = try decode("""
        {"sessionId":"s1","update":{"sessionUpdate":"totally_new_thing","x":1}}
        """)
        #expect(note.update == .unknown("totally_new_thing"))

        let weirdKind = try decode("""
        {"sessionId":"s1","update":{"sessionUpdate":"tool_call","toolCallId":"tc2",
         "title":"?","kind":"quantum","status":"pending"}}
        """)
        guard case .toolCall(let call) = weirdKind.update else {
            Issue.record("expected toolCall"); return
        }
        #expect(call.kind == .other)
        #expect(call.content.isEmpty)
    }

    @Test func permissionRequestDecodesAndOutcomeEncodes() throws {
        let params = try JSONDecoder().decode(RequestPermissionParams.self, from: Data("""
        {"sessionId":"s1","toolCall":{"toolCallId":"tc1"},
         "options":[{"optionId":"allow","name":"Allow","kind":"allow_once"},
                    {"optionId":"reject","name":"Reject","kind":"reject_once"}]}
        """.utf8))
        #expect(params.toolCall.toolCallId == "tc1")
        #expect(params.options.map(\.kind) == [.allowOnce, .rejectOnce])

        let selected = try JSONValue.encoding(
            RequestPermissionResult(outcome: .selected(optionId: "allow")))
        #expect(selected["outcome"]?["outcome"]?.stringValue == "selected")
        #expect(selected["outcome"]?["optionId"]?.stringValue == "allow")

        let cancelled = try JSONValue.encoding(RequestPermissionResult(outcome: .cancelled))
        #expect(cancelled["outcome"]?["outcome"]?.stringValue == "cancelled")
    }

    @Test func fsMessagesRoundTrip() throws {
        let read = try JSONDecoder().decode(ReadTextFileParams.self, from: Data(
            #"{"sessionId":"s1","path":"/w/a.swift","line":10,"limit":50}"#.utf8))
        #expect(read.path == "/w/a.swift")
        #expect(read.line == 10)

        let write = try JSONDecoder().decode(WriteTextFileParams.self, from: Data(
            #"{"sessionId":"s1","path":"/w/a.swift","content":"let x = 1"}"#.utf8))
        #expect(write.content == "let x = 1")
    }
}
