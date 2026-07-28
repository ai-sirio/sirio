import Testing
import Foundation
import os
@testable import TillerTerminal

@Test func spawnCapturesOutput() async throws {
    let collected = OutputCollector()
    let pty = PtyProcess { data in Task { await collected.append(data) } }
    try pty.spawn(
        executable: "/bin/sh",
        arguments: ["-c", "printf tiller-pty-ok"],
        environment: ["PATH=/usr/bin:/bin"],
        initialCols: 80,
        initialRows: 24
    )
    // Poll up to 2s for the marker to arrive through the PTY.
    for _ in 0..<40 {
        if await collected.text.contains("tiller-pty-ok") { break }
        try await Task.sleep(for: .milliseconds(50))
    }
    #expect(await collected.text.contains("tiller-pty-ok"))
    pty.terminate()
}

@Test func writeReachesChildStdin() async throws {
    let collected = OutputCollector()
    let pty = PtyProcess { data in Task { await collected.append(data) } }
    try pty.spawn(
        executable: "/bin/cat",
        arguments: [],
        environment: ["PATH=/usr/bin:/bin"],
        initialCols: 80,
        initialRows: 24
    )
    pty.write(Data("echo-me\n".utf8))
    for _ in 0..<40 {
        if await collected.text.contains("echo-me") { break }
        try await Task.sleep(for: .milliseconds(50))
    }
    #expect(await collected.text.contains("echo-me"))
    pty.terminate()
}
@Test func spawnTwiceThrowsError() async throws {
    let pty = PtyProcess { _ in }
    try pty.spawn(
        executable: "/bin/sh",
        arguments: ["-c", "true"],
        environment: ["PATH=/usr/bin:/bin"],
        initialCols: 80,
        initialRows: 24
    )
    defer { pty.terminate() }
    #expect(throws: PtyError.alreadySpawned) {
        try pty.spawn(
            executable: "/bin/sh",
            arguments: ["-c", "true"],
            environment: ["PATH=/usr/bin:/bin"],
            initialCols: 80,
            initialRows: 24
        )
    }
}

@Test func spawnHonorsWorkingDirectory() async throws {
    let dir = NSTemporaryDirectory() + "tiller-cwd-" + UUID().uuidString
    try FileManager.default.createDirectory(atPath: dir, withIntermediateDirectories: true)
    defer { try? FileManager.default.removeItem(atPath: dir) }
    let collected = OutputCollector()
    let pty = PtyProcess { data in Task { await collected.append(data) } }
    try pty.spawn(
        executable: "/bin/sh",
        arguments: ["-c", "pwd"],
        environment: ["PATH=/usr/bin:/bin"],
        workingDirectory: dir,
        initialCols: 80,
        initialRows: 24
    )
    for _ in 0..<40 {
        if await collected.text.contains("tiller-cwd-") { break }
        try await Task.sleep(for: .milliseconds(50))
    }
    #expect(await collected.text.contains("tiller-cwd-"))
    pty.terminate()
}

actor OutputCollector {
    private(set) var text = ""
    func append(_ data: Data) { text += String(decoding: data, as: UTF8.self) }
}

@Test func manyConcurrentSpawnsDoNotDeadlock() async throws {
    // Regression guard: building argv/envp via strdup *after* fork() can
    // deadlock a child on an inherited malloc lock in a multithreaded
    // process. Many concurrent forkpty() calls reproduce the malloc
    // contention that exposes the race.
    try await withThrowingTaskGroup(of: Void.self) { group in
        for _ in 0..<30 {
            group.addTask {
                let collected = OutputCollector()
                let pty = PtyProcess { data in Task { await collected.append(data) } }
                try pty.spawn(
                    executable: "/bin/sh",
                    arguments: ["-c", "printf tiller-pty-ok"],
                    environment: ["PATH=/usr/bin:/bin"],
                    initialCols: 80,
                    initialRows: 24
                )
                for _ in 0..<40 {
                    if await collected.text.contains("tiller-pty-ok") { break }
                    try await Task.sleep(for: .milliseconds(50))
                }
                #expect(await collected.text.contains("tiller-pty-ok"))
                pty.terminate()
            }
        }
        try await group.waitForAll()
    }
}

@Test func onExitReportsExitCode() async throws {
    let pty = PtyProcess { _ in }
    let exitCode = await withCheckedContinuation { (cont: CheckedContinuation<Int32, Never>) in
        pty.onExit = { code in cont.resume(returning: code) }
        try? pty.spawn(
            executable: "/bin/sh",
            arguments: ["-c", "exit 7"],
            environment: ["PATH=/usr/bin:/bin"],
            initialCols: 80,
            initialRows: 24
        )
    }
    #expect(exitCode == 7)
}


@Test func onExitFiresExactlyOnceAcrossTerminateAndEof() async throws {
    // Verify that both the EOF path and terminate() can call onExit,
    // but it fires exactly once. Wait briefly after spawn so the child
    // (sh -c "exit 0") exits naturally and the EOF handler fires/reaps
    // before terminate() is called — terminate's reapChild sees
    // exitReported=true and is a no-op.
    let lock = OSAllocatedUnfairLock<Int>(initialState: 0)
    let pty = PtyProcess { _ in }
    pty.onExit = { _ in lock.withLock { $0 += 1 } }
    try pty.spawn(
        executable: "/bin/sh",
        arguments: ["-c", "exit 0"],
        environment: ["PATH=/usr/bin:/bin"],
        initialCols: 80,
        initialRows: 24
    )
    try await Task.sleep(for: .milliseconds(200))
    pty.terminate()
    try await Task.sleep(for: .milliseconds(500))
    #expect(lock.withLock({ $0 }) == 1)
}

@Test func terminateDuringEofStorm() async throws {
    // Stress test: 50 iterations of spawn-immediate-exit then
    // terminate. Before queue-confinement this could double-close
    // masterFD. Wait briefly after spawn so the child exits and
    // the EOF path has a chance to fire before terminate().
    for iteration in 0..<50 {
        let exited = OSAllocatedUnfairLock<Bool>(initialState: false)
        let pty = PtyProcess { _ in }
        pty.onExit = { _ in exited.withLock { $0 = true } }
        try pty.spawn(
            executable: "/bin/sh",
            arguments: ["-c", "true"],
            environment: ["PATH=/usr/bin:/bin"],
            initialCols: 80,
            initialRows: 24
        )
        try await Task.sleep(for: .milliseconds(100))
        pty.terminate()
        for _ in 0..<40 {
            if exited.withLock({ $0 }) { break }
            try await Task.sleep(for: .milliseconds(50))
        }
        let reported = exited.withLock { $0 }
        #expect(reported, "iteration \(iteration): exit must be reported within 2s")
    }
}

@Test func deinitAfterReapDoesNotBlockOrSignal() async throws {
    let deadline = ContinuousClock.now
    do {
        let exited = OSAllocatedUnfairLock<Bool>(initialState: false)
        let pty = PtyProcess { _ in }
        pty.onExit = { _ in exited.withLock { $0 = true } }
        try pty.spawn(
            executable: "/bin/sh",
            arguments: ["-c", "exit 5"],
            environment: ["PATH=/usr/bin:/bin"],
            initialCols: 80,
            initialRows: 24
        )
        for _ in 0..<40 {
            if exited.withLock({ $0 }) { break }
            try await Task.sleep(for: .milliseconds(50))
        }
        let reported = exited.withLock { $0 }
        #expect(reported)
    }
    // The invariant is that deinit-after-reap does not BLOCK — i.e. it never waits on
    // the child. The elapsed time also covers the exit-polling loop above, which is
    // allowed to spend 40 * 50ms = 2s, so a 1s bound was tighter than the test's own
    // budget and went red whenever the machine was loaded (ci.sh tests every package
    // in parallel). Bound it above that budget instead: a real block would hang, not
    // land near 2s.
    let elapsed = ContinuousClock.now - deadline
    #expect(elapsed < .seconds(4))
}
