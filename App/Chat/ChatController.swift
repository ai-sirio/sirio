import Foundation
import Observation
import TillerACP
import TillerCore

/// Drives one chat tab: owns the agent child process (via ACPSession),
/// folds events into the transcript, persists at settle points, and
/// surfaces activity status to AppModel. Analog of MarkdownDocument.
@MainActor @Observable
final class ChatController {
    enum ChatState: Equatable {
        case idle, connecting, ready, prompting, needsAuth
        case disconnected(message: String?)
    }

    let tabId: UUID
    let agentId: String
    private let worktreeId: UUID
    private let worktreePath: String
    private let store: ChatSessionStore?

    private(set) var state: ChatState = .idle
    private(set) var modes: SessionModeState?
    private(set) var models: SessionModelState?
    /// OpenCode-only reasoning-effort select; nil for agents without it.
    private(set) var effortOption: SessionConfigOption?
    /// True when the agent exposes the model as a `configOptions` select;
    /// drives the `setModel` transport (`set_config_option` vs `set_model`).
    private var hasModelConfigOption = false
    private(set) var didResume = false
    private(set) var queued: [String] = []
    /// Transcript from a previous run shown read-only when the agent could
    /// not resume; emptied when session/load replay rebuilds the reducer.
    private(set) var restored: [TranscriptItem] = []
    /// Error from the last prompt turn, shown as a dismissable banner. The
    /// turn would otherwise fail silently (e.g. OpenCode model/auth errors).
    var promptError: String?
    /// Warning non bloccante da .mcp.json invalido; la sessione parte senza MCP.
    var mcpWarning: String?
    private var reducer = TranscriptReducer()
    var onStatusChange: ((AgentStatus) -> Void)?
    /// Following ("Segui l'agente"): default off, non persistito.
    var isFollowing = false
    var onFollowLocation: ((String) -> Void)?
    @ObservationIgnored private var lastFollowAt = Date.distantPast
    private static let followThrottle: TimeInterval = 0.5

    var items: [TranscriptItem] { restored + reducer.items }
    /// Subagent spawns visible in the transcript (Task-type tool calls),
    /// consumed by the Agents panel.
    var activeSubagentTasks: [SubagentTaskInfo] { SubagentTasks.extract(from: items) }
    var currentModeId: String? { reducer.currentModeId ?? modes?.currentModeId }
    var contextUsage: ContextUsage? { reducer.contextUsage }
    var availableCommands: [AvailableCommand] { reducer.availableCommands }
    var hasPendingPermission: Bool {
        items.contains {
            if case .toolCall(let item) = $0 { return item.permission?.isPending == true }
            return false
        }
    }

    private var session: ACPSession?
    private var pumpTask: Task<Void, Never>?
    private var sessionRecordId: String?
    private var forceNewSession = false

    init(tabId: UUID, agentId: String, worktreeId: UUID,
         worktreePath: String, store: ChatSessionStore?) {
        self.tabId = tabId
        self.agentId = agentId
        self.worktreeId = worktreeId
        self.worktreePath = worktreePath
        self.store = store
    }

    // MARK: - Lifecycle

    func start() async {
        guard state == .idle || isDisconnected else { return }
        guard let spec = AgentLaunchSpec.forAgent(id: agentId) else {
            state = .disconnected(message: "Agent has no ACP support")
            return
        }
        state = .connecting

        var record = try? store?.latestSession(worktreeId: worktreeId.uuidString)
        if forceNewSession { record = nil }
        if let record, let stored = try? store?.loadTranscript(sessionId: record.id) {
            restored = stored
        }
        if let record, let used = record.contextUsageUsed, let size = record.contextUsageSize {
            reducer.restoreContextUsage(ContextUsage(used: used, size: size))
        }

        let transport = ProcessTransport(
            executable: spec.executable, arguments: spec.arguments,
            cwd: worktreePath,
            environment: AgentLaunchSpec.launchEnvironment(),
            onStderrLine: { line in
                NSLog("[chat:\(spec.arguments.last ?? "?")] %@", line)
            })
        let session = ACPSession(
            client: ACPClient(transport: transport),
            fileSystem: WorktreeFileSystem(root: worktreePath))
        self.session = session

        do {
            try await session.start()
            pumpTask = Task { [weak self] in
                for await event in session.events {
                    await MainActor.run { self?.handle(event) }
                }
            }
            var mcpServers: [McpServerSpec] = []
            do {
                mcpServers = try McpConfig.load(worktreeRoot: worktreePath)
            } catch {
                mcpWarning = "Invalid .mcp.json file: session started without MCP servers."
            }
            let handle = try await session.connect(
                cwd: worktreePath, resumeSessionId: record?.acpSessionId,
                mcpServers: mcpServers)
            modes = handle.modes
            models = handle.models
            hasModelConfigOption = handle.configOptions.contains { $0.id == "model" }
            effortOption = handle.configOptions.first { $0.id == "effort" }
            didResume = handle.didResume
            if handle.didResume, let record {
                // Replay rebuilds the live transcript; drop the local copy.
                restored = []
                sessionRecordId = record.id
                // The agent may have re-registered the conversation under a
                // new id (SDK resume can fork); persist whatever it answered
                // so the next resume targets the live conversation.
                try? store?.setACPSessionId(handle.sessionId, sessionId: record.id)
            } else {
                let created = try store?.createSession(
                    worktreeId: worktreeId.uuidString, agentId: agentId)
                sessionRecordId = created?.id
                if let sessionRecordId {
                    try? store?.setACPSessionId(handle.sessionId,
                                                sessionId: sessionRecordId)
                }
            }
            forceNewSession = false
            state = .ready
        } catch let ACPClientError.agentError(error)
            where error.message.lowercased().contains("auth") {
            state = .needsAuth
        } catch {
            state = .disconnected(message: "\(error)")
        }
    }

    func stop() async {
        persist()
        pumpTask?.cancel()
        if let session { await session.stop() }
        session = nil
        if state != .needsAuth { state = .disconnected(message: nil) }
    }

    func newConversation() async {
        await stop()
        reducer = TranscriptReducer()
        restored = []
        queued = []
        sessionRecordId = nil
        forceNewSession = true
        state = .idle
        await start()
    }

    private var isDisconnected: Bool {
        if case .disconnected = state { return true }
        return false
    }

    // MARK: - Prompting

    func send(text: String, mentionPaths: [String], images: [ImageAttachment]) {
        if state == .prompting {
            queued.append(text)
            return
        }
        guard state == .ready else { return }
        let blocks = ChatPromptBuilder.build(
            text: text, mentionPaths: mentionPaths, images: images,
            worktreePath: worktreePath)
        guard !blocks.isEmpty else { return }
        reducer.userPrompted(blocks)
        state = .prompting
        promptError = nil
        onStatusChange?(.running)
        Task { [weak self] in
            guard let self, let session = self.session else { return }
            do {
                let reason = try await session.prompt(blocks)
                self.reducer.turnEnded(reason)
            } catch {
                self.reducer.turnEnded(.cancelled)
                if self.isDisconnectedError(error) {
                    self.state = .disconnected(message: "\(error)")
                } else {
                    self.promptError = Self.describePromptError(error)
                }
            }
            self.persist()
            if self.state == .prompting { self.state = .ready }
            self.onStatusChange?(.done)
            self.dispatchQueued()
        }
    }

    private func dispatchQueued() {
        guard state == .ready, !queued.isEmpty else { return }
        let next = queued.removeFirst()
        send(text: next, mentionPaths: [], images: [])
    }

    func cancelTurn() async {
        await session?.cancel()
    }

    /// Optimistic like `setModel`: the pill reflects the choice immediately,
    /// reverted on error (some agents never send `current_mode_update`).
    func setMode(_ modeId: String) async {
        let previous = modes?.currentModeId
        modes?.currentModeId = modeId
        do {
            try await session?.setMode(modeId)
        } catch {
            if let previous { modes?.currentModeId = previous }
            promptError = Self.describePromptError(error)
        }
    }

    /// Optimistic: the pill reflects the choice immediately, reverted on
    /// error. Agents that expose the model as a `configOptions` select
    /// (claude-agent-acp, OpenCode) don't implement `session/set_model`, so
    /// route through `set_config_option` and resync from its echo — a model
    /// switch can also change the available effort levels.
    func setModel(_ modelId: String) async {
        let previous = models?.currentModelId
        models?.currentModelId = modelId
        do {
            if hasModelConfigOption {
                if let updated = try await session?.setConfigOption(
                    id: "model", value: modelId) {
                    if let synced = SessionModelState(configOptions: updated) {
                        models = synced
                    }
                    effortOption = updated.first { $0.id == "effort" }
                }
            } else {
                try await session?.setModel(modelId)
            }
        } catch {
            if let previous { models?.currentModelId = previous }
            promptError = Self.describePromptError(error)
        }
    }

    /// OpenCode echoes the whole updated option list back; use it to resync
    /// effort (and model, which the same payload carries) after the set.
    func setEffort(_ value: String) async {
        let previous = effortOption?.currentValue
        effortOption?.currentValue = value
        do {
            guard let updated = try await session?.setConfigOption(
                id: "effort", value: value) else { return }
            effortOption = updated.first { $0.id == "effort" } ?? effortOption
            if let syncedModels = SessionModelState(configOptions: updated) {
                models = syncedModels
            }
        } catch {
            effortOption?.currentValue = previous
            promptError = Self.describePromptError(error)
        }
    }

    func answerPermission(requestId: JSONRPCID, optionId: String?) async {
        if let optionId {
            reducer.permissionResolved(requestId: requestId,
                                       resolution: .selected(optionId: optionId))
            await session?.answerPermission(requestId: requestId,
                                            outcome: .selected(optionId: optionId))
            onStatusChange?(.running)
        } else {
            reducer.permissionResolved(requestId: requestId, resolution: .cancelled)
            await session?.answerPermission(requestId: requestId, outcome: .cancelled)
        }
        persist()
    }

    // MARK: - Events

    private func handle(_ event: ACPSessionEvent) {
        switch event {
        case .update(let update):
            reducer.apply(update)
            if isFollowing, let location = update.toolCallLocations.last,
               Date().timeIntervalSince(lastFollowAt) >= Self.followThrottle {
                lastFollowAt = Date()
                onFollowLocation?(location.path)
            }
            if case .toolCallUpdate(let change) = update,
               change.status == .completed || change.status == .failed {
                persist()
            }
            if case .usageUpdate(let usage) = update, let sessionRecordId {
                try? store?.setContextUsage(usage, sessionId: sessionRecordId)
            }
        case .permissionRequested(let requestId, let toolCall, let options):
            reducer.permissionRequested(requestId: requestId,
                                        toolCall: toolCall, options: options)
            onStatusChange?(.needsInput)
        case .disconnected:
            if state != .needsAuth, !isDisconnected {
                state = .disconnected(message: "Agent process terminated")
            }
        }
    }

    private func isDisconnectedError(_ error: Error) -> Bool {
        if case ACPClientError.transportClosed = error { return true }
        return false
    }

    private static func describePromptError(_ error: Error) -> String {
        if case ACPClientError.agentError(let rpcError) = error {
            var text = rpcError.message
            if let details = rpcError.data?["details"]?.stringValue {
                text += " — \(details)"
            }
            return text
        }
        return "\(error)"
    }

    // MARK: - Persistence

    private func persist() {
        guard let store, let sessionRecordId else { return }
        try? store.saveTranscript(sessionId: sessionRecordId, items: items)
    }
}
