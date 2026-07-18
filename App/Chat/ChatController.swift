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
    private(set) var didResume = false
    private(set) var queued: [String] = []
    /// Transcript from a previous run shown read-only when the agent could
    /// not resume; emptied when session/load replay rebuilds the reducer.
    private(set) var restored: [TranscriptItem] = []
    /// Error from the last prompt turn, shown as a dismissable banner. The
    /// turn would otherwise fail silently (e.g. OpenCode model/auth errors).
    var promptError: String?
    private var reducer = TranscriptReducer()
    var onStatusChange: ((AgentStatus) -> Void)?

    var items: [TranscriptItem] { restored + reducer.items }
    var currentModeId: String? { reducer.currentModeId ?? modes?.currentModeId }
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
            state = .disconnected(message: "Agente senza supporto ACP")
            return
        }
        state = .connecting

        var record = try? store?.latestSession(
            worktreeId: worktreeId.uuidString, agentId: agentId)
        if forceNewSession { record = nil }
        if let record, let stored = try? store?.loadTranscript(sessionId: record.id) {
            restored = stored
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
            let handle = try await session.connect(
                cwd: worktreePath, resumeSessionId: record?.acpSessionId)
            modes = handle.modes
            didResume = handle.didResume
            if handle.didResume, let record {
                // Replay rebuilds the live transcript; drop the local copy.
                restored = []
                sessionRecordId = record.id
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

    func setMode(_ modeId: String) async {
        try? await session?.setMode(modeId)
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
            if case .toolCallUpdate(let change) = update,
               change.status == .completed || change.status == .failed {
                persist()
            }
        case .permissionRequested(let requestId, let toolCall, let options):
            reducer.permissionRequested(requestId: requestId,
                                        toolCall: toolCall, options: options)
            onStatusChange?(.needsInput)
        case .disconnected:
            if state != .needsAuth, !isDisconnected {
                state = .disconnected(message: "Processo agente terminato")
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
