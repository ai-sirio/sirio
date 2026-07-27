import Foundation

/// Drives Codex's app-server JSON-RPC protocol over stdio and translates its
/// item/thread/turn notifications into Tiller's canonical session events.
public actor CodexAppServerDriver: AgentDriver {
    private enum DriverError: Error, Sendable {
        case notStarted
        case notConnected
        case unsupported
        case transportClosed
    }

    private enum ApprovalResponseStyle: Sendable, Equatable {
        case legacy
        case appServer
    }

    private let client: ACPClient
    private var mode: PermissionMode
    private var model: String?
    private var effort: String?
    private let requestedResumeConversationId: String?
    private let eventContinuation: AsyncStream<ACPSessionEvent>.Continuation
    public nonisolated let events: AsyncStream<ACPSessionEvent>

    private var readTask: Task<Void, Never>?
    private var started = false
    private var finished = false
    private var threadId: String?
    private var turnId: String?
    private var cancelRequested = false
    private var promptContinuation: CheckedContinuation<StopReason, Error>?
    private var queuedStopReasons: [StopReason] = []
    private var approvalResponseStyles: [JSONRPCID: ApprovalResponseStyle] = [:]

    public init(client: ACPClient, permissionMode: PermissionMode, model: String?,
                effort: String?, resumeConversationId: String?) {
        self.client = client
        self.mode = permissionMode
        self.model = model
        self.effort = effort
        requestedResumeConversationId = resumeConversationId
        (events, eventContinuation) = AsyncStream.makeStream(of: ACPSessionEvent.self)
    }

    /// Builds the Codex app-server process command used by the worktree host.
    public static func launchTransport(
        worktreePath: String,
        onStderrLine: (@Sendable (String) -> Void)? = nil
    ) -> ProcessTransport {
        ProcessTransport(executable: "/bin/zsh", arguments: ["-lc", "exec codex app-server"],
                          cwd: worktreePath, onStderrLine: onStderrLine)
    }

    public func start() async throws {
        guard !started else { return }
        try await client.start()
        started = true
        let client = self.client
        readTask = Task { [weak self] in
            for await incoming in client.incoming {
                guard !Task.isCancelled else { break }
                await self?.handle(incoming)
            }
            await self?.finish()
        }
    }

    public func stop() async {
        readTask?.cancel()
        await client.stop()
        finish()
    }

    public func connect(cwd: String, resumeSessionId: String?,
                        mcpServers: [McpServerSpec]) async throws -> SessionHandle {
        _ = mcpServers // Codex app-server discovers MCP configuration itself.
        guard started else { throw DriverError.notStarted }

        _ = try await client.request(
            "initialize",
            params: JSONValue.object([
                "clientInfo": .object([
                    "name": .string("tiller"),
                    "title": .string("Tiller"),
                    "version": .string("0.1.0")
                ]),
                "capabilities": .object(["experimentalApi": .bool(true)])
            ]),
            as: JSONValue.self)
        try await client.notify("initialized", params: JSONValue.object([:]))

        let requestedId = resumeSessionId ?? requestedResumeConversationId
        if let requestedId {
            do {
                let resumed = try await client.request(
                    "thread/resume",
                    params: threadParams(cwd: cwd, threadId: requestedId),
                    as: JSONValue.self)
                if let liveId = threadId(from: resumed) {
                    threadId = liveId
                    return makeHandle(from: resumed, threadId: liveId, didResume: true)
                }
            } catch {
                // A stale conversation id falls back to a new Codex thread.
            }
        }

        let created = try await client.request(
            "thread/start", params: threadParams(cwd: cwd), as: JSONValue.self)
        guard let liveId = threadId(from: created) else {
            throw DriverError.transportClosed
        }
        threadId = liveId
        return makeHandle(from: created, threadId: liveId, didResume: false)
    }

    public func prompt(_ blocks: [ContentBlock]) async throws -> StopReason {
        guard started else { throw DriverError.notStarted }
        guard let threadId else { throw DriverError.notConnected }

        cancelRequested = false
        let result = try await client.request(
            "turn/start", params: turnParams(threadId: threadId, blocks: blocks),
            as: JSONValue.self)
        self.turnId = result["turn"]?["id"]?.stringValue ?? result["turnId"]?.stringValue
        let reason: StopReason
        if !queuedStopReasons.isEmpty {
            reason = queuedStopReasons.removeFirst()
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
        guard let threadId, let turnId else { return }
        try? await client.notify("turn/interrupt", params: JSONValue.object([
            "threadId": .string(threadId), "turnId": .string(turnId)
        ]))
    }

    /// Codex's approval policy is per turn, so changing mode is local state.
    public func setMode(_ modeId: String) async throws {
        guard threadId != nil else { throw DriverError.notConnected }
        if let mode = PermissionMode(rawValue: modeId) { self.mode = mode }
        eventContinuation.yield(.update(.currentModeUpdate(modeId)))
    }

    /// Model selection is also sent as a per-turn override by Codex.
    public func setModel(_ modelId: String) async throws {
        guard threadId != nil else { throw DriverError.notConnected }
        model = modelId
    }

    /// Not part of AgentDriver yet, but used by the chat controller's Codex
    /// effort setting and applied to the next turn/start request.
    public func setEffort(_ effort: String?) async {
        self.effort = effort
    }

    /// Codex sends this straight through as the native `effort` turn param;
    /// values match OpenAI's reasoning_effort enum.
    private static let effortLevels: [SessionConfigOption.Choice] = [
        .init(value: "minimal", name: "Minimal"),
        .init(value: "low", name: "Low"),
        .init(value: "medium", name: "Medium"),
        .init(value: "high", name: "High")
    ]

    public func staticEffortOptions() async -> SessionConfigOption? {
        SessionConfigOption(id: "effort", name: "Effort",
                             currentValue: effort, options: Self.effortLevels)
    }

    public func setConfigOption(id: String, value: String) async throws
        -> [SessionConfigOption]? {
        _ = id
        _ = value
        throw DriverError.unsupported
    }

    public func answerPermission(requestId: JSONRPCID, outcome: PermissionOutcome) async {
        let style = approvalResponseStyles.removeValue(forKey: requestId) ?? .legacy
        let decision: String
        switch outcome {
        case .selected(let optionId):
            if optionId.hasPrefix("allow") {
                decision = style == .appServer ? "accept" : "approved"
            } else {
                decision = style == .appServer ? "decline" : "denied"
            }
        case .answered(let optionId, _):
            if optionId.hasPrefix("allow") {
                decision = style == .appServer ? "accept" : "approved"
            } else {
                decision = style == .appServer ? "decline" : "denied"
            }
        case .cancelled:
            decision = style == .appServer ? "cancel" : "denied"
        }
        try? await client.respond(to: requestId,
                                  result: JSONValue.object(["decision": .string(decision)]))
    }

    private func handle(_ incoming: ACPIncoming) async {
        switch incoming {
        case .notification(let method, let params):
            handleNotification(method: method, params: params)
        case .request(let id, let method, let params):
            switch method {
            case "execCommandApproval":
                emitPermission(requestId: id, params: params, kind: .execute, style: .legacy)
            case "applyPatchApproval":
                emitPermission(requestId: id, params: params, kind: .edit, style: .legacy)
            case "item/commandExecution/requestApproval":
                emitPermission(requestId: id, params: params, kind: .execute,
                               style: .appServer)
            case "item/fileChange/requestApproval":
                emitPermission(requestId: id, params: params, kind: .edit, style: .appServer)
            default:
                try? await client.respondError(to: id, code: -32601,
                                               message: "method not supported: \(method)")
            }
        }
    }

    private func handleNotification(method: String, params: JSONValue?) {
        switch method {
        case "item/agentMessage/delta", "agent_message_delta":
            emitTextUpdate({ .agentMessageChunk($0) }, params: params)
        case "item/reasoning/summaryTextDelta", "item/reasoning/delta",
             "agent_reasoning_delta", "agent_reasoning":
            emitTextUpdate({ .agentThoughtChunk($0) }, params: params)
        case "item/started":
            handleItemStarted(params)
        case "item/completed":
            handleItemCompleted(params)
        case "item/commandExecution/outputDelta", "item/commandExecution/output_delta",
             "exec_command_output_delta":
            handleCommandOutput(params)
        case "exec_command_begin":
            emitCommandStarted(params)
        case "exec_command_end":
            emitCommandCompleted(params)
        case "patch_apply_begin":
            emitPatchStarted(params)
        case "patch_apply_end":
            emitPatchCompleted(params)
        case "thread/tokenUsage/updated", "token_count":
            emitUsage(params)
        case "turn/completed", "task_complete":
            resolvePrompt(.endTurn)
        case "turn/aborted", "turn_aborted":
            resolvePrompt(.cancelled)
        default:
            break
        }
    }

    private func handleItemStarted(_ params: JSONValue?) {
        guard let item = params?["item"] else { return }
        switch item["type"]?.stringValue {
        case "commandExecution": emitCommandStarted(item)
        case "fileChange": emitPatchStarted(item)
        default: break
        }
    }

    private func handleItemCompleted(_ params: JSONValue?) {
        guard let item = params?["item"] else { return }
        switch item["type"]?.stringValue {
        case "commandExecution": emitCommandCompleted(item)
        case "fileChange": emitPatchCompleted(item)
        default: break
        }
    }

    private func emitTextUpdate(_ kind: (ContentBlock) -> SessionUpdate,
                                params: JSONValue?) {
        guard let text = params?["delta"]?.stringValue ?? params?["text"]?.stringValue else {
            return
        }
        eventContinuation.yield(.update(kind(.text(text))))
    }

    private func emitCommandStarted(_ value: JSONValue?) {
        guard let value, let id = itemId(from: value) else { return }
        let command = commandTitle(from: value)
        eventContinuation.yield(.update(.toolCall(ToolCall(
            toolCallId: id, title: command, kind: .execute, status: .inProgress,
            rawInput: value))))
    }

    private func emitCommandCompleted(_ value: JSONValue?) {
        guard let value, let id = itemId(from: value) else { return }
        var status: ToolCallStatus = .completed
        if let exitCode = value["exitCode"]?.intValue ?? value["exit_code"]?.intValue,
           exitCode != 0 { status = .failed }
        if ["failed", "incomplete", "interrupted"].contains(value["status"]?.stringValue) {
            status = .failed
        }
        let output = value["aggregatedOutput"]?.stringValue
            ?? value["aggregated_output"]?.stringValue
            ?? value["output"]?.stringValue
        let content: [ToolCallContent]? = output.map {
            [.content(.text($0))]
        }
        eventContinuation.yield(.update(.toolCallUpdate(ToolCallUpdate(
            toolCallId: id, status: status, content: content))))
    }

    private func handleCommandOutput(_ params: JSONValue?) {
        guard let params,
              let id = itemId(from: params),
              let delta = params["delta"]?.stringValue ?? params["outputDelta"]?.stringValue else {
            return
        }
        eventContinuation.yield(.update(.toolCallUpdate(ToolCallUpdate(
            toolCallId: id, content: [.content(.text(delta))]))))
    }

    private func emitPatchStarted(_ value: JSONValue?) {
        guard let value, let id = itemId(from: value) else { return }
        eventContinuation.yield(.update(.toolCall(ToolCall(
            toolCallId: id, title: patchTitle(from: value), kind: .edit,
            status: .inProgress, rawInput: value))))
    }

    private func emitPatchCompleted(_ value: JSONValue?) {
        guard let value, let id = itemId(from: value) else { return }
        let failed = value["status"]?.stringValue == "failed"
        eventContinuation.yield(.update(.toolCallUpdate(ToolCallUpdate(
            toolCallId: id, status: failed ? .failed : .completed))))
    }

    private func emitPermission(requestId: JSONRPCID, params: JSONValue?, kind: ToolKind,
                                style: ApprovalResponseStyle) {
        approvalResponseStyles[requestId] = style
        let id = params?["itemId"]?.stringValue ?? requestId.stringValue
        let title: String
        switch kind {
        case .execute:
            let command = commandTitle(from: params ?? .object([:]))
            title = command.isEmpty ? "Execute command" : command
        case .edit: title = patchTitle(from: params)
        default: title = "Tool approval"
        }
        let toolCall = ToolCallUpdate(toolCallId: id, title: title, kind: kind,
                                      status: .pending, rawInput: params)
        let options = [
            PermissionOption(optionId: "allow_once", name: "Allow once", kind: .allowOnce),
            PermissionOption(optionId: "reject_once", name: "Reject", kind: .rejectOnce)
        ]
        eventContinuation.yield(.permissionRequested(requestId: requestId,
                                                      toolCall: toolCall, options: options))
    }

    private func emitUsage(_ params: JSONValue?) {
        let usage = params?["tokenUsage"] ?? params
        let total = usage?["total"] ?? usage
        let used = firstInt([
            total?["totalTokens"], total?["total_tokens"],
            usage?["totalTokens"], usage?["total_tokens"],
            params?["info"]?["total_token_usage"]?["total_tokens"]
        ])
        let size = firstInt([
            usage?["modelContextWindow"], usage?["model_context_window"],
            params?["modelContextWindow"], params?["model_context_window"],
            params?["info"]?["model_context_window"]
        ])
        guard let used, let size,
              used >= 0, size > 0 else { return }
        eventContinuation.yield(.update(.usageUpdate(ContextUsage(used: used, size: size))))
    }

    private func firstInt(_ values: [JSONValue?]) -> Int? {
        values.compactMap { $0?.intValue }.first
    }

    private func resolvePrompt(_ reason: StopReason) {
        let reason = cancelRequested && reason == .endTurn ? .cancelled : reason
        if let continuation = promptContinuation {
            promptContinuation = nil
            continuation.resume(returning: reason)
        } else {
            queuedStopReasons.append(reason)
        }
    }

    private func threadParams(cwd: String, threadId: String? = nil) -> JSONValue {
        var params: [String: JSONValue] = [
            "cwd": .string(cwd),
            "approvalPolicy": .string(mode.codexApprovalPolicy),
            "sandbox": .string(mode.codexSandbox)
        ]
        if let threadId { params["threadId"] = .string(threadId) }
        if let model { params["model"] = .string(model) }
        return .object(params)
    }

    private func turnParams(threadId: String, blocks: [ContentBlock]) throws -> JSONValue {
        var params: [String: JSONValue] = [
            "threadId": .string(threadId),
            "input": .array(try blocks.map { try JSONValue.encoding($0) }),
            "approvalPolicy": .string(mode.codexApprovalPolicy),
            "sandboxPolicy": sandboxPolicy(for: mode.codexSandbox)
        ]
        if let model { params["model"] = .string(model) }
        if let effort { params["effort"] = .string(effort) }
        return .object(params)
    }

    private func threadId(from value: JSONValue) -> String? {
        value["thread"]?["id"]?.stringValue ?? value["threadId"]?.stringValue
    }

    private func sandboxPolicy(for value: String) -> JSONValue {
        switch value {
        case "workspace-write": return .object(["type": .string("workspaceWrite")])
        case "read-only": return .object(["type": .string("readOnly")])
        case "danger-full-access": return .object(["type": .string("dangerFullAccess")])
        default: return .object(["type": .string("workspaceWrite")])
        }
    }

    private func makeHandle(from value: JSONValue, threadId: String,
                            didResume: Bool) -> SessionHandle {
        let modelId = value["model"]?.stringValue ?? model
        let models = modelId.map {
            SessionModelState(currentModelId: $0,
                              availableModels: [ModelInfo(modelId: $0, name: $0)])
        }
        let modes = SessionModeState(
            currentModeId: mode.rawValue,
            availableModes: PermissionMode.supported(byDriverFor: "codex-acp").map(\.sessionMode))
        return SessionHandle(sessionId: threadId, agentCapabilities: AgentCapabilities(),
                             modes: modes, models: models, configOptions: [], didResume: didResume)
    }

    private func itemId(from value: JSONValue) -> String? {
        value["item"]?["id"]?.stringValue
            ?? value["itemId"]?.stringValue
            ?? value["toolCallId"]?.stringValue
            ?? value["id"]?.stringValue
    }

    private func commandTitle(from value: JSONValue) -> String {
        if let command = value["command"]?.stringValue { return command }
        if let argv = value["command"]?.arrayValue {
            return argv.compactMap(\.stringValue).joined(separator: " ")
        }
        let commands = value["commandActions"]?.arrayValue?.compactMap {
            $0["command"]?.stringValue
        } ?? []
        return commands.joined(separator: " && ")
    }

    private func patchTitle(from value: JSONValue?) -> String {
        guard let value else { return "Apply patch" }
        if let title = value["title"]?.stringValue { return title }
        if let patch = value["patch"]?.stringValue, !patch.isEmpty { return patch }
        return "Apply patch"
    }

    private func finish() {
        guard !finished else { return }
        finished = true
        promptContinuation?.resume(throwing: DriverError.transportClosed)
        promptContinuation = nil
        eventContinuation.yield(.disconnected)
        eventContinuation.finish()
    }
}

private extension JSONRPCID {
    var stringValue: String {
        switch self {
        case .number(let value): String(value)
        case .string(let value): value
        }
    }
}
