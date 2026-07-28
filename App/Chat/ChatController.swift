import Foundation
import Observation
import TillerACP
import TillerCore
import TillerTerminal

/// Drives one chat tab: owns the agent child process (via AgentDriver),
/// folds events into the transcript, persists at settle points, and
/// surfaces activity status to AppModel. Analog of MarkdownDocument.
@MainActor @Observable
final class ChatController {
    enum ChatState: Equatable {
        case idle, connecting, ready, prompting, needsAuth
        case disconnected(message: String?)
    }

    let tabId: UUID
    private(set) var agentId: String
    private let worktreeId: UUID
    private let worktreePath: String
    private let store: ChatSessionStore?
    private let persistenceCoordinator: PersistenceCoordinator?
    private let installStore: AgentInstallStore
    typealias DriverFactory = (
        String, String, AgentInstallStore, PermissionMode, String?, String?, String?
    ) -> (any AgentDriver)?
    private let driverFactory: DriverFactory
    /// Set on agent switch; the next send() prepends the handoff preamble.
    private var pendingHandoff = false

    private(set) var state: ChatState = .idle
    private(set) var modes: SessionModeState?
    private(set) var models: SessionModelState?
    /// Reasoning-effort select; nil for agents without one.
    private(set) var effortOption: SessionConfigOption?
    /// True when `effortOption` came from `driver.staticEffortOptions()`
    /// (Claude's prompt prefix, Codex's native field) rather than from the
    /// agent's own `configOptions` echo (OpenCode) — decides which transport
    /// `setEffort` uses.
    private var effortIsStatic = false
    /// True when the agent exposes the model as a `configOptions` select;
    /// drives the `setModel` transport (`set_config_option` vs `set_model`).
    private var hasModelConfigOption = false
    /// Native agents expose the unified permission selector; ACP agents use
    /// their own `modes` payload and keep this nil.
    private(set) var permissionMode: PermissionMode?
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
    /// Test hook: fires once per persistence snapshot enqueued.
    @ObservationIgnored var onPersist: (() -> Void)?
    /// Following ("Segui l'agente"): default off, non persistito.
    var isFollowing = false
    var onFollowLocation: ((String) -> Void)?
    /// Transcript id the view should scroll to; cleared by the transcript once
    /// it has scrolled. Set by the pending-question bar.
    var scrollTarget: String?
    /// Bumped once per flushed event batch. The transcript scrolls on this
    /// rather than on `items.count`, which never changes while a message grows.
    private(set) var streamTick = 0
    /// Published transcript-derived values reused by all chat views.
    private(set) var presentationSnapshot = ChatPresentationSnapshot.empty
    @ObservationIgnored private var presentationCache = ChatPresentationSnapshot.Cache()
    /// Test hook: fires only when a changed presentation snapshot is published.
    @ObservationIgnored var onPresentationSnapshotChange: (() -> Void)?

    /// Title of the tool call currently in flight, for the working row.
    var currentActivity: String? {
        for item in items.reversed() {
            guard case .toolCall(let call) = item else { continue }
            if call.status == .pending || call.status == .inProgress { return call.title }
        }
        return nil
    }
    /// Timeline expansion state (work groups and folded turns the user opened).
    var expandedWorkGroups: Set<String> = []
    var unfoldedTurns: Set<String> = []
    @ObservationIgnored private var lastFollowAt = Date.distantPast
    private static let followThrottle: TimeInterval = 0.5

    var items: [TranscriptItem] { restored + reducer.items }
    /// Roots + children + pending plan approval. Recomputed on every access;
    /// callers should read it once per redraw rather than once per row.
    var grouped: ToolCallTree.Grouped { ToolCallTree.group(items: items) }
    var timelineRows: [TimelineRow] {
        TimelineBuilder.rows(items: items, state: TimelineState(
            expandedWorkGroups: expandedWorkGroups,
            unfoldedTurns: unfoldedTurns,
            isStreaming: state == .prompting,
            turnDurations: reducer.turnDurations))
    }
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
    /// Pending permissions shown in the composer approval panel (plan-mode
    /// exits excluded — they render in the proposed-plan card).
    var composerPermissions: [ComposerPermission] {
        ComposerPermissions.extract(from: items)
    }
    var hasPlanAwaitingApproval: Bool {
        grouped.pendingPlanApproval?.isPending == true
    }

    func rebuildPresentationSnapshot() {
        let result = ChatPresentationSnapshot.build(
            items: restored + reducer.items,
            previousCache: presentationCache)
        presentationCache = result.cache
        ChatPresentationSnapshot.assignIfChanged(
            &presentationSnapshot, result.snapshot,
            onPublish: { [weak self] in self?.onPresentationSnapshotChange?() })
    }

    private var driver: (any AgentDriver)?
    private var pumpTask: Task<Void, Never>?
    /// Open between `userPrompted` and whichever closes the turn first.
    @ObservationIgnored private var turnIsOpen = false
    @ObservationIgnored private var pendingEvents: [ACPSessionEvent] = []
    @ObservationIgnored private var eventFlushScheduled = false
    @ObservationIgnored private var lastEventFlush = ContinuousClock.now
    private var sessionRecordId: String?
    private var selectedModel: String?
    private var selectedEffort: String?
    private var forceNewSession = false
    private var lifecycleGeneration = 0
    @ObservationIgnored private var persistenceEnqueueTask: Task<Void, Never>?

    init(tabId: UUID, agentId: String, worktreeId: UUID,
         worktreePath: String, store: ChatSessionStore?,
         installStore: AgentInstallStore,
         persistenceCoordinator: PersistenceCoordinator? = nil,
         startNewConversation: Bool = false,
         driverFactory: @escaping DriverFactory = {
             agentId, worktreePath, installStore, permissionMode, model, effort, resumeSessionId in
             AgentDriverFactory.makeDriver(
                 agentId: agentId, worktreePath: worktreePath,
                 installStore: installStore, permissionMode: permissionMode,
                 model: model, effort: effort, resumeSessionId: resumeSessionId)
         }) {
        self.tabId = tabId
        self.agentId = AgentIdMigration.canonical(agentId)
        self.worktreeId = worktreeId
        self.worktreePath = worktreePath
        self.store = store
        self.persistenceCoordinator = persistenceCoordinator ?? store.map { store in
            PersistenceCoordinator(
                transcriptWriter: { sessionId, items in
                    let sid = SignpostMetrics.makeSignpostID()
                    let state = SignpostMetrics.beginInterval("transcriptPersist", id: sid)
                    defer {
                        SignpostMetrics.endInterval(
                            "transcriptPersist", state,
                            message: "items: \(items.count)")
                    }
                    try store.saveTranscript(sessionId: sessionId, items: items)
                },
                scrollbackWriter: { _, _, _ in
                    preconditionFailure(
                        "ChatController fallback persistence coordinator cannot persist scrollback")
                })
        }
        self.installStore = installStore
        self.driverFactory = driverFactory
        self.forceNewSession = startNewConversation
    }

    // MARK: - Lifecycle

    func start() async {
        guard state == .idle || isDisconnected else { return }
        let generation = lifecycleGeneration

        var record = try? store?.latestSession(worktreeId: worktreeId.uuidString)
        if forceNewSession { record = nil }
        // First start of a reopened chat: adopt the session's last-used agent.
        if let record, sessionRecordId == nil, restored.isEmpty, reducer.items.isEmpty {
            agentId = AgentIdMigration.canonical(record.agentId)
        }
        state = .connecting

        if let record, sessionRecordId == nil,
           let stored = try? store?.loadTranscript(sessionId: record.id) {
            restored = stored
            reducer = TranscriptReducer(existingIDs: Set(stored.map(\.id)))
            rebuildPresentationSnapshot()
        }
        if let record, let used = record.contextUsageUsed, let size = record.contextUsageSize {
            reducer.restoreContextUsage(ContextUsage(used: used, size: size))
        }

        let requestedMode = PermissionMode(rawValue: record?.permissionMode ?? "") ?? .ask
        permissionMode = PermissionMode.pillSelection(forAgent: agentId,
                                                      requested: requestedMode)
        selectedModel = record?.selectedModel
        selectedEffort = record?.selectedEffort
        // Resume only a session created by this same agent. A persisted
        // pi-acp record has an ACP token incompatible with Pi RPC, even
        // though its identity migrates to native Pi.
        let migratedFromPiACP = record?.agentId == "pi-acp"
        let resumeId = !migratedFromPiACP
            && AgentIdMigration.canonical(record?.agentId ?? "") == agentId
            ? record?.acpSessionId : nil
        guard let initialDriver = driverFactory(
            agentId, worktreePath, installStore, requestedMode,
            selectedModel, selectedEffort, resumeId) else {
            state = .disconnected(message: "Agent not installed. Install it from Settings → Agents.")
            return
        }
        self.driver = initialDriver

        do {
            try await initialDriver.start()
            guard generation == lifecycleGeneration else {
                await initialDriver.stop()
                return
            }
            startPump(for: initialDriver)
            var mcpServers: [McpServerSpec] = []
            do {
                mcpServers = try McpConfig.load(worktreeRoot: worktreePath)
            } catch {
                mcpWarning = "Invalid .mcp.json file: session started without MCP servers."
            }
            let handle: SessionHandle
            do {
                handle = try await initialDriver.connect(
                    cwd: worktreePath, resumeSessionId: resumeId,
                    mcpServers: mcpServers)
            } catch where resumeId != nil {
                // Native drivers cannot recover a failed resume in place. The
                // old transcript remains in `restored`; start a new driver
                // and create a fresh conversation without the stale token.
                await initialDriver.stop()
                pumpTask?.cancel()
                pumpTask = nil
                guard generation == lifecycleGeneration else { return }
                guard let freshDriver = driverFactory(
                    agentId, worktreePath, installStore, requestedMode,
                    selectedModel, selectedEffort, nil) else {
                    throw DriverError.unavailable
                }
                self.driver = freshDriver
                try await freshDriver.start()
                guard generation == lifecycleGeneration else {
                    await freshDriver.stop()
                    return
                }
                startPump(for: freshDriver)
                handle = try await freshDriver.connect(
                    cwd: worktreePath, resumeSessionId: nil,
                    mcpServers: mcpServers)
                restored.append(.systemNotice(
                    id: UUID().uuidString,
                    text: "Session resumed as history — new conversation started."))
            }
            guard generation == lifecycleGeneration else {
                await driver?.stop()
                return
            }
            modes = handle.modes
            models = handle.models
            hasModelConfigOption = handle.configOptions.contains { $0.id == "model" }
            if let dynamicEffort = handle.configOptions.first(where: { $0.id == "effort" }) {
                effortOption = dynamicEffort
                effortIsStatic = false
            } else {
                effortOption = await driver?.staticEffortOptions()
                effortIsStatic = effortOption != nil
            }
            selectedModel = models?.currentModelId ?? selectedModel
            selectedEffort = effortOption?.currentValue ?? selectedEffort
            didResume = handle.didResume
            if handle.didResume, let record {
                restored = []
                sessionRecordId = record.id
                try? store?.setACPSessionId(handle.sessionId, sessionId: record.id)
            } else if let sessionRecordId {
                // Agent switch reuses the existing record for transcript continuity.
                try? store?.setACPSessionId(handle.sessionId, sessionId: sessionRecordId)
            } else {
                let created = try store?.createSession(
                    worktreeId: worktreeId.uuidString, agentId: agentId)
                sessionRecordId = created?.id
                if let sessionRecordId {
                    try? store?.setACPSessionId(handle.sessionId, sessionId: sessionRecordId)
                }
            }
            if let sessionRecordId {
                try? store?.setTransportKind(
                    AgentDriverFactory.transportKind(for: agentId).rawValue,
                    sessionId: sessionRecordId)
                persistSessionSettings()
            }
            rebuildPresentationSnapshot()
            forceNewSession = false
            state = .ready
        } catch let ACPClientError.agentError(error)
            where error.message.lowercased().contains("auth") {
            await cleanupFailedStart(generation: generation)
            guard generation == lifecycleGeneration else { return }
            state = .needsAuth
        } catch {
            await cleanupFailedStart(generation: generation)
            guard generation == lifecycleGeneration else { return }
            state = .disconnected(message: "\(error)")
        }
    }

    private func cleanupFailedStart(generation: Int) async {
        guard generation == lifecycleGeneration else { return }
        pumpTask?.cancel()
        pumpTask = nil
        guard let failedDriver = driver else { return }
        await failedDriver.stop()
        guard generation == lifecycleGeneration, driver === failedDriver else { return }
        driver = nil
    }

    func stop() async {
        lifecycleGeneration &+= 1
        reducer.closeAgentMessage()
        persist()
        if let persistenceEnqueueTask {
            await persistenceEnqueueTask.value
        }
        if let persistenceCoordinator, let sessionRecordId {
            await persistenceCoordinator.flush(sessionId: sessionRecordId)
        }
        pumpTask?.cancel()
        pumpTask = nil
        pendingEvents = []
        eventFlushScheduled = false
        if let driver { await driver.stop() }
        driver = nil
        rebuildPresentationSnapshot()
        if state != .needsAuth { state = .disconnected(message: nil) }
    }
    /// Waits only for snapshots already queued by `persist()`.
    func drainPendingPersistence() async {
        await persistenceEnqueueTask?.value
    }

    func newConversation() async {
        await stop()
        reducer = TranscriptReducer()
        restored = []
        rebuildPresentationSnapshot()
        queued = []
        sessionRecordId = nil
        selectedModel = nil
        selectedEffort = nil
        permissionMode = nil
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
        var blocks = ChatPromptBuilder.build(
            text: text, mentionPaths: mentionPaths, images: images,
            worktreePath: worktreePath)
        guard !blocks.isEmpty else { return }
        reducer.userPrompted(blocks)
        turnIsOpen = true
        rebuildPresentationSnapshot()
        if pendingHandoff {
            pendingHandoff = false
            if let preamble = TranscriptHandoff.preamble(items: restored) {
                blocks.insert(.text(preamble), at: 0)
            }
        }
        state = .prompting
        promptError = nil
        onStatusChange?(.running)
        Task { [weak self] in
            guard let self, let driver = self.driver else { return }
            do {
                // The turn is closed by the driver's `turnEnded` event, which
                // the stream delivers behind the updates it terminates.
                _ = try await driver.prompt(blocks)
                self.flushPendingEvents()
            } catch {
                self.flushPendingEvents()
                self.endTurn(.cancelled)
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
        await driver?.cancel()
    }

    /// Optimistic like `setModel`: the pill reflects the choice immediately,
    /// reverted on error (some agents never send `current_mode_update`).
    func setMode(_ modeId: String) async {
        let previous = modes?.currentModeId
        modes?.currentModeId = modeId
        do {
            try await driver?.setMode(modeId)
        } catch {
            if let previous { modes?.currentModeId = previous }
            promptError = Self.describePromptError(error)
        }
    }

    /// Native agents use Tiller's unified permission mode. ACP agents keep
    /// their protocol-provided `modes` and expose nil here.
    func setPermissionMode(_ mode: PermissionMode) async {
        do {
            try await driver?.setMode(mode.rawValue)
            permissionMode = mode
            persistSessionSettings()
        } catch {
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
                if let updated = try await driver?.setConfigOption(
                    id: "model", value: modelId) {
                    if let synced = SessionModelState(configOptions: updated) {
                        models = synced
                    }
                    effortOption = updated.first { $0.id == "effort" }
                }
            } else {
                try await driver?.setModel(modelId)
            }
            selectedModel = models?.currentModelId ?? modelId
            persistSessionSettings()
        } catch {
            if let previous { models?.currentModelId = previous }
            selectedModel = previous
            persistSessionSettings()
            promptError = Self.describePromptError(error)
        }
    }

    /// OpenCode echoes the whole updated option list back; use it to resync
    /// effort (and model, which the same payload carries) after the set.
    func setEffort(_ value: String) async {
        let previous = effortOption?.currentValue
        effortOption?.currentValue = value
        guard !effortIsStatic else {
            await driver?.setEffort(value)
            selectedEffort = value
            persistSessionSettings()
            return
        }
        do {
            if let updated = try await driver?.setConfigOption(
                id: "effort", value: value) {
                effortOption = updated.first { $0.id == "effort" } ?? effortOption
                if let syncedModels = SessionModelState(configOptions: updated) {
                    models = syncedModels
                }
            }
            selectedEffort = effortOption?.currentValue ?? value
            selectedModel = models?.currentModelId ?? selectedModel
            persistSessionSettings()
        } catch {
            effortOption?.currentValue = previous
            selectedEffort = previous
            persistSessionSettings()
            promptError = Self.describePromptError(error)
        }
    }

    /// Answers a question card. Uses the structured channel when the driver
    /// supports it, so the agent learns *which* option was chosen; otherwise
    /// falls back to the plain allow/reject the permission gate already uses.
    func answerQuestion(_ question: ChatQuestion, optionId: String) async {
        guard let driver else { return }
        let option = question.options.first { $0.id == optionId }
        if driver.supportsStructuredAnswers, option?.isRejection != true,
           question.isStructured {
            reducer.permissionResolved(requestId: question.requestId,
                                       resolution: .selected(optionId: optionId))
            rebuildPresentationSnapshot()
            await driver.answerPermission(
                requestId: question.requestId,
                outcome: .answered(optionId: optionId,
                                   updatedInput: .object(["choice": .string(optionId)])))
            onStatusChange?(.running)
            persist()
        } else {
            await answerPermission(requestId: question.requestId, optionId: optionId)
        }
    }

    func answerQuestion(_ question: ChatQuestion, text: String) async {
        let answer = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !answer.isEmpty, let driver else { return }
        reducer.permissionResolved(requestId: question.requestId,
                                   resolution: .selected(optionId: answer))
        rebuildPresentationSnapshot()
        await driver.answerPermission(
            requestId: question.requestId,
            outcome: .answered(optionId: answer,
                               updatedInput: .object(["choice": .string(answer)])))
        onStatusChange?(.running)
        persist()
    }

    func answerPermission(requestId: JSONRPCID, optionId: String?) async {
        if let optionId {
            reducer.permissionResolved(requestId: requestId,
                                       resolution: .selected(optionId: optionId))
            rebuildPresentationSnapshot()
            await driver?.answerPermission(requestId: requestId,
                                            outcome: .selected(optionId: optionId))
            onStatusChange?(.running)
        } else {
            reducer.permissionResolved(requestId: requestId, resolution: .cancelled)
            await driver?.answerPermission(requestId: requestId, outcome: .cancelled)
            rebuildPresentationSnapshot()
        }
        persist()
    }

    // MARK: - Events

    private func startPump(for driver: any AgentDriver) {
        pendingEvents = []
        eventFlushScheduled = false
        pumpTask = Task { [weak self] in
            for await event in driver.events {
                await MainActor.run { self?.enqueue(event) }
            }
        }
    }

    /// Native drivers can emit one `update` per streamed token (Codex sends
    /// `agent_message_delta` for every fragment). Applying each event queued
    /// a SwiftUI transaction whose layout flush outgrew the arrival interval
    /// and hung the main thread, so updates are buffered and applied in
    /// batches at most once per interval; non-update events (permissions,
    /// disconnect) flush the buffer and apply immediately.
    private static let baseFlushInterval: Duration = .milliseconds(40)
    /// SwiftUI's own layout commit for the flushed transaction (the actual
    /// long pole per the freeze investigation — a full-message TextKit
    /// re-render whose cost grows with transcript length) runs on the main
    /// run loop *after* this function returns, not inside it, so timing the
    /// apply loop alone would almost always read ~0 and never engage
    /// backpressure. Instead this compares the requested delay against how
    /// long the main thread actually took to come back around: any excess
    /// ("overrun") was spent on that commit (or anything else blocking the
    /// main thread), and stretches the next interval accordingly.
    private static let overrunMultiplier: Double = 2
    private static let maxFlushInterval: Duration = .milliseconds(500)
    @ObservationIgnored private var scheduledFlushInterval = ChatController.baseFlushInterval
    @ObservationIgnored private var pendingFlushRequestedAt = ContinuousClock.now
    @ObservationIgnored private var pendingFlushDelay = ChatController.baseFlushInterval

    static func nextFlushInterval(lastFlushCost overrun: Duration) -> Duration {
        let stretched = overrun * overrunMultiplier
        return min(max(stretched, baseFlushInterval), maxFlushInterval)
    }

    private func enqueue(_ event: ACPSessionEvent) {
        guard case .update = event else {
            flushPendingEvents(rebuildSnapshot: false)
            handle(event)
            rebuildPresentationSnapshot()
            return
        }
        pendingEvents.append(event)
        guard !eventFlushScheduled else { return }
        eventFlushScheduled = true
        let delay = scheduledFlushInterval - (ContinuousClock.now - lastEventFlush)
        pendingFlushRequestedAt = ContinuousClock.now
        pendingFlushDelay = max(delay, .zero)
        Task { [weak self] in
            if delay > .zero { try? await Task.sleep(for: delay) }
            self?.flushPendingEvents()
        }
    }

    private func flushPendingEvents(rebuildSnapshot: Bool = true) {
        eventFlushScheduled = false
        let actualWait = ContinuousClock.now - pendingFlushRequestedAt
        let overrun = actualWait - pendingFlushDelay
        scheduledFlushInterval = Self.nextFlushInterval(lastFlushCost: max(overrun, .zero))
        lastEventFlush = ContinuousClock.now
        guard !pendingEvents.isEmpty else { return }
        let events = pendingEvents
        pendingEvents = []
        for event in events { handle(event) }
        if rebuildSnapshot { rebuildPresentationSnapshot() }
        streamTick &+= 1
    }

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
                schedulePersist()
            }
            if case .usageUpdate(let usage) = update, let sessionRecordId {
                try? store?.setContextUsage(usage, sessionId: sessionRecordId)
            }
        case .permissionRequested(let requestId, let toolCall, let options):
            reducer.permissionRequested(requestId: requestId,
                                        toolCall: toolCall, options: options)
            onStatusChange?(.needsInput)
        case .turnEnded(let reason):
            endTurn(reason)
        case .disconnected:
            reducer.cancelPendingPermissions()
            if state != .needsAuth, !isDisconnected {
                state = .disconnected(message: "Agent process terminated")
            }
        }
    }

    /// Closes the open turn exactly once. The driver's `turnEnded` event is
    /// the normal path; the prompt's error path closes turns the driver never
    /// got to finish, and one of the two always arrives second.
    private func endTurn(_ reason: StopReason) {
        guard turnIsOpen else { return }
        turnIsOpen = false
        reducer.turnEnded(reason)
        rebuildPresentationSnapshot()
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

    func persist() {
        guard let sessionRecordId, let persistenceCoordinator else { return }
        let snapshot = items
        let previous = persistenceEnqueueTask
        persistenceEnqueueTask = Task { [persistenceCoordinator] in
            _ = await previous?.value
            await persistenceCoordinator.enqueueTranscript(
                sessionId: sessionRecordId, items: snapshot)
        }
        onPersist?()
    }

    /// Tool-heavy turns resolve several tool calls in a tight burst. This
    /// coalesces snapshots before the persistence actor writes them, while
    /// the 800 ms debounce keeps the existing settle-point optimization.
    private static let persistDebounceInterval: Duration = .milliseconds(800)
    @ObservationIgnored private var persistScheduled = false

    private func schedulePersist() {
        guard !persistScheduled else { return }
        persistScheduled = true
        Task { [weak self] in
            try? await Task.sleep(for: Self.persistDebounceInterval)
            guard let self else { return }
            self.persistScheduled = false
            self.persist()
        }
    }

    private func persistSessionSettings() {
        guard let store, let sessionRecordId else { return }
        try? store.setSessionSettings(
            permissionMode: permissionMode?.rawValue,
            selectedModel: selectedModel,
            selectedEffort: selectedEffort,
            sessionId: sessionRecordId)
    }
}

private enum DriverError: Error {
    case unavailable
}
