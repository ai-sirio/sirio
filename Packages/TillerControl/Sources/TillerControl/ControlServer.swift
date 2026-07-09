import Darwin
import Foundation
import os

/// NDJSON control server on a unix domain socket. Each connection may send
/// multiple newline-delimited ControlRequest lines; each gets exactly one
/// ControlResponse line (order per-connection is request order).
///
/// Exception: a malformed-line error response may overtake in-flight chained
/// responses (the client sent garbage mid-pipeline).
///
/// `@unchecked Sendable` justification: all mutable state (listenFD,
/// acceptSource, connections dict, per-ConnectionState readSource, buffer,
/// responseChain) is confined to the private serial `queue`; the handler is
/// @Sendable and invoked from Tasks. No shared mutable state escapes
/// unsynchronized.
public final class ControlServer: @unchecked Sendable {
    public typealias Handler = @Sendable (ControlRequest) async -> ControlResponse

    private let socketPath: String
    private let handler: Handler
    private let queue = DispatchQueue(label: "tiller.control.server")
    private var listenFD: Int32 = -1
    private var acceptSource: DispatchSourceRead?
    private var connections: [Int32: ConnectionState] = [:]
    private let logger = Logger(subsystem: "dev.tiller", category: "control")

    private final class ConnectionState {
        let fd: Int32
        var readSource: DispatchSourceRead?
        var buffer = Data()
        /// Serial task chain that ensures responses are written in request
        /// order. Read/written only on `queue`.
        var responseChain: Task<Void, Never>?

        init(fd: Int32) {
            self.fd = fd
        }
    }

    public init(socketPath: String, handler: @escaping Handler) {
        self.socketPath = socketPath
        self.handler = handler
    }

    deinit {
        // Best-effort cleanup; stop() should be called explicitly.
        // At deinit time no other reference exists, so listenFD and
        // socketPath are safe to access without queue sync.
        if listenFD >= 0 {
            close(listenFD)
            listenFD = -1
        }
        if !socketPath.isEmpty {
            Darwin.unlink(socketPath)
        }
    }

    // MARK: - Lifecycle

    /// Maximum bytes in `sockaddr_un.sun_path` including null terminator (Darwin).
    private static let maxSocketPathLength = MemoryLayout<sockaddr_un>.size - MemoryLayout.offset(of: \sockaddr_un.sun_path)!
    /// Per-connection input buffer cap (1 MiB). Prevents local memory DoS.
    private static let maxBufferBytes = 1 << 20

    public func start() throws {
        try queue.sync {
            // Remove stale socket if present.
            Darwin.unlink(socketPath)
            guard socketPath.utf8CString.count <= Self.maxSocketPathLength else {
                throw ControlServerError.pathTooLong(socketPath)
            }

            let fd = Darwin.socket(AF_UNIX, SOCK_STREAM, 0)
            guard fd >= 0 else { throw ControlServerError.socketFailed(errno) }
            listenFD = fd

            try bindSocket()
            if Darwin.chmod(socketPath, mode_t(0o600)) != 0 {
                logger.error("chmod 0o600 failed for \(self.socketPath): \(String(cString: Darwin.strerror(errno)))")
            }
            try postBindSanityCheck()

            guard Darwin.listen(fd, 16) == 0 else {
                let savedErrno = errno
                close(fd); listenFD = -1
                Darwin.unlink(socketPath)
                throw ControlServerError.listenFailed(savedErrno)
            }

            let source = DispatchSource.makeReadSource(fileDescriptor: fd, queue: queue)
            source.setEventHandler { [weak self] in
                self?.acceptConnection()
            }
            source.resume()
            acceptSource = source
        }
    }

    public func stop() {
        queue.async { [weak self] in
            guard let self else { return }
            guard listenFD >= 0 else { return } // idempotent

            acceptSource?.cancel()
            acceptSource = nil
            close(listenFD)
            listenFD = -1

            for (_, conn) in connections {
                conn.readSource?.cancel()
                conn.readSource = nil
                close(conn.fd)
            }
            connections.removeAll()

            Darwin.unlink(socketPath)
        }
    }

    // MARK: - Bind

    private func bindSocket() throws {
        let fd = listenFD
        var addr = sockaddr_un()
        addr.sun_family = sa_family_t(AF_UNIX)
        withUnsafeMutableBytes(of: &addr.sun_path) { pathBuf in
            let len = min(socketPath.utf8CString.count, pathBuf.count)
            socketPath.utf8CString.withUnsafeBytes { srcBuf in
                pathBuf.copyMemory(from: UnsafeRawBufferPointer(start: srcBuf.baseAddress!, count: len))
            }
        }
        let addrLen = socklen_t(MemoryLayout<sockaddr_un>.size)
        let rc = withUnsafePointer(to: &addr) {
            $0.withMemoryRebound(to: sockaddr.self, capacity: 1) {
                Darwin.bind(fd, $0, addrLen)
            }
        }
        guard rc == 0 else {
            let savedErrno = errno
            close(fd); listenFD = -1
            throw ControlServerError.bindFailed(savedErrno)
        }
    }

    /// Verifies the bound socket is a socket owned by the current user.
    private func postBindSanityCheck() throws {
        var s = stat()
        guard stat(socketPath, &s) == 0 else {
            throw ControlServerError.insecureSocket("stat failed: \(String(cString: Darwin.strerror(errno)))")
        }
        guard (s.st_mode & S_IFMT) == S_IFSOCK else {
            throw ControlServerError.insecureSocket("not a socket")
        }
        guard s.st_uid == Darwin.getuid() else {
            throw ControlServerError.insecureSocket("wrong owner")
        }
    }

    // MARK: - Accept

    private func acceptConnection() {
        let fd = listenFD
        var addr = sockaddr_un()
        var addrLen = socklen_t(MemoryLayout<sockaddr_un>.size)
        let clientFD = withUnsafeMutablePointer(to: &addr) {
            $0.withMemoryRebound(to: sockaddr.self, capacity: 1) {
                Darwin.accept(fd, $0, &addrLen)
            }
        }
        guard clientFD >= 0 else {
            logger.error("accept failed: errno \(errno)")
            return
        }

        let state = ConnectionState(fd: clientFD)
        let source = DispatchSource.makeReadSource(fileDescriptor: clientFD, queue: queue)
        source.setEventHandler { [weak self] in
            self?.handleRead(for: state)
        }
        source.resume()
        state.readSource = source
        connections[clientFD] = state
    }

    // MARK: - Read

    private func handleRead(for state: ConnectionState) {
        var buf = [UInt8](repeating: 0, count: 64 * 1024)
        let n = Darwin.read(state.fd, &buf, buf.count)
        if n > 0 {
            state.buffer.append(Data(buf[0..<n]))
            if state.buffer.count > Self.maxBufferBytes {
                let response = ControlResponse.failure(id: "?", error: "request line too large")
                _ = writeLine(response, fd: state.fd)
                closeConnection(state)
                return
            }
            processBuffer(state)
        } else {
            // EOF (n == 0) or error — tear down the connection.
            closeConnection(state)
        }
    }

    /// Splits the accumulated buffer on `0x0A` and dispatches each complete
    /// line to `handleLine`. Handles:
    ///   - multiple requests packed in one read
    ///   - one request split across multiple reads
    private func processBuffer(_ state: ConnectionState) {
        while let nl = state.buffer.firstIndex(of: 0x0A) {
            let line = state.buffer[..<nl]
            state.buffer.removeSubrange(...nl)
            handleLine(Data(line), state: state)
        }
    }

    // MARK: - Dispatch

    private func handleLine(_ lineData: Data, state: ConnectionState) {
        guard !lineData.isEmpty else { return }

        do {
            let request = try ControlFraming.decodeRequest(line: lineData)
            let previous = state.responseChain
            let fd = state.fd
            // Note: Task closure only captures Sendable values (request,
            // previous, fd) to avoid data-race warnings — `state` itself
            // is confined to `queue` and not needed inside the task.
            state.responseChain = Task { [self, previous, fd, request] in
                // FIFO: wait for the prior response to finish.
                _ = await previous?.value
                let response = await handler(request)
                queue.async { [self, fd] in
                    guard connections[fd] != nil else { return }
                    if !writeLine(response, fd: fd) {
                        guard let conn = connections[fd] else { return }
                        closeConnection(conn)
                    }
                }
            }
        } catch {
            let response = ControlResponse.failure(id: "?", error: "malformed request: \(error.localizedDescription)")
            if !writeLine(response, fd: state.fd) {
                guard let conn = connections[state.fd] else { return }
                closeConnection(conn)
            }
        }
    }

    // MARK: - Write

    /// Full-write loop with EINTR retry. MUST be called on `queue`.
    /// Returns `true` if the full response was written, `false` on failure.
    @discardableResult
    private func writeLine(_ response: ControlResponse, fd: Int32) -> Bool {
        guard let data = try? ControlFraming.encodeLine(response) else {
            logger.error("failed to encode response")
            return false
        }
        return data.withUnsafeBytes { buf in
            var offset = 0
            while offset < buf.count {
                let n = Darwin.write(fd, buf.baseAddress!.advanced(by: offset), buf.count - offset)
                if n < 0 && errno == EINTR { continue }
                if n <= 0 {
                    if errno == EPIPE {
                        logger.info("write failed: client disconnected")
                    } else {
                        logger.error("write failed after \(offset)/\(buf.count) bytes: errno \(errno)")
                    }
                    return false
                }
                offset += n
            }
            return true
        }
    }

    // MARK: - Connection teardown

    private func closeConnection(_ state: ConnectionState) {
        state.readSource?.cancel()
        state.readSource = nil
        close(state.fd)
        connections[state.fd] = nil
    }
}

// MARK: - Errors

enum ControlServerError: Error, CustomStringConvertible {
    case socketFailed(Int32)
    case bindFailed(Int32)
    case listenFailed(Int32)
    case pathTooLong(String)
    case insecureSocket(String)

    var description: String {
        switch self {
        case .socketFailed(let e):     return "socket: \(String(cString: strerror(e)))"
        case .bindFailed(let e):       return "bind: \(String(cString: strerror(e)))"
        case .listenFailed(let e):     return "listen: \(String(cString: strerror(e)))"
        case .pathTooLong(let p):      return "socket path too long (\(p.utf8CString.count) bytes, max 104): \(p)"
        case .insecureSocket(let msg): return "insecure socket: \(msg)"
        }
    }
}
