import Foundation

/// Drives one `opencode serve` process and translates its HTTP/SSE events into
/// Tiller's canonical ACP session event stream.
public actor OpenCodeHTTPDriver: AgentDriver {
    private enum DriverError: Error, Sendable {
        case notStarted
        case notConnected
        case invalidResponse
        case transportClosed
        case unsupported
    }

    private struct ToolPartSnapshot: Equatable {
        let status: ToolCallStatus
        let input: JSONValue?
        let output: String?
        let error: String?
    }

    private struct ModelCatalog: Sendable {
        var models: SessionModelState?
        var configOptions: [SessionConfigOption]
        var contextSizes: [String: Int]
    }

    private let connection: any OpenCodeConnection
    private var mode: PermissionMode
    private let requestedResumeSessionId: String?
    private let eventContinuation: AsyncStream<ACPSessionEvent>.Continuation
    public nonisolated let events: AsyncStream<ACPSessionEvent>

    private var readTask: Task<Void, Never>?
    private var promptContinuation: CheckedContinuation<StopReason, Error>?
    private var queuedStopReasons: [StopReason] = []
    private var permissionSessions: [String: String] = [:]
    private var permissionOptions: [String: [PermissionOption]] = [:]
    private var messageRoles: [String: String] = [:]
    private var partText: [String: String] = [:]
    /// Last emitted (status, rawInput, output/error) per tool callID. OpenCode
    /// resends `message.part.updated` for a tool part on every SSE tick even
    /// when nothing changed; without this the chat controller re-applies the
    /// same tool-call event repeatedly, amplifying its per-flush render cost.
    private var lastEmittedToolState: [String: ToolPartSnapshot] = [:]
    private var contextSizes: [String: Int] = [:]
    private var configOptions: [SessionConfigOption] = []
    private var currentContextSize: Int?
    private var currentModel: String?
    private var effort: String?
    private var permissionRules: [String: String] = [:]
    private var started = false
    private var finished = false
    private var cancelRequested = false
    private var sessionId: String?

    public init(connection: any OpenCodeConnection, permissionMode: PermissionMode,
                resumeSessionId: String?) {
        self.connection = connection
        self.mode = permissionMode
        requestedResumeSessionId = resumeSessionId
        permissionRules = Self.rules(for: permissionMode)
        (events, eventContinuation) = AsyncStream.makeStream(of: ACPSessionEvent.self)
    }

    public func start() async throws {
        guard !started else { return }
        started = true
        let connection = self.connection
        readTask = Task { [weak self] in
            do {
                for try await data in connection.events() {
                    guard !Task.isCancelled else { break }
                    await self?.handle(data)
                }
            } catch {
                // A transport error has the same lifecycle meaning as an
                // orderly SSE close to the chat controller.
            }
            await self?.finish()
        }
    }

    public func stop() async {
        readTask?.cancel()
        await connection.close()
        finish()
    }

    public func connect(cwd: String, resumeSessionId: String?,
                        mcpServers: [McpServerSpec]) async throws -> SessionHandle {
        _ = cwd
        _ = mcpServers
        guard started else { throw DriverError.notStarted }

        let requestedId = resumeSessionId ?? requestedResumeSessionId
        let liveId: String
        let didResume: Bool
        if let requestedId {
            liveId = requestedId
            didResume = true
        } else {
            let body = try encodedJSON(.object(["title": .string("Tiller")]))
            let response = try await connection.request(method: "POST", path: "/session",
                                                         body: body)
            let value = try decodeJSON(response)
            guard let createdId = value["id"]?.stringValue
                    ?? value["sessionID"]?.stringValue else {
                throw DriverError.invalidResponse
            }
            liveId = createdId
            didResume = false
        }

        sessionId = liveId
        let providerResponse = try await connection.request(
            method: "GET", path: "/config/providers", body: nil)
        let catalog = makeCatalog(from: try decodeJSON(providerResponse))
        contextSizes = catalog.contextSizes
        configOptions = catalog.configOptions
        if let currentModel {
            currentContextSize = contextSizes[currentModel]
        }

        let modes = SessionModeState(
            currentModeId: mode.rawValue,
            availableModes: PermissionMode.supported(byDriverFor: "opencode")
                .map(\.sessionMode))
        return SessionHandle(sessionId: liveId, agentCapabilities: AgentCapabilities(),
                             modes: modes, models: catalog.models,
                             configOptions: catalog.configOptions, didResume: didResume)
    }

    public func prompt(_ blocks: [ContentBlock]) async throws -> StopReason {
        guard started else { throw DriverError.notStarted }
        guard let sessionId else { throw DriverError.notConnected }

        cancelRequested = false
        let body = try promptBody(blocks)
        _ = try await connection.request(method: "POST",
                                         path: "/session/\(sessionId)/message",
                                         body: body)
        let reason: StopReason
        if !queuedStopReasons.isEmpty {
            reason = queuedStopReasons.removeFirst()
        } else {
            reason = try await withCheckedThrowingContinuation {
                (continuation: CheckedContinuation<StopReason, Error>) in
                promptContinuation = continuation
            }
        }
        // Last event of the turn: everything the event loop yielded for it is
        // already in the stream ahead of this.
        eventContinuation.yield(.turnEnded(reason))
        return reason
    }

    public func cancel() async {
        cancelRequested = true
        guard let sessionId else { return }
        _ = try? await connection.request(method: "POST",
                                           path: "/session/\(sessionId)/abort", body: nil)
    }

    public func setMode(_ modeId: String) async throws {
        guard sessionId != nil else { throw DriverError.notConnected }
        if let mode = PermissionMode(rawValue: modeId) {
            self.mode = mode
            permissionRules = Self.rules(for: mode)
        }
        eventContinuation.yield(.update(.currentModeUpdate(modeId)))
    }

    public func setModel(_ modelId: String) async throws {
        guard sessionId != nil else { throw DriverError.notConnected }
        currentModel = modelId
        currentContextSize = contextSizes[modelId]
        updateConfigOption(id: "model", value: modelId)
    }

    public func setConfigOption(id: String, value: String) async throws
        -> [SessionConfigOption]? {
        guard sessionId != nil else { throw DriverError.notConnected }
        switch id {
        case "model":
            currentModel = value
            currentContextSize = contextSizes[value]
            updateConfigOption(id: id, value: value)
        case "effort", "variant":
            effort = value
            updateConfigOption(id: "effort", value: value)
        default:
            throw DriverError.unsupported
        }
        return currentConfigOptions()
    }

    public func answerPermission(requestId: JSONRPCID, outcome: PermissionOutcome) async {
        let permissionId = requestId.stringValue
        guard let sessionId = permissionSessions.removeValue(forKey: permissionId) else {
            return
        }
        let options = permissionOptions.removeValue(forKey: permissionId) ?? []
        let response: String
        switch outcome {
        case .cancelled:
            response = "reject"
        case .selected(let optionId):
            let kind = options.first(where: { $0.optionId == optionId })?.kind
                ?? PermissionOptionKind(rawValue: optionId)
                ?? .rejectOnce
            switch kind {
            case .allowOnce: response = "once"
            case .allowAlways: response = "always"
            case .rejectOnce, .rejectAlways: response = "reject"
            }
        case .answered(let optionId, _):
            let kind = options.first(where: { $0.optionId == optionId })?.kind
                ?? PermissionOptionKind(rawValue: optionId)
                ?? .rejectOnce
            switch kind {
            case .allowOnce: response = "once"
            case .allowAlways: response = "always"
            case .rejectOnce, .rejectAlways: response = "reject"
            }
        }
        let body = try? encodedJSON(.object(["response": .string(response)]))
        _ = try? await connection.request(
            method: "POST",
            path: "/session/\(sessionId)/permissions/\(permissionId)",
            body: body)
    }

    private func handle(_ data: Data) {
        guard let value = try? decodeJSON(data),
              let type = value["type"]?.stringValue else { return }
        let properties = value["properties"] ?? .object([:])
        guard eventBelongsToCurrentSession(properties) else { return }

        switch type {
        case "session.updated": handleSessionUpdated(properties)
        case "message.updated": handleMessageUpdated(properties)
        case "message.part.updated": handlePartUpdated(properties)
        case "permission.asked": handlePermission(properties)
        case "session.status":
            if properties["status"]?["type"]?.stringValue == "idle" {
                resolvePrompt(cancelRequested ? .cancelled : .endTurn)
            }
        case "session.idle", "message.completed":
            resolvePrompt(cancelRequested ? .cancelled : .endTurn)
        default: break
        }
    }

    private func eventBelongsToCurrentSession(_ properties: JSONValue) -> Bool {
        guard let liveSessionId = sessionId,
              let eventSessionId = properties["sessionID"]?.stringValue else {
            return true
        }
        return liveSessionId == eventSessionId
    }

    private func handleSessionUpdated(_ properties: JSONValue) {
        guard let info = properties["info"] else { return }
        if let providerId = info["model"]?["providerID"]?.stringValue,
           let modelId = info["model"]?["id"]?.stringValue {
            updateCurrentModel(providerId: providerId, modelId: modelId)
        }
        if let providerId = info["providerID"]?.stringValue,
           let modelId = info["modelID"]?.stringValue {
            updateCurrentModel(providerId: providerId, modelId: modelId)
        }
    }

    private func handleMessageUpdated(_ properties: JSONValue) {
        guard let info = properties["info"],
              let messageId = info["id"]?.stringValue else { return }
        if let role = info["role"]?.stringValue { messageRoles[messageId] = role }
        if let providerId = info["providerID"]?.stringValue,
           let modelId = info["modelID"]?.stringValue {
            updateCurrentModel(providerId: providerId, modelId: modelId)
        }
    }

    private func handlePartUpdated(_ properties: JSONValue) {
        guard let part = properties["part"],
              let partType = part["type"]?.stringValue else { return }
        let messageId = part["messageID"]?.stringValue
        if let messageId, messageRoles[messageId] == "user" { return }

        switch partType {
        case "text":
            emitTextPart(part, properties: properties, thought: false)
        case "reasoning":
            emitTextPart(part, properties: properties, thought: true)
        case "tool":
            emitToolPart(part)
        case "step-finish":
            emitUsage(part["tokens"])
        default: break
        }
    }

    private func emitTextPart(_ part: JSONValue, properties: JSONValue, thought: Bool) {
        guard let partId = part["id"]?.stringValue else { return }
        let delta = properties["delta"]?.stringValue
        let text: String
        if let delta {
            text = delta
            partText[partId, default: ""] += delta
        } else if let fullText = part["text"]?.stringValue {
            let previous = partText[partId] ?? ""
            guard fullText != previous else { return }
            text = fullText.hasPrefix(previous) ? String(fullText.dropFirst(previous.count)) : fullText
            partText[partId] = fullText
        } else {
            return
        }
        guard !text.isEmpty else { return }
        let update: SessionUpdate = thought
            ? .agentThoughtChunk(.text(text))
            : .agentMessageChunk(.text(text))
        eventContinuation.yield(.update(update))
    }

    private func emitToolPart(_ part: JSONValue) {
        guard let callId = part["callID"]?.stringValue
                ?? part["id"]?.stringValue else { return }
        let state = part["state"] ?? .object([:])
        let status = toolStatus(state["status"]?.stringValue)
        let input = state["input"]
        let output = state["output"]?.stringValue
        let error = state["error"]?.stringValue
        let snapshot = ToolPartSnapshot(status: status, input: input, output: output, error: error)
        guard lastEmittedToolState[callId] != snapshot else { return }
        lastEmittedToolState[callId] = snapshot

        let title = part["tool"]?.stringValue
            ?? state["title"]?.stringValue
            ?? "Tool call"
        if status == .inProgress || status == .pending {
            eventContinuation.yield(.update(.toolCall(ToolCall(
                toolCallId: callId, title: title, kind: .execute, status: status,
                rawInput: input))))
        } else {
            var content: [ToolCallContent]?
            if let output {
                content = [.content(.text(output))]
            } else if let error {
                content = [.content(.text(error))]
            }
            eventContinuation.yield(.update(.toolCallUpdate(ToolCallUpdate(
                toolCallId: callId, status: status, content: content, rawInput: input))))
        }
    }

    private func handlePermission(_ properties: JSONValue) {
        guard let permissionId = properties["id"]?.stringValue else { return }
        let sessionId = properties["sessionID"]?.stringValue ?? self.sessionId ?? ""
        let permissionName = properties["permission"]?.stringValue ?? "permission"
        let patterns = properties["patterns"]?.arrayValue?.compactMap(\.stringValue) ?? []
        let title = patterns.first ?? permissionName
        let toolCall = ToolCallUpdate(toolCallId: permissionId, title: title,
                                      kind: .execute, status: .pending,
                                      rawInput: properties)
        let options = [
            PermissionOption(optionId: "allow_once", name: "Allow once", kind: .allowOnce),
            PermissionOption(optionId: "allow_always", name: "Allow always", kind: .allowAlways),
            PermissionOption(optionId: "reject_once", name: "Reject", kind: .rejectOnce)
        ]
        permissionSessions[permissionId] = sessionId
        permissionOptions[permissionId] = options
        eventContinuation.yield(.permissionRequested(
            requestId: .string(permissionId), toolCall: toolCall, options: options))
    }

    private func emitUsage(_ tokens: JSONValue?) {
        guard let used = tokens?["total"]?.intValue,
              let size = currentContextSize,
              used >= 0, size > 0 else { return }
        eventContinuation.yield(.update(.usageUpdate(ContextUsage(used: used, size: size))))
    }

    private func resolvePrompt(_ reason: StopReason) {
        if let continuation = promptContinuation {
            promptContinuation = nil
            continuation.resume(returning: reason)
        } else {
            queuedStopReasons.append(reason)
        }
    }

    private func finish() {
        guard !finished else { return }
        finished = true
        promptContinuation?.resume(throwing: DriverError.transportClosed)
        promptContinuation = nil
        eventContinuation.yield(.disconnected)
        eventContinuation.finish()
    }

    private func promptBody(_ blocks: [ContentBlock]) throws -> Data {
        var parts: [JSONValue] = []
        for block in blocks {
            switch block {
            case .text(let text):
                parts.append(.object(["type": .string("text"), "text": .string(text)]))
            case .image(let mimeType, let data):
                parts.append(.object([
                    "type": .string("file"), "mime": .string(mimeType),
                    "url": .string("data:\(mimeType);base64,\(data)")
                ]))
            default:
                parts.append(try JSONValue.encoding(block))
            }
        }
        var body: [String: JSONValue] = [
            "parts": .array(parts),
            "permission": .object(permissionRules.mapValues(JSONValue.string))
        ]
        if let model = selectedModelPayload() { body["model"] = model }
        if let effort { body["variant"] = .string(effort) }
        return try encodedJSON(.object(body))
    }

    private func selectedModelPayload() -> JSONValue? {
        guard let currentModel else { return nil }
        let pieces = currentModel.split(separator: "/", maxSplits: 1).map(String.init)
        guard pieces.count == 2 else { return nil }
        return .object(["providerID": .string(pieces[0]), "modelID": .string(pieces[1])])
    }

    private func makeCatalog(from value: JSONValue) -> ModelCatalog {
        guard let providers = value["providers"]?.arrayValue else {
            return ModelCatalog(models: nil, configOptions: [], contextSizes: [:])
        }
        var choices: [SessionConfigOption.Choice] = []
        var contexts: [String: Int] = [:]
        var variants: [String] = []
        var current: String?
        for provider in providers {
            guard let providerId = provider["id"]?.stringValue,
                  let models = provider["models"],
                  case .object(let modelValues) = models else { continue }
            let providerName = provider["name"]?.stringValue ?? providerId
            let defaultModel = value["default"]?[providerId]?.stringValue
            for modelId in modelValues.keys.sorted() {
                guard let model = modelValues[modelId] else { continue }
                let modelKey = "\(providerId)/\(modelId)"
                let modelName = model["name"]?.stringValue ?? modelId
                choices.append(.init(value: modelKey, name: "\(providerName)/\(modelName)"))
                if let size = model["limit"]?["context"]?.intValue, size > 0 {
                    contexts[modelKey] = size
                }
                if modelId == defaultModel { current = current ?? modelKey }
                if modelId == defaultModel,
                   case .object(let variantValues)? = model["variants"] {
                    variants = variantValues.keys.sorted()
                }
            }
        }
        current = current ?? choices.first?.value
        if currentModel == nil { self.currentModel = current }
        if let current { currentContextSize = contexts[current] }

        var options: [SessionConfigOption] = []
        if let current {
            options.append(SessionConfigOption(id: "model", name: "Model",
                                               currentValue: current, options: choices))
        }
        if !variants.isEmpty {
            options.append(SessionConfigOption(
                id: "effort", name: "Effort", currentValue: variants.first,
                options: variants.map { .init(value: $0, name: $0.capitalized) }))
        }
        let models = current.map { SessionModelState(
            currentModelId: $0,
            availableModels: choices.map { ModelInfo(modelId: $0.value, name: $0.name) }) }
        return ModelCatalog(models: models, configOptions: options, contextSizes: contexts)
    }

    private func updateCurrentModel(providerId: String, modelId: String) {
        let value = "\(providerId)/\(modelId)"
        currentModel = value
        currentContextSize = contextSizes[value]
    }

    private func currentConfigOptions() -> [SessionConfigOption] {
        configOptions
    }

    private func updateConfigOption(id: String, value: String) {
        guard let index = configOptions.firstIndex(where: { $0.id == id }) else { return }
        configOptions[index].currentValue = value
    }

    private func toolStatus(_ raw: String?) -> ToolCallStatus {
        switch raw {
        case "completed", "success": .completed
        case "error", "failed": .failed
        case "pending", "waiting": .pending
        default: .inProgress
        }
    }

    private static func rules(for mode: PermissionMode) -> [String: String] {
        switch mode {
        case .ask: ["*": "ask"]
        case .acceptEdits: ["edit": "allow", "*": "ask"]
        case .plan: ["*": "deny"]
        case .fullAuto: ["*": "allow"]
        }
    }

    private func decodeJSON(_ data: Data) throws -> JSONValue {
        try JSONDecoder().decode(JSONValue.self, from: data)
    }

    private func encodedJSON(_ value: JSONValue) throws -> Data {
        try JSONEncoder().encode(value)
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
