import Testing
import Foundation
@testable import TillerTerminal

@Test func writeAndSnapshotRoundTrip() async {
    let registry = PaneRegistry()
    let paneId = UUID()
    let buffer = ScrollbackBuffer(capacity: 1024)
    let received = SendableBox()
    let pty = PtyProcess { data in Task { await received.append(data) } }
    await registry.register(paneId: paneId, pty: pty, scrollback: buffer)
    #expect(await registry.isRegistered(paneId: paneId))
    await buffer.append(Data("hello".utf8))
    #expect(await registry.snapshot(paneId: paneId) == Data("hello".utf8))
    #expect(await registry.snapshot(paneId: UUID()) == nil)
}

@Test func waitExitReturnsCodeAfterMarkExited() async {
    let registry = PaneRegistry()
    let paneId = UUID()
    await registry.register(paneId: paneId, pty: PtyProcess { _ in }, scrollback: ScrollbackBuffer())
    let task = Task { await registry.waitExit(paneId: paneId, timeoutMs: nil) }
    try? await Task.sleep(for: .milliseconds(50))
    await registry.markExited(paneId: paneId, code: 7)
    #expect(await task.value == 7)
}

@Test func waitExitTimesOutWithNil() async {
    let registry = PaneRegistry()
    let paneId = UUID()
    await registry.register(paneId: paneId, pty: PtyProcess { _ in }, scrollback: ScrollbackBuffer())
    #expect(await registry.waitExit(paneId: paneId, timeoutMs: 50) == nil)
}

@Test func waitExitAfterExitReturnsImmediately() async {
    let registry = PaneRegistry()
    let paneId = UUID()
    await registry.register(paneId: paneId, pty: PtyProcess { _ in }, scrollback: ScrollbackBuffer())
    await registry.markExited(paneId: paneId, code: 0)
    #expect(await registry.waitExit(paneId: paneId, timeoutMs: nil) == 0)
}

/// Regression: a short timeout must resolve its own waiter, not a long-waiter.
@Test func shortTimeoutDoesNotStealLongWaiter() async {
    let registry = PaneRegistry()
    let paneId = UUID()
    await registry.register(paneId: paneId, pty: PtyProcess { _ in }, scrollback: ScrollbackBuffer())

    let tracker = WaiterTracker()

    Task {
        let result = await registry.waitExit(paneId: paneId, timeoutMs: 5000)
        await tracker.setA(result)
    }
    // Let A park before starting B.
    try? await Task.sleep(for: .milliseconds(50))

    Task {
        let result = await registry.waitExit(paneId: paneId, timeoutMs: 100)
        await tracker.setB(result)
    }

    // Wait for B's timeout (100 ms) with generous margin.
    try? await Task.sleep(for: .milliseconds(300))

    let bState = await tracker.b
    #expect(bState.isDone)
    #expect(bState.result == nil)

    let aState = await tracker.a
    #expect(!aState.isDone)

    await registry.markExited(paneId: paneId, code: 3)
    try? await Task.sleep(for: .milliseconds(100))

    let aFinal = await tracker.a
    #expect(aFinal.isDone)
    #expect(aFinal.result == 3)
}

/// unregister drains any parked waiter with the last known exit code.
@Test func unregisterDrainsWaitersWithKnownCode() async {
    let registry = PaneRegistry()

    // Pane 1: waiter parks, markExited resolves it, second waiter gets code immediately.
    let paneId1 = UUID()
    await registry.register(paneId: paneId1, pty: PtyProcess { _ in }, scrollback: ScrollbackBuffer())
    let task1 = Task { await registry.waitExit(paneId: paneId1, timeoutMs: nil) }
    try? await Task.sleep(for: .milliseconds(50))
    await registry.markExited(paneId: paneId1, code: 0)
    #expect(await task1.value == 0)
    #expect(await registry.waitExit(paneId: paneId1, timeoutMs: nil) == 0)

    // Pane 2: unregister while waiter is parked drains it with nil (no exit code set).
    let paneId2 = UUID()
    await registry.register(paneId: paneId2, pty: PtyProcess { _ in }, scrollback: ScrollbackBuffer())
    let task2 = Task { await registry.waitExit(paneId: paneId2, timeoutMs: nil) }
    try? await Task.sleep(for: .milliseconds(50))
    await registry.unregister(paneId: paneId2)
    #expect(await task2.value == nil)
}

@Test func waitUntilRegisteredTimesOut() async {
    let registry = PaneRegistry()
    let paneId = UUID()

    #expect(await registry.waitUntilRegistered(paneId: paneId, timeoutMs: 20) == false)
}

@Test func waitUntilRegisteredResumesWhenPaneRegisters() async {
    let registry = PaneRegistry()
    let paneId = UUID()
    let waiter = Task { await registry.waitUntilRegistered(paneId: paneId, timeoutMs: 1_000) }

    try? await Task.sleep(for: .milliseconds(20))
    await registry.register(
        paneId: paneId,
        pty: PtyProcess { _ in },
        scrollback: ScrollbackBuffer()
    )

    #expect(await waiter.value)
}

@Test func cancelBeforeRegisterPreventsLateRegistration() async {
    let registry = PaneRegistry()
    let paneId = UUID()
    let waiter = Task { await registry.waitUntilRegistered(paneId: paneId, timeoutMs: 1_000) }

    try? await Task.sleep(for: .milliseconds(20))
    await registry.cancelRegistration(paneId: paneId)
    await registry.unregister(paneId: paneId)
    await registry.register(
        paneId: paneId,
        pty: PtyProcess { _ in },
        scrollback: ScrollbackBuffer()
    )

    #expect(await waiter.value == false)
    #expect(await registry.isRegistered(paneId: paneId) == false)
}

@Test func cancelAfterRegisterRemovesPaneAndDrainsExitWaiters() async {
    let registry = PaneRegistry()
    let paneId = UUID()
    await registry.register(
        paneId: paneId,
        pty: PtyProcess { _ in },
        scrollback: ScrollbackBuffer()
    )
    let exitWaiter = Task { await registry.waitExit(paneId: paneId, timeoutMs: nil) }

    try? await Task.sleep(for: .milliseconds(20))
    await registry.cancelRegistration(paneId: paneId)

    #expect(await registry.isRegistered(paneId: paneId) == false)
    #expect(await exitWaiter.value == nil)
}

@Test func normalUnregisterDoesNotTombstonePaneId() async {
    let registry = PaneRegistry()
    let paneId = UUID()
    await registry.register(
        paneId: paneId,
        pty: PtyProcess { _ in },
        scrollback: ScrollbackBuffer()
    )

    await registry.unregister(paneId: paneId)
    await registry.register(
        paneId: paneId,
        pty: PtyProcess { _ in },
        scrollback: ScrollbackBuffer()
    )

    #expect(await registry.isRegistered(paneId: paneId))
}

@Test func rejectedLateRegistrationConsumesCancellationTombstone() async {
    let registry = PaneRegistry()
    let paneId = UUID()
    await registry.cancelRegistration(paneId: paneId)

    await registry.register(
        paneId: paneId,
        pty: PtyProcess { _ in },
        scrollback: ScrollbackBuffer()
    )
    #expect(await registry.isRegistered(paneId: paneId) == false)

    await registry.register(
        paneId: paneId,
        pty: PtyProcess { _ in },
        scrollback: ScrollbackBuffer()
    )
    #expect(await registry.isRegistered(paneId: paneId))
}

@Test func cancellationTombstonesHaveBoundedRetention() async {
    let registry = PaneRegistry()
    let oldestPaneId = UUID()
    await registry.cancelRegistration(paneId: oldestPaneId)
    for _ in 0..<1_024 {
        await registry.cancelRegistration(paneId: UUID())
    }

    await registry.register(
        paneId: oldestPaneId,
        pty: PtyProcess { _ in },
        scrollback: ScrollbackBuffer()
    )

    #expect(await registry.isRegistered(paneId: oldestPaneId))
}

@Test func cancellingInfiniteExitWaitResumesPromptly() async {
    let registry = PaneRegistry()
    let paneId = UUID()
    let tracker = CancellationTracker()
    await registry.register(
        paneId: paneId, pty: PtyProcess { _ in }, scrollback: ScrollbackBuffer()
    )
    let waiter = Task {
        let result = await registry.waitExit(paneId: paneId, timeoutMs: nil)
        await tracker.finish(result)
        return result
    }
    try? await Task.sleep(for: .milliseconds(20))

    waiter.cancel()
    try? await Task.sleep(for: .milliseconds(20))
    #expect(await tracker.finished)

    await registry.unregister(paneId: paneId)
    _ = await waiter.value
}

private actor CancellationTracker {
    private(set) var finished = false
    func finish(_ result: Int32?) { finished = true }
}

/// Tracks completion of two waiters for the regression test.
actor WaiterTracker {
    struct State {
        var isDone = false
        var result: Int32?
    }
    private var _a = State()
    private var _b = State()

    var a: State { _a }
    var b: State { _b }

    func setA(_ result: Int32?) { _a = State(isDone: true, result: result) }
    func setB(_ result: Int32?) { _b = State(isDone: true, result: result) }
}

/// Tiny helper: actor-isolated byte sink for output callbacks.
actor SendableBox {
    private(set) var data = Data()
    func append(_ d: Data) { data.append(d) }
}
