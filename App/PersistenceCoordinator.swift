import Foundation
import OSLog
import TillerACP

/// Serializes off-main persistence while coalescing snapshots that have not
/// started writing yet. Each session and pane has an independent worker so a
/// slow database write cannot stall unrelated persistence keys.
actor PersistenceCoordinator {
    enum Operation: Sendable, Equatable {
        case transcript(sessionId: String)
        case scrollback(worktreeId: UUID, paneId: UUID)
    }

    typealias TranscriptWriter = @Sendable (String, [TranscriptItem]) async throws -> Void
    typealias ScrollbackWriter = @Sendable (UUID, UUID, Data) async throws -> Void
    typealias ErrorHandler = @Sendable (Operation, String) -> Void

    private let transcriptWriter: TranscriptWriter
    private let scrollbackWriter: ScrollbackWriter
    private let onError: ErrorHandler
    private var transcriptQueues: [String: TranscriptQueue] = [:]
    private var scrollbackQueues: [UUID: ScrollbackQueue] = [:]

    init(
        transcriptWriter: @escaping TranscriptWriter,
        scrollbackWriter: @escaping ScrollbackWriter,
        onError: @escaping ErrorHandler = { operation, message in
            Logger(subsystem: "dev.tiller", category: "persistence")
                .error("Persistence failed for \(String(describing: operation), privacy: .public): \(message, privacy: .public)")
        }
    ) {
        self.transcriptWriter = transcriptWriter
        self.scrollbackWriter = scrollbackWriter
        self.onError = onError
    }

    func enqueueTranscript(sessionId: String, items: [TranscriptItem]) async {
        let queue = transcriptQueues[sessionId] ?? TranscriptQueue(
            sessionId: sessionId, writer: transcriptWriter, onError: onError)
        transcriptQueues[sessionId] = queue
        await queue.enqueue(Array(items))
    }

    func enqueueScrollback(worktreeId: UUID, paneId: UUID, data: Data) async {
        let queue = scrollbackQueues[paneId] ?? ScrollbackQueue(
            worktreeId: worktreeId, paneId: paneId,
            writer: scrollbackWriter, onError: onError)
        scrollbackQueues[paneId] = queue
        await queue.enqueue(Data(data))
    }

    func flush(sessionId: String) async {
        await transcriptQueues[sessionId]?.flush()
    }

    func flush(paneId: UUID) async {
        await scrollbackQueues[paneId]?.flush()
    }

    func flushAll() async {
        let transcriptQueues = Array(transcriptQueues.values)
        let scrollbackQueues = Array(scrollbackQueues.values)
        await withTaskGroup(of: Void.self) { group in
            for queue in transcriptQueues {
                group.addTask { await queue.flush() }
            }
            for queue in scrollbackQueues {
                group.addTask { await queue.flush() }
            }
        }
    }
}

private actor TranscriptQueue {
    private let sessionId: String
    private let writer: PersistenceCoordinator.TranscriptWriter
    private let onError: PersistenceCoordinator.ErrorHandler
    private var pending: [TranscriptItem]?
    private var isWriting = false
    private var flushWaiters: [CheckedContinuation<Void, Never>] = []

    init(
        sessionId: String,
        writer: @escaping PersistenceCoordinator.TranscriptWriter,
        onError: @escaping PersistenceCoordinator.ErrorHandler
    ) {
        self.sessionId = sessionId
        self.writer = writer
        self.onError = onError
    }

    func enqueue(_ items: [TranscriptItem]) {
        pending = items
        startNextIfNeeded()
    }

    func flush() async {
        guard isWriting || pending != nil else { return }
        await withCheckedContinuation { continuation in
            flushWaiters.append(continuation)
        }
    }

    private func startNextIfNeeded() {
        guard !isWriting, let items = pending else {
            if !isWriting { resumeFlushWaiters() }
            return
        }
        pending = nil
        isWriting = true
        let sessionId: String = self.sessionId
        let writer: PersistenceCoordinator.TranscriptWriter = self.writer
        let onError: PersistenceCoordinator.ErrorHandler = self.onError
        Task {
            do {
                try await Self.performWrite(writer: writer, sessionId: sessionId, items: items)
            } catch {
                onError(.transcript(sessionId: sessionId), String(describing: error))
            }
            await self.writeFinished()
        }
    }

    private nonisolated static func performWrite(
        writer: PersistenceCoordinator.TranscriptWriter,
        sessionId: String,
        items: [TranscriptItem]
    ) async throws {
        try await writer(sessionId, items)
    }

    private func writeFinished() {
        isWriting = false
        startNextIfNeeded()
    }

    private func resumeFlushWaiters() {
        guard !flushWaiters.isEmpty else { return }
        let waiters = flushWaiters
        flushWaiters.removeAll()
        for waiter in waiters { waiter.resume() }
    }
}

private actor ScrollbackQueue {
    private let worktreeId: UUID
    private let paneId: UUID
    private let writer: PersistenceCoordinator.ScrollbackWriter
    private let onError: PersistenceCoordinator.ErrorHandler
    private var pending: Data?
    private var isWriting = false
    private var flushWaiters: [CheckedContinuation<Void, Never>] = []

    init(
        worktreeId: UUID,
        paneId: UUID,
        writer: @escaping PersistenceCoordinator.ScrollbackWriter,
        onError: @escaping PersistenceCoordinator.ErrorHandler
    ) {
        self.worktreeId = worktreeId
        self.paneId = paneId
        self.writer = writer
        self.onError = onError
    }

    func enqueue(_ data: Data) {
        pending = data
        startNextIfNeeded()
    }

    func flush() async {
        guard isWriting || pending != nil else { return }
        await withCheckedContinuation { continuation in
            flushWaiters.append(continuation)
        }
    }

    private func startNextIfNeeded() {
        guard !isWriting, let data = pending else {
            if !isWriting { resumeFlushWaiters() }
            return
        }
        pending = nil
        isWriting = true
        let worktreeId: UUID = self.worktreeId
        let paneId: UUID = self.paneId
        let writer: PersistenceCoordinator.ScrollbackWriter = self.writer
        let onError: PersistenceCoordinator.ErrorHandler = self.onError
        Task {
            do {
                try await Self.performWrite(
                    writer: writer, worktreeId: worktreeId, paneId: paneId, data: data)
            } catch {
                onError(
                    .scrollback(worktreeId: worktreeId, paneId: paneId),
                    String(describing: error))
            }
            await self.writeFinished()
        }
    }

    private nonisolated static func performWrite(
        writer: PersistenceCoordinator.ScrollbackWriter,
        worktreeId: UUID,
        paneId: UUID,
        data: Data
    ) async throws {
        try await writer(worktreeId, paneId, data)
    }

    private func writeFinished() {
        isWriting = false
        startNextIfNeeded()
    }

    private func resumeFlushWaiters() {
        guard !flushWaiters.isEmpty else { return }
        let waiters = flushWaiters
        flushWaiters.removeAll()
        for waiter in waiters { waiter.resume() }
    }
}
