import Testing
import Foundation
@testable import TillerTerminal

@Test func stormCoalescesToSingleSettle() async throws {
    let collected = SettleCollector()
    let debouncer = ResizeDebouncer(interval: .milliseconds(30))
    debouncer.onSettle = { cols, rows in Task { await collected.append(cols: cols, rows: rows) } }
    for c in UInt16(80)...UInt16(99) {
        debouncer.push(cols: c, rows: 24)
    }
    try await Task.sleep(for: .milliseconds(250))
    #expect(await collected.events == [[99, 24]])
}

@Test func separateGesturesSettleSeparately() async throws {
    let collected = SettleCollector()
    let debouncer = ResizeDebouncer(interval: .milliseconds(30))
    debouncer.onSettle = { cols, rows in Task { await collected.append(cols: cols, rows: rows) } }
    debouncer.push(cols: 100, rows: 30)
    try await Task.sleep(for: .milliseconds(150))
    debouncer.push(cols: 120, rows: 40)
    try await Task.sleep(for: .milliseconds(150))
    #expect(await collected.events == [[100, 30], [120, 40]])
}

@Test func cancelDropsPendingSettle() async throws {
    let collected = SettleCollector()
    let debouncer = ResizeDebouncer(interval: .milliseconds(30))
    debouncer.onSettle = { cols, rows in Task { await collected.append(cols: cols, rows: rows) } }
    debouncer.push(cols: 100, rows: 30)
    debouncer.cancel()
    try await Task.sleep(for: .milliseconds(150))
    #expect(await collected.events.isEmpty)
}

@Test func runtimeDoesNotSpawnBeforeFirstSettledResize() async throws {
    let paneId = UUID()
    let runtime = PtyRuntime(paneId: paneId)
    runtime.start()
    try await Task.sleep(for: .milliseconds(150))
    #expect(await PaneRegistry.shared.isRegistered(paneId: paneId) == false)
    runtime.stop()
}

@Test func runtimeSpawnsOnFirstSettledResize() async throws {
    let paneId = UUID()
    let runtime = PtyRuntime(paneId: paneId)
    runtime.start()
    runtime.handleSettledResize(cols: 120, rows: 40)
    var registered = false
    for _ in 0..<40 {
        if await PaneRegistry.shared.isRegistered(paneId: paneId) { registered = true; break }
        try await Task.sleep(for: .milliseconds(50))
    }
    #expect(registered)
    runtime.stop()
}

@Test func stopBeforeResizePreventsSpawn() async throws {
    let paneId = UUID()
    let runtime = PtyRuntime(paneId: paneId)
    runtime.start()
    runtime.stop()
    runtime.handleSettledResize(cols: 120, rows: 40)
    try await Task.sleep(for: .milliseconds(150))
    #expect(await PaneRegistry.shared.isRegistered(paneId: paneId) == false)
}

private actor SettleCollector {
    private(set) var events: [[UInt16]] = []
    func append(cols: UInt16, rows: UInt16) { events.append([cols, rows]) }
}
