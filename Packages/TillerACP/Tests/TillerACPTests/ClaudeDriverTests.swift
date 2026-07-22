import Foundation
import Testing
@testable import TillerACP

@Suite struct ClaudeDriverTests {
    private actor EventCollector {
        private var values: [ACPSessionEvent] = []

        func append(_ value: ACPSessionEvent) {
            values.append(value)
        }

        func snapshot() -> [ACPSessionEvent] {
            values
        }
    }

    private func fixtureLines() throws -> [String] {
        let url = Bundle.module.url(forResource: "claude-init-turn",
                                    withExtension: "ndjson",
                                    subdirectory: "Fixtures")!
        return try String(contentsOf: url, encoding: .utf8)
            .split(separator: "\n")
            .map(String.init)
    }

    private func collect(_ driver: ClaudeStreamJSONDriver,
                         into collector: EventCollector) -> Task<Void, Never> {
        Task {
            for await event in driver.events {
                await collector.append(event)
            }
        }
    }

    private func waitForEvents(_ collector: EventCollector,
                               count: Int) async -> [ACPSessionEvent] {
        for _ in 0..<200 {
            let values = await collector.snapshot()
            if values.count >= count { return values }
            try? await Task.sleep(for: .milliseconds(5))
        }
        return await collector.snapshot()
    }

    private func jsonValue(_ data: Data) throws -> JSONValue {
        let line = data.last == UInt8(ascii: "\n") ? data.dropLast() : data[...]
        return try JSONDecoder().decode(JSONValue.self, from: line)
    }

    @Test func fixtureTurnMapsToCanonicalUpdates() async throws {
        let mock = MockTransport()
        let driver = ClaudeStreamJSONDriver(transport: mock, permissionMode: .ask,
                                            model: nil, resumeSessionId: nil)
        let collector = EventCollector()
        let eventTask = collect(driver, into: collector)
        try await driver.start()

        let fixture = try fixtureLines()
        await mock.emit(fixture[0])
        let handle = try await driver.connect(cwd: "/tmp/w", resumeSessionId: nil,
                                              mcpServers: [])
        #expect(handle.sessionId == "4e3c4112-ac2f-42fe-a224-e7dbea976d0c")
        #expect(handle.didResume == false)
        #expect(handle.models?.currentModelId == "claude-haiku-4-5-20251001")

        await mock.emit(#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"hello"}]}}"#)
        for line in fixture.dropFirst() { await mock.emit(line) }

        let reason = try await driver.prompt([.text("run the command")])
        let events = await waitForEvents(collector, count: 5)
        let updates = events.compactMap { event -> SessionUpdate? in
            guard case .update(let update) = event else { return nil }
            return update
        }

        #expect(updates.contains { if case .availableCommandsUpdate = $0 { true } else { false } })
        #expect(updates.contains { if case .agentMessageChunk(.text("hello")) = $0 { true } else { false } })
        #expect(updates.contains { if case .toolCall(let call) = $0 {
            call.toolCallId == "toolu_019NCm2MgW5v58jSzsLBLHrZ" && call.kind == .execute
        } else { false } })
        #expect(updates.contains { if case .toolCallUpdate(let update) = $0 {
            update.toolCallId == "toolu_019NCm2MgW5v58jSzsLBLHrZ" && update.status == .completed
        } else { false } })
        #expect(updates.contains { if case .usageUpdate = $0 { true } else { false } })
        #expect(reason == .endTurn)

        let sent = try await mock.waitForSent(count: 1)
        let prompt = try jsonValue(sent[0])
        #expect(prompt["type"]?.stringValue == "user")
        #expect(prompt["message"]?["role"]?.stringValue == "user")
        eventTask.cancel()
        await driver.stop()
    }

    @Test func canUseToolRoundTrip() async throws {
        let mock = MockTransport()
        let driver = ClaudeStreamJSONDriver(transport: mock, permissionMode: .ask,
                                            model: nil, resumeSessionId: nil)
        let collector = EventCollector()
        let eventTask = collect(driver, into: collector)
        try await driver.start()
        await mock.emit(#"{"type":"system","subtype":"init","session_id":"s1","model":"claude-test","slash_commands":[]}"#)
        _ = try await driver.connect(cwd: "/tmp/w", resumeSessionId: nil, mcpServers: [])

        await mock.emit(#"{"type":"control_request","request_id":"req-1","request":{"subtype":"can_use_tool","tool_name":"Bash","input":{"command":"echo hello"},"permission_suggestions":[{"type":"addRules"}]}}"#)
        let events = await waitForEvents(collector, count: 2)
        guard let permission = events.compactMap({ event -> (JSONRPCID, ToolCallUpdate, [PermissionOption])? in
            guard case .permissionRequested(let requestId, let toolCall, let options) = event else { return nil }
            return (requestId, toolCall, options)
        }).first else {
            Issue.record("expected permissionRequested")
            return
        }
        #expect(permission.0 == .string("req-1"))
        #expect(permission.1.toolCallId == "req-1")
        #expect(permission.1.title == "Bash: echo hello")
        #expect(permission.2.map(\.optionId) == ["allow_once", "reject_once", "allow_always"])

        await driver.answerPermission(requestId: .string("req-1"),
                                       outcome: .selected(optionId: "allow_once"))
        let sent = try await mock.waitForSent(count: 1)
        let response = try jsonValue(sent[0])
        #expect(response["type"]?.stringValue == "control_response")
        #expect(response["response"]?["request_id"]?.stringValue == "req-1")
        #expect(response["response"]?["response"]?["behavior"]?.stringValue == "allow")
        eventTask.cancel()
        await driver.stop()
    }

    @Test func setModeWritesControlRequest() async throws {
        let mock = MockTransport()
        let driver = ClaudeStreamJSONDriver(transport: mock, permissionMode: .ask,
                                            model: nil, resumeSessionId: nil)
        let collector = EventCollector()
        let eventTask = collect(driver, into: collector)
        try await driver.start()
        await mock.emit(#"{"type":"system","subtype":"init","session_id":"s1","model":"claude-test","slash_commands":[]}"#)
        _ = try await driver.connect(cwd: "/tmp/w", resumeSessionId: nil, mcpServers: [])

        let modeTask = Task { try await driver.setMode(PermissionMode.acceptEdits.rawValue) }
        _ = try await mock.waitForSent(count: 1)
        let request = try jsonValue(await mock.sent[0])
        #expect(request["type"]?.stringValue == "control_request")
        #expect(request["request"]?["subtype"]?.stringValue == "set_permission_mode")
        #expect(request["request"]?["mode"]?.stringValue == "acceptEdits")
        let requestId = request["request_id"]?.stringValue
        #expect(requestId != nil)
        let responseLine = #"{"type":"control_response","request_id":"__ID__","response":{"subtype":"success"}}"#
        await mock.emit(responseLine.replacingOccurrences(of: "__ID__", with: requestId ?? ""))
        try await modeTask.value

        let events = await waitForEvents(collector, count: 2)
        #expect(events.contains { if case .update(.currentModeUpdate("acceptEdits")) = $0 { true } else { false } })
        eventTask.cancel()
        await driver.stop()
    }

    @Test func eofEmitsDisconnected() async throws {
        let mock = MockTransport()
        let driver = ClaudeStreamJSONDriver(transport: mock, permissionMode: .ask,
                                            model: nil, resumeSessionId: nil)
        let collector = EventCollector()
        let eventTask = collect(driver, into: collector)
        try await driver.start()
        await mock.emit(#"{"type":"system","subtype":"init","session_id":"s1","model":"claude-test","slash_commands":[]}"#)
        _ = try await driver.connect(cwd: "/tmp/w", resumeSessionId: nil, mcpServers: [])
        let promptTask = Task { try? await driver.prompt([.text("hello")]) }
        _ = try await mock.waitForSent(count: 1)
        await mock.close()
        _ = await promptTask.value

        let events = await waitForEvents(collector, count: 2)
        #expect(events.filter { if case .disconnected = $0 { true } else { false } }.count == 1)
        eventTask.cancel()
    }

    @Test func launchCommandLine() {
        let transport = ClaudeStreamJSONDriver.launchTransport(
            worktreePath: "/tmp/w", permissionMode: .plan,
            model: "claude-sonnet-5", resumeSessionId: "abc")
        #expect(transport.arguments.last!.contains("--permission-mode plan"))
        #expect(transport.arguments.last!.contains("--resume abc"))
        #expect(transport.arguments.last!.contains("--model claude-sonnet-5"))
    }
}
