import Foundation
import TillerCore

protocol WorkspaceCheckpointClock: Sendable {
    func sleep(for duration: Duration) async
}

struct SystemWorkspaceCheckpointClock: WorkspaceCheckpointClock {
    func sleep(for duration: Duration) async {
        try? await Task.sleep(for: duration)
    }
}

enum WorkspaceCheckpointTrigger: Sendable {
    case immediate
    case keyboardDividerStep
    case accessibilityDividerStep
}

actor WorkspaceCheckpointQueue {
    typealias Save = @Sendable (UUID, Int, WorkspaceSnapshot) async throws -> Void
    typealias SidecarWriter = @Sendable (UUID, Int, WorkspaceSnapshot) throws -> Void

    private struct Item: Sendable {
        let revision: Int
        let snapshot: WorkspaceSnapshot
    }

    private let worktreeID: UUID
    private let save: Save
    private let clock: any WorkspaceCheckpointClock
    private let retrySchedule: [Duration]
    private let onFailure: @Sendable (any Error) -> Void
    private let onRecovery: @Sendable () -> Void
    private let sidecarWriter: SidecarWriter?
    private var pending: Item?
    private var latest: Item?
    private var worker: Task<Void, Never>?
    private var debounceGeneration = 0
    private var failureWasNotified = false
    private(set) var isDirty = false

    init(worktreeID: UUID, save: @escaping Save,
         clock: any WorkspaceCheckpointClock,
         retrySchedule: [Duration] = [.seconds(1), .seconds(2), .seconds(4), .seconds(8), .seconds(16), .seconds(30)],
         onFailure: @escaping @Sendable (any Error) -> Void = { _ in },
         onRecovery: @escaping @Sendable () -> Void = {},
         sidecarWriter: SidecarWriter? = nil) {
        self.worktreeID = worktreeID
        self.save = save
        self.clock = clock
        self.retrySchedule = retrySchedule
        self.onFailure = onFailure
        self.onRecovery = onRecovery
        self.sidecarWriter = sidecarWriter
    }

    init(worktreeID: UUID, persistence: WorkspaceLayoutPersistence,
         clock: any WorkspaceCheckpointClock,
         retrySchedule: [Duration] = [.seconds(1), .seconds(2), .seconds(4), .seconds(8), .seconds(16), .seconds(30)],
         onFailure: @escaping @Sendable (any Error) -> Void = { _ in },
         onRecovery: @escaping @Sendable () -> Void = {},
         sidecarDirectory: URL? = nil) {
        self.init(
            worktreeID: worktreeID,
            save: { id, revision, snapshot in
                await persistence.checkpoint(worktreeID: id, revision: revision, snapshot: snapshot)
                try await persistence.flush(worktreeID: id)
            },
            clock: clock, retrySchedule: retrySchedule,
            onFailure: onFailure, onRecovery: onRecovery,
            sidecarWriter: { id, revision, snapshot in
                try WorkspaceRecoverySidecar.write(
                    worktreeID: id, revision: revision, snapshot: snapshot, directory: sidecarDirectory)
            })
    }

    func enqueue(revision: Int, snapshot: WorkspaceSnapshot,
                 trigger: WorkspaceCheckpointTrigger = .immediate) {
        let item = Item(revision: revision, snapshot: snapshot)
        latest = item
        pending = item
        switch trigger {
        case .immediate:
            debounceGeneration += 1
            startWorker()
        case .keyboardDividerStep, .accessibilityDividerStep:
            debounceGeneration += 1
            let generation = debounceGeneration
            Task { [weak self] in
                guard let self else { return }
                await self.clock.sleep(for: .milliseconds(250))
                await self.fireDebounced(generation: generation)
            }
        }
    }

    func enqueueSelection(revision: Int, snapshot: WorkspaceSnapshot) {
        enqueue(revision: revision, snapshot: snapshot, trigger: .immediate)
    }

    func enqueuePointerDividerRelease(revision: Int, snapshot: WorkspaceSnapshot) {
        enqueue(revision: revision, snapshot: snapshot, trigger: .immediate)
    }

    func enqueueKeyboardDividerStep(revision: Int, snapshot: WorkspaceSnapshot) {
        enqueue(revision: revision, snapshot: snapshot, trigger: .keyboardDividerStep)
    }

    func enqueueAccessibilityDividerStep(revision: Int, snapshot: WorkspaceSnapshot) {
        enqueue(revision: revision, snapshot: snapshot, trigger: .accessibilityDividerStep)
    }

    func flush() async {
        debounceGeneration += 1
        startWorker()
        if let worker, !isDirty {
            await worker.value
        } else if let worker {
            worker.cancel()
            await worker.value
        }
        if isDirty, let item = latest, let sidecarWriter {
            try? sidecarWriter(worktreeID, item.revision, item.snapshot)
        }
    }

    private func fireDebounced(generation: Int) {
        guard generation == debounceGeneration else { return }
        startWorker()
    }

    private func startWorker() {
        guard worker == nil else { return }
        guard pending != nil else { return }
        worker = Task { [weak self] in await self?.runWorker() }
    }

    private func runWorker() async {
        defer { worker = nil }
        guard var item = pending else { return }
        pending = nil
        var retryIndex = 0
        while !Task.isCancelled {
            do {
                try await save(worktreeID, item.revision, item.snapshot)
                if isDirty { isDirty = false; onRecovery() }
                failureWasNotified = false
                if let replacement = pending {
                    item = replacement
                    pending = nil
                    retryIndex = 0
                    continue
                }
                return
            } catch {
                isDirty = true
                if !failureWasNotified {
                    failureWasNotified = true
                    onFailure(error)
                }
                guard retryIndex < retrySchedule.count else {
                    pending = pending ?? item
                    return
                }
                await clock.sleep(for: retrySchedule[retryIndex])
                retryIndex += 1
                if let replacement = pending {
                    item = replacement
                    pending = nil
                }
            }
        }
        pending = pending ?? item
    }
}
