import Foundation

public enum GitError: Error, Equatable {
    case commandFailed(code: Int32, stderr: String)
}

/// Thread-safe mutable state for streaming stderr line-by-line.
/// Locked internally — safe to call from the readability handler.
private final class StreamingState: @unchecked Sendable {
    private let lock = NSLock()
    private var buffer = ""
    private(set) var captured = ""

    /// Appends a chunk, returns any completed lines (split on \r or \n).
    func append(_ chunk: String) -> [String] {
        lock.lock(); defer { lock.unlock() }
        buffer += chunk
        captured += chunk
        var completed: [String] = []
        var current = ""
        for char in buffer {
            if char == "\r" || char == "\n" {
                if !current.isEmpty { completed.append(current) }
                current = ""
            } else {
                current.append(char)
            }
        }
        buffer = current
        return completed
    }
}

/// Runs `git <arguments>` in a directory, capturing stdout. Async so callers
/// never block a UI thread on a slow filesystem.
public enum GitRunner {
    public static func run(_ arguments: [String], in directory: String) async throws -> String {
        try await withCheckedThrowingContinuation { continuation in
            DispatchQueue.global(qos: .userInitiated).async {
                let process = Process()
                process.executableURL = URL(fileURLWithPath: "/usr/bin/git")
                process.arguments = arguments
                process.currentDirectoryURL = URL(fileURLWithPath: directory)
                let stdout = Pipe(), stderr = Pipe()
                process.standardOutput = stdout
                process.standardError = stderr
                do {
                    try process.run()
                } catch {
                    continuation.resume(throwing: error)
                    return
                }
                let outData = stdout.fileHandleForReading.readDataToEndOfFile()
                let errData = stderr.fileHandleForReading.readDataToEndOfFile()
                process.waitUntilExit()
                if process.terminationStatus == 0 {
                    continuation.resume(returning: String(decoding: outData, as: UTF8.self))
                } else {
                    continuation.resume(throwing: GitError.commandFailed(
                        code: process.terminationStatus,
                        stderr: String(decoding: errData, as: UTF8.self)
                    ))
                }
            }
        }
    }

    /// Like `run`, but streams stderr line-by-line to `onLine` as it arrives
    /// instead of buffering it. Git writes progress updates (e.g. clone
    /// percentages) to stderr using `\r` for in-place refresh — those count
    /// as line boundaries here, same as `\n`.
    public static func runStreaming(
        _ arguments: [String], in directory: String,
        onLine: @escaping @Sendable (String) -> Void
    ) async throws {
        try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<Void, Error>) in
            DispatchQueue.global(qos: .userInitiated).async {
                let process = Process()
                process.executableURL = URL(fileURLWithPath: "/usr/bin/git")
                process.arguments = arguments
                process.currentDirectoryURL = URL(fileURLWithPath: directory)
                let stderr = Pipe()
                process.standardOutput = Pipe()
                process.standardError = stderr

                let state = StreamingState()

                stderr.fileHandleForReading.readabilityHandler = { handle in
                    let data = handle.availableData
                    guard !data.isEmpty else { return }
                    let chunk = String(decoding: data, as: UTF8.self)
                    let completedLines = state.append(chunk)
                    for line in completedLines { onLine(line) }
                }

                do {
                    try process.run()
                } catch {
                    stderr.fileHandleForReading.readabilityHandler = nil
                    continuation.resume(throwing: error)
                    return
                }
                process.waitUntilExit()
                stderr.fileHandleForReading.readabilityHandler = nil
                if process.terminationStatus == 0 {
                    continuation.resume(returning: ())
                } else {
                    continuation.resume(throwing: GitError.commandFailed(
                        code: process.terminationStatus, stderr: state.captured
                    ))
                }
            }
        }
    }
}
