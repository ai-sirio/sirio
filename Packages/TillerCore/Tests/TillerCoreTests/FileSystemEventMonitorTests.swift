import Foundation
import Testing
@testable import TillerCore

private actor EventPaths {
    private var values: [String] = []
    func append(_ urls: [URL]) { values.append(contentsOf: urls.map(\.path)) }
    func contains(_ suffix: String) -> Bool { values.contains { $0.hasSuffix(suffix) } }
    func count() -> Int { values.count }
}


private func wait(_ semaphore: DispatchSemaphore, timeout: TimeInterval) -> DispatchTimeoutResult {
    semaphore.wait(timeout: .now() + timeout)
}

private func makeMonitorRoot(_ label: String) throws -> URL {
    let root = FileManager.default.temporaryDirectory
        .appendingPathComponent("tiller-fsevents-\(label)-\(UUID().uuidString)", isDirectory: true)
    try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
    return root
}

@Test func monitorDeliversNestedAndSecondRootEvents() async throws {
    let worktree = try makeMonitorRoot("worktree")
    let gitDir = try makeMonitorRoot("gitdir")
    defer {
        try? FileManager.default.removeItem(at: worktree)
        try? FileManager.default.removeItem(at: gitDir)
    }
    let monitor = try #require(FileSystemEventMonitor(roots: [worktree, gitDir], latency: 0.05))
    defer { monitor.stop() }
    let paths = EventPaths()
    let reader = Task {
        for await batch in monitor.events { await paths.append(batch) }
    }
    defer { reader.cancel() }

    let nested = worktree.appendingPathComponent("Sources", isDirectory: true)
    try FileManager.default.createDirectory(at: nested, withIntermediateDirectories: true)
    try "x".write(to: nested.appendingPathComponent("A.swift"), atomically: true, encoding: .utf8)
    try "index".write(to: gitDir.appendingPathComponent("index"), atomically: true, encoding: .utf8)

    for _ in 0..<60 {
        let sawNested = await paths.contains("Sources/A.swift")
        let sawIndex = await paths.contains("index")
        if sawNested && sawIndex { break }
        try await Task.sleep(for: .milliseconds(50))
    }
    let sawNestedFile = await paths.contains("Sources/A.swift")
    let sawGitIndex = await paths.contains("index")
    #expect(sawNestedFile)
    #expect(sawGitIndex)
}

@Test func stoppedMonitorFinishesStreamAndDeliversNoNewEvents() async throws {
    let root = try makeMonitorRoot("stop")
    defer { try? FileManager.default.removeItem(at: root) }
    let monitor = try #require(FileSystemEventMonitor(roots: [root], latency: 0.05))
    let paths = EventPaths()
    let reader = Task {
        for await batch in monitor.events { await paths.append(batch) }
    }

    monitor.stop()
    _ = await reader.result
    let countAfterStop = await paths.count()
    try "late".write(to: root.appendingPathComponent("late.txt"), atomically: true, encoding: .utf8)
    try await Task.sleep(for: .milliseconds(200))
    let finalCount = await paths.count()
    #expect(finalCount == countAfterStop)
}


@Test func callbackGateSerializesStopWithActiveDelivery() async throws {
    let gate = EventCallbackGate()
    let entered = DispatchSemaphore(value: 0)
    let release = DispatchSemaphore(value: 0)
    let callbackCompleted = DispatchSemaphore(value: 0)
    Task.detached {
        _ = gate.withDelivery {
            entered.signal()
            release.wait()
        }
        callbackCompleted.signal()
    }

    #expect(wait(entered, timeout: 1) == .success)
    let stopStarted = DispatchSemaphore(value: 0)
    let stopCompleted = DispatchSemaphore(value: 0)
    Task.detached {
        stopStarted.signal()
        gate.stop()
        stopCompleted.signal()
    }
    #expect(wait(stopStarted, timeout: 1) == .success)
    #expect(wait(stopCompleted, timeout: 0.05) == .timedOut)

    release.signal()
    #expect(wait(callbackCompleted, timeout: 1) == .success)
    #expect(wait(stopCompleted, timeout: 1) == .success)
    #expect(gate.withDelivery {} == false)
}
