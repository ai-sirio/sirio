import Foundation
import Testing
@testable import TillerACP

private actor PiEventCollector {
    private(set) var values: [ACPSessionEvent] = []
    func append(_ value: ACPSessionEvent) { values.append(value) }
}

private func collect(_ driver: PiRPCDriver, into collector: PiEventCollector)
    -> Task<Void, Never> {
    Task {
        for await event in driver.events { await collector.append(event) }
    }
}

private actor BlockedPromptTransport: ACPTransport {
    private(set) var sent: [Data] = []
    private var continuation: AsyncThrowingStream<Data, Error>.Continuation?
    private var pendingLines: [Data] = []
    private var promptWriteEntered = false
    private var releasePrompt: CheckedContinuation<Void, Never>?

    func start() {}

    func terminate() async {}

    func send(line: Data) async throws {
        sent.append(line)
        let value = try? JSONDecoder().decode(JSONValue.self, from: line)
        guard value?["type"]?.stringValue == "prompt" else { return }
        promptWriteEntered = true
        await withCheckedContinuation { (continuation: CheckedContinuation<Void, Never>) in
            releasePrompt = continuation
        }
    }

    nonisolated func lines() -> AsyncThrowingStream<Data, Error> {
        AsyncThrowingStream { continuation in
            Task { await self.attach(continuation) }
        }
    }

    private func attach(_ continuation: AsyncThrowingStream<Data, Error>.Continuation) {
        self.continuation = continuation
        for line in pendingLines { continuation.yield(line) }
        pendingLines = []
    }

    func emit(_ json: String) {
        let data = Data(json.utf8)
        if let continuation { continuation.yield(data) } else { pendingLines.append(data) }
    }

    func waitForPromptWrite() async throws {
        for _ in 0..<100 {
            if promptWriteEntered { return }
            try await Task.sleep(for: .milliseconds(5))
        }
        throw CancellationError()
    }

    func releasePromptWrite() {
        releasePrompt?.resume()
        releasePrompt = nil
    }

    func waitForSent(count: Int) async throws -> [Data] {
        for _ in 0..<100 {
            if sent.count >= count { return sent }
            try await Task.sleep(for: .milliseconds(5))
        }
        return sent
    }
}

@Suite struct PiDriverTests {
    private func value(_ data: Data) throws -> JSONValue {
        let line = data.last == UInt8(ascii: "\n") ? data.dropLast() : data[...]
        return try JSONDecoder().decode(JSONValue.self, from: line)
    }

    private func requestId(_ data: Data) throws -> String {
        try #require(value(data)["id"]?.stringValue)
    }

    private func successResponse(id: String, command: String,
                                 data: JSONValue? = nil) throws -> String {
        var fields: [String: JSONValue] = [
            "id": .string(id), "type": .string("response"),
            "command": .string(command), "success": .bool(true)
        ]
        fields["data"] = data
        let encoded = try JSONEncoder().encode(JSONValue.object(fields))
        return String(decoding: encoded, as: UTF8.self)
    }

    @Test func launchUsesLoginShellRpcApproveModelAndResume() {
        let transport = PiRPCDriver.launchTransport(
            worktreePath: "/tmp/work tree", model: "anthropic/claude-sonnet-4",
            resumeSessionId: "/tmp/session file.jsonl", onStderrLine: nil)
        #expect(transport.arguments == [
            "-lc",
            "exec pi --mode rpc --approve --model anthropic/claude-sonnet-4 " +
            "--session '/tmp/session file.jsonl'"
        ])
    }

    @Test func connectBuildsSessionModelsThinkingAndCommands() async throws {
        let mock = MockTransport()
        let driver = PiRPCDriver(transport: mock, model: nil, effort: nil,
                                 resumeSessionId: "/tmp/old-session.jsonl")
        var eventIterator = driver.events.makeAsyncIterator()
        try await driver.start()
        let connect = Task {
            try await driver.connect(
                cwd: "/tmp/work", resumeSessionId: "/tmp/old-session.jsonl",
                mcpServers: [])
        }

        var sent = try await mock.waitForSent(count: 1)
        let stateId = try requestId(sent[0])
        let stateData: JSONValue = .object([
            "model": .object([
                "id": .string("claude-sonnet-4"),
                "name": .string("Sonnet 4"),
                "provider": .string("anthropic")
            ]),
            "thinkingLevel": .string("medium"),
            "isStreaming": .bool(false),
            "sessionFile": .string("/tmp/pi-session.jsonl"),
            "sessionId": .string("pi-id")
        ])
        await mock.emit(try successResponse(id: stateId, command: "get_state",
                                             data: stateData))

        sent = try await mock.waitForSent(count: 2)
        let modelsId = try requestId(sent[1])
        let modelsData: JSONValue = .object([
            "models": .array([
                .object(["id": .string("claude-sonnet-4"),
                         "name": .string("Sonnet 4"),
                         "provider": .string("anthropic")]),
                .object(["id": .string("gpt-5"),
                         "name": .string("GPT-5"),
                         "provider": .string("openai")])
            ])
        ])
        await mock.emit(try successResponse(id: modelsId,
                                             command: "get_available_models",
                                             data: modelsData))

        sent = try await mock.waitForSent(count: 3)
        let thinkingId = try requestId(sent[2])
        await mock.emit(#"{"id":"\#(thinkingId)","type":"response","command":"get_available_thinking_levels","success":true,"data":{"levels":["off","medium","high"]}}"#)

        sent = try await mock.waitForSent(count: 4)
        let commandsId = try requestId(sent[3])
        let commandsData: JSONValue = .object([
            "commands": .array([
                .object(["name": .string("fix-tests"),
                         "description": .string("Fix failing tests"),
                         "source": .string("prompt")])
            ])
        ])
        await mock.emit(try successResponse(id: commandsId,
                                             command: "get_commands",
                                             data: commandsData))

        let handle = try await connect.value
        #expect(handle.sessionId == "/tmp/pi-session.jsonl")
        #expect(handle.didResume == true)
        #expect(handle.agentCapabilities.loadSession == true)
        #expect(handle.models?.currentModelId == "anthropic/claude-sonnet-4")
        #expect(handle.models?.availableModels.map(\.modelId) == [
            "anthropic/claude-sonnet-4", "openai/gpt-5"
        ])
        #expect(handle.configOptions.first?.id == "effort")
        #expect(handle.configOptions.first?.currentValue == "medium")
        #expect(handle.configOptions.first?.options?.map(\.value) == ["off", "medium", "high"])
        guard case .some(.update(.availableCommandsUpdate(let commands))) =
                await eventIterator.next() else {
            Issue.record("expected available command update")
            return
        }
        #expect(commands == [.init(name: "fix-tests", description: "Fix failing tests")])
    }

    @Test func setModelSplitsProviderFromModelId() async throws {
        let mock = MockTransport()
        let driver = PiRPCDriver(transport: mock, model: nil, effort: nil,
                                 resumeSessionId: nil)
        try await driver.start()
        await driver.markConnectedForTesting(sessionId: "session")

        let change = Task { try await driver.setModel("anthropic/claude-sonnet-4") }
        let sent = try await mock.waitForSent(count: 1)
        let payload = try value(sent[0])
        #expect(payload["type"]?.stringValue == "set_model")
        #expect(payload["provider"]?.stringValue == "anthropic")
        #expect(payload["modelId"]?.stringValue == "claude-sonnet-4")
        let id = try requestId(sent[0])
        await mock.emit(#"{"id":"\#(id)","type":"response","command":"set_model","success":true,"data":{"id":"claude-sonnet-4","provider":"anthropic","name":"Sonnet 4"}}"#)
        try await change.value
    }

    @Test func concurrentCommandsCorrelateOutOfOrderResponsesById() async throws {
        let mock = MockTransport()
        let driver = PiRPCDriver(transport: mock, model: nil, effort: nil,
                                 resumeSessionId: nil)
        try await driver.start()
        await driver.markConnectedForTesting(sessionId: "session")

        let model = Task { try await driver.setModel("anthropic/claude-sonnet-4") }
        let effort = Task {
            try await driver.setConfigOption(id: "effort", value: "high")
        }
        let sent = try await mock.waitForSent(count: 2)
        let first = try value(sent[0])
        let second = try value(sent[1])
        let firstId = try #require(first["id"]?.stringValue)
        let secondId = try #require(second["id"]?.stringValue)
        let firstCommand = try #require(first["type"]?.stringValue)
        let secondCommand = try #require(second["type"]?.stringValue)
        let modelData: JSONValue = .object([
            "id": .string("claude-sonnet-4"), "provider": .string("anthropic"),
            "name": .string("Sonnet 4")
        ])

        await mock.emit(try successResponse(
            id: secondId, command: secondCommand,
            data: secondCommand == "set_model" ? modelData : nil))
        await mock.emit(try successResponse(
            id: firstId, command: firstCommand,
            data: firstCommand == "set_model" ? modelData : nil))

        try await model.value
        let effortOptions = try await effort.value
        #expect(effortOptions?.first?.currentValue == "high")
    }

    @Test func agentEndDoesNotFinishPromptButAgentSettledDoes() async throws {
        let mock = MockTransport()
        let driver = PiRPCDriver(transport: mock, model: nil, effort: nil,
                                 resumeSessionId: nil)
        try await driver.start()
        await driver.markConnectedForTesting(sessionId: "session")

        let prompt = Task { try await driver.prompt([.text("hello")]) }
        let sent = try await mock.waitForSent(count: 1)
        let id = try requestId(sent[0])
        await mock.emit(#"{"id":"\#(id)","type":"response","command":"prompt","success":true}"#)
        await mock.emit(#"{"type":"agent_start"}"#)
        await mock.emit(#"{"type":"agent_end","messages":[],"willRetry":true}"#)

        try await Task.sleep(for: .milliseconds(30))
        #expect(await driver.hasPendingPromptForTesting)

        await mock.emit(#"{"type":"agent_settled"}"#)
        #expect(try await prompt.value == .endTurn)
        #expect(await driver.hasPendingPromptForTesting == false)
    }

    @Test func oneSettlementResolvesInitialPromptAndSteerWithOneTurnEnd() async throws {
        let mock = MockTransport()
        let driver = PiRPCDriver(transport: mock, model: nil, effort: nil,
                                 resumeSessionId: nil)
        let collector = PiEventCollector()
        let collecting = collect(driver, into: collector)
        try await driver.start()
        await driver.markConnectedForTesting(sessionId: "session")

        let initial = Task { try await driver.prompt([.text("start")]) }
        var sent = try await mock.waitForSent(count: 1)
        let initialId = try requestId(sent[0])
        await mock.emit(#"{"id":"\#(initialId)","type":"response","command":"prompt","success":true}"#)
        await mock.emit(#"{"type":"agent_start"}"#)
        try await driver.waitForStreamingForTesting()

        let steer = Task { try await driver.prompt([.text("change direction")]) }
        sent = try await mock.waitForSent(count: 2)
        #expect((try value(sent[1]))["streamingBehavior"]?.stringValue == "steer")
        let steerId = try requestId(sent[1])
        await mock.emit(#"{"id":"\#(steerId)","type":"response","command":"prompt","success":true}"#)
        await mock.emit(#"{"type":"agent_settled"}"#)

        #expect(try await initial.value == .endTurn)
        #expect(try await steer.value == .endTurn)
        for _ in 0..<100 {
            if !(await collector.values).isEmpty { break }
            await Task.yield()
        }
        let turnEnds = (await collector.values).filter {
            if case .turnEnded = $0 { return true }
            return false
        }
        #expect(turnEnds.count == 1)
        collecting.cancel()
    }

    @Test func confirmedAgentStartStateIsUsedForSteering() async throws {
        let mock = MockTransport()
        let driver = PiRPCDriver(transport: mock, model: nil, effort: nil,
                                 resumeSessionId: nil)
        try await driver.start()
        await driver.markConnectedForTesting(sessionId: "session")
        await mock.emit(#"{"type":"agent_start"}"#)
        try await driver.waitForStreamingForTesting()

        let prompt = Task { try await driver.prompt([.text("steer now")]) }
        let sent = try await mock.waitForSent(count: 1)
        #expect((try value(sent[0]))["streamingBehavior"]?.stringValue == "steer")
        let id = try requestId(sent[0])
        await mock.emit(#"{"id":"\#(id)","type":"response","command":"prompt","success":true}"#)
        await mock.emit(#"{"type":"agent_settled"}"#)
        #expect(try await prompt.value == .endTurn)
    }

    @Test func cancelIsSentBeforeFollowingPromptAndAppliesToActiveRun() async throws {
        let mock = MockTransport()
        let driver = PiRPCDriver(transport: mock, model: nil, effort: nil,
                                 resumeSessionId: nil)
        try await driver.start()
        await driver.markConnectedForTesting(sessionId: "session")

        let initial = Task { try await driver.prompt([.text("start")]) }
        var sent = try await mock.waitForSent(count: 1)
        let initialId = try requestId(sent[0])
        await mock.emit(#"{"id":"\#(initialId)","type":"response","command":"prompt","success":true}"#)
        await mock.emit(#"{"type":"agent_start"}"#)
        try await driver.waitForStreamingForTesting()

        await driver.cancel()
        sent = try await mock.waitForSent(count: 2)
        #expect((try value(sent[1]))["type"]?.stringValue == "abort")

        let steer = Task { try await driver.prompt([.text("change")]) }
        sent = try await mock.waitForSent(count: 3)
        #expect((try value(sent[2]))["streamingBehavior"]?.stringValue == "steer")
        let steerId = try requestId(sent[2])
        await mock.emit(#"{"id":"\#(steerId)","type":"response","command":"prompt","success":true}"#)
        await mock.emit(#"{"type":"agent_settled"}"#)
        #expect(try await initial.value == .cancelled)
        #expect(try await steer.value == .cancelled)
    }

    @Test func cancelCannotOvertakeBlockedPromptWrite() async throws {
        let mock = BlockedPromptTransport()
        let driver = PiRPCDriver(transport: mock, model: nil, effort: nil,
                                 resumeSessionId: nil)
        try await driver.start()
        await driver.markConnectedForTesting(sessionId: "session")

        let prompt = Task { try await driver.prompt([.text("start")]) }
        try await mock.waitForPromptWrite()

        let cancel = Task { await driver.cancel() }
        try await Task.sleep(for: .milliseconds(30))
        let blockedSent = await mock.sent
        #expect(blockedSent.count == 1)
        #expect((try value(blockedSent[0]))["type"]?.stringValue == "prompt")

        await mock.releasePromptWrite()
        let sent = try await mock.waitForSent(count: 2)
        #expect((try value(sent[0]))["type"]?.stringValue == "prompt")
        #expect((try value(sent[1]))["type"]?.stringValue == "abort")

        let promptId = try requestId(sent[0])
        await mock.emit(#"{"id":"\#(promptId)","type":"response","command":"prompt","success":true}"#)
        await mock.emit(#"{"type":"agent_settled"}"#)
        #expect(try await prompt.value == .cancelled)
        await cancel.value
    }

    @Test func idleCancellationDoesNotLeakIntoNextSettledRun() async throws {
        let mock = MockTransport()
        let driver = PiRPCDriver(transport: mock, model: nil, effort: nil,
                                 resumeSessionId: nil)
        try await driver.start()
        await driver.markConnectedForTesting(sessionId: "session")

        await driver.cancel()
        let prompt = Task { try await driver.prompt([.text("fresh run")]) }
        let sent = try await mock.waitForSent(count: 1)
        let id = try requestId(sent[0])
        await mock.emit(#"{"id":"\#(id)","type":"response","command":"prompt","success":true}"#)
        await mock.emit(#"{"type":"agent_settled"}"#)
        #expect(try await prompt.value == .endTurn)
    }

    @Test func promptAfterEOFIsRejectedAsDisconnected() async throws {
        let mock = MockTransport()
        let driver = PiRPCDriver(transport: mock, model: nil, effort: nil,
                                 resumeSessionId: nil)
        try await driver.start()
        await driver.markConnectedForTesting(sessionId: "session")
        await mock.close()
        try await driver.waitForFinishForTesting()

        do {
            _ = try await driver.prompt([.text("after eof")])
            Issue.record("prompt after EOF unexpectedly succeeded")
        } catch let error as PiRPCDriverError {
            #expect(error == .disconnected)
        }
    }

    @Test func ordinaryMidStreamPromptSteersButSlashCommandDoesNot() async throws {
        let mock = MockTransport()
        let driver = PiRPCDriver(transport: mock, model: nil, effort: nil,
                                 resumeSessionId: nil)
        try await driver.start()
        await driver.markConnectedForTesting(sessionId: "session")
        await mock.emit(#"{"type":"agent_start"}"#)
        try await driver.waitForStreamingForTesting()

        let ordinary = Task { try await driver.prompt([.text("change direction")]) }
        var sent = try await mock.waitForSent(count: 1)
        #expect((try value(sent[0]))["streamingBehavior"]?.stringValue == "steer")
        let ordinaryId = try requestId(sent[0])
        await mock.emit(#"{"id":"\#(ordinaryId)","type":"response","command":"prompt","success":true}"#)
        await mock.emit(#"{"type":"agent_settled"}"#)
        _ = try await ordinary.value

        await mock.emit(#"{"type":"agent_start"}"#)
        try await driver.waitForStreamingForTesting()
        let slash = Task { try await driver.prompt([.text("/fix-tests")]) }
        sent = try await mock.waitForSent(count: 2)
        #expect((try value(sent[1]))["streamingBehavior"] == nil)
        let slashId = try requestId(sent[1])
        await mock.emit(#"{"id":"\#(slashId)","type":"response","command":"prompt","success":true}"#)
        await mock.emit(#"{"type":"agent_settled"}"#)
        _ = try await slash.value
    }
}
