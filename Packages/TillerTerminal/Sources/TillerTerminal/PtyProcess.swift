import Darwin
import Foundation
import os

public enum PtyError: Error, Equatable {
    case forkFailed(Int32)
    case alreadySpawned
}

/// Owns one PTY-backed child process: forkpty + execve, a DispatchSource
/// read loop on the master fd, and TIOCSWINSZ resizes. Deliberately has no
/// knowledge of libghostty — the bridge to a terminal surface lives in
/// PtyTerminalPane so agents (Fase 1+) can reuse PtyProcess headlessly.
///
/// `@unchecked Sendable` justification: `masterFD` and `processId` are
/// written once in `spawn()` (which throws on a second call) and closed only
/// on the private serial `queue` via `stopReadLoop()`. The read-loop event
/// handler touches the fd solely on `queue`. `write(_:)`/`resize(_:_:)`
/// after spawn only read the fd value; POSIX write/ioctl on a PTY fd are
/// themselves thread-safe, though a write racing `stopReadLoop` gets EBADF
/// (logged, accepted). `exitCode`, `exitReported`, and `onExit` are mutated
/// only on `queue` — `terminate()` dispatches `reapChild()` and
/// `stopReadLoop()` via `queue.async`; the EOF retry chain runs on `queue`.
/// `onExit` is set once before `spawn()` per the API contract. No shared
/// mutable state escapes unsynchronized.
public final class PtyProcess: @unchecked Sendable {
    public private(set) var processId: pid_t = -1
    public private(set) var exitCode: Int32? = nil
    /// Set before spawn(). Called exactly once, off the main thread, when the
    /// child has been reaped. Normal exit → WEXITSTATUS; killed by signal →
    /// 128 + signal number (shell convention).
    /// Set before spawn(); setting it after spawn races with exit delivery.
    public var onExit: (@Sendable (Int32) -> Void)?
    private var exitReported = false
    private var masterFD: Int32 = -1
    private let onOutput: @Sendable (Data) -> Void
    private var readSource: DispatchSourceRead?
    private let queue = DispatchQueue(label: "tiller.pty.read")

    public init(onOutput: @escaping @Sendable (Data) -> Void) {
        self.onOutput = onOutput
    }

    deinit {
        // exitReported is mutated only on queue; at deinit no other strong refs exist,
        // so a plain read is safe. Guards against signaling a recycled PID.
        if processId > 0, !exitReported {
            kill(processId, SIGKILL)
            var status: Int32 = 0
            waitpid(processId, &status, 0)  // blocking OK in deinit — no remaining work
        }
        readSource?.cancel()
        if masterFD >= 0 { close(masterFD) }
    }

    public func spawn(
        executable: String,
        arguments: [String],
        environment: [String],
        workingDirectory: String? = nil,
        initialCols: UInt16,
        initialRows: UInt16
    ) throws {
        guard masterFD < 0 else { throw PtyError.alreadySpawned }
        var size = winsize(ws_row: initialRows, ws_col: initialCols, ws_xpixel: 0, ws_ypixel: 0)
        var master: Int32 = -1
        // Built before fork(): strdup/Array/String allocate, and allocating
        // in the child after fork() can deadlock on a malloc lock inherited
        // mid-acquire from another thread in this (multithreaded) process.
        var argv: [UnsafeMutablePointer<CChar>?] = ([executable] + arguments).map { strdup($0) }
        argv.append(nil)
        var envp: [UnsafeMutablePointer<CChar>?] = environment.map { strdup($0) }
        envp.append(nil)
        let pid = forkpty(&master, nil, nil, &size)
        if pid < 0 { throw PtyError.forkFailed(errno) }
        if pid == 0 {
            // Child: exec immediately — nothing async-signal-unsafe between
            // fork and exec (Foundation/ObjC calls here can deadlock).
            // chdir is async-signal-safe.
            if let dir = workingDirectory {
                dir.withCString { _ = chdir($0) }
            }
            execve(executable, argv, envp)
            _exit(127)
        }
        processId = pid
        masterFD = master
        startReadLoop()
    }

    public func write(_ data: Data) {
        guard masterFD >= 0 else { return }
        data.withUnsafeBytes { buf in
            var offset = 0
            while offset < buf.count {
                let n = Darwin.write(masterFD, buf.baseAddress!.advanced(by: offset), buf.count - offset)
                if n < 0 && errno == EINTR { continue }
                if n <= 0 {
                    Self.logger.error("pty write failed after \(offset)/\(buf.count) bytes: errno \(errno)")
                    break
                }
                offset += n
            }
        }
    }

    private static let logger = Logger(subsystem: "dev.tiller", category: "pty")

    public func resize(cols: UInt16, rows: UInt16) {
        guard masterFD >= 0 else { return }
        var size = winsize(ws_row: rows, ws_col: cols, ws_xpixel: 0, ws_ypixel: 0)
        _ = ioctl(masterFD, TIOCSWINSZ, &size)
    }

    private func reapChild() {
        guard processId > 0, !exitReported else { return }
        var status: Int32 = 0
        let reaped = waitpid(processId, &status, WNOHANG)
        guard reaped == processId else { return }
        reportExit(status: status)
    }

    private func reportExit(status: Int32) {
        guard !exitReported else { return }
        exitReported = true
        let code: Int32 = (status & 0x7f) == 0
            ? (status >> 8) & 0xff          // WEXITSTATUS
            : 128 + (status & 0x7f)         // died by signal
        exitCode = code
        onExit?(code)
    }


    private func reapChildWithRetry(attempt: Int = 0) {
        reapChild()
        guard !exitReported, processId > 0 else { return }
        if attempt >= 20 {
            var status: Int32 = 0
            if waitpid(processId, &status, 0) == processId { reportExit(status: status) }
            return
        }
        queue.asyncAfter(deadline: .now() + .milliseconds(50)) { [weak self] in
            self?.reapChildWithRetry(attempt: attempt + 1)
        }
    }

    public func terminate() {
        if processId > 0 { kill(processId, SIGTERM) }
        queue.async { [weak self] in
            self?.stopReadLoop()
            self?.reapChild()
        }
    }

    private func startReadLoop() {
        let source = DispatchSource.makeReadSource(fileDescriptor: masterFD, queue: queue)
        source.setEventHandler { [weak self] in
            guard let self, self.masterFD >= 0 else { return }
            let sid = SignpostMetrics.makeSignpostID()
            let state = SignpostMetrics.beginInterval("ptyIngest", id: sid)
            var buffer = [UInt8](repeating: 0, count: 64 * 1024)
            let n = read(self.masterFD, &buffer, buffer.count)
            if n > 0 {
                self.onOutput(Data(buffer[0..<n]))
                SignpostMetrics.endInterval("ptyIngest", state, message: "bytes: \(n)")
            } else {
                SignpostMetrics.endInterval("ptyIngest", state, message: "eof")
                self.stopReadLoop()
                self.reapChildWithRetry()
            }
        }
        source.resume()
        readSource = source
    }

    /// MUST be called on `queue`. Closes `masterFD` and cancels `readSource`.
    private func stopReadLoop() {
        readSource?.cancel()
        readSource = nil
        if masterFD >= 0 {
            close(masterFD)
            masterFD = -1
        }
    }
}
