import Foundation

public enum GitError: Error, Equatable {
    case commandFailed(code: Int32, stderr: String)
    case outputTooLarge(maxBytes: Int, maxLines: Int)
}

extension GitError: LocalizedError {
    public var errorDescription: String? {
        switch self {
        case .commandFailed(let code, let stderr):
            let message = stderr.trimmingCharacters(in: .whitespacesAndNewlines)
            return message.isEmpty ? "Git exited with status \(code)." : message
        case .outputTooLarge(let maxBytes, let maxLines):
            return "Diff exceeds \(maxBytes) bytes or \(maxLines) lines."
        }
    }
}

public struct GitOutputLimits: Equatable, Sendable {
    public let maxBytes: Int
    public let maxLines: Int

    public init(maxBytes: Int, maxLines: Int) {
        self.maxBytes = maxBytes
        self.maxLines = maxLines
    }
}

public struct GitCommandResult: Equatable, Sendable {
    public let stdout: Data
    public let stderr: String
    public let exitCode: Int32

    public var stdoutString: String { String(decoding: stdout, as: UTF8.self) }
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

private final class CapturedOutputState: @unchecked Sendable {
    private let lock = NSLock()
    private var output = Data()
    private var error = Data()
    private var lineCount = 0
    private(set) var exceeded = false

    func appendStdout(_ data: Data, limits: GitOutputLimits?) -> Bool {
        lock.lock(); defer { lock.unlock() }
        guard !exceeded else { return false }
        let newLines = data.reduce(into: 0) { count, byte in
            if byte == 0x0A { count += 1 }
        }
        if let limits,
           output.count + data.count > limits.maxBytes || lineCount + newLines > limits.maxLines {
            exceeded = true
            return true
        }
        output.append(data)
        lineCount += newLines
        return false
    }

    func appendStderr(_ data: Data) {
        lock.lock(); defer { lock.unlock() }
        error.append(data)
    }

    func snapshot() -> (stdout: Data, stderr: String, exceeded: Bool) {
        lock.lock(); defer { lock.unlock() }
        return (output, String(decoding: error, as: UTF8.self), exceeded)
    }
}

private final class SendableProcessBox: @unchecked Sendable {
    let process: Process
    init(_ process: Process) { self.process = process }
}

/// Runs `git <arguments>` in a directory, capturing stdout. Async so callers
/// never block a UI thread on a slow filesystem.
public enum GitRunner {
    public static func run(_ arguments: [String], in directory: String) async throws -> String {
        try await runCaptured(arguments, in: directory).stdoutString
    }

    public static func runCaptured(
        _ arguments: [String],
        in directory: String,
        acceptedExitCodes: Set<Int32> = [0],
        limits: GitOutputLimits? = nil
    ) async throws -> GitCommandResult {
        try await withCheckedThrowingContinuation { continuation in
            DispatchQueue.global(qos: .userInitiated).async {
                let process = Process()
                let processBox = SendableProcessBox(process)
                process.executableURL = URL(fileURLWithPath: "/usr/bin/git")
                process.arguments = arguments
                process.currentDirectoryURL = URL(fileURLWithPath: directory)
                let stdout = Pipe()
                let stderr = Pipe()
                process.standardOutput = stdout
                process.standardError = stderr
                let state = CapturedOutputState()

                do {
                    try process.run()
                } catch {
                    continuation.resume(throwing: error)
                    return
                }

                let readers = DispatchGroup()
                readers.enter()
                DispatchQueue.global(qos: .userInitiated).async {
                    while true {
                        let data = stdout.fileHandleForReading.availableData
                        if data.isEmpty { break }
                        if state.appendStdout(data, limits: limits) {
                            processBox.process.terminate()
                        }
                    }
                    readers.leave()
                }
                readers.enter()
                DispatchQueue.global(qos: .userInitiated).async {
                    while true {
                        let data = stderr.fileHandleForReading.availableData
                        if data.isEmpty { break }
                        state.appendStderr(data)
                    }
                    readers.leave()
                }

                process.waitUntilExit()
                readers.wait()
                let snapshot = state.snapshot()
                if snapshot.exceeded, let limits {
                    continuation.resume(throwing: GitError.outputTooLarge(
                        maxBytes: limits.maxBytes, maxLines: limits.maxLines))
                } else if acceptedExitCodes.contains(process.terminationStatus) {
                    continuation.resume(returning: GitCommandResult(
                        stdout: snapshot.stdout,
                        stderr: snapshot.stderr,
                        exitCode: process.terminationStatus))
                } else {
                    continuation.resume(throwing: GitError.commandFailed(
                        code: process.terminationStatus, stderr: snapshot.stderr))
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
