import Foundation
import Testing
import TillerCore
@testable import Tiller

@Suite(.serialized)
struct WorkspaceCheckpointQueueTests {
    @Test func selectionEnqueuesImmediatelyAndPointerReleasePersistsAtOnce() async {
        let clock = FakeClock()
        let recorder = SaveRecorder()
        let queue = makeQueue(clock: clock, recorder: recorder)
        await queue.enqueueSelection(revision: 1, snapshot: snapshot(title: "selection"))
        await queue.flush()
        await queue.enqueuePointerDividerRelease(revision: 2, snapshot: snapshot(title: "pointer"))
        await queue.flush()

        #expect(await recorder.values.map(\.revision) == [1, 2])
        #expect(await queue.isDirty == false)
    }

    @Test func keyboardDividerStepsCoalesceOnATwoHundredFiftyMillisecondTrailingEdge() async {
        let clock = FakeClock(blockDebounce: true)
        let recorder = SaveRecorder()
        let queue = makeQueue(clock: clock, recorder: recorder)
        await queue.enqueueKeyboardDividerStep(revision: 1, snapshot: snapshot(title: "first"))
        await queue.enqueueKeyboardDividerStep(revision: 2, snapshot: snapshot(title: "last"))
        await clock.releaseDebounce()
        await queue.flush()

        #expect(await recorder.values.map(\.revision) == [2])
        #expect(await clock.sleeps.contains(.milliseconds(250)))
    }

    @Test func aFailedSaveMarksDirtyWithoutRevertingTheVisibleState() async {
        let clock = FakeClock()
        let recorder = SaveRecorder(failures: 7)
        let queue = makeQueue(clock: clock, recorder: recorder)
        await queue.enqueue(revision: 5, snapshot: snapshot(title: "visible"))
        await queue.flush()
        #expect(await queue.isDirty == true)
        #expect(await recorder.values.last?.snapshot == snapshot(title: "visible"))
    }

    @Test func retriesFollowOneTwoFourEightSixteenThirty() async {
        let clock = FakeClock()
        let recorder = SaveRecorder(failures: 6)
        let queue = makeQueue(clock: clock, recorder: recorder)
        await queue.enqueue(revision: 1, snapshot: snapshot())
        await queue.flush()

        #expect(await clock.sleeps.filter { $0 != .milliseconds(250) } ==
                [.seconds(1), .seconds(2), .seconds(4), .seconds(8), .seconds(16), .seconds(30)])
    }

    @Test func laterMutationsReplaceTheQueuedPayloadRatherThanQueueingBehindIt() async {
        let clock = FakeClock()
        let recorder = LatestWinsSaveRecorder()
        let queue = WorkspaceCheckpointQueue(
            worktreeID: fixedWorktreeID,
            save: { id, revision, snapshot in
                try await recorder.save(id: id, revision: revision, snapshot: snapshot)
            }, clock: clock)
        await queue.enqueue(revision: 1, snapshot: snapshot(title: "old"))
        await recorder.waitForFirstStart()
        await queue.enqueue(revision: 2, snapshot: snapshot(title: "new"))
        await recorder.releaseFirstSave()
        await queue.flush()

        let values = await recorder.values
        #expect(!values.isEmpty)
        #expect(values.map(\.revision) == [1, 2])
        #expect(values.last?.snapshot == snapshot(title: "new"))
    }

    @Test func failureAndRecoveryAreNotifiedOnceEach() async {
        let clock = FakeClock()
        let recorder = SaveRecorder(failures: 1)
        let events = EventRecorder()
        let queue = WorkspaceCheckpointQueue(
            worktreeID: fixedWorktreeID, save: { id, revision, snapshot in
                try await recorder.save(id: id, revision: revision, snapshot: snapshot)
            }, clock: clock, onFailure: { _ in events.failure() }, onRecovery: { events.recovery() })
        await queue.enqueue(revision: 1, snapshot: snapshot())
        await queue.flush()

        #expect(events.failures == 1)
        #expect(events.recoveries == 1)
    }

    @Test func quitWaitsAtMostTwoSecondsThenWritesTheSidecar() async {
        let clock = FakeClock()
        let recorder = SaveRecorder(failures: 7)
        let sidecar = SidecarRecorder()
        let queue = makeQueue(clock: clock, recorder: recorder, sidecar: sidecar)
        await queue.enqueue(revision: 4, snapshot: snapshot(title: "quit"))
        await queue.flush()

        #expect(sidecar.values.last?.revision == 4)
    }

    @Test func aSidecarIsImportedOnlyWhenHashValidAndRevisionNewer() throws {
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("tiller-sidecar-\(UUID().uuidString)", isDirectory: true)
        defer { try? FileManager.default.removeItem(at: directory) }
        let id = fixedWorktreeID
        let value = try WorkspaceSnapshot(layout: WorkspaceLayout.empty()).canonicalPayload()
        let envelope = WorkspaceRecoverySidecarEnvelope(
            schemaVersion: 1, revision: 9, payload: String(decoding: value, as: UTF8.self),
            checksum: SHA256Hex.digest(value))
        try WorkspaceRecoverySidecar.write(envelope: envelope, worktreeID: id, directory: directory)
        #expect(try WorkspaceRecoverySidecar.read(worktreeID: id, directory: directory) == envelope)

        var invalid = envelope
        invalid = WorkspaceRecoverySidecarEnvelope(
            schemaVersion: invalid.schemaVersion, revision: invalid.revision,
            payload: invalid.payload + " ", checksum: invalid.checksum)
        try WorkspaceRecoverySidecar.write(envelope: invalid, worktreeID: id, directory: directory)
        #expect(throws: WorkspaceRecoverySidecarError.invalidChecksum) {
            try WorkspaceRecoverySidecar.read(worktreeID: id, directory: directory)
        }
    }

    private func makeQueue(clock: FakeClock, recorder: SaveRecorder,
                           sidecar: SidecarRecorder? = nil) -> WorkspaceCheckpointQueue {
        let sidecarWriter: WorkspaceCheckpointQueue.SidecarWriter? = sidecar.map { recorder in
            let writer: WorkspaceCheckpointQueue.SidecarWriter = { id, revision, snapshot in
                recorder.write(id: id, revision: revision, snapshot: snapshot)
            }
            return writer
        }
        return WorkspaceCheckpointQueue(
            worktreeID: fixedWorktreeID,
            save: { id, revision, snapshot in
                try await recorder.save(id: id, revision: revision, snapshot: snapshot)
            }, clock: clock,
            sidecarWriter: sidecarWriter)
    }

    private func snapshot(title: String = "terminal") -> WorkspaceSnapshot {
        let groupID = PaneGroupID(UUID(uuidString: "00000000-0000-4000-8000-000000000001")!)
        let tabID = WorkspaceTabID(UUID(uuidString: "00000000-0000-4000-8000-000000000002")!)
        let tab = WorkspaceTab(id: tabID, title: title, titleIsAutoNamed: true,
                               content: .terminal(TerminalContentID(tabID.rawValue)))
        let layout = try! WorkspaceLayout.make(
            root: .group(groupID), groups: [groupID: PaneGroup(id: groupID, tabs: [tab], activeTabID: tabID)],
            activeGroupID: groupID).get()
        return WorkspaceSnapshot(layout: layout)
    }

    private static let fixedWorktreeID = UUID(uuidString: "00000000-0000-4000-8000-000000000099")!
    private var fixedWorktreeID: UUID { Self.fixedWorktreeID }
}

private actor FakeClock: WorkspaceCheckpointClock {
    private let blockDebounce: Bool
    private(set) var sleeps: [Duration] = []
    private var debounceWaiters: [CheckedContinuation<Void, Never>] = []

    init(blockDebounce: Bool = false) { self.blockDebounce = blockDebounce }

    func sleep(for duration: Duration) async {
        sleeps.append(duration)
        guard blockDebounce && duration == .milliseconds(250) else { return }
        await withCheckedContinuation { debounceWaiters.append($0) }
    }

    func releaseDebounce() {
        let waiters = debounceWaiters
        debounceWaiters.removeAll()
        for waiter in waiters { waiter.resume() }
    }
}

private actor SaveRecorder {
    struct Value: Sendable, Equatable { let revision: Int; let snapshot: WorkspaceSnapshot }
    private var remainingFailures: Int
    private(set) var values: [Value] = []

    init(failures: Int = 0) { remainingFailures = failures }

    func save(id: UUID, revision: Int, snapshot: WorkspaceSnapshot) throws {
        values.append(Value(revision: revision, snapshot: snapshot))
        if remainingFailures > 0 {
            remainingFailures -= 1
            throw SaveError.failed
        }
    }
}

private actor LatestWinsSaveRecorder {
    private(set) var values: [SaveRecorder.Value] = []
    private var firstSaveStarted = false
    private var firstSaveReleased = false
    private var startWaiters: [CheckedContinuation<Void, Never>] = []
    private var releaseWaiters: [CheckedContinuation<Void, Never>] = []

    func waitForFirstStart() async {
        if firstSaveStarted { return }
        await withCheckedContinuation { startWaiters.append($0) }
    }

    func releaseFirstSave() {
        firstSaveReleased = true
        let waiters = releaseWaiters
        releaseWaiters.removeAll()
        for waiter in waiters { waiter.resume() }
    }

    func save(id: UUID, revision: Int, snapshot: WorkspaceSnapshot) async throws {
        values.append(SaveRecorder.Value(revision: revision, snapshot: snapshot))
        guard values.count == 1 else { return }
        firstSaveStarted = true
        let startWaiters = startWaiters
        self.startWaiters.removeAll()
        for waiter in startWaiters { waiter.resume() }
        if !firstSaveReleased {
            await withCheckedContinuation { releaseWaiters.append($0) }
        }
        throw SaveError.failed
    }
}

private final class EventRecorder: @unchecked Sendable {
    private let lock = NSLock()
    private var failureCount = 0
    private var recoveryCount = 0
    var failures: Int { lock.lock(); defer { lock.unlock() }; return failureCount }
    var recoveries: Int { lock.lock(); defer { lock.unlock() }; return recoveryCount }
    func failure() { lock.lock(); failureCount += 1; lock.unlock() }
    func recovery() { lock.lock(); recoveryCount += 1; lock.unlock() }
}

private final class SidecarRecorder: @unchecked Sendable {
    struct Value: Sendable { let revision: Int; let snapshot: WorkspaceSnapshot }
    private let lock = NSLock()
    private var storedValues: [Value] = []
    var values: [Value] { lock.lock(); defer { lock.unlock() }; return storedValues }
    func write(id: UUID, revision: Int, snapshot: WorkspaceSnapshot) {
        lock.lock(); storedValues.append(Value(revision: revision, snapshot: snapshot)); lock.unlock()
    }
}

private enum SaveError: Error { case failed }
