import Foundation

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
    private var started = false
    private var finished = false
    private var stopping = false
    private var isStreaming = false
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
        try await transport.start()
        started = true
        readTask = Task { [weak self] in await self?.readLoop() }
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
        let id = makeRequestId()
        let line = try PiWire.command(id: id, type: type, fields: fields)
        return try await withCheckedThrowingContinuation { continuation in
            pendingResponses[id] = continuation
            let transport = self.transport
            Task { [weak self] in
                do { try await transport.send(line: line) }
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

    private func readLoop() async {
        do {
            for try await line in transport.lines() {
                guard !Task.isCancelled else { break }
                do { handle(try PiWire.decode(line)) }
                catch {
                    eventContinuation.yield(.update(.agentThoughtChunk(
                        .text("Pi RPC decode error: \(error.localizedDescription)"))))
                }
            }
        } catch {
            // Transport failure and EOF share the same session-level finish path.
        }
        finish()
    }

    private func handle(_ message: PiWire.Message) {
        switch message {
        case .response(let response):
            guard let id = response.id,
                  let continuation = pendingResponses.removeValue(forKey: id) else { return }
            continuation.resume(returning: response)
        case .event(let type, _):
            switch type {
            case "agent_start": isStreaming = true
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
        if isStreaming && !slashCommand { fields["streamingBehavior"] = .string("steer") }
        return fields
    }

    public func prompt(_ blocks: [ContentBlock]) async throws -> StopReason {
        // Give the single reader a chance to apply an event emitted immediately
        // before this prompt (notably agent_start for a steering prompt).
        for _ in 0..<3 { await Task.yield() }
        guard sessionId != nil else { throw PiRPCDriverError.notConnected }
        let observedSettlement = settlementSequence
        let response = try await sendCommand("prompt", fields: promptFields(blocks))
        _ = try requireSuccess(response)
        if settlementSequence != observedSettlement { return lastSettlementReason }
        return try await withCheckedThrowingContinuation { continuation in
            promptContinuations.append(continuation)
        }
    }

    private func settleCurrentRun() {
        isStreaming = false
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
        cancelRequested = true
        guard started else { return }
        Task { [weak self] in
            do { _ = try await self?.sendCommand("abort") } catch { }
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
}
