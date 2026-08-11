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

    /// Waits for the event a test actually cares about. Counting events
    /// instead couples the test to how many unrelated ones the driver emits.
    private func waitForEvent(
        _ collector: EventCollector,
        where predicate: @Sendable (ACPSessionEvent) -> Bool
    ) async -> [ACPSessionEvent] {
        for _ in 0..<200 {
            let values = await collector.snapshot()
            if values.contains(where: predicate) { return values }
            try? await Task.sleep(for: .milliseconds(5))
        }
        return await collector.snapshot()
    }

    private func jsonValue(_ data: Data) throws -> JSONValue {
        let line = data.last == UInt8(ascii: "\n") ? data.dropLast() : data[...]
        return try JSONDecoder().decode(JSONValue.self, from: line)
    }

    private func connectWithInitializeResponse(
        _ driver: ClaudeStreamJSONDriver,
        mock: MockTransport,
        modelId: String = "claude-test",
        resumeSessionId: String? = nil
    ) async throws -> SessionHandle {
        let connectTask = Task {
            try await driver.connect(cwd: "/tmp/w", resumeSessionId: resumeSessionId,
                                     mcpServers: [])
        }
        let sent = try await mock.waitForSent(count: 1)
        let request = try jsonValue(sent[0])
        #expect(request["type"]?.stringValue == "control_request")
        #expect(request["request"]?["subtype"]?.stringValue == "initialize")
        let requestId = request["request_id"]?.stringValue
        #expect(requestId != nil)
        let response = """
        {"type":"control_response","response":{"subtype":"success","request_id":"\(requestId ?? "")","response":{"commands":[{"name":"doctor","description":"Diagnose the session","argumentHint":""}],"models":[{"value":"\(modelId)","resolvedModel":"\(modelId)","displayName":"Test model","description":"A test model"}]}}}
        """
        await mock.emit(response)
        return try await connectTask.value
    }

    @Test func fixtureTurnMapsToCanonicalUpdates() async throws {
        let mock = MockTransport()
        let driver = ClaudeStreamJSONDriver(transport: mock, permissionMode: .ask,
                                            model: nil, resumeSessionId: nil)
        let collector = EventCollector()
        let eventTask = collect(driver, into: collector)
        try await driver.start()

        let fixture = try fixtureLines()
        let handle = try await connectWithInitializeResponse(
            driver, mock: mock, modelId: "claude-haiku-4-5-20251001")
        #expect(UUID(uuidString: handle.sessionId) != nil)
        #expect(handle.didResume == false)
        #expect(handle.models?.currentModelId == "claude-haiku-4-5-20251001")

        let promptTask = Task { try await driver.prompt([.text("run the command")]) }
        _ = try await mock.waitForSent(count: 2)
        await mock.emit(#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"hello"}]}}"#)
        for line in fixture { await mock.emit(line) }
        let reason = try await promptTask.value
        let events = await waitForEvents(collector, count: 4)
        let updates = events.compactMap { event -> SessionUpdate? in
            guard case .update(let update) = event else { return nil }
            return update
        }

        #expect(updates.contains {
            if case .availableCommandsUpdate([AvailableCommand(
                name: "doctor", description: "Diagnose the session")]) = $0 { true } else { false }
        })
        #expect(updates.contains { if case .agentMessageChunk(.text("hello")) = $0 { true } else { false } })
        #expect(updates.contains { if case .toolCall(let call) = $0 {
            call.toolCallId == "toolu_019NCm2MgW5v58jSzsLBLHrZ" && call.kind == .execute
        } else { false } })
        #expect(updates.contains { if case .toolCallUpdate(let update) = $0 {
            update.toolCallId == "toolu_019NCm2MgW5v58jSzsLBLHrZ" && update.status == .completed
        } else { false } })
        #expect(!updates.contains { if case .usageUpdate = $0 { true } else { false } })
        #expect(reason == .endTurn)

        let sent = try await mock.waitForSent(count: 2)
        let prompt = try jsonValue(sent[1])
        #expect(prompt["type"]?.stringValue == "user")
        #expect(prompt["message"]?["role"]?.stringValue == "user")
        eventTask.cancel()
        await driver.stop()
    }

    @Test func commandsChangedRefreshesTheCommandList() async throws {
        let mock = MockTransport()
        let driver = ClaudeStreamJSONDriver(transport: mock, permissionMode: .ask,
                                            model: nil, resumeSessionId: nil)
        let collector = EventCollector()
        let eventTask = collect(driver, into: collector)
        try await driver.start()
        _ = try await connectWithInitializeResponse(driver, mock: mock)

        await mock.emit(#"{"type":"commands_changed","commands":[{"name":"newcmd","description":"Added later"}]}"#)

        let events = await waitForEvent(collector) { event in
            guard case .update(.availableCommandsUpdate(let commands)) = event else { return false }
            return commands.contains { $0.name == "newcmd" }
        }
        #expect(events.contains { event in
            guard case .update(.availableCommandsUpdate(let commands)) = event else { return false }
            return commands.contains { $0.name == "newcmd" }
        })

        eventTask.cancel()
        await driver.stop()
    }

    @Test func modelsCarryTheirEffortLevels() async throws {
        let mock = MockTransport()
        let driver = ClaudeStreamJSONDriver(transport: mock, permissionMode: .ask,
                                            model: "sonnet", resumeSessionId: nil)
        try await driver.start()

        let connectTask = Task {
            try await driver.connect(cwd: "/tmp/w", resumeSessionId: nil, mcpServers: [])
        }
        let sent = try await mock.waitForSent(count: 1)
        let requestId = try jsonValue(sent[0])["request_id"]?.stringValue ?? ""
        await mock.emit("""
        {"type":"control_response","response":{"subtype":"success","request_id":"\(requestId)","response":{"models":[\
        {"value":"sonnet","displayName":"Sonnet","supportsEffort":true,"supportedEffortLevels":["low","medium","high","xhigh","max"]},\
        {"value":"haiku","displayName":"Haiku"}]}}}
        """)
        let handle = try await connectTask.value

        let models = handle.models?.availableModels ?? []
        #expect(models.first { $0.modelId == "sonnet" }?.supportedEffortLevels
            == ["low", "medium", "high", "xhigh", "max"])
        #expect(models.first { $0.modelId == "haiku" }?.supportedEffortLevels == nil)

        await driver.stop()
    }

    @Test func effortIsSentAsAFlagSettingAndValidated() async throws {
        let mock = MockTransport()
        let driver = ClaudeStreamJSONDriver(transport: mock, permissionMode: .ask,
                                            model: "sonnet", resumeSessionId: nil)
        try await driver.start()

        let connectTask = Task {
            try await driver.connect(cwd: "/tmp/w", resumeSessionId: nil, mcpServers: [])
        }
        let sent = try await mock.waitForSent(count: 1)
        let requestId = try jsonValue(sent[0])["request_id"]?.stringValue ?? ""
        await mock.emit("""
        {"type":"control_response","response":{"subtype":"success","request_id":"\(requestId)","response":{"models":[\
        {"value":"sonnet","displayName":"Sonnet","supportsEffort":true,"supportedEffortLevels":["low","medium","high","xhigh","max"]}]}}}
        """)
        _ = try await connectTask.value

        await driver.setEffort("xhigh")
        let afterValid = try await mock.waitForSent(count: 2)
        let request = try jsonValue(afterValid[1])
        #expect(request["request"]?["subtype"]?.stringValue == "apply_flag_settings")
        #expect(request["request"]?["settings"]?["effort"]?.stringValue == "xhigh")

        // Rejected before it reaches the wire: the CLI would answer "success".
        await driver.setEffort("bogus_level")
        #expect(await mock.sent.count == 2)

        let options = await driver.staticEffortOptions()
        #expect(options?.options?.map(\.value)
            == ["low", "medium", "high", "xhigh", "max"])
        #expect(options?.currentValue == "xhigh")

        await driver.stop()
    }

    @Test func modelWithoutEffortSupportOffersNoOptions() async throws {
        let mock = MockTransport()
        let driver = ClaudeStreamJSONDriver(transport: mock, permissionMode: .ask,
                                            model: "haiku", resumeSessionId: nil)
        try await driver.start()

        let connectTask = Task {
            try await driver.connect(cwd: "/tmp/w", resumeSessionId: nil, mcpServers: [])
        }
        let sent = try await mock.waitForSent(count: 1)
        let requestId = try jsonValue(sent[0])["request_id"]?.stringValue ?? ""
        await mock.emit("""
        {"type":"control_response","response":{"subtype":"success","request_id":"\(requestId)","response":{"models":[\
        {"value":"haiku","displayName":"Haiku"}]}}}
        """)
        _ = try await connectTask.value

        #expect(await driver.staticEffortOptions() == nil)
        await driver.stop()
    }

    @Test func promptCarriesNoEffortPrefix() async throws {
        let mock = MockTransport()
        let driver = ClaudeStreamJSONDriver(transport: mock, permissionMode: .ask,
                                            model: nil, resumeSessionId: nil,
                                            effort: "high")
        try await driver.start()
        _ = try await connectWithInitializeResponse(driver, mock: mock)

        let promptTask = Task { try await driver.prompt([.text("do the thing")]) }
        let sent = try await mock.waitForSent(count: 2)
        let text = try jsonValue(sent[1])["message"]?["content"]?
            .arrayValue?.first?["text"]?.stringValue
        #expect(text == "do the thing")

        await mock.emit(#"{"type":"result","subtype":"success","is_error":false,"session_id":"s1"}"#)
        _ = try await promptTask.value
        await driver.stop()
    }

    @Test func systemInitDoesNotReplaceRichCommandDescriptions() async throws {
        let mock = MockTransport()
        let driver = ClaudeStreamJSONDriver(transport: mock, permissionMode: .ask,
                                            model: nil, resumeSessionId: nil)
        let collector = EventCollector()
        let eventTask = collect(driver, into: collector)
        try await driver.start()
        _ = try await connectWithInitializeResponse(driver, mock: mock)

        let initialEvents = await waitForEvent(collector) { event in
            guard case .update(.availableCommandsUpdate(let commands)) = event else { return false }
            return commands.contains {
                $0.name == "doctor" && !$0.description.isEmpty
            }
        }
        #expect(initialEvents.contains { event in
            guard case .update(.availableCommandsUpdate(let commands)) = event else { return false }
            return commands.contains { $0.name == "doctor" && !$0.description.isEmpty }
        })

        await mock.emit(#"{"type":"system","subtype":"init","session_id":"s1","slash_commands":["doctor"]}"#)

        let events = await waitForEvents(collector, count: 2)
        let commandUpdates = events.compactMap { event -> [AvailableCommand]? in
            guard case .update(.availableCommandsUpdate(let commands)) = event else { return nil }
            return commands
        }
        #expect(commandUpdates.last?.allSatisfy { !$0.description.isEmpty } == true)

        eventTask.cancel()
        await driver.stop()
    }

    /// Turn completion has to reach the transcript through the same stream as
    /// the content it closes. Applied out of band it can overtake chunks the
    /// consumer has not read yet, and the tail of a reply lands after the
    /// turn divider instead of inside the message.
    @Test func turnEndReachesTheStreamAfterTheLastChunk() async throws {
        let mock = MockTransport()
        let driver = ClaudeStreamJSONDriver(transport: mock, permissionMode: .ask,
                                            model: nil, resumeSessionId: nil)
        let collector = EventCollector()
        let eventTask = collect(driver, into: collector)
        try await driver.start()
        _ = try await connectWithInitializeResponse(driver, mock: mock)

        let promptTask = Task { try await driver.prompt([.text("hello")]) }
        _ = try await mock.waitForSent(count: 2)
        await mock.emit(#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"hi"}]}}"#)
        await mock.emit(#"{"type":"result","subtype":"success","is_error":false,"session_id":"s1"}"#)
        #expect(try await promptTask.value == .endTurn)

        let events = await waitForEvents(collector, count: 2)
        let chunkIndex = events.firstIndex {
            if case .update(.agentMessageChunk(.text("hi"))) = $0 { true } else { false }
        }
        let endIndex = events.firstIndex {
            if case .turnEnded(.endTurn) = $0 { true } else { false }
        }
        #expect(chunkIndex != nil)
        #expect(endIndex != nil)
        if let chunkIndex, let endIndex { #expect(chunkIndex < endIndex) }
        eventTask.cancel()
        await driver.stop()
    }

    @Test func contextUsageProbeAfterResult() async throws {
        let mock = MockTransport()
        let driver = ClaudeStreamJSONDriver(transport: mock, permissionMode: .ask,
                                            model: nil, resumeSessionId: nil)
        let collector = EventCollector()
        let eventTask = collect(driver, into: collector)
        try await driver.start()
        _ = try await connectWithInitializeResponse(driver, mock: mock)

        let promptTask = Task { try await driver.prompt([.text("hello")]) }
        _ = try await mock.waitForSent(count: 2)
        await mock.emit(#"{"type":"result","subtype":"success","is_error":false,"session_id":"s1","usage":{"input_tokens":42}}"#)

        let sent = try await mock.waitForSent(count: 3)
        let probe = try jsonValue(sent[2])
        #expect(probe["type"]?.stringValue == "control_request")
        #expect(probe["request"]?["subtype"]?.stringValue == "get_context_usage")
        let requestId = probe["request_id"]?.stringValue
        #expect(requestId != nil)
        let success = #"{"type":"control_response","request_id":"__ID__","response":{"subtype":"success","totalTokens":1000,"maxTokens":200000,"rawMaxTokens":200000}}"#
        await mock.emit(success.replacingOccurrences(of: "__ID__", with: requestId ?? ""))

        #expect(try await promptTask.value == .endTurn)
        let events = await waitForEvent(collector) {
            if case .update(.usageUpdate) = $0 { true } else { false }
        }
        let updates = events.compactMap { event -> SessionUpdate? in
            guard case .update(let update) = event else { return nil }
            return update
        }
        #expect(updates.contains {
            if case .usageUpdate(ContextUsage(used: 1000, size: 200000)) = $0 { true } else { false }
        })

        let secondPromptTask = Task { try await driver.prompt([.text("again")]) }
        _ = try await mock.waitForSent(count: 4)
        await mock.emit(#"{"type":"result","subtype":"success","is_error":false,"session_id":"s1"}"#)
        let secondSent = try await mock.waitForSent(count: 5)
        let secondProbe = try jsonValue(secondSent[4])
        let secondRequestId = secondProbe["request_id"]?.stringValue
        let error = #"{"type":"control_response","request_id":"__ID__","response":{"subtype":"error","error":"unsupported"}}"#
        await mock.emit(error.replacingOccurrences(of: "__ID__", with: secondRequestId ?? ""))
        #expect(try await secondPromptTask.value == .endTurn)

        try await Task.sleep(for: .milliseconds(20))
        let finalEvents = await waitForEvents(collector, count: events.count)
        let usageUpdates = finalEvents.compactMap { event -> ContextUsage? in
            guard case .update(.usageUpdate(let usage)) = event else { return nil }
            return usage
        }
        #expect(usageUpdates == [ContextUsage(used: 1000, size: 200000)])
        eventTask.cancel()
        await driver.stop()
    }

    @Test func contextUsageComesFromModelUsageWithoutProbing() async throws {
        let mock = MockTransport()
        let driver = ClaudeStreamJSONDriver(transport: mock, permissionMode: .ask,
                                            model: nil, resumeSessionId: nil)
        let collector = EventCollector()
        let eventTask = collect(driver, into: collector)
        try await driver.start()
        _ = try await connectWithInitializeResponse(driver, mock: mock)

        let promptTask = Task { try await driver.prompt([.text("hi")]) }
        _ = try await mock.waitForSent(count: 2)
        await mock.emit(#"{"type":"result","subtype":"success","is_error":false,"session_id":"s1","usage":{"input_tokens":4,"output_tokens":123,"cache_read_input_tokens":83967},"modelUsage":{"claude-sonnet-5":{"contextWindow":1000000}}}"#)
        _ = try await promptTask.value

        let events = await waitForEvent(collector) { event in
            if case .update(.usageUpdate) = event { return true }
            return false
        }
        #expect(events.contains { event in
            guard case .update(.usageUpdate(let usage)) = event else { return false }
            return usage.size == 1_000_000 && usage.used == 4 + 123 + 83967
        })

        eventTask.cancel()
        await driver.stop()
    }

    @Test func contextUsageFromModelUsageIncludesCostAndBreakdown() async throws {
        let mock = MockTransport()
        let driver = ClaudeStreamJSONDriver(transport: mock, permissionMode: .ask,
                                            model: nil, resumeSessionId: nil)
        let collector = EventCollector()
        let eventTask = collect(driver, into: collector)
        try await driver.start()
        _ = try await connectWithInitializeResponse(driver, mock: mock)

        let promptTask = Task { try await driver.prompt([.text("hi")]) }
        _ = try await mock.waitForSent(count: 2)
        await mock.emit(#"{"type":"result","subtype":"success","is_error":false,"session_id":"s1","total_cost_usd":0.0421,"usage":{"input_tokens":4,"output_tokens":123,"cache_read_input_tokens":83967,"cache_creation_input_tokens":512},"modelUsage":{"claude-sonnet-5":{"contextWindow":1000000}}}"#)
        _ = try await promptTask.value

        let events = await waitForEvent(collector) { event in
            if case .update(.usageUpdate) = event { return true }
            return false
        }
        #expect(events.contains { event in
            guard case .update(.usageUpdate(let usage)) = event else { return false }
            return usage.costUsd == 0.0421
                && usage.inputTokens == 4
                && usage.outputTokens == 123
                && usage.cacheReadTokens == 83967
                && usage.cacheCreationTokens == 512
        })

        eventTask.cancel()
        await driver.stop()
    }

    @Test func contextUsageOmitsCostAndBreakdownWhenResultLacksThem() async throws {
        let mock = MockTransport()
        let driver = ClaudeStreamJSONDriver(transport: mock, permissionMode: .ask,
                                            model: nil, resumeSessionId: nil)
        let collector = EventCollector()
        let eventTask = collect(driver, into: collector)
        try await driver.start()
        _ = try await connectWithInitializeResponse(driver, mock: mock)

        let promptTask = Task { try await driver.prompt([.text("hi")]) }
        _ = try await mock.waitForSent(count: 2)
        await mock.emit(#"{"type":"result","subtype":"success","is_error":false,"session_id":"s1","usage":{"input_tokens":4,"output_tokens":123},"modelUsage":{"claude-sonnet-5":{"contextWindow":1000000}}}"#)
        _ = try await promptTask.value

        let events = await waitForEvent(collector) { event in
            if case .update(.usageUpdate) = event { return true }
            return false
        }
        #expect(events.contains { event in
            guard case .update(.usageUpdate(let usage)) = event else { return false }
            return usage.costUsd == nil
                && usage.inputTokens == 4
                && usage.outputTokens == 123
                && usage.cacheReadTokens == nil
                && usage.cacheCreationTokens == nil
        })

        eventTask.cancel()
        await driver.stop()
    }

    @Test func resultEmitsTurnStats() async throws {
        let mock = MockTransport()
        let driver = ClaudeStreamJSONDriver(transport: mock, permissionMode: .ask,
                                            model: nil, resumeSessionId: nil)
        let collector = EventCollector()
        let eventTask = collect(driver, into: collector)
        try await driver.start()
        _ = try await connectWithInitializeResponse(driver, mock: mock)

        let promptTask = Task { try await driver.prompt([.text("hi")]) }
        _ = try await mock.waitForSent(count: 2)
        await mock.emit(#"{"type":"result","subtype":"success","is_error":false,"session_id":"s1","total_cost_usd":0.408,"duration_ms":11672,"ttft_ms":3794,"num_turns":2,"usage":{"input_tokens":4,"output_tokens":123}}"#)
        _ = try await promptTask.value

        let events = await waitForEvent(collector) { event in
            if case .update(.notice) = event { return true }
            return false
        }
        let notice = events.compactMap { event -> String? in
            guard case .update(.notice(let text)) = event else { return nil }
            return text
        }.first
        #expect(notice?.contains("$0.41") == true)
        #expect(notice?.contains("11.7s") == true)
        #expect(notice?.contains("123") == true)

        eventTask.cancel()
        await driver.stop()
    }

    @Test func compactionAndRateLimitBecomeNotices() async throws {
        let mock = MockTransport()
        let driver = ClaudeStreamJSONDriver(transport: mock, permissionMode: .ask,
                                            model: nil, resumeSessionId: nil)
        let collector = EventCollector()
        let eventTask = collect(driver, into: collector)
        try await driver.start()
        _ = try await connectWithInitializeResponse(driver, mock: mock)

        await mock.emit(#"{"type":"system","subtype":"compact_boundary","session_id":"s1"}"#)
        await mock.emit(#"{"type":"rate_limit_event","rate_limit_info":{"status":"allowed","rateLimitType":"five_hour"}}"#)
        await mock.emit(#"{"type":"rate_limit_event","rate_limit_info":{"status":"rejected","rateLimitType":"five_hour"}}"#)

        _ = await waitForEvent(collector) { event in
            guard case .update(.notice(let text)) = event else { return false }
            return text.lowercased().contains("rate limit")
        }
        let notices = await collector.snapshot().compactMap { event -> String? in
            guard case .update(.notice(let text)) = event else { return nil }
            return text
        }
        #expect(notices.contains { $0.lowercased().contains("compact") })
        // "allowed" is the normal state and must stay silent.
        #expect(notices.filter { $0.lowercased().contains("rate limit") }.count == 1)

        eventTask.cancel()
        await driver.stop()
    }

    @Test func effortOptionsRequireModelCapabilities() async throws {
        let mock = MockTransport()
        let driver = ClaudeStreamJSONDriver(transport: mock, permissionMode: .ask,
                                            model: nil, resumeSessionId: nil)
        #expect(await driver.staticEffortOptions() == nil)
    }

    @Test func effortIsNotInjectedIntoPrompt() async throws {
        let mock = MockTransport()
        let driver = ClaudeStreamJSONDriver(transport: mock, permissionMode: .ask,
                                            model: nil, resumeSessionId: nil)
        let collector = EventCollector()
        let eventTask = collect(driver, into: collector)
        try await driver.start()
        _ = try await connectWithInitializeResponse(driver, mock: mock)

        let promptTask = Task { try await driver.prompt([.text("hi")]) }
        let sent = try await mock.waitForSent(count: 2)
        let prompt = try jsonValue(sent[1])
        let content = prompt["message"]?["content"]?.arrayValue
        #expect(content?.first?["text"]?.stringValue
                == "hi")

        await mock.emit(#"{"type":"result","subtype":"success","is_error":false,"session_id":"s1"}"#)
        _ = try await promptTask.value
        let events = await waitForEvents(collector, count: 1)
        #expect(!events.contains {
            if case .update(.userMessageChunk) = $0 { true } else { false }
        })
        eventTask.cancel()
        await driver.stop()
    }

    /// Claude Code forwards `message.content` to the Messages API verbatim,
    /// so ACP-only blocks have to be rewritten before they hit the wire:
    /// `resource_link` is answered with a 400, and an ACP-shaped image makes
    /// the CLI exit without any output.
    @Test func acpOnlyBlocksAreRewrittenForTheMessagesAPI() async throws {
        let mock = MockTransport()
        let driver = ClaudeStreamJSONDriver(transport: mock, permissionMode: .ask,
                                            model: nil, resumeSessionId: nil)
        try await driver.start()
        _ = try await connectWithInitializeResponse(driver, mock: mock)

        let promptTask = Task {
            try await driver.prompt([
                .text("hi"),
                .resourceLink(uri: "file:///tmp/w/App/A%20B.swift", name: "A B.swift"),
                .resource(uri: "file:///tmp/w/note.md", text: "inline note"),
                .image(mimeType: "image/png", data: "AAA"),
                .unknown(type: "future_block")
            ])
        }
        let sent = try await mock.waitForSent(count: 2)
        let content = try jsonValue(sent[1])["message"]?["content"]?.arrayValue

        #expect(content?.allSatisfy {
            ["text", "image"].contains($0["type"]?.stringValue ?? "")
        } == true)
        #expect(content?.count == 4)
        #expect(content?[1]["text"]?.stringValue == "@/tmp/w/App/A B.swift")
        #expect(content?[2]["text"]?.stringValue == "inline note")
        #expect(content?[3]["source"] == .object([
            "type": .string("base64"),
            "media_type": .string("image/png"),
            "data": .string("AAA")
        ]))

        await mock.emit(#"{"type":"result","subtype":"success","is_error":false,"session_id":"s1"}"#)
        _ = try await promptTask.value
        await driver.stop()
    }

    @Test func canUseToolRoundTrip() async throws {
        let mock = MockTransport()
        let driver = ClaudeStreamJSONDriver(transport: mock, permissionMode: .ask,
                                            model: nil, resumeSessionId: nil)
        let collector = EventCollector()
        let eventTask = collect(driver, into: collector)
        try await driver.start()
        _ = try await connectWithInitializeResponse(driver, mock: mock)

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
        let sent = try await mock.waitForSent(count: 2)
        let response = try jsonValue(sent[1])
        #expect(response["type"]?.stringValue == "control_response")
        #expect(response["response"]?["request_id"]?.stringValue == "req-1")
        #expect(response["response"]?["response"]?["behavior"]?.stringValue == "allow")
        eventTask.cancel()
        await driver.stop()
    }

    @Test func unhandledControlRequestsStillGetAResponse() async throws {
        let mock = MockTransport()
        let driver = ClaudeStreamJSONDriver(transport: mock, permissionMode: .ask,
                                            model: nil, resumeSessionId: nil)
        let collector = EventCollector()
        let eventTask = collect(driver, into: collector)
        try await driver.start()
        _ = try await connectWithInitializeResponse(driver, mock: mock)

        await mock.emit(#"{"type":"control_request","request_id":"rq-9","request":{"subtype":"request_user_dialog"}}"#)

        let sent = try await mock.waitForSent(count: 2)
        let reply = try jsonValue(sent[1])
        #expect(reply["type"]?.stringValue == "control_response")
        #expect(reply["response"]?["request_id"]?.stringValue == "rq-9")
        #expect(reply["response"]?["subtype"]?.stringValue == "error")

        eventTask.cancel()
        await driver.stop()
    }

    @Test func fullAutoAutomaticallyAllowsCanUseTool() async throws {
        let mock = MockTransport()
        let driver = ClaudeStreamJSONDriver(transport: mock, permissionMode: .fullAuto,
                                            model: nil, resumeSessionId: nil)
        let collector = EventCollector()
        let eventTask = collect(driver, into: collector)
        try await driver.start()
        _ = try await connectWithInitializeResponse(driver, mock: mock)

        await mock.emit(#"{"type":"control_request","request_id":"req-auto","request":{"subtype":"can_use_tool","tool_name":"Bash","input":{"command":"echo hello"}}}"#)
        let sent = try await mock.waitForSent(count: 2)
        let response = try jsonValue(sent[1])
        #expect(response["type"]?.stringValue == "control_response")
        #expect(response["response"]?["request_id"]?.stringValue == "req-auto")
        #expect(response["response"]?["subtype"]?.stringValue == "success")
        #expect(response["response"]?["response"]?["behavior"]?.stringValue == "allow")

        try await Task.sleep(for: .milliseconds(20))
        let events = await collector.snapshot()
        #expect(!events.contains {
            if case .permissionRequested = $0 { true } else { false }
        })
        eventTask.cancel()
        await driver.stop()
    }

    @Test func toolResultCarriesStructuredPayload() async throws {
        let mock = MockTransport()
        let driver = ClaudeStreamJSONDriver(transport: mock, permissionMode: .ask,
                                            model: nil, resumeSessionId: nil)
        let collector = EventCollector()
        let eventTask = collect(driver, into: collector)
        try await driver.start()
        _ = try await connectWithInitializeResponse(driver, mock: mock)

        await mock.emit(#"{"type":"user","message":{"role":"user","content":[{"tool_use_id":"tu-1","type":"tool_result","content":"1\thi\n"}]},"tool_use_result":{"type":"text","file":{"filePath":"/w/f.txt","numLines":2,"totalLines":2}}}"#)

        let events = await waitForEvent(collector) { event in
            if case .update(.toolCallUpdate) = event { return true }
            return false
        }
        let update = events.compactMap { event -> ToolCallUpdate? in
            guard case .update(.toolCallUpdate(let update)) = event else { return nil }
            return update
        }.first
        #expect(update?.toolCallId == "tu-1")
        #expect(update?.rawOutput?["file"]?["numLines"]?.intValue == 2)

        eventTask.cancel()
        await driver.stop()
    }

    @Test func setModeErrorIncludesClaudeMessage() async throws {
        let mock = MockTransport()
        let driver = ClaudeStreamJSONDriver(transport: mock, permissionMode: .ask,
                                            model: nil, resumeSessionId: nil)
        try await driver.start()
        _ = try await connectWithInitializeResponse(driver, mock: mock)

        let modeTask = Task { try await driver.setMode("bypassPermissions") }
        _ = try await mock.waitForSent(count: 2)
        let request = try jsonValue(await mock.sent[1])
        let requestId = request["request_id"]?.stringValue
        let message = "Cannot set permission mode to bypassPermissions because the session was not launched with --dangerously-skip-permissions"
        let responseLine = #"{"type":"control_response","request_id":"__ID__","response":{"subtype":"error","error":"MESSAGE"}}"#
            .replacingOccurrences(of: "__ID__", with: requestId ?? "")
            .replacingOccurrences(of: "MESSAGE", with: message)
        await mock.emit(responseLine)

        do {
            try await modeTask.value
            Issue.record("setMode should throw when Claude rejects the mode")
        } catch {
            #expect(String(describing: error) == message)
        }
        await driver.stop()
    }

    @Test func setModeWritesControlRequest() async throws {
        let mock = MockTransport()
        let driver = ClaudeStreamJSONDriver(transport: mock, permissionMode: .ask,
                                            model: nil, resumeSessionId: nil)
        let collector = EventCollector()
        let eventTask = collect(driver, into: collector)
        try await driver.start()
        _ = try await connectWithInitializeResponse(driver, mock: mock)

        let modeTask = Task { try await driver.setMode(PermissionMode.acceptEdits.rawValue) }
        _ = try await mock.waitForSent(count: 2)
        let request = try jsonValue(await mock.sent[1])
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
        _ = try await connectWithInitializeResponse(driver, mock: mock)
        let promptTask = Task { try? await driver.prompt([.text("hello")]) }
        _ = try await mock.waitForSent(count: 2)
        await mock.close()
        _ = await promptTask.value

        let events = await waitForEvents(collector, count: 2)
        #expect(events.filter { if case .disconnected = $0 { true } else { false } }.count == 1)
        eventTask.cancel()
    }

    @Test func connectDoesNotRequireInitMessage() async throws {
        let mock = MockTransport()
        let driver = ClaudeStreamJSONDriver(transport: mock, permissionMode: .ask,
                                            model: nil, resumeSessionId: nil)
        try await driver.start()

        let handle = try await connectWithInitializeResponse(driver, mock: mock)

        #expect(!handle.sessionId.isEmpty)
        #expect(UUID(uuidString: handle.sessionId) != nil)
        #expect(handle.models?.availableModels.map(\.modelId) == ["claude-test"])
        await driver.stop()
    }

    @Test func connectUsesResumeSessionId() async throws {
        let mock = MockTransport()
        let driver = ClaudeStreamJSONDriver(transport: mock, permissionMode: .ask,
                                            model: nil, resumeSessionId: "abc")
        try await driver.start()

        let handle = try await connectWithInitializeResponse(
            driver, mock: mock, resumeSessionId: "abc")

        #expect(handle.sessionId == "abc")
        #expect(handle.didResume)
        await driver.stop()
    }

    @Test func connectThrowsWhenTransportClosesBeforeInitializeResponse() async throws {
        let mock = MockTransport()
        let driver = ClaudeStreamJSONDriver(transport: mock, permissionMode: .ask,
                                            model: nil, resumeSessionId: nil)
        try await driver.start()

        let connectTask = Task { try await driver.connect(cwd: "/tmp/w",
                                                          resumeSessionId: nil,
                                                          mcpServers: []) }
        _ = try await mock.waitForSent(count: 1)
        await mock.close()

        var didThrow = false
        do {
            _ = try await connectTask.value
            Issue.record("connect should fail when initialize response cannot arrive")
        } catch {
            didThrow = true
        }
        #expect(didThrow)
    }

    @Test func launchCommandLine() {
        let resumeLaunch = ClaudeStreamJSONDriver.launchTransport(
            worktreePath: "/tmp/w", permissionMode: .plan,
            model: "claude-sonnet-5", resumeSessionId: "abc", effort: "xhigh")
        let resumeCommand = resumeLaunch.transport.arguments.last!
        #expect(resumeCommand.contains("--permission-mode plan"))
        #expect(resumeCommand.contains("--resume abc"))
        #expect(!resumeCommand.contains("--session-id"))
        #expect(resumeLaunch.sessionId == "abc")
        #expect(resumeCommand.contains("--model claude-sonnet-5"))
        #expect(resumeCommand.contains("--effort xhigh"))

        let freshLaunch = ClaudeStreamJSONDriver.launchTransport(
            worktreePath: "/tmp/w", permissionMode: .plan,
            model: nil, resumeSessionId: nil)
        let freshCommand = freshLaunch.transport.arguments.last!
        #expect(freshCommand.contains("--session-id \(freshLaunch.sessionId)"))
        #expect(!freshCommand.contains("--resume"))
        #expect(UUID(uuidString: freshLaunch.sessionId) != nil)
    }
}
