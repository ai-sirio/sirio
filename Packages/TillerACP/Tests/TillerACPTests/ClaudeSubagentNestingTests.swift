import Foundation
import Testing
@testable import TillerACP

@Suite struct ClaudeSubagentNestingTests {
    /// Feeds raw wire lines through the driver and collects everything it emits
    /// until the transport closes. `MockTransport` already exists in this target
    /// (`Tests/TillerACPTests/MockTransport.swift`) — do not add another double.
    func events(from lines: [String]) async throws -> [ACPSessionEvent] {
        let transport = MockTransport()
        let driver = ClaudeStreamJSONDriver(transport: transport,
                                            permissionMode: .ask,
                                            model: nil, resumeSessionId: nil)
        let stream = driver.events
        try await driver.start()
        for line in lines { await transport.emit(line) }
        await transport.close()
        var collected: [ACPSessionEvent] = []
        for await event in stream { collected.append(event) }
        return collected
    }

    @Test func childToolCallCarriesItsParent() async throws {
        let line = """
        {"type":"assistant","session_id":"s","parent_tool_use_id":"task-1",\
        "message":{"role":"assistant","content":[{"type":"tool_use","id":"c1",\
        "name":"Read","input":{"file_path":"/tmp/a.swift"}}]}}
        """
        let events = try await events(from: [line])
        let calls: [ToolCall] = events.compactMap {
            if case .update(.toolCall(let call)) = $0 { return call } else { return nil }
        }
        #expect(calls.count == 1)
        #expect(calls[0].parentToolCallId == "task-1")
    }

    @Test func topLevelToolCallHasNoParent() async throws {
        let line = """
        {"type":"assistant","session_id":"s",\
        "message":{"role":"assistant","content":[{"type":"tool_use","id":"t1",\
        "name":"Read","input":{}}]}}
        """
        let events = try await events(from: [line])
        let calls: [ToolCall] = events.compactMap {
            if case .update(.toolCall(let call)) = $0 { return call } else { return nil }
        }
        #expect(calls[0].parentToolCallId == nil)
    }

    @Test func subagentTextIsNotEmittedAsAgentMessage() async throws {
        let line = """
        {"type":"assistant","session_id":"s","parent_tool_use_id":"task-1",\
        "message":{"role":"assistant","content":[{"type":"text",\
        "text":"looking around the repo"}]}}
        """
        let events = try await events(from: [line])
        let chunks = events.filter {
            if case .update(.agentMessageChunk) = $0 { return true } else { return false }
        }
        #expect(chunks.isEmpty)
    }

    @Test func mainAgentTextStillBecomesAnAgentMessage() async throws {
        let line = """
        {"type":"assistant","session_id":"s",\
        "message":{"role":"assistant","content":[{"type":"text","text":"done"}]}}
        """
        let events = try await events(from: [line])
        let chunks = events.filter {
            if case .update(.agentMessageChunk) = $0 { return true } else { return false }
        }
        #expect(chunks.count == 1)
    }
}
