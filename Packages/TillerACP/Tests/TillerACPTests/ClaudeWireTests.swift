import Testing
import Foundation
@testable import TillerACP

@Suite struct ClaudeWireTests {
    func fixtureLines() throws -> [Data] {
        let url = Bundle.module.url(forResource: "claude-init-turn",
                                    withExtension: "ndjson",
                                    subdirectory: "Fixtures")!
        return try String(contentsOf: url, encoding: .utf8)
            .split(separator: "\n").map { Data($0.utf8) }
    }

    @Test func decodesInitAndResult() throws {
        let messages = try fixtureLines().map {
            try JSONDecoder().decode(ClaudeWireMessage.self, from: $0)
        }
        guard case .systemInit(let initMsg) = messages.first else {
            Issue.record("first message is not init"); return
        }
        #expect(!initMsg.sessionId.isEmpty)
        #expect(!initMsg.model.isEmpty)
        guard case .result(let result) = messages.last else {
            Issue.record("last message is not result"); return
        }
        #expect(result.sessionId == initMsg.sessionId)
        #expect(result.usage != nil)
    }

    @Test func decodesToolUseAndToolResult() throws {
        let messages = try fixtureLines().map {
            try JSONDecoder().decode(ClaudeWireMessage.self, from: $0)
        }
        guard case .assistant(let assistant) = messages.first(where: {
            if case .assistant = $0 { return true }
            return false
        }) else {
            Issue.record("missing assistant message"); return
        }
        guard case .toolUse(let toolUse) = assistant.message.content.first else {
            Issue.record("missing tool use"); return
        }
        #expect(toolUse.name == "Bash")
        #expect(toolUse.input?["command"]?.stringValue == "echo hello")

        guard case .user(let user) = messages.first(where: {
            if case .user = $0 { return true }
            return false
        }) else {
            Issue.record("missing user tool result"); return
        }
        guard case .toolResult(let toolResult) = user.message.content.first else {
            Issue.record("missing tool result"); return
        }
        #expect(toolResult.content?.stringValue == "hello")
        #expect(toolResult.isError == false)
    }

    @Test func decodesControlRequestAndResponse() throws {
        let request = Data(#"{"type":"control_request","request_id":"req-1","request":{"subtype":"can_use_tool","tool_name":"Bash","input":{"command":"echo hello"},"permission_suggestions":[{"type":"addRules"}]}}"#.utf8)
        let response = Data(#"{"type":"control_response","request_id":"req-1","response":{"subtype":"success"}}"#.utf8)

        guard case .controlRequest(let id, let control) = try JSONDecoder().decode(ClaudeWireMessage.self, from: request) else {
            Issue.record("expected control request"); return
        }
        #expect(id == "req-1")
        #expect(control.subtype == "can_use_tool")
        #expect(control.toolName == "Bash")
        #expect(control.input?["command"]?.stringValue == "echo hello")
        #expect(control.permissionSuggestions != nil)

        guard case .controlResponse(let responseID, let payload) = try JSONDecoder().decode(ClaudeWireMessage.self, from: response) else {
            Issue.record("expected control response"); return
        }
        #expect(responseID == "req-1")
        #expect(payload?["subtype"]?.stringValue == "success")
    }

    @Test func unknownTypesDecodeAsUnknown() throws {
        let data = Data(#"{"type":"totally_new_thing"}"#.utf8)
        let message = try JSONDecoder().decode(ClaudeWireMessage.self, from: data)
        guard case .unknown = message else { Issue.record("expected unknown"); return }
    }

    @Test func decodesSystemEventSubtypes() throws {
        let compact = Data(#"{"type":"system","subtype":"compact_boundary","session_id":"s1"}"#.utf8)
        guard case .systemEvent(let event) = try JSONDecoder()
            .decode(ClaudeWireMessage.self, from: compact) else {
            Issue.record("expected system event"); return
        }
        #expect(event.subtype == "compact_boundary")

        let thinking = Data(#"{"type":"system","subtype":"thinking_tokens","estimated_tokens":50,"estimated_tokens_delta":50}"#.utf8)
        guard case .systemEvent(let tokens) = try JSONDecoder()
            .decode(ClaudeWireMessage.self, from: thinking) else {
            Issue.record("expected system event"); return
        }
        #expect(tokens.subtype == "thinking_tokens")
        #expect(tokens.payload["estimated_tokens"]?.intValue == 50)
    }

    @Test func systemInitStillDecodesAsInit() throws {
        let data = Data(#"{"type":"system","subtype":"init","session_id":"s1","model":"m","tools":[],"slash_commands":[]}"#.utf8)
        guard case .systemInit = try JSONDecoder()
            .decode(ClaudeWireMessage.self, from: data) else {
            Issue.record("init must not be swallowed by systemEvent"); return
        }
    }

    @Test func decodesRateLimitAndPromptSuggestion() throws {
        let limit = Data(#"{"type":"rate_limit_event","rate_limit_info":{"status":"allowed","resetsAt":1786298400,"rateLimitType":"five_hour"}}"#.utf8)
        guard case .rateLimit(let info) = try JSONDecoder()
            .decode(ClaudeWireMessage.self, from: limit) else {
            Issue.record("expected rate limit"); return
        }
        #expect(info.status == "allowed")
        #expect(info.rateLimitType == "five_hour")

        let suggestion = Data(#"{"type":"prompt_suggestion","suggestion":"delete f.txt"}"#.utf8)
        guard case .promptSuggestion(let text) = try JSONDecoder()
            .decode(ClaudeWireMessage.self, from: suggestion) else {
            Issue.record("expected prompt suggestion"); return
        }
        #expect(text == "delete f.txt")
    }
}
