import Darwin
import Foundation

public enum ControlClientError: Error, CustomStringConvertible {
    case connectFailed(String)
    case io(String)
    case badResponse

    public var description: String {
        switch self {
        case .connectFailed(let detail): return "connect: \(detail)"
        case .io(let detail):            return "io: \(detail)"
        case .badResponse:               return "bad response"
        }
    }
}

public enum ControlClient {
    /// Blocking request/response over a fresh connection. Used by tillerctl (a
    /// short-lived CLI — synchronous by design) and by contract tests.
    ///
    /// - Parameters:
    ///   - socketPath: Path to the unix domain socket.
    ///   - request: The request to send.
    ///   - timeoutSeconds: Receive timeout for `SO_RCVTIMEO`.
    /// - Returns: The decoded response.
    public static func roundTrip(
        socketPath: String,
        request: ControlRequest,
        timeoutSeconds: Int = 3600
    ) throws -> ControlResponse {
        let sunPathCapacity = MemoryLayout<sockaddr_un>.size - MemoryLayout.offset(of: \sockaddr_un.sun_path)!
        guard socketPath.utf8CString.count <= sunPathCapacity else {
            throw ControlClientError.connectFailed("socket path too long: \(socketPath)")
        }
        let fd = Darwin.socket(AF_UNIX, SOCK_STREAM, 0)
        guard fd >= 0 else {
            throw ControlClientError.connectFailed("socket: \(errnoDescription)")
        }
        defer { close(fd) }

        // SO_RCVTIMEO so panel wait can block long but not forever.
        var tv = timeval(tv_sec: timeoutSeconds, tv_usec: 0)
        let rcvOptRet = withUnsafePointer(to: &tv) {
            $0.withMemoryRebound(to: timeval.self, capacity: 1) { tvp in
                setsockopt(fd, SOL_SOCKET, SO_RCVTIMEO, tvp, socklen_t(MemoryLayout<timeval>.size))
            }
        }
        guard rcvOptRet == 0 else {
            throw ControlClientError.connectFailed("setsockopt SO_RCVTIMEO: \(errnoDescription)")
        }

        try connectSocket(fd: fd, socketPath: socketPath)
        try writeRequest(fd: fd, request: request)
        return try readResponse(fd: fd)
    }

    // MARK: - Connect

    private static func connectSocket(fd: Int32, socketPath: String) throws {
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
                Darwin.connect(fd, $0, addrLen)
            }
        }
        guard rc == 0 else {
            throw ControlClientError.connectFailed("connect: \(errnoDescription)")
        }
    }

    // MARK: - Write

    private static func writeRequest(fd: Int32, request: ControlRequest) throws {
        let data = try ControlFraming.encodeLine(request)
        try data.withUnsafeBytes { buf in
            var offset = 0
            while offset < buf.count {
                let n = Darwin.write(fd, buf.baseAddress!.advanced(by: offset), buf.count - offset)
                if n < 0 && errno == EINTR { continue }
                if n < 0 {
                    throw ControlClientError.io("write: \(errnoDescription)")
                }
                if n == 0 {
                    throw ControlClientError.io("write returned 0")
                }
                offset += n
            }
        }
    }

    // MARK: - Read

    private static func readResponse(fd: Int32) throws -> ControlResponse {
        var accumulator = Data()
        var buf = [UInt8](repeating: 0, count: 64 * 1024)
        while true {
            let n = Darwin.read(fd, &buf, buf.count)
            if n < 0 {
                if errno == EINTR { continue }
                throw ControlClientError.io("read: \(errnoDescription)")
            }
            if n == 0 {
                throw ControlClientError.io("connection closed before newline")
            }
            accumulator.append(Data(buf[0..<n]))
            if accumulator.firstIndex(of: 0x0A) != nil {
                break
            }
        }
        // Strip trailing newline before decoding.
        let line = accumulator.dropLast()
        guard let response = try? ControlFraming.decodeResponse(line: line) else {
            throw ControlClientError.badResponse
        }
        return response
    }

    // MARK: - Helpers

    private static var errnoDescription: String {
        String(cString: Darwin.strerror(errno))
    }
}
