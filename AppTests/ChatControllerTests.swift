import Foundation
import AppKit
import SwiftUI
import Testing
import TillerPersistence
import TillerCore
import TillerTerminal
@testable import TillerACP
@testable import Tiller

@Suite(.serialized)
@MainActor
struct ChatControllerTests {
    @Test func newChatDoesNotResumeLatestWorktreeSession() async throws {
        let database = try AppDatabase.inMemory()
        let projectId = UUID().uuidString
        let worktreeId = UUID()
        try database.write { db in
            try ProjectRecord(id: projectId, name: "P", rootPath: "/tmp/chat-controller",
                              createdAt: Date()).insert(db)
            try WorktreeRecord(id: worktreeId.uuidString, projectId: projectId,
                               branch: "main", path: "/tmp/chat-controller",
                               createdAt: Date()).insert(db)
        }
        let store = ChatSessionStore(database: database)
        let previous = try store.createSession(
            worktreeId: worktreeId.uuidString, agentId: "claude-acp")
        try store.setACPSessionId("old-session", sessionId: previous.id)
        try store.saveTranscript(sessionId: previous.id, items: [
            .agentMessage(id: "old-message", text: "old chat", isComplete: true)
        ])

        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let installStore = AgentInstallStore(rootDirectory: root)

        let controller = ChatController(
            tabId: UUID(), agentId: "claude-acp", worktreeId: worktreeId,
            worktreePath: root.path, store: store, installStore: installStore,
            startNewConversation: true,
            driverFactory: { _, _, _, _, _, _, _ in nil })
        await controller.start()

        #expect(controller.state == ChatController.ChatState.disconnected(message: "Agent not installed. Install it from Settings → Agents."))
        #expect(controller.items.isEmpty)
    }

    @Test func driverSeamReachesReadyForwardsPromptAndFoldsEvents() async throws {
        let worktreeId = UUID()
        let (store, installStore, root) = try makeChatTestFixture(worktreeId: worktreeId)
        defer { try? FileManager.default.removeItem(at: root) }
        let driver = ChatTestDriver(handle: makeChatTestHandle(sessionId: "native-session"))
        let controller = ChatController(
            tabId: UUID(), agentId: "claude-acp", worktreeId: worktreeId,
            worktreePath: root.path, store: store, installStore: installStore,
            driverFactory: { _, _, _, _, _, _, _ in driver })

        await controller.start()
        #expect(controller.state == .ready)

        controller.send(text: "hello", mentionPaths: [], images: [])
        for _ in 0..<50 {
            if controller.items.contains(where: {
                if case .agentMessage(_, "stub response", _) = $0 { return true }
                return false
            }) { break }
            await Task.yield()
        }

        #expect(await driver.promptCount == 1)
        #expect(controller.items.contains(where: {
            if case .userMessage = $0 { return true }
            return false
        }))
        #expect(controller.items.contains(where: {
            if case .agentMessage(_, "stub response", _) = $0 { return true }
            return false
        }))
    }

    @Test func permissionModeForwardsRawValueAndPersistsSettings() async throws {
        let worktreeId = UUID()
        let (store, installStore, root) = try makeChatTestFixture(worktreeId: worktreeId)
        defer { try? FileManager.default.removeItem(at: root) }
        let record = try store.createSession(
            worktreeId: worktreeId.uuidString, agentId: "claude-acp")
        try store.setACPSessionId("resume-token", sessionId: record.id)
        try store.saveTranscript(sessionId: record.id, items: [
            .agentMessage(id: "history", text: "history", isComplete: true)
        ])
        let driver = ChatTestDriver(
            handle: makeChatTestHandle(sessionId: "resumed", didResume: true))
        let controller = ChatController(
            tabId: UUID(), agentId: "claude-acp", worktreeId: worktreeId,
            worktreePath: root.path, store: store, installStore: installStore,
            driverFactory: { _, _, _, _, _, _, _ in driver })

        await controller.start()
        await controller.setPermissionMode(.acceptEdits)

        #expect(await driver.modeIds == ["acceptEdits"])
        let saved = try store.latestSession(worktreeId: worktreeId.uuidString)
        #expect(saved?.permissionMode == "acceptEdits")
        #expect(saved?.transportKind == "native")
    }

    @Test func resumeFailureKeepsHistoryAndStartsFreshConversation() async throws {
        let worktreeId = UUID()
        let (store, installStore, root) = try makeChatTestFixture(worktreeId: worktreeId)
        defer { try? FileManager.default.removeItem(at: root) }
        let record = try store.createSession(
            worktreeId: worktreeId.uuidString, agentId: "claude-acp")
        try store.setACPSessionId("legacy-token", sessionId: record.id)
        try store.saveTranscript(sessionId: record.id, items: [
            .agentMessage(id: "history", text: "old conversation", isComplete: true)
        ])
        let resumeDriver = ChatTestDriver(
            handle: makeChatTestHandle(sessionId: "unused"), failsResume: true)
        let freshDriver = ChatTestDriver(
            handle: makeChatTestHandle(sessionId: "fresh-token"))
        var drivers = [resumeDriver, freshDriver]
        let controller = ChatController(
            tabId: UUID(), agentId: "claude-acp", worktreeId: worktreeId,
            worktreePath: root.path, store: store, installStore: installStore,
            driverFactory: { _, _, _, _, _, _, _ in drivers.removeFirst() })

        await controller.start()

        #expect(controller.state == .ready)
        #expect(controller.restored.contains(where: {
            if case .agentMessage(_, "old conversation", true) = $0 { return true }
            return false
        }))
        #expect(controller.restored.contains(where: {
            if case .systemNotice(_, "Session resumed as history — new conversation started.") = $0 {
                return true
            }
            return false
        }))
        #expect(await resumeDriver.resumeIds == ["legacy-token"])
        #expect(await freshDriver.resumeIds == [nil])
    }

    /// Streamed updates are coalesced before hitting the reducer (token-rate
    /// drivers were saturating the main thread); the buffer must not lose or
    /// reorder chunks while batching them.
    @Test func burstOfStreamedChunksIsCoalescedWithoutLoss() async throws {
        let worktreeId = UUID()
        let (store, installStore, root) = try makeChatTestFixture(worktreeId: worktreeId)
        defer { try? FileManager.default.removeItem(at: root) }
        let driver = ChatTestDriver(handle: makeChatTestHandle(sessionId: "burst"))
        let controller = ChatController(
            tabId: UUID(), agentId: "claude-acp", worktreeId: worktreeId,
            worktreePath: root.path, store: store, installStore: installStore,
            driverFactory: { _, _, _, _, _, _, _ in driver })

        await controller.start()
        let fragments = (0..<20).map { "t\($0);" }
        for fragment in fragments {
            driver.emit(.update(.agentMessageChunk(.text(fragment))))
        }

        let expected = fragments.joined()
        var folded = false
        for _ in 0..<100 where !folded {
            folded = controller.items.contains(where: {
                if case .agentMessage(_, expected, _) = $0 { return true }
                return false
            })
            if !folded { try await Task.sleep(for: .milliseconds(10)) }
        }
        #expect(folded)
        #expect(controller.items.count == 1)
    }

    /// Backpressure: the flush interval stretches with the measured cost of
    /// the previous flush (apply loop plus late wake-up, the observable
    /// shadow of the SwiftUI layout commit), so event application cannot
    /// saturate the main thread once the transcript grows.
    @Test func flushIntervalStretchesWithLastFlushCostAndClamps() {
        #expect(ChatController.nextFlushInterval(lastFlushCost: .zero) == .milliseconds(40))
        #expect(ChatController.nextFlushInterval(lastFlushCost: .milliseconds(10)) == .milliseconds(40))
        #expect(ChatController.nextFlushInterval(lastFlushCost: .milliseconds(100)) == .milliseconds(200))
        #expect(ChatController.nextFlushInterval(lastFlushCost: .seconds(2)) == .milliseconds(500))
    }

    /// A tool-heavy turn resolves several tool calls in a tight burst; each
    /// completion used to call `persist()` synchronously (full transcript
    /// serialization, main thread) so the burst serialized N times in a row.
    /// Rapid completions must coalesce into far fewer writes.
    @Test func rapidToolCompletionsCoalescePersistCalls() async throws {
        let worktreeId = UUID()
        let (store, installStore, root) = try makeChatTestFixture(worktreeId: worktreeId)
        defer { try? FileManager.default.removeItem(at: root) }
        let driver = ChatTestDriver(handle: makeChatTestHandle(sessionId: "tool-burst"))
        let controller = ChatController(
            tabId: UUID(), agentId: "claude-acp", worktreeId: worktreeId,
            worktreePath: root.path, store: store, installStore: installStore,
            driverFactory: { _, _, _, _, _, _, _ in driver })
        await controller.start()

        var persistCount = 0
        controller.onPersist = { persistCount += 1 }

        for index in 0..<5 {
            driver.emit(.update(.toolCallUpdate(ToolCallUpdate(
                toolCallId: "call-\(index)", status: .completed,
                content: [.content(.text("done"))]))))
        }
        for _ in 0..<50 where controller.items.count < 5 {
            try await Task.sleep(for: .milliseconds(10))
        }
        #expect(controller.items.count == 5)
        #expect(persistCount < 5)

        try await Task.sleep(for: .milliseconds(900))
        #expect(persistCount >= 1)
    }

    @Test func pendingPermissionRendersAboveComposerWhilePromptRemainsOpen() async throws {
        let worktreeId = UUID()
        let (store, installStore, root) = try makeChatTestFixture(worktreeId: worktreeId)
        defer { try? FileManager.default.removeItem(at: root) }
        let permission = ACPSessionEvent.permissionRequested(
            requestId: .string("request-1"),
            toolCall: ToolCallUpdate(toolCallId: "write-1", title: "Write file",
                                     kind: .edit, status: .pending),
            options: [PermissionOption(optionId: "allow_once", name: "Allow once",
                                       kind: .allowOnce)])
        let driver = ChatTestDriver(handle: makeChatTestHandle(sessionId: "permission"),
                                    promptEvents: [permission], holdPromptOpen: true)
        let controller = ChatController(
            tabId: UUID(), agentId: "claude-acp", worktreeId: worktreeId,
            worktreePath: root.path, store: store, installStore: installStore,
            driverFactory: { _, _, _, _, _, _, _ in driver })

        await controller.start()
        controller.send(text: "retry", mentionPaths: [], images: [])
        for _ in 0..<100 where controller.composerPermissions.isEmpty {
            try await Task.sleep(for: .milliseconds(10))
        }

        #expect(controller.composerPermissions.count == 1)
        #expect(await driver.promptIsOpen)
        #expect(await driver.promptCount == 1)

        let worktree = Worktree(id: worktreeId, projectId: UUID(), branch: "main", path: root.path)
        let appModel = AppModel(paneRegistry: PaneRegistry(), activateApplication: {})
        let host = NSHostingView(rootView: ChatPaneView(
            controller: controller, worktree: worktree, appModel: appModel))
        host.setFrameSize(NSSize(width: 640, height: 480))
        host.layoutSubtreeIfNeeded()

        let approvalPanel = try #require(host.descendant(withAccessibilityIdentifier: "chat-approval-panel"))
        let composer = try #require(host.descendant(withAccessibilityIdentifier: "chat-composer"))
        let approvalFrame = approvalPanel.convert(approvalPanel.bounds, to: host)
        let composerFrame = composer.convert(composer.bounds, to: host)
        #expect(approvalFrame.maxY <= composerFrame.minY)

        await driver.releasePrompt()
    }
}

private extension NSView {
    func descendant(withAccessibilityIdentifier identifier: String) -> NSView? {
        if accessibilityIdentifier() == identifier { return self }
        for child in subviews {
            if let match = child.descendant(withAccessibilityIdentifier: identifier) {
                return match
            }
        }
        return nil
    }
}

private func makeChatTestFixture(worktreeId: UUID) throws
    -> (ChatSessionStore, AgentInstallStore, URL) {
    let database = try AppDatabase.inMemory()
    let projectId = UUID().uuidString
    let root = FileManager.default.temporaryDirectory
        .appendingPathComponent("chat-controller-\(UUID().uuidString)", isDirectory: true)
    try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
    try database.write { db in
        try ProjectRecord(id: projectId, name: "P", rootPath: root.path,
                          createdAt: Date()).insert(db)
        try WorktreeRecord(id: worktreeId.uuidString, projectId: projectId,
                           branch: "main", path: root.path, createdAt: Date()).insert(db)
    }
    let installRoot = root.appendingPathComponent("agents", isDirectory: true)
    try FileManager.default.createDirectory(at: installRoot, withIntermediateDirectories: true)
    return (ChatSessionStore(database: database), AgentInstallStore(rootDirectory: installRoot), root)
}

private func makeChatTestHandle(sessionId: String, didResume: Bool = false) -> SessionHandle {
    SessionHandle(sessionId: sessionId, agentCapabilities: AgentCapabilities(), modes: nil,
                  models: nil, configOptions: [], didResume: didResume)
}

private enum ChatTestDriverError: Error {
    case resumeFailed
}

private actor ChatTestDriver: AgentDriver {
    nonisolated let events: AsyncStream<ACPSessionEvent>
    private let continuation: AsyncStream<ACPSessionEvent>.Continuation
    private let handle: SessionHandle
    private let failsResume: Bool
    private let promptEvents: [ACPSessionEvent]
    private let holdPromptOpen: Bool
    private var promptReleased = false
    private(set) var promptIsOpen = false
    private(set) var promptCount = 0
    private(set) var modeIds: [String] = []
    private(set) var resumeIds: [String?] = []

    init(handle: SessionHandle, failsResume: Bool = false,
         promptEvents: [ACPSessionEvent] = [], holdPromptOpen: Bool = false) {
        let (events, continuation) = AsyncStream.makeStream(of: ACPSessionEvent.self)
        self.events = events
        self.continuation = continuation
        self.handle = handle
        self.failsResume = failsResume
        self.promptEvents = promptEvents
        self.holdPromptOpen = holdPromptOpen
    }

    func start() async throws {}
    func stop() async { continuation.finish() }

    nonisolated func emit(_ event: ACPSessionEvent) { continuation.yield(event) }

    func connect(cwd: String, resumeSessionId: String?,
                 mcpServers: [McpServerSpec]) async throws -> SessionHandle {
        _ = cwd
        _ = mcpServers
        resumeIds.append(resumeSessionId)
        if failsResume, resumeSessionId != nil { throw ChatTestDriverError.resumeFailed }
        return handle
    }

    func prompt(_ blocks: [ContentBlock]) async throws -> StopReason {
        _ = blocks
        promptCount += 1
        promptIsOpen = true
        for event in promptEvents { continuation.yield(event) }
        while holdPromptOpen && !promptReleased { await Task.yield() }
        promptIsOpen = false
        for _ in 0..<10 { await Task.yield() }
        continuation.yield(.update(.agentMessageChunk(.text("stub response"))))
        await Task.yield()
        return .endTurn
    }

    func cancel() async {}

    func releasePrompt() { promptReleased = true }

    func setMode(_ modeId: String) async throws {
        modeIds.append(modeId)
    }

    func setModel(_ modelId: String) async throws { _ = modelId }

    func setConfigOption(id: String, value: String) async throws
        -> [SessionConfigOption]? {
        _ = id
        _ = value
        return nil
    }

    func answerPermission(requestId: JSONRPCID, outcome: PermissionOutcome) async {
        _ = requestId
        _ = outcome
    }
}
