import Foundation
import Testing
@testable import TillerACP

@Suite struct CodexDriverTests {
    // Codex 0.145.0 uses thread/start and turn/start (not the older
    // newConversation/sendUserTurn names from the initial plan). Its live
    // stream uses item/* notifications, thread/tokenUsage/updated, and
    // turn/completed; commandExecution is the exec begin/end pair.
    private actor EventCollector {
        private var values: [ACPSessionEvent] = []

        func append(_ value: ACPSessionEvent) { values.append(value) }
        func snapshot() -> [ACPSessionEvent] { values }
    }

    private func fixtureLines() throws -> [String] {
        let url = Bundle.module.url(forResource: "codex-init-turn",
                                    withExtension: "ndjson",
                                    subdirectory: "Fixtures")!
        return try String(contentsOf: url, encoding: .utf8)
            .split(separator: "\n")
            .map(String.init)
    }

    private func collect(_ driver: CodexAppServerDriver,
                         into collector: EventCollector) -> Task<Void, Never> {
        Task {
            for await event in driver.events { await collector.append(event) }
        }
    }

    private func waitForEvents(_ collector: EventCollector,
                               count: Int) async -> [ACPSessionEvent] {
        for _ in 0..<400 {
            let values = await collector.snapshot()
            if values.count >= count { return values }
            try? await Task.sleep(for: .milliseconds(5))
        }
        return await collector.snapshot()
    }

    private func updates(_ events: [ACPSessionEvent]) -> [SessionUpdate] {
        events.compactMap { event in
            guard case .update(let update) = event else { return nil }
            return update
        }
    }

    @Test func fixtureTurnMapsToOrderedCanonicalUpdates() async throws {
        let mock = MockTransport()
        let driver = CodexAppServerDriver(client: ACPClient(transport: mock),
                                          permissionMode: .acceptEdits,
                                          model: nil, effort: "high",
                                          resumeConversationId: nil)
        let collector = EventCollector()
        let eventTask = collect(driver, into: collector)
        try await driver.start()

        let fixture = try fixtureLines()
        let connectTask = Task {
            try await driver.connect(cwd: "/tmp/w", resumeSessionId: nil, mcpServers: [])
        }
        _ = try await mock.waitForSent(count: 1)
        await mock.emit(fixture[0])
        _ = try await mock.waitForSent(count: 2)
        await mock.emit(fixture[2])
        let handle = try await connectTask.value

        #expect(!handle.sessionId.isEmpty)
        #expect(handle.didResume == false)
        #expect(handle.models?.currentModelId == "gpt-5.6-terra")

        let promptTask = Task {
            try await driver.prompt([.text("Run: echo hello then reply done")])
        }
        _ = try await mock.waitForSent(count: 3)
        for line in fixture.dropFirst(3) { await mock.emit(line) }
        #expect(try await promptTask.value == .endTurn)

        let events = await waitForEvents(collector, count: 14)
        let actual = updates(events)
        guard actual.count >= 14 else {
            Issue.record("expected fixture to produce at least 14 canonical updates")
            eventTask.cancel()
            await driver.stop()
            return
        }

        #expect(actual[0] == .agentThoughtChunk(.text("**Preparing tool invocation**")))
        #expect(actual[1] == .agentMessageChunk(.text("E")))
        #expect(actual[2] == .agentMessageChunk(.text("segu")))
        #expect(actual[7] == .agentMessageChunk(.text("esto")))
        #expect(actual[8] == .agentMessageChunk(.text(".")))
        guard case .toolCall(let call) = actual[9] else {
            Issue.record("expected commandExecution item/started to map to toolCall")
            return
        }
        #expect(call.toolCallId.hasPrefix("exec-"))
        #expect(call.kind == .execute)
        #expect(call.title == "/bin/zsh -lc 'echo hello'")
        guard case .toolCallUpdate(let commandEnd) = actual[10] else {
            Issue.record("expected commandExecution item/completed to map to toolCallUpdate")
            return
        }
        #expect(commandEnd.status == .completed)
        #expect(commandEnd.content == [.content(.text("hello\n"))])
        #expect(actual[11] == .usageUpdate(ContextUsage(used: 28857, size: 258400)))
        #expect(actual[12] == .agentMessageChunk(.text("done")))
        #expect(actual[13] == .usageUpdate(ContextUsage(used: 57744, size: 258400)))

        eventTask.cancel()
        await driver.stop()
    }

    @Test func approvalRequestRoundTripsThroughJSONRPC() async throws {
        let mock = MockTransport()
        let driver = CodexAppServerDriver(client: ACPClient(transport: mock),
                                          permissionMode: .ask,
                                          model: nil, effort: nil,
                                          resumeConversationId: nil)
        let collector = EventCollector()
        let eventTask = collect(driver, into: collector)
        try await driver.start()
        let connectTask = Task {
            try await driver.connect(cwd: "/tmp/w", resumeSessionId: nil, mcpServers: [])
        }
        _ = try await mock.waitForSent(count: 1)
        await mock.emit(#"{"id":1,"result":{"userAgent":"test/0.145.0"}}"#)
        _ = try await mock.waitForSent(count: 2)
        await mock.emit(#"{"id":2,"result":{"thread":{"id":"thread-1"},"model":"gpt-test","approvalPolicy":"untrusted","sandbox":{"type":"workspaceWrite"}}}"#)
        _ = try await connectTask.value

        await mock.emit(#"{"jsonrpc":"2.0","id":42,"method":"execCommandApproval","params":{"threadId":"thread-1","turnId":"turn-1","itemId":"exec-1","command":"echo hello","cwd":"/tmp/w","reason":"needs approval"}}"#)
        let events = await waitForEvents(collector, count: 1)
        guard case .permissionRequested(let requestId, let toolCall, let options)? = events.first else {
            Issue.record("expected permissionRequested")
            return
        }
        #expect(requestId == .number(42))
        #expect(toolCall.toolCallId == "exec-1")
        #expect(toolCall.kind == .execute)
        #expect(toolCall.title == "echo hello")
        #expect(options.map(\.optionId) == ["allow_once", "reject_once"])

        await driver.answerPermission(requestId: requestId,
                                      outcome: .selected(optionId: "allow_once"))
        _ = try await mock.waitForSent(count: 4)
        guard case .response(let responseId, let result, let error) = try await mock.sentMessage(3) else {
            Issue.record("expected JSON-RPC permission response")
            return
        }
        #expect(responseId == .number(42))
        #expect(error == nil)
        #expect(result?["decision"]?.stringValue == "approved")

        eventTask.cancel()
        await driver.stop()
    }

    @Test func modeIsAppliedOnNextTurnAndEmittedImmediately() async throws {
        let mock = MockTransport()
        let driver = CodexAppServerDriver(client: ACPClient(transport: mock),
                                          permissionMode: .ask,
                                          model: "gpt-test", effort: "medium",
                                          resumeConversationId: nil)
        let collector = EventCollector()
        let eventTask = collect(driver, into: collector)
        try await driver.start()
        let connectTask = Task {
            try await driver.connect(cwd: "/tmp/w", resumeSessionId: nil, mcpServers: [])
        }
        _ = try await mock.waitForSent(count: 1)
        await mock.emit(#"{"id":1,"result":{}}"#)
        _ = try await mock.waitForSent(count: 2)
        await mock.emit(#"{"id":2,"result":{"thread":{"id":"thread-1"},"model":"gpt-test"}}"#)
        _ = try await connectTask.value

        try await driver.setMode(PermissionMode.fullAuto.rawValue)
        let modeEvents = await waitForEvents(collector, count: 1)
        #expect(modeEvents.contains { if case .update(.currentModeUpdate("fullAuto")) = $0 { true } else { false } })

        let promptTask = Task { try await driver.prompt([.text("hello")]) }
        _ = try await mock.waitForSent(count: 4)
        let request = try await mock.sentMessage(3)
        guard case .request(_, let method, let params) = request else {
            Issue.record("expected turn/start request")
            return
        }
        #expect(method == "turn/start")
        #expect(params?["threadId"]?.stringValue == "thread-1")
        #expect(params?["approvalPolicy"]?.stringValue == "never")
        #expect(params?["model"]?.stringValue == "gpt-test")
        #expect(params?["effort"]?.stringValue == "medium")
        #expect(params?["sandboxPolicy"]?["type"]?.stringValue == "workspaceWrite")
        await mock.emit(#"{"id":3,"result":{"turn":{"id":"turn-1"}}}"#)
        await mock.emit(#"{"method":"turn/completed","params":{"threadId":"thread-1","turn":{"status":"completed"}}}"#)
        #expect(try await promptTask.value == .endTurn)

        eventTask.cancel()
        await driver.stop()
    }

    @Test func eofEmitsDisconnectedExactlyOnce() async throws {
        let mock = MockTransport()
        let driver = CodexAppServerDriver(client: ACPClient(transport: mock),
                                          permissionMode: .ask,
                                          model: nil, effort: nil,
                                          resumeConversationId: nil)
        let collector = EventCollector()
        let eventTask = collect(driver, into: collector)
        try await driver.start()
        let connectTask = Task {
            try await driver.connect(cwd: "/tmp/w", resumeSessionId: nil, mcpServers: [])
        }
        _ = try await mock.waitForSent(count: 1)
        await mock.emit(#"{"id":1,"result":{}}"#)
        _ = try await mock.waitForSent(count: 2)
        await mock.emit(#"{"id":2,"result":{"thread":{"id":"thread-1"}}}"#)
        _ = try await connectTask.value
        await mock.close()

        let events = await waitForEvents(collector, count: 1)
        #expect(events.filter { if case .disconnected = $0 { true } else { false } }.count == 1)
        eventTask.cancel()
    }
}
