import Foundation

private final class PiStreamingState: @unchecked Sendable {
    private let lock = NSLock()
    private var streaming = false

    var isStreaming: Bool {
        lock.lock()
        defer { lock.unlock() }
        return streaming
    }

    func setStreaming(_ value: Bool) {
        lock.lock()
        streaming = value
        lock.unlock()
    }
}

private actor PiCommandSendGate {
    func send(_ line: Data, via transport: any ACPTransport) async throws {
        try await transport.send(line: line)
    }
}

public enum PiRPCDriverError: Error, Sendable, Equatable {
    case notStarted
    case notConnected
    case invalidModelId(String)
    case commandFailed(command: String, message: String)
    case disconnected
}

public actor PiRPCDriver: AgentDriver {
    private enum UIRequestKind: Equatable { case select, confirm, input }

    private let transport: any ACPTransport
    private let requestedModel: String?
    private let requestedEffort: String?
    private let requestedResumeSessionId: String?
    private let streamingState = PiStreamingState()
    private let sendGate = PiCommandSendGate()
    private var started = false
    private var finished = false
    private var stopping = false
    private var runActive = false
    private var cancelRequested = false
    private var sessionId: String?
    private var nextRequestNumber = 1
    private var readTask: Task<Void, Never>?
    private var pendingResponses: [String: CheckedContinuation<PiWire.Response, Error>] = [:]
    private var promptContinuations: [CheckedContinuation<StopReason, Error>] = []
    private var settlementSequence = 0
    private var lastSettlementReason: StopReason = .endTurn
    private var pendingUIRequests: [String: UIRequestKind] = [:]
    private var effortOption: SessionConfigOption?

    private let eventContinuation: AsyncStream<ACPSessionEvent>.Continuation
    public nonisolated let events: AsyncStream<ACPSessionEvent>
    public nonisolated var supportsStructuredAnswers: Bool { true }

    public init(transport: any ACPTransport, model: String?, effort: String?,
                resumeSessionId: String?) {
        self.transport = transport
        requestedModel = model
        requestedEffort = effort
        requestedResumeSessionId = resumeSessionId
        (events, eventContinuation) = AsyncStream.makeStream(of: ACPSessionEvent.self)
    }

    public static func launchTransport(
        worktreePath: String, model: String?, resumeSessionId: String?,
        onStderrLine: (@Sendable (String) -> Void)? = nil
    ) -> ProcessTransport {
        var command = "exec pi --mode rpc --approve"
        if let model { command += " --model \(shellArgument(model))" }
        if let resumeSessionId { command += " --session \(shellArgument(resumeSessionId))" }
        return ProcessTransport(
            executable: "/bin/zsh", arguments: ["-lc", command], cwd: worktreePath,
            onStderrLine: onStderrLine)
    }

    private static func shellArgument(_ value: String) -> String {
        let safe = value.allSatisfy { $0.isLetter || $0.isNumber || "-._/".contains($0) }
        guard !safe else { return value }
        return "'" + value.replacingOccurrences(of: "'", with: "'\\''") + "'"
    }

    public func start() async throws {
        guard !started else { return }
        guard !finished, !stopping else { throw PiRPCDriverError.disconnected }
        try await transport.start()
        started = true
        let transport = self.transport
        let streamingState = self.streamingState
        readTask = Task { [weak self] in
            await self?.readLoop(transport: transport, streamingState: streamingState)
        }
    }

    public func stop() async {
        stopping = true
        readTask?.cancel()
        await transport.terminate()
        finish()
    }

    private func makeRequestId() -> String {
        defer { nextRequestNumber += 1 }
        return "req-\(nextRequestNumber)"
    }

    private func sendCommand(_ type: String,
                             fields: [String: JSONValue] = [:]) async throws -> PiWire.Response {
        guard started else { throw PiRPCDriverError.notStarted }
        guard !finished, !stopping else { throw PiRPCDriverError.disconnected }
        let id = makeRequestId()
        let line = try PiWire.command(id: id, type: type, fields: fields)
        return try await withCheckedThrowingContinuation { continuation in
            pendingResponses[id] = continuation
            let transport = self.transport
            let sendGate = self.sendGate
            Task { [weak self] in
                do { try await sendGate.send(line, via: transport) }
                catch { await self?.failResponse(id: id, error: error) }
            }
        }
    }

    private func requireSuccess(_ response: PiWire.Response) throws -> JSONValue? {
        guard response.success else {
            throw PiRPCDriverError.commandFailed(
                command: response.command, message: response.error ?? "Unknown Pi RPC error")
        }
        return response.data
    }

    private func failResponse(id: String, error: Error) {
        pendingResponses.removeValue(forKey: id)?.resume(throwing: error)
    }

    private nonisolated func readLoop(transport: any ACPTransport,
                                      streamingState: PiStreamingState) async {
        do {
            for try await line in transport.lines() {
                guard !Task.isCancelled else { break }
                do {
                    let message = try PiWire.decode(line)
                    // Update the state used for prompt classification in the
                    // reader before handing the message back to the driver actor.
                    if case .event(let type, _) = message {
                        switch type {
                        case "agent_start": streamingState.setStreaming(true)
                        case "agent_settled": streamingState.setStreaming(false)
                        default: break
                        }
                    }
                    await self.handle(message)
                } catch {
                    await self.reportDecodeError(error)
                }
            }
        } catch {
            // Transport failure and EOF share the same session-level finish path.
        }
        await self.finish()
    }

    private func reportDecodeError(_ error: Error) {
        eventContinuation.yield(.update(.agentThoughtChunk(
            .text("Pi RPC decode error: \(error.localizedDescription)"))))
    }

    private func handle(_ message: PiWire.Message) {
        switch message {
        case .response(let response):
            guard let id = response.id,
                  let continuation = pendingResponses.removeValue(forKey: id) else { return }
            continuation.resume(returning: response)
        case .event(let type, _):
            switch type {
            case "agent_start": streamingState.setStreaming(true)
            case "agent_end": break
            case "agent_settled": settleCurrentRun()
            default: break
            }
        }
    }

    private func finish() {
        guard !finished else { return }
        finished = true
        let responses = Array(pendingResponses.values)
        pendingResponses.removeAll()
        for continuation in responses {
            continuation.resume(throwing: PiRPCDriverError.disconnected)
        }
        let prompts = promptContinuations
        promptContinuations.removeAll()
        for continuation in prompts {
            continuation.resume(throwing: PiRPCDriverError.disconnected)
        }
        streamingState.setStreaming(false)
        runActive = false
        cancelRequested = false
        if !stopping { eventContinuation.yield(.disconnected) }
        eventContinuation.finish()
    }

    public func connect(cwd: String, resumeSessionId: String?,
                        mcpServers: [McpServerSpec]) async throws -> SessionHandle {
        _ = cwd
        _ = resumeSessionId
        _ = mcpServers
        let stateResponse = try await sendCommand("get_state")
        let state = try requireSuccess(stateResponse)
        let stateModel = state?["model"].flatMap(modelInfo)
        let stateThinking = state?["thinkingLevel"]?.stringValue

        if let requestedEffort {
            let response = try await sendCommand("set_thinking_level", fields: [
                "level": .string(requestedEffort)
            ])
            _ = try requireSuccess(response)
        }

        let modelsResponse = try await sendCommand("get_available_models")
        let modelsData = try requireSuccess(modelsResponse)
        let models = modelsData?["models"]?.arrayValue?.compactMap(modelInfo) ?? []

        let thinkingResponse = try await sendCommand("get_available_thinking_levels")
        let thinkingData = try requireSuccess(thinkingResponse)
        let levels = thinkingData?["levels"]?.arrayValue?.compactMap(\.stringValue) ?? []
        let thinkingValue = stateThinking ?? requestedEffort
        if thinkingValue != nil || !levels.isEmpty {
            effortOption = SessionConfigOption(
                id: "effort", name: "Thinking", currentValue: thinkingValue,
                options: levels.map { .init(value: $0, name: $0.capitalized) })
        }

        let commandsResponse = try await sendCommand("get_commands")
        let commandsData = try requireSuccess(commandsResponse)
        let commands = commandsData?["commands"]?.arrayValue?.compactMap { value -> AvailableCommand? in
            guard let name = value["name"]?.stringValue else { return nil }
            return AvailableCommand(name: name, description: value["description"]?.stringValue ?? "")
        } ?? []
        eventContinuation.yield(.update(.availableCommandsUpdate(commands)))

        guard let sessionRef = state?["sessionFile"]?.stringValue
                ?? state?["sessionId"]?.stringValue else {
            throw PiRPCDriverError.commandFailed(
                command: "get_state", message: "Missing session reference")
        }
        sessionId = sessionRef
        let currentModelId = stateModel?.modelId ?? requestedModel ?? models.first?.modelId
        return SessionHandle(
            sessionId: sessionRef,
            agentCapabilities: AgentCapabilities(loadSession: true),
            modes: nil,
            models: currentModelId.map {
                SessionModelState(currentModelId: $0, availableModels: models)
            },
            configOptions: effortOption.map { [$0] } ?? [],
            didResume: resumeSessionId != nil || requestedResumeSessionId != nil)
    }

    private func modelInfo(_ value: JSONValue) -> ModelInfo? {
        guard let provider = value["provider"]?.stringValue,
              let id = value["id"]?.stringValue else { return nil }
        return ModelInfo(modelId: "\(provider)/\(id)",
                         name: value["name"]?.stringValue ?? id)
    }

    private func promptFields(_ blocks: [ContentBlock]) -> [String: JSONValue] {
        var textParts: [String] = []
        var images: [JSONValue] = []
        for block in blocks {
            switch block {
            case .text(let text): textParts.append(text)
            case .image(let mimeType, let data):
                images.append(.object(["type": .string("image"),
                                       "data": .string(data),
                                       "mimeType": .string(mimeType)]))
            case .resourceLink(let uri, let name): textParts.append("\(name): \(uri)")
            case .resource(let uri, let text): textParts.append("\(uri)\n\(text)")
            case .unknown: break
            }
        }
        let message = textParts.joined(separator: "\n")
        var fields: [String: JSONValue] = ["message": .string(message)]
        if !images.isEmpty { fields["images"] = .array(images) }
        let slashCommand = message.trimmingCharacters(in: .whitespacesAndNewlines).hasPrefix("/")
        if streamingState.isStreaming && !slashCommand {
            fields["streamingBehavior"] = .string("steer")
        }
        return fields
    }

    public func prompt(_ blocks: [ContentBlock]) async throws -> StopReason {
        guard sessionId != nil else { throw PiRPCDriverError.notConnected }
        let observedSettlement = settlementSequence
        let wasIdle = !runActive
        runActive = true
        do {
            let response = try await sendCommand("prompt", fields: promptFields(blocks))
            _ = try requireSuccess(response)
            if settlementSequence != observedSettlement { return lastSettlementReason }
            return try await withCheckedThrowingContinuation { continuation in
                promptContinuations.append(continuation)
            }
        } catch {
            if wasIdle { runActive = false }
            throw error
        }
    }

    private func settleCurrentRun() {
        guard runActive || !promptContinuations.isEmpty else {
            cancelRequested = false
            return
        }
        streamingState.setStreaming(false)
        runActive = false
        settlementSequence += 1
        let reason: StopReason = cancelRequested ? .cancelled : .endTurn
        cancelRequested = false
        lastSettlementReason = reason
        eventContinuation.yield(.turnEnded(reason))
        let continuations = promptContinuations
        promptContinuations.removeAll()
        for continuation in continuations { continuation.resume(returning: reason) }
    }

    public func cancel() async {
        guard started, !finished, !stopping, runActive else { return }
        cancelRequested = true
        do {
            let line = try PiWire.command(id: makeRequestId(), type: "abort")
            // Await the serialized send so a following prompt cannot overtake it.
            try await sendGate.send(line, via: transport)
        } catch {
            // Cancellation remains represented by the next settled event even
            // when the transport rejects the best-effort abort write.
        }
    }

    public func setModel(_ modelId: String) async throws {
        guard sessionId != nil else { throw PiRPCDriverError.notConnected }
        let parts = modelId.split(separator: "/", maxSplits: 1,
                                  omittingEmptySubsequences: false)
        guard parts.count == 2, !parts[0].isEmpty, !parts[1].isEmpty else {
            throw PiRPCDriverError.invalidModelId(modelId)
        }
        let response = try await sendCommand("set_model", fields: [
            "provider": .string(String(parts[0])),
            "modelId": .string(String(parts[1]))
        ])
        _ = try requireSuccess(response)
    }

    public func setConfigOption(id: String, value: String) async throws
        -> [SessionConfigOption]? {
        guard sessionId != nil else { throw PiRPCDriverError.notConnected }
        guard id == "effort" else {
            throw PiRPCDriverError.commandFailed(command: id, message: "Unsupported config option")
        }
        let response = try await sendCommand("set_thinking_level", fields: [
            "level": .string(value)
        ])
        _ = try requireSuccess(response)
        effortOption?.currentValue = value
        return effortOption.map { [$0] }
    }

    public func setMode(_ modeId: String) async throws {
        _ = modeId
    }

}

extension PiRPCDriver {
    public func answerPermission(requestId: JSONRPCID,
                                 outcome: PermissionOutcome) async {
        guard case .string(let id) = requestId,
              let kind = pendingUIRequests.removeValue(forKey: id) else { return }
        do {
            let line: Data
            switch outcome {
            case .cancelled:
                line = try PiWire.extensionUICancel(id: id)
            case .selected(let optionId):
                if optionId == "__cancel__" {
                    line = try PiWire.extensionUICancel(id: id)
                } else if kind == .confirm {
                    line = try PiWire.extensionUIConfirmation(
                        id: id, confirmed: optionId == "true")
                } else {
                    line = try PiWire.extensionUIValue(id: id, value: optionId)
                }
            case .answered(let optionId, let updatedInput):
                if kind == .confirm {
                    line = try PiWire.extensionUIConfirmation(
                        id: id, confirmed: optionId == "true")
                } else if let text = updatedInput["choice"]?.stringValue {
                    line = try PiWire.extensionUIValue(id: id, value: text)
                } else {
                    line = try PiWire.extensionUICancel(id: id)
                }
            }
            try await transport.send(line: line)
        } catch {
            eventContinuation.yield(.update(.agentThoughtChunk(
                .text("Pi UI response failed: \(error.localizedDescription)"))))
        }
    }

    // Test seam: avoids four-command connect setup in event-only tests.
    func markConnectedForTesting(sessionId: String) {
        self.sessionId = sessionId
        effortOption = SessionConfigOption(
            id: "effort", name: "Thinking", currentValue: "medium",
            options: [.init(value: "off", name: "Off"),
                      .init(value: "medium", name: "Medium"),
                      .init(value: "high", name: "High")])
    }

    var hasPendingPromptForTesting: Bool { !promptContinuations.isEmpty }

    // Test seam: waits for the reader's confirmed event state, not scheduler turns.
    func waitForStreamingForTesting() async throws {
        for _ in 0..<100 {
            if streamingState.isStreaming { return }
            try await Task.sleep(for: .milliseconds(1))
        }
        throw CancellationError()
    }

    // Test seam: waits until EOF has completed the driver's lifecycle.
    func waitForFinishForTesting() async throws {
        for _ in 0..<100 {
            if finished { return }
            try await Task.sleep(for: .milliseconds(1))
        }
        throw CancellationError()
    }
}
