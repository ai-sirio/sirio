import Foundation
import Testing
import TillerACP
import TillerCore
import TillerTerminal
@testable import Tiller

@Suite(.serialized)
struct PersistenceCoordinatorTests {
    @Test func rapidSnapshotsPersistNewestNotYetWritten() async {
        let writer = TestPersistenceWriter()
        await writer.block(.transcript("session-1"))
        let coordinator = makeCoordinator(writer: writer)

        await coordinator.enqueueTranscript(
            sessionId: "session-1",
            items: [.agentMessage(id: "a", text: "A", isComplete: true)])
        await writer.waitUntilStarted(.transcript("session-1"))
        await coordinator.enqueueTranscript(
            sessionId: "session-1",
            items: [.agentMessage(id: "b", text: "B", isComplete: true)])
        await coordinator.enqueueTranscript(
            sessionId: "session-1",
            items: [.agentMessage(id: "c", text: "C", isComplete: true)])
        await writer.release(.transcript("session-1"))
        await coordinator.flush(sessionId: "session-1")

        let values = await writer.transcripts(for: "session-1")
        #expect(values.map { $0.last?.id } == ["a", "c"])
    }

    @Test func flushWaitsForNewestWrite() async {
        let writer = TestPersistenceWriter()
        await writer.block(.transcript("session-1"))
        let coordinator = makeCoordinator(writer: writer)

        await coordinator.enqueueTranscript(
            sessionId: "session-1",
            items: [.agentMessage(id: "a", text: "A", isComplete: true)])
        await writer.waitUntilStarted(.transcript("session-1"))
        let flushTask = Task { await coordinator.flush(sessionId: "session-1") }
        try? await Task.sleep(for: .milliseconds(30))
        #expect(await writer.transcripts(for: "session-1").isEmpty)
        #expect(!flushTask.isCancelled)

        await writer.release(.transcript("session-1"))
        await flushTask.value
        #expect(await writer.transcripts(for: "session-1").map { $0.last?.id } == ["a"])
    }

    @Test func differentKeysDoNotBlockEachOther() async {
        let writer = TestPersistenceWriter()
        let blockedPane = UUID(uuidString: "00000000-0000-0000-0000-000000000001")!
        let freePane = UUID(uuidString: "00000000-0000-0000-0000-000000000002")!
        await writer.block(.transcript("blocked"))
        await writer.block(.scrollback(blockedPane))
        let coordinator = makeCoordinator(writer: writer)

        await coordinator.enqueueTranscript(
            sessionId: "blocked",
            items: [.agentMessage(id: "blocked", text: "B", isComplete: true)])
        await coordinator.enqueueTranscript(
            sessionId: "free",
            items: [.agentMessage(id: "free", text: "F", isComplete: true)])
        await writer.waitUntilStarted(.transcript("blocked"))
        await coordinator.flush(sessionId: "free")
        #expect(await writer.transcripts(for: "free").map { $0.last?.id } == ["free"])

        await coordinator.enqueueScrollback(
            worktreeId: UUID(), paneId: blockedPane, data: Data("blocked".utf8))
        await coordinator.enqueueScrollback(
            worktreeId: UUID(), paneId: freePane, data: Data("free".utf8))
        await writer.waitUntilStarted(.scrollback(blockedPane))
        await coordinator.flush(paneId: freePane)
        #expect(await writer.scrollbacks(for: freePane) == [Data("free".utf8)])

        await writer.release(.transcript("blocked"))
        await writer.release(.scrollback(blockedPane))
        await coordinator.flushAll()
    }

    @Test func writerFailureDoesNotWedgeLaterOrOtherKeys() async {
        let writer = TestPersistenceWriter()
        await writer.failNext(.transcript("bad"))
        let coordinator = makeCoordinator(writer: writer)

        await coordinator.enqueueTranscript(
            sessionId: "bad",
            items: [.agentMessage(id: "first", text: "1", isComplete: true)])
        await coordinator.flush(sessionId: "bad")
        await coordinator.enqueueTranscript(
            sessionId: "bad",
            items: [.agentMessage(id: "second", text: "2", isComplete: true)])
        await coordinator.enqueueTranscript(
            sessionId: "good",
            items: [.agentMessage(id: "good", text: "G", isComplete: true)])
        await coordinator.flushAll()

        #expect(await writer.transcripts(for: "bad").map { $0.last?.id } == ["second"])
        #expect(await writer.transcripts(for: "good").map { $0.last?.id } == ["good"])
        #expect(await writer.failureCount == 1)
    }


    private func makeCoordinator(writer: TestPersistenceWriter) -> PersistenceCoordinator {
        PersistenceCoordinator(
            transcriptWriter: { sessionId, items in
                try await writer.write(.transcript(sessionId), value: items)
            },
            scrollbackWriter: { _, paneId, data in
                try await writer.write(.scrollback(paneId), value: data)
            },
            onError: { _, _ in })
    }
}


enum TestWriteKey: Hashable, Sendable {
    case transcript(String)
    case scrollback(UUID)
}

actor TestPersistenceWriter {
    private var blocked: Set<TestWriteKey> = []
    private var released: Set<TestWriteKey> = []
    private var started: Set<TestWriteKey> = []
    private var waiters: [TestWriteKey: [CheckedContinuation<Void, Never>]] = [:]
    private var failures: Set<TestWriteKey> = []
    private(set) var failureCount = 0
    private var transcriptValues: [String: [[TranscriptItem]]] = [:]
    private var scrollbackValues: [UUID: [Data]] = [:]

    func block(_ key: TestWriteKey) { blocked.insert(key) }

    func failNext(_ key: TestWriteKey) { failures.insert(key) }

    func waitUntilStarted(_ key: TestWriteKey) async {
        if started.contains(key) { return }
        await withCheckedContinuation { continuation in
            waiters[key, default: []].append(continuation)
        }
    }
    func hasStarted(_ key: TestWriteKey) -> Bool {
        started.contains(key)
    }

    func release(_ key: TestWriteKey) {
        released.insert(key)
        for continuation in waiters.removeValue(forKey: key) ?? [] {
            continuation.resume()
        }
    }

    func write<T: Sendable>(_ key: TestWriteKey, value: T) async throws {
        started.insert(key)
        for continuation in waiters.removeValue(forKey: key) ?? [] {
            continuation.resume()
        }
        if blocked.contains(key) && !released.contains(key) {
            await withCheckedContinuation { continuation in
                waiters[key, default: []].append(continuation)
            }
        }
        if failures.remove(key) != nil {
            failureCount += 1
            throw TestError.failed
        }
        if let items = value as? [TranscriptItem], case .transcript(let sessionId) = key {
            transcriptValues[sessionId, default: []].append(items)
        } else if let data = value as? Data, case .scrollback(let paneId) = key {
            scrollbackValues[paneId, default: []].append(data)
        }
    }

    func transcripts(for sessionId: String) -> [[TranscriptItem]] {
        transcriptValues[sessionId, default: []]
    }
    func allTranscripts() -> [[TranscriptItem]] {
        transcriptValues.values.flatMap { $0 }
    }

    func scrollbacks(for paneId: UUID) -> [Data] {
        scrollbackValues[paneId, default: []]
    }
}

enum TestError: Error {
    case failed
}
