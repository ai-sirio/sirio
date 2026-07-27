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
            try await Task.sleep(for: .milliseconds(20))
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

    /// The reply belongs inside the turn it answers, above the divider that
    /// closes it — even when the last chunk and the turn result arrive back
    /// to back and the event pump has not drained yet.
    @Test func agentReplyIsOrderedBeforeTheTurnDivider() async throws {
        let worktreeId = UUID()
        let (store, installStore, root) = try makeChatTestFixture(worktreeId: worktreeId)
        defer { try? FileManager.default.removeItem(at: root) }
        let driver = ChatTestDriver(handle: makeChatTestHandle(sessionId: "native-session"))
        let controller = ChatController(
            tabId: UUID(), agentId: "claude-acp", worktreeId: worktreeId,
            worktreePath: root.path, store: store, installStore: installStore,
            driverFactory: { _, _, _, _, _, _, _ in driver })
        await controller.start()

        controller.send(text: "hello", mentionPaths: [], images: [])
        func index(_ match: (TranscriptItem) -> Bool) -> Int? {
            controller.items.firstIndex(where: match)
        }
        let isReply: (TranscriptItem) -> Bool = {
            if case .agentMessage(_, "stub response", _) = $0 { return true }
            return false
        }
        let isDivider: (TranscriptItem) -> Bool = {
            if case .turnDivider = $0 { return true }
            return false
        }
        for _ in 0..<50 {
            if index(isReply) != nil, index(isDivider) != nil { break }
            try await Task.sleep(for: .milliseconds(20))
        }

        let replyIndex = index(isReply)
        let dividerIndex = index(isDivider)
        #expect(replyIndex != nil)
        #expect(dividerIndex != nil)
        if let replyIndex, let dividerIndex { #expect(replyIndex < dividerIndex) }
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

    @Test func migratedPiACPSessionStartsFreshWithoutResumingACPToken() async throws {
        let worktreeId = UUID()
        let (store, installStore, root) = try makeChatTestFixture(worktreeId: worktreeId)
        defer { try? FileManager.default.removeItem(at: root) }
        let record = try store.createSession(
            worktreeId: worktreeId.uuidString, agentId: "pi-acp")
        try store.setACPSessionId("legacy-acp-token", sessionId: record.id)
        let driver = ChatTestDriver(handle: makeChatTestHandle(sessionId: "fresh-pi"))
        let controller = ChatController(
            tabId: UUID(), agentId: "pi", worktreeId: worktreeId,
            worktreePath: root.path, store: store, installStore: installStore,
            driverFactory: { _, _, _, _, _, _, resumeId in
                #expect(resumeId == nil)
                return driver
            })

        await controller.start()

        #expect(controller.agentId == "pi")
        #expect(await driver.resumeIds == [nil])
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

    @Test func restoredHistoryAndFreshReducerKeepItemIdsUnique() async throws {
        let worktreeId = UUID()
        let (store, installStore, root) = try makeChatTestFixture(worktreeId: worktreeId)
        defer { try? FileManager.default.removeItem(at: root) }
        let record = try store.createSession(
            worktreeId: worktreeId.uuidString, agentId: "claude-acp")
        try store.setACPSessionId("legacy-token", sessionId: record.id)
        try store.saveTranscript(sessionId: record.id, items: [
            .userMessage(id: "user-0", blocks: [.text("old prompt")]),
            .agentMessage(id: "agent-1", text: "old response", isComplete: true)
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
        controller.send(text: "new prompt", mentionPaths: [], images: [])

        let ids = controller.items.map(\.id)
        #expect(Set(ids).count == ids.count)
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
}

extension ChatControllerTests {
    @Test func tabCloseFlushesLastTranscript() async throws {
        let worktreeId = UUID()
        let (store, installStore, root) = try makeChatTestFixture(worktreeId: worktreeId)
        defer { try? FileManager.default.removeItem(at: root) }
        let writer = TestPersistenceWriter()
        let coordinator = PersistenceCoordinator(
            transcriptWriter: { sessionId, items in
                try await writer.write(.transcript(sessionId), value: items)
            },
            scrollbackWriter: { _, _, _ in },
            onError: { _, _ in })
        let driver = ChatTestDriver(handle: makeChatTestHandle(sessionId: "tab-close"))
        let controller = ChatController(
            tabId: UUID(), agentId: "claude-acp", worktreeId: worktreeId,
            worktreePath: root.path, store: store, installStore: installStore,
            persistenceCoordinator: coordinator,
            driverFactory: { _, _, _, _, _, _, _ in driver })
        await controller.start()
        driver.emit(.update(.agentMessageChunk(.text("last transcript"))))
        for _ in 0..<100 where controller.items.isEmpty {
            try await Task.sleep(for: .milliseconds(10))
        }

        let model = AppModel(activateApplication: {})
        model.chatControllers[controller.tabId] = controller
        model.teardownChatController(tabId: controller.tabId)
        for _ in 0..<100 where await writer.allTranscripts().isEmpty {
            try await Task.sleep(for: .milliseconds(10))
        }

        let saved = await writer.allTranscripts().last ?? []
        #expect(saved.contains {
            if case .agentMessage(_, let text, _) = $0 {
                return text == "last transcript"
            }
            return false
        })
    }

    @Test func quitFlushesLastTranscriptAndScrollback() async throws {
        let worktreeId = UUID()
        let (store, installStore, root) = try makeChatTestFixture(worktreeId: worktreeId)
        defer { try? FileManager.default.removeItem(at: root) }
        let writer = TestPersistenceWriter()
        let coordinator = PersistenceCoordinator(
            transcriptWriter: { sessionId, items in
                try await writer.write(.transcript(sessionId), value: items)
            },
            scrollbackWriter: { _, paneId, data in
                try await writer.write(.scrollback(paneId), value: data)
            },
            onError: { _, _ in })
        let chatTabId = UUID()
        let driver = ChatTestDriver(handle: makeChatTestHandle(sessionId: "quit"))
        let controller = ChatController(
            tabId: chatTabId, agentId: "claude-acp", worktreeId: worktreeId,
            worktreePath: root.path, store: store, installStore: installStore,
            persistenceCoordinator: coordinator,
            driverFactory: { _, _, _, _, _, _, _ in driver })
        await controller.start()
        driver.emit(.update(.agentMessageChunk(.text("last transcript"))))
        for _ in 0..<100 where controller.items.isEmpty {
            try await Task.sleep(for: .milliseconds(10))
        }

        let paneId = UUID()
        let data = Data("last scrollback".utf8)
        let registry = PaneRegistry()
        let buffer = ScrollbackBuffer()
        await buffer.append(data)
        await registry.register(
            paneId: paneId, pty: PtyProcess { _ in }, scrollback: buffer)
        let model = AppModel(
            paneRegistry: registry,
            activateApplication: {},
            persistenceCoordinator: coordinator)
        model.tabs[worktreeId] = [
            WorkspaceTab(id: chatTabId, title: "Chat", content: .chat(agentId: "claude-acp")),
            WorkspaceTab(id: UUID(), title: "Terminal 1", tree: .leaf(id: paneId))
        ]
        model.chatControllers[chatTabId] = controller

        await model.flushLiveScrollback()

        #expect(await driver.stopCount == 0)
        #expect(await writer.scrollbacks(for: paneId) == [data])
        let saved = await writer.allTranscripts().last ?? []
        #expect(saved.contains {
            if case .agentMessage(_, let text, _) = $0 {
                return text == "last transcript"
            }
            return false
        })
    }
    @Test func quitPersistsPendingControllerEnqueue() async throws {
        let worktreeId = UUID()
        let (store, installStore, root) = try makeChatTestFixture(worktreeId: worktreeId)
        defer { try? FileManager.default.removeItem(at: root) }
        let writer = TestPersistenceWriter()
        let coordinator = PersistenceCoordinator(
            transcriptWriter: { sessionId, items in
                try await writer.write(.transcript(sessionId), value: items)
            },
            scrollbackWriter: { _, _, _ in },
            onError: { _, _ in })
        let session = try store.createSession(
            worktreeId: worktreeId.uuidString, agentId: "claude-acp")
        try store.setACPSessionId("pending", sessionId: session.id)
        try store.saveTranscript(sessionId: session.id, items: [
            .agentMessage(id: "seed", text: "seed", isComplete: true)
        ])
        let driver = ChatTestDriver(
            handle: makeChatTestHandle(sessionId: "pending", didResume: true))
        let controller = ChatController(
            tabId: UUID(), agentId: "claude-acp", worktreeId: worktreeId,
            worktreePath: root.path, store: store, installStore: installStore,
            persistenceCoordinator: coordinator,
            driverFactory: { _, _, _, _, _, _, _ in driver })
        await controller.start()
        await writer.block(.transcript(session.id))
        controller.persist()
        let model = AppModel(
            paneRegistry: PaneRegistry(),
            activateApplication: {},
            persistenceCoordinator: coordinator)
        model.chatControllers[controller.tabId] = controller
        let flushTask = Task { await model.flushLiveScrollback() }
        await writer.waitUntilStarted(.transcript(session.id))
        await writer.release(.transcript(session.id))
        await flushTask.value

        let saved = await writer.transcripts(for: session.id)
        #expect(!saved.isEmpty)
    }

    @Test func quitFlushesControllerPersistenceConcurrently() async throws {
        let firstWorktreeId = UUID()
        let secondWorktreeId = UUID()
        let (firstStore, firstInstallStore, firstRoot) = try makeChatTestFixture(
            worktreeId: firstWorktreeId)
        let (secondStore, secondInstallStore, secondRoot) = try makeChatTestFixture(
            worktreeId: secondWorktreeId)
        defer {
            try? FileManager.default.removeItem(at: firstRoot)
            try? FileManager.default.removeItem(at: secondRoot)
        }
        let firstSession = try firstStore.createSession(
            worktreeId: firstWorktreeId.uuidString, agentId: "claude-acp")
        try firstStore.setACPSessionId("first", sessionId: firstSession.id)
        try firstStore.saveTranscript(sessionId: firstSession.id, items: [
            .agentMessage(id: "first-seed", text: "seed", isComplete: true)
        ])
        let secondSession = try secondStore.createSession(
            worktreeId: secondWorktreeId.uuidString, agentId: "claude-acp")
        try secondStore.setACPSessionId("second", sessionId: secondSession.id)
        try secondStore.saveTranscript(sessionId: secondSession.id, items: [
            .agentMessage(id: "second-seed", text: "seed", isComplete: true)
        ])
        let writer = TestPersistenceWriter()
        let coordinator = PersistenceCoordinator(
            transcriptWriter: { sessionId, items in
                try await writer.write(.transcript(sessionId), value: items)
            },
            scrollbackWriter: { _, _, _ in },
            onError: { _, _ in })
        let firstDriver = ChatTestDriver(
            handle: makeChatTestHandle(sessionId: "first", didResume: true))
        let secondDriver = ChatTestDriver(
            handle: makeChatTestHandle(sessionId: "second", didResume: true))
        let firstController = ChatController(
            tabId: UUID(), agentId: "claude-acp", worktreeId: firstWorktreeId,
            worktreePath: firstRoot.path, store: firstStore,
            installStore: firstInstallStore, persistenceCoordinator: coordinator,
            driverFactory: { _, _, _, _, _, _, _ in firstDriver })
        let secondController = ChatController(
            tabId: UUID(), agentId: "claude-acp", worktreeId: secondWorktreeId,
            worktreePath: secondRoot.path, store: secondStore,
            installStore: secondInstallStore, persistenceCoordinator: coordinator,
            driverFactory: { _, _, _, _, _, _, _ in secondDriver })
        await firstController.start()
        await secondController.start()
        await writer.block(.transcript(firstSession.id))
        await writer.block(.transcript(secondSession.id))
        firstController.persist()
        secondController.persist()

        let model = AppModel(
            paneRegistry: PaneRegistry(),
            activateApplication: {},
            persistenceCoordinator: coordinator)
        model.chatControllers[firstController.tabId] = firstController
        model.chatControllers[secondController.tabId] = secondController
        let flushTask = Task { await model.flushLiveScrollback() }
        for _ in 0..<100 {
            if await writer.hasStarted(.transcript(firstSession.id)),
               await writer.hasStarted(.transcript(secondSession.id)) {
                break
            }
            await Task.yield()
        }

        #expect(await writer.hasStarted(.transcript(firstSession.id)))
        #expect(await writer.hasStarted(.transcript(secondSession.id)))

        await writer.release(.transcript(firstSession.id))
        await writer.release(.transcript(secondSession.id))
        await flushTask.value
        let firstSaved = await writer.transcripts(for: firstSession.id)
        let secondSaved = await writer.transcripts(for: secondSession.id)
        #expect(!firstSaved.isEmpty)
        #expect(!secondSaved.isEmpty)
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
        let capture = ChatPaneLayoutCapture()
        let host = NSHostingView(rootView: ChatPaneLayoutProbe(capture: capture) {
            ChatPaneView(controller: controller, worktree: worktree, appModel: appModel)
                .environment(\.chatPaneLayoutCaptureEnabled, true)
        })
        host.setFrameSize(NSSize(width: 640, height: 480))
        host.layoutSubtreeIfNeeded()
        for _ in 0..<20 {
            if capture.frames[.approvalPanel] != nil, capture.frames[.composer] != nil { break }
            await Task.yield()
        }

        let approvalFrame = try #require(capture.frames[.approvalPanel])
        let composerFrame = try #require(capture.frames[.composer])
        #expect(approvalFrame.maxY <= composerFrame.minY)

        await driver.releasePrompt()
    }
}

@MainActor
extension ChatControllerTests {
    @Test func customTextQuestionSendsStructuredAnswer() async throws {
        let worktreeId = UUID()
        let (store, installStore, root) = try makeChatTestFixture(worktreeId: worktreeId)
        defer { try? FileManager.default.removeItem(at: root) }
        let driver = ChatTestDriver(
            handle: makeChatTestHandle(sessionId: "pi-session"),
            supportsStructuredAnswers: true)
        let controller = ChatController(
            tabId: UUID(), agentId: "pi", worktreeId: worktreeId,
            worktreePath: root.path, store: store, installStore: installStore,
            driverFactory: { _, _, _, _, _, _, _ in driver })
        await controller.start()

        var call = ToolCallItem(toolCallId: "pi-ui-input", title: "Branch",
                                kind: .other, status: .pending)
        call.rawInput = .object([
            "questions": .array([.object([
                "header": .string("Branch"), "question": .string("Branch name"),
                "options": .array([])
            ])]),
            "_tillerTextInput": .object(["placeholder": .string("feature/name")])
        ])
        call.permission = PermissionState(requestId: .string("ui-input"), options: [])
        let question = try #require(ChatQuestion.from(call))

        await controller.answerQuestion(question, text: " feature/pi ")

        #expect(await driver.permissionAnswers.last?.1 == .answered(
            optionId: "feature/pi",
            updatedInput: .object(["choice": .string("feature/pi")])))
    }
}

@MainActor
private final class ChatPaneLayoutCapture {
    var frames: [ChatPaneLayoutRole: CGRect] = [:]
}

private struct ChatPaneLayoutProbe<Content: View>: View {
    let capture: ChatPaneLayoutCapture
    let content: () -> Content

    var body: some View {
        content()
            .onPreferenceChange(ChatPaneLayoutPreferenceKey.self) { frames in
                capture.frames = frames
            }
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
    nonisolated let supportsStructuredAnswers: Bool
    private(set) var permissionAnswers: [(JSONRPCID, PermissionOutcome)] = []
    private var promptReleased = false
    private(set) var promptIsOpen = false
    private(set) var promptCount = 0
    private(set) var modeIds: [String] = []
    private(set) var resumeIds: [String?] = []
    private(set) var stopCount = 0

    init(handle: SessionHandle, failsResume: Bool = false,
         promptEvents: [ACPSessionEvent] = [], holdPromptOpen: Bool = false,
         supportsStructuredAnswers: Bool = false) {
        let (events, continuation) = AsyncStream.makeStream(of: ACPSessionEvent.self)
        self.events = events
        self.continuation = continuation
        self.handle = handle
        self.failsResume = failsResume
        self.promptEvents = promptEvents
        self.holdPromptOpen = holdPromptOpen
        self.supportsStructuredAnswers = supportsStructuredAnswers
    }

    func start() async throws {}
    func stop() async {
        stopCount += 1
        continuation.finish()
    }

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
        // Mirrors the real drivers: the turn closes through the stream, as
        // the last event behind the updates it terminates.
        continuation.yield(.turnEnded(.endTurn))
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
        permissionAnswers.append((requestId, outcome))
    }
}
