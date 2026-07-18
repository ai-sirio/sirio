import Foundation

/// ACP transport over a child process's stdin/stdout. Stderr lines are
/// forwarded to `onStderrLine` (diagnostics), never mixed into the protocol.
public final class ProcessTransport: ACPTransport, @unchecked Sendable {
    public struct LaunchFailure: Error { public let underlying: Error }

    private let process = Process()
    private let stdinPipe = Pipe()
    private let stdoutPipe = Pipe()
    private let stderrPipe = Pipe()
    private let onStderrLine: (@Sendable (String) -> Void)?
    private let lock = NSLock()
    private var lineContinuation: AsyncThrowingStream<Data, Error>.Continuation?
    private var stdoutBuffer = Data()
    private var stderrBuffer = Data()

    public init(executable: String, arguments: [String], cwd: String,
                environment: [String: String]? = nil,
                onStderrLine: (@Sendable (String) -> Void)? = nil) {
        process.executableURL = URL(fileURLWithPath: executable)
        process.arguments = arguments
        process.currentDirectoryURL = URL(fileURLWithPath: cwd)
        if let environment { process.environment = environment }
        process.standardInput = stdinPipe
        process.standardOutput = stdoutPipe
        process.standardError = stderrPipe
        self.onStderrLine = onStderrLine
    }

    public func start() async throws {
        stdoutPipe.fileHandleForReading.readabilityHandler = { [weak self] handle in
            self?.consumeStdout(handle.availableData)
        }
        stderrPipe.fileHandleForReading.readabilityHandler = { [weak self] handle in
            self?.consumeStderr(handle.availableData)
        }
        process.terminationHandler = { [weak self] _ in
            guard let self else { return }
            // Drain remaining buffered output, then finish the stream.
            self.consumeStdout(self.stdoutPipe.fileHandleForReading.availableData)
            self.lock.lock()
            let continuation = self.lineContinuation
            self.lineContinuation = nil
            self.lock.unlock()
            continuation?.finish()
        }
        do {
            try process.run()
        } catch {
            throw LaunchFailure(underlying: error)
        }
    }

    public func send(line: Data) async throws {
        try stdinPipe.fileHandleForWriting.write(contentsOf: line)
    }

    public func lines() -> AsyncThrowingStream<Data, Error> {
        AsyncThrowingStream { continuation in
            lock.lock()
            lineContinuation = continuation
            let buffered = drainBufferedLinesLocked()
            lock.unlock()
            for line in buffered { continuation.yield(line) }
            if !process.isRunning, process.processIdentifier != 0 {
                continuation.finish()
            }
        }
    }

    public func terminate() async {
        stdoutPipe.fileHandleForReading.readabilityHandler = nil
        stderrPipe.fileHandleForReading.readabilityHandler = nil
        if process.isRunning { process.terminate() }
    }

    private func consumeStdout(_ data: Data) {
        guard !data.isEmpty else { return }
        lock.lock()
        stdoutBuffer.append(data)
        let lines = drainBufferedLinesLocked()
        let continuation = lineContinuation
        lock.unlock()
        for line in lines { continuation?.yield(line) }
    }

    /// Caller must hold `lock`. Splits complete lines off `stdoutBuffer`.
    private func drainBufferedLinesLocked() -> [Data] {
        var lines: [Data] = []
        while let newline = stdoutBuffer.firstIndex(of: UInt8(ascii: "\n")) {
            lines.append(stdoutBuffer.subdata(in: stdoutBuffer.startIndex..<newline))
            stdoutBuffer.removeSubrange(stdoutBuffer.startIndex...newline)
        }
        return lines
    }

    private func consumeStderr(_ data: Data) {
        guard !data.isEmpty, let onStderrLine else { return }
        lock.lock()
        stderrBuffer.append(data)
        var lines: [String] = []
        while let newline = stderrBuffer.firstIndex(of: UInt8(ascii: "\n")) {
            let line = stderrBuffer.subdata(in: stderrBuffer.startIndex..<newline)
            lines.append(String(decoding: line, as: UTF8.self))
            stderrBuffer.removeSubrange(stderrBuffer.startIndex...newline)
        }
        lock.unlock()
        for line in lines { onStderrLine(line) }
    }
}
