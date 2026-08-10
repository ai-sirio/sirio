import Foundation

/// Drives Claude Code's `-p --input-format stream-json --output-format
/// stream-json` protocol and translates it into Tiller's canonical session
/// events.
public struct ClaudeLaunch: Sendable {
    public let transport: ProcessTransport
    public let sessionId: String

    public init(transport: ProcessTransport, sessionId: String) {
        self.transport = transport
        self.sessionId = sessionId
    }
}

public actor ClaudeStreamJSONDriver: AgentDriver {
    private enum DriverError: Error, Sendable, CustomStringConvertible, LocalizedError {
        case notStarted
        case notConnected
        case transportClosed
        case requestFailed(String)
        case unsupported

        var description: String {
            switch self {
            case .notStarted: "Claude driver has not started"
            case .notConnected: "Claude driver is not connected"
            case .transportClosed: "Claude transport closed"
            case .requestFailed(let message): message
            case .unsupported: "Claude operation is unsupported"
            }
        }

        var errorDescription: String? { description }
    }

    private let transport: any ACPTransport
    private var permissionMode: PermissionMode
    private let requestedModel: String?
    private let requestedResumeSessionId: String?
    private let pinnedSessionId: String
    private let eventContinuation: AsyncStream<ACPSessionEvent>.Continuation
    public nonisolated let events: AsyncStream<ACPSessionEvent>

    private var readTask: Task<Void, Never>?
    private var started = false
    private var finished = false
    private var sessionId: String?
    private var promptContinuation: CheckedContinuation<StopReason, Error>?
    private var queuedResults: [ClaudeResult] = []
    private var pending: [String: CheckedContinuation<JSONValue?, Error>] = [:]
    private var cancelRequested = false
    private var effort: String?
    private var effortLevels: [String] = []
    public private(set) var diagnostics: [ClaudeDiagnostic] = []

    public nonisolated var supportsStructuredAnswers: Bool { true }

    public init(transport: any ACPTransport, permissionMode: PermissionMode,
                model: String?, resumeSessionId: String?, effort: String? = nil,
                pinnedSessionId: String? = nil) {
        self.transport = transport
        self.permissionMode = permissionMode
        requestedModel = model
        requestedResumeSessionId = resumeSessionId
        self.pinnedSessionId = pinnedSessionId ?? resumeSessionId ?? UUID().uuidString
        self.effort = effort
        (events, eventContinuation) = AsyncStream.makeStream(of: ACPSessionEvent.self)
    }

    /// Builds the Claude Code process command used by the app's worktree host.
    public static func launchTransport(worktreePath: String,
                                       permissionMode: PermissionMode,
                                       model: String?,
                                       resumeSessionId: String?,
                                       effort: String? = nil,
                                       onStderrLine: (@Sendable (String) -> Void)? = nil) -> ClaudeLaunch {
        var command = "exec claude -p --input-format stream-json --output-format stream-json --verbose"
        command += " --permission-mode \(shellArgument(permissionMode.claudeValue))"
        if let model {
            command += " --model \(shellArgument(model))"
        }
        if let effort {
            command += " --effort \(shellArgument(effort))"
        }
        let sessionId = resumeSessionId ?? UUID().uuidString
        if let resumeSessionId {
            command += " --resume \(shellArgument(resumeSessionId))"
        } else {
            command += " --session-id \(shellArgument(sessionId))"
        }
        return ClaudeLaunch(
            transport: ProcessTransport(executable: "/bin/zsh", arguments: ["-lc", command],
                                        cwd: worktreePath, onStderrLine: onStderrLine),
            sessionId: sessionId)
    }

    public func start() async throws {
        guard !started else { return }
        try await transport.start()
        started = true
        readTask = Task { [weak self] in
            await self?.readLoop()
        }
    }

    public func stop() async {
        readTask?.cancel()
        await transport.terminate()
        finish()
    }

    public func connect(cwd: String, resumeSessionId: String?,
                        mcpServers: [McpServerSpec]) async throws -> SessionHandle {
        _ = cwd
        _ = mcpServers
        guard started else { throw DriverError.notStarted }

        let response = try await sendControlRequest(.object([
            "subtype": .string("initialize")
        ]))
        guard isSuccessful(response) else { throw requestError(from: response) }
        let connectedSessionId = resumeSessionId ?? pinnedSessionId
        sessionId = connectedSessionId
        return makeHandle(from: response, sessionId: connectedSessionId,
                          didResume: (resumeSessionId ?? requestedResumeSessionId) != nil)
    }

    public func prompt(_ blocks: [ContentBlock]) async throws -> StopReason {
        guard started else { throw DriverError.notStarted }
        guard sessionId != nil else { throw DriverError.notConnected }

        cancelRequested = false
        let line = try makeUserPromptLine(blocks)
        try await transport.send(line: line)

        let reason: StopReason
        if !queuedResults.isEmpty {
            reason = stopReason(for: queuedResults.removeFirst())
        } else {
            reason = try await withCheckedThrowingContinuation {
                (continuation: CheckedContinuation<StopReason, Error>) in
                promptContinuation = continuation
            }
        }
        // Last event of the turn: everything the read loop yielded for it is
        // already in the stream ahead of this.
        eventContinuation.yield(.turnEnded(reason))
        return reason
    }

    public func cancel() async {
        cancelRequested = true
        guard started else { return }
        let requestId = makeRequestId()
        let request = JSONValue.object([
            "type": .string("control_request"),
            "request_id": .string(requestId),
            "request": .object(["subtype": .string("interrupt")])
        ])
        try? await transport.send(line: makeLine(request))
    }

    public func setMode(_ modeId: String) async throws {
        guard sessionId != nil else { throw DriverError.notConnected }
        let mode = PermissionMode(rawValue: modeId)?.claudeValue ?? modeId
        let response = try await sendControlRequest(.object([
            "subtype": .string("set_permission_mode"),
            "mode": .string(mode)
        ]))
        guard isSuccessful(response) else { throw requestError(from: response) }
        if let permissionMode = PermissionMode(rawValue: modeId) {
            self.permissionMode = permissionMode
        }
        eventContinuation.yield(.update(.currentModeUpdate(modeId)))
    }

    public func setModel(_ modelId: String) async throws {
        guard sessionId != nil else { throw DriverError.notConnected }
        let response = try await sendControlRequest(.object([
            "subtype": .string("set_model"),
            "model": .string(modelId)
        ]))
        guard isSuccessful(response) else { throw requestError(from: response) }
    }

    public func setEffort(_ effort: String?) async {
        guard let effort else {
            self.effort = nil
            return
        }
        guard effortLevels.contains(effort) else { return }
        self.effort = effort
        Task { [weak self] in
            _ = try? await self?.sendControlRequest(.object([
                "subtype": .string("apply_flag_settings"),
                "settings": .object(["effort": .string(effort)])
            ]))
        }
    }

    public func staticEffortOptions() async -> SessionConfigOption? {
        guard !effortLevels.isEmpty else { return nil }
        return SessionConfigOption(
            id: "effort", name: "Effort", currentValue: effort,
            options: effortLevels.map {
                SessionConfigOption.Choice(value: $0, name: $0.capitalized)
            })
    }

    public func setConfigOption(id: String, value: String) async throws
        -> [SessionConfigOption]? {
        _ = id
        _ = value
        throw DriverError.unsupported
    }

    private func sendControlResponse(id: String, payload: JSONValue) async {
        let response = JSONValue.object([
            "type": .string("control_response"),
            "response": payload
        ])
        try? await transport.send(line: makeLine(response))
    }

    /// Every control request must be answered, including ones this driver
    /// does not implement: the CLI correlates by `request_id` and blocks
    /// until it hears back. An explicit refusal is an answer; silence is not.
    private func declineControlRequest(id: String, reason: String) {
        Task { [weak self] in
            await self?.sendControlResponse(id: id, payload: .object([
                "request_id": .string(id),
                "subtype": .string("error"),
                "error": .string(reason)
            ]))
        }
    }

    public func answerPermission(requestId: JSONRPCID, outcome: PermissionOutcome) async {
        let id = requestIdString(requestId)
        var payload: [String: JSONValue]
        switch outcome {
        case .selected(let optionId):
            payload = ["behavior": .string(optionId.hasPrefix("allow") ? "allow" : "deny")]
        case .answered(_, let updatedInput):
            payload = ["behavior": .string("allow"), "updatedInput": updatedInput]
        case .cancelled:
            payload = ["behavior": .string("deny")]
        }
        let response = JSONValue.object([
            "type": .string("control_response"),
            "response": .object([
                "request_id": .string(id),
                "subtype": .string("success"),
                "response": .object(payload)
            ])
        ])
        try? await transport.send(line: makeLine(response))
    }

    private func readLoop() async {
        do {
            for try await line in transport.lines() {
                guard !Task.isCancelled else { break }
                guard let message = try? JSONDecoder().decode(ClaudeWireMessage.self,
                                                               from: line) else { continue }
                handle(message)
            }
        } catch {
            // EOF and transport errors have the same session-level meaning.
        }
        finish()
    }

    private func handle(_ message: ClaudeWireMessage) {
        switch message {
        case .systemInit(let initMessage):
            if !initMessage.sessionId.isEmpty {
                sessionId = initMessage.sessionId
            }
            // `system/init` is strictly worse than initialize on count and content,
            // so it has nothing to contribute. Refreshes come from `commands_changed`.

        case .assistant(let assistant):
            for block in assistant.message.content {
                handleAssistantBlock(block,
                                     parentToolUseId: assistant.parentToolUseId)
            }

        case .user(let user):
            for block in user.message.content {
                if case .toolResult(let result) = block {
                    let output = result.content.map(renderJSON) ?? ""
                    eventContinuation.yield(.update(.toolCallUpdate(ToolCallUpdate(
                        toolCallId: result.toolUseId,
                        status: result.isError ? .failed : .completed,
                        content: [.content(.text(output))],
                        rawOutput: user.toolUseResult))))
                }
            }

        case .result(let result):
            if let usage = contextUsage(from: result) {
                eventContinuation.yield(.update(.usageUpdate(usage)))
            } else {
                Task { [weak self] in await self?.probeContextUsage() }
            }
            if let denials = result.permissionDenials, !denials.isEmpty {
                emitNotice("\(denials.count) permission request(s) denied")
            }
            if let promptContinuation {
                self.promptContinuation = nil
                promptContinuation.resume(returning: stopReason(for: result))
            } else {
                queuedResults.append(result)
            }

        case .controlRequest(let id, let request):
            guard request.subtype == "can_use_tool" else {
                declineControlRequest(
                    id: id,
                    reason: "Tiller does not implement \(request.subtype)")
                return
            }
            if permissionMode == .fullAuto {
                Task { [weak self] in
                    await self?.answerPermission(
                        requestId: .string(id),
                        outcome: .selected(optionId: "allow_once"))
                }
                return
            }
            let name = request.toolName ?? "Tool"
            let toolCall = ToolCallUpdate(
                toolCallId: id,
                title: toolTitle(name: name, input: request.input),
                kind: toolKind(for: name),
                status: .pending,
                rawInput: request.input)
            var options = [
                PermissionOption(optionId: "allow_once", name: "Allow once",
                                 kind: .allowOnce),
                PermissionOption(optionId: "reject_once", name: "Reject",
                                 kind: .rejectOnce)
            ]
            if request.permissionSuggestions?.arrayValue?.isEmpty == false {
                options.append(PermissionOption(optionId: "allow_always",
                                                name: "Allow always",
                                                kind: .allowAlways))
            }
            eventContinuation.yield(.permissionRequested(
                requestId: .string(id), toolCall: toolCall, options: options))

        case .controlResponse(let id, let value):
            guard let continuation = pending.removeValue(forKey: id) else { return }
            continuation.resume(returning: value)

        case .commandsChanged(let values):
            eventContinuation.yield(.update(.availableCommandsUpdate(
                commands(from: values))))

        case .systemEvent(let event):
            switch event.subtype {
            case "compact_boundary":
                emitNotice("Context compacted")
            case "model_fallback", "model_refusal_fallback",
                 "model_consent_fallback", "model_refusal_no_fallback":
                let model = event.payload["model"]?.stringValue ?? "another model"
                emitNotice("Model fell back to \(model)")
            default:
                if let diagnostic = ClaudeDiagnostics.diagnostic(for: event) {
                    record(diagnostic)
                }
            }

        case .rateLimit(let info):
            guard info.status != "allowed" else { return }
            emitNotice("Rate limit \(info.status) (\(info.rateLimitType ?? "unknown"))")

        case .promptSuggestion, .streamEvent:
            break

        case .unknown:
            break
        }
    }

    /// `parentToolUseId` is non-nil for blocks produced *inside* a subagent.
    /// Its tool calls nest under the spawning Task; its prose is intermediate
    /// chatter and is dropped — the final report arrives as the tool result.
    private func handleAssistantBlock(_ block: ClaudeContentBlock,
                                      parentToolUseId: String?) {
        switch block {
        case .text(let text):
            guard parentToolUseId == nil else { return }
            eventContinuation.yield(.update(.agentMessageChunk(.text(text))))
        case .thinking(let thinking):
            guard parentToolUseId == nil else { return }
            eventContinuation.yield(.update(.agentThoughtChunk(.text(thinking))))
        case .toolUse(let toolUse):
            eventContinuation.yield(.update(.toolCall(ToolCall(
                toolCallId: toolUse.id,
                title: toolTitle(name: toolUse.name, input: toolUse.input),
                kind: toolKind(for: toolUse.name),
                status: .inProgress,
                rawInput: toolUse.input,
                parentToolCallId: parentToolUseId))))
        case .toolResult, .unknown:
            break
        }
    }

    private func sendControlRequest(_ request: JSONValue) async throws -> JSONValue? {
        guard started else { throw DriverError.notStarted }
        let id = makeRequestId()
        let value = JSONValue.object([
            "type": .string("control_request"),
            "request_id": .string(id),
            "request": request
        ])
        let line = makeLine(value)
        return try await withCheckedThrowingContinuation {
            (continuation: CheckedContinuation<JSONValue?, Error>) in
            pending[id] = continuation
            Task { [weak self] in
                do {
                    try await self?.transport.send(line: line)
                } catch {
                    await self?.failPending(id: id, error: error)
                }
            }
        }
    }

    private func failPending(id: String, error: Error) {
        guard let continuation = pending.removeValue(forKey: id) else { return }
        continuation.resume(throwing: error)
    }

    private func probeContextUsage() async {
        do {
            let response = try await sendControlRequest(.object([
                "subtype": .string("get_context_usage")
            ]))
            guard let usage = contextUsage(from: response) else { return }
            eventContinuation.yield(.update(.usageUpdate(usage)))
        } catch {
            // Older Claude Code versions may not support this control request.
        }
    }

    private func makeHandle(from response: JSONValue?, sessionId: String,
                            didResume: Bool) -> SessionHandle {
        let supported = PermissionMode.supported(byDriverFor: "claude")
        let modes = SessionModeState(
            currentModeId: permissionMode.rawValue,
            availableModes: supported.map(\.sessionMode))
        let payload = response?["response"] ?? response
        emitAvailableCommands(from: payload)
        let models = modelState(from: payload)
        let selected = requestedModel ?? models?.currentModelId
        effortLevels = models?.availableModels
            .first { $0.modelId == selected }?.supportedEffortLevels ?? []
        return SessionHandle(
            sessionId: sessionId,
            agentCapabilities: AgentCapabilities(),
            modes: modes,
            models: models,
            configOptions: [],
            didResume: didResume)
    }

    private func commands(from values: [JSONValue]) -> [AvailableCommand] {
        values.compactMap { value in
            guard let name = value["name"]?.stringValue else { return nil }
            return AvailableCommand(name: name,
                                    description: value["description"]?.stringValue ?? "")
        }
    }

    private func emitAvailableCommands(from payload: JSONValue?) {
        guard let values = payload?["commands"]?.arrayValue else { return }
        eventContinuation.yield(.update(.availableCommandsUpdate(commands(from: values))))
    }

    private func modelState(from payload: JSONValue?) -> SessionModelState? {
        guard let values = payload?["models"]?.arrayValue else { return nil }
        let models = values.compactMap { value -> ModelInfo? in
            guard let modelId = value["value"]?.stringValue
                ?? value["id"]?.stringValue else { return nil }
            let name = value["displayName"]?.stringValue
                ?? value["name"]?.stringValue
                ?? modelId
            let levels: [String]? = value["supportsEffort"]?.boolValue == true
                ? value["supportedEffortLevels"]?.arrayValue?.compactMap(\.stringValue)
                : nil
            return ModelInfo(modelId: modelId, name: name,
                             description: value["description"]?.stringValue,
                             supportedEffortLevels: levels)
        }
        guard !models.isEmpty else { return nil }
        let currentModelId = requestedModel ?? models[0].modelId
        return SessionModelState(currentModelId: currentModelId,
                                 availableModels: models)
    }

    private func contextUsage(from response: JSONValue?) -> ContextUsage? {
        guard isSuccessful(response) else { return nil }
        let payload = response?["response"] ?? response
        guard let used = payload?["totalTokens"]?.intValue,
              let size = payload?["maxTokens"]?.intValue ?? payload?["rawMaxTokens"]?.intValue,
              used >= 0, size > 0 else { return nil }
        return ContextUsage(used: used, size: size)
    }

    /// `modelUsage` is keyed by model name; any entry carries the same
    /// context window, so the first one with the field wins.
    private func contextUsage(from result: ClaudeResult) -> ContextUsage? {
        guard case .object(let byModel)? = result.modelUsage,
              let size = byModel.values.compactMap({ $0["contextWindow"]?.intValue }).first,
              size > 0, let usage = result.usage else { return nil }
        let used = (usage.inputTokens ?? 0)
            + (usage.outputTokens ?? 0)
            + (usage.cacheReadInputTokens ?? 0)
            + (usage.cacheCreationInputTokens ?? 0)
        guard used >= 0 else { return nil }
        return ContextUsage(used: used, size: size)
    }

    private func emitNotice(_ text: String) {
        eventContinuation.yield(.update(.notice(text)))
    }

    private func record(_ diagnostic: ClaudeDiagnostic) {
        diagnostics.append(diagnostic)
        if diagnostics.count > 500 {
            diagnostics.removeFirst(diagnostics.count - 500)
        }
    }

    private func stopReason(for result: ClaudeResult) -> StopReason {
        if cancelRequested { return .cancelled }
        return result.isError ? .refusal : .endTurn
    }

    private func isSuccessful(_ value: JSONValue?) -> Bool {
        value?["subtype"]?.stringValue == "success"
            || value?["response"]?["subtype"]?.stringValue == "success"
    }

    private func requestError(from response: JSONValue?) -> DriverError {
        .requestFailed(
            response?["error"]?.stringValue
                ?? response?["response"]?["error"]?.stringValue
                ?? "Claude request failed")
    }

    private func finish() {
        guard !finished else { return }
        finished = true
        let error = DriverError.transportClosed
        promptContinuation?.resume(throwing: error)
        promptContinuation = nil
        for continuation in pending.values { continuation.resume(throwing: error) }
        pending.removeAll()
        eventContinuation.yield(.disconnected)
        eventContinuation.finish()
    }

    private func makeUserPromptLine(_ blocks: [ContentBlock]) throws -> Data {
        let content = JSONValue.array(wireBlocks(blocks))
        return makeLine(.object([
            "type": .string("user"),
            "message": .object([
                "role": .string("user"),
                "content": content
            ])
        ]))
    }

    /// Claude Code forwards `message.content` to the Messages API unchanged,
    /// so only its content tags may go out: a `resource_link` comes back as a
    /// 400, and an image in ACP's shape (`mimeType`/`data`) kills the CLI
    /// before it ever sends the request. Links become `@path` mentions, the
    /// shape the CLI's own input uses.
    private func wireBlocks(_ blocks: [ContentBlock]) -> [JSONValue] {
        blocks.compactMap { block in
            switch block {
            case .text(let text):
                .object(["type": .string("text"), "text": .string(text)])
            case .resourceLink(let uri, let name):
                .object(["type": .string("text"),
                         "text": .string("@" + (URL(string: uri)?.path ?? name))])
            case .resource(_, let text):
                .object(["type": .string("text"), "text": .string(text)])
            case .image(let mimeType, let data):
                .object(["type": .string("image"),
                         "source": .object(["type": .string("base64"),
                                            "media_type": .string(mimeType),
                                            "data": .string(data)])])
            case .unknown:
                nil
            }
        }
    }

    private func makeLine(_ value: JSONValue) -> Data {
        var data = (try? JSONEncoder().encode(value)) ?? Data("{}".utf8)
        data.append(UInt8(ascii: "\n"))
        return data
    }

    private func makeRequestId() -> String {
        "tiller-\(UUID().uuidString)"
    }

    private func requestIdString(_ id: JSONRPCID) -> String {
        switch id {
        case .number(let value): String(value)
        case .string(let value): value
        }
    }

    private func toolKind(for name: String) -> ToolKind {
        switch name.lowercased() {
        case "bash": .execute
        case "edit", "write": .edit
        case "read": .read
        default: .other
        }
    }

    private func toolTitle(name: String, input: JSONValue?) -> String {
        guard let primary = primaryInput(from: input), !primary.isEmpty else { return name }
        return "\(name): \(primary)"
    }

    private func primaryInput(from input: JSONValue?) -> String? {
        guard let input else { return nil }
        if let string = input.stringValue { return string }
        guard case .object(let values) = input else { return renderJSON(input) }
        for key in ["command", "file_path", "path", "pattern", "query", "description"] {
            if let value = values[key], let string = value.stringValue { return string }
        }
        for value in values.values where value.stringValue != nil {
            return value.stringValue
        }
        return nil
    }

    private func renderJSON(_ value: JSONValue) -> String {
        if let string = value.stringValue { return string }
        guard let data = try? JSONEncoder().encode(value) else { return "" }
        return String(decoding: data, as: UTF8.self)
    }

    private static func shellArgument(_ value: String) -> String {
        let safe = value.allSatisfy { $0.isLetter || $0.isNumber || "-._/".contains($0) }
        guard !safe else { return value }
        return "'" + value.replacingOccurrences(of: "'", with: "'\\''") + "'"
    }
}
