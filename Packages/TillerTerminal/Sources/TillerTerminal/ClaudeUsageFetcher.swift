import Foundation
import TillerCore
import os

// MARK: - PTY abstraction

/// Injectable seam for the PTY used by `ClaudeUsageFetcher`.
protocol UsagePty: AnyObject {
    func spawn(executable: String, arguments: [String], environment: [String],
               initialCols: UInt16, initialRows: UInt16) throws
    func write(_ data: Data)
    func terminate()
}

/// Production adapter that wraps `PtyProcess` behind the `UsagePty` protocol.
final class PtyProcessUsageAdapter: UsagePty {
    private let pty: PtyProcess

    init(onOutput: @escaping @Sendable (Data) -> Void) {
        pty = PtyProcess(onOutput: onOutput)
    }

    func spawn(executable: String, arguments: [String], environment: [String],
               initialCols: UInt16, initialRows: UInt16) throws {
        try pty.spawn(executable: executable, arguments: arguments,
                       environment: environment,
                       initialCols: initialCols, initialRows: initialRows)
    }

    func write(_ data: Data) { pty.write(data) }
    func terminate() { pty.terminate() }
}

// MARK: - Thread-safe buffer

/// Lock-guarded synchronous data accumulator.
final class Buffer: @unchecked Sendable {
    private let lock = OSAllocatedUnfairLock(initialState: Data())

    func append(_ chunk: Data) { lock.withLock { $0.append(chunk) } }
    func text() -> String { lock.withLock { String(decoding: $0, as: UTF8.self) } }
}

// MARK: - Fetcher

/// Drives a hidden `claude` PTY: spawn via the login shell, wait for the TUI to
/// settle, send `/usage`, and parse the rendered panel. Ported from Orca's
/// `claude-pty.ts`, macOS-only.
public enum ClaudeUsageFetcher {
    private static let defaultTimeout: TimeInterval = 25
    private static let defaultSettle: Duration = .seconds(2)
    private static let defaultPoll: Duration = .milliseconds(300)

    public static func fetch() async -> UsageFetchOutcome {
        await fetch(
            makePty: { PtyProcessUsageAdapter(onOutput: $0) },
            settle: defaultSettle,
            poll: defaultPoll,
            timeout: defaultTimeout
        )
    }

    /// Internal overload that injects the PTY seam and timing parameters for
    /// testing. `makePty` receives an `onOutput` closure that the returned PTY
    /// invokes each time it produces output bytes.
    static func fetch(
        makePty: @escaping @Sendable (@escaping @Sendable (Data) -> Void) -> any UsagePty,
        settle: Duration,
        poll: Duration,
        timeout: TimeInterval
    ) async -> UsageFetchOutcome {
        let buffer = Buffer()
        let pty = makePty { chunk in buffer.append(chunk) }
        defer { pty.terminate() }

        let shell = ProcessInfo.processInfo.environment["SHELL"] ?? "/bin/zsh"
        let env = ProcessInfo.processInfo.environment
            .merging(["TERM": "xterm-256color"]) { _, new in new }
            .map { "\($0.key)=\($0.value)" }

        do {
            try pty.spawn(
                executable: shell,
                arguments: ["-l", "-c", "claude"],
                environment: env,
                initialCols: 120,
                initialRows: 40
            )
        } catch {
            return .unavailable(.notInstalled)
        }

        try? await Task.sleep(for: settle)
        pty.write(Data("/usage\r".utf8))

        var paletteConfirmed = false
        let deadline = Date().addingTimeInterval(timeout)
        while Date() < deadline {
            let text = buffer.text()
            let lower = text.lowercased()

            if lower.contains("command not found") { return .unavailable(.notInstalled) }
            if lower.contains("failed to load usage") { return .unavailable(.error) }
            if lower.contains("please run /login") || lower.contains("not logged in")
                || lower.contains("invalid api key") {
                return .unavailable(.loggedOut)
            }
            // A command-palette prompt ("Show plan usage limits") needs one Enter.
            if !paletteConfirmed,
               lower.range(of: #"show plan|usage limits"#, options: .regularExpression) != nil {
                pty.write(Data("\r".utf8))
                paletteConfirmed = true
            }
            if let usage = parseClaudeUsage(text) { return .success(usage) }

            try? await Task.sleep(for: poll)
        }
        return .timedOut
    }
}
