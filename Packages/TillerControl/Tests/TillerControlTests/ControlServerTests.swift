import Testing
import Foundation
@testable import TillerControl

private actor AsyncGate {
    private var isOpen = false
    private var waiters: [CheckedContinuation<Void, Never>] = []

    func wait() async {
        guard !isOpen else { return }
        await withCheckedContinuation { continuation in
            waiters.append(continuation)
        }
    }

    func open() {
        guard !isOpen else { return }
        isOpen = true
        let pending = waiters
        waiters.removeAll()
        for continuation in pending {
            continuation.resume()
        }
    }
}

@Test func echoRoundTripOverUnixSocket() throws {
    let path = NSTemporaryDirectory() + "tiller-test-\(UUID().uuidString.prefix(8)).sock"
    let server = ControlServer(socketPath: path) { request in
        .success(id: request.id, result: ["echo": request.params["msg"] ?? ""])
    }
    try server.start()
    defer { server.stop() }
    let response = try ControlClient.roundTrip(
        socketPath: path,
        request: ControlRequest(id: "t1", method: "ping", params: ["msg": "ciao"])
    )
    #expect(response.ok && response.result?["echo"] == "ciao")
}

@Test func optionalReceiveTimeoutNilAllowsDelayedResponse() async throws {
    let path = NSTemporaryDirectory() + "tiller-test-\(UUID().uuidString.prefix(8)).sock"
    let requestReceived = AsyncGate()
    let releaseResponse = AsyncGate()
    let server = ControlServer(socketPath: path) { request in
        await requestReceived.open()
        await releaseResponse.wait()
        return .success(id: request.id, result: ["status": "released"])
    }
    try server.start()
    defer { server.stop() }

    let client = Task.detached {
        try ControlClient.roundTrip(
            socketPath: path,
            request: ControlRequest(id: "no-deadline", method: "wait", params: [:]),
            timeoutSeconds: nil
        )
    }
    await requestReceived.wait()
    await releaseResponse.open()

    let response = try await client.value
    #expect(response.ok)
    #expect(response.result?["status"] == "released")
}

@Test func optionalReceiveTimeoutFiniteDeadlineFailsBeforeResponse() async throws {
    let path = NSTemporaryDirectory() + "tiller-test-\(UUID().uuidString.prefix(8)).sock"
    let requestReceived = AsyncGate()
    let releaseResponse = AsyncGate()
    let server = ControlServer(socketPath: path) { request in
        await requestReceived.open()
        await releaseResponse.wait()
        return .success(id: request.id)
    }
    try server.start()
    defer { server.stop() }

    let client = Task.detached {
        try ControlClient.roundTrip(
            socketPath: path,
            request: ControlRequest(id: "deadline", method: "wait", params: [:]),
            timeoutSeconds: 1
        )
    }
    await requestReceived.wait()

    do {
        _ = try await client.value
        Issue.record("Expected the receive deadline to expire before the response was released")
    } catch let error as ControlClientError {
        guard case .io(let detail) = error else {
            Issue.record("Unexpected control client error: \(error)")
            await releaseResponse.open()
            return
        }
        #expect(detail.contains("read:"))
    } catch {
        Issue.record("Unexpected receive-timeout error: \(error)")
    }

    await releaseResponse.open()
}

@Test func socketFileHasOwnerOnlyPermissions() throws {
    let path = NSTemporaryDirectory() + "tiller-test-\(UUID().uuidString.prefix(8)).sock"
    let server = ControlServer(socketPath: path) { .success(id: $0.id) }
    try server.start()
    defer { server.stop() }
    let attrs = try FileManager.default.attributesOfItem(atPath: path)
    let perms = (attrs[.posixPermissions] as? NSNumber)?.uint16Value ?? 0
    #expect(perms == 0o600)
}

@Test func malformedLineGetsErrorResponse() throws {
    let path = NSTemporaryDirectory() + "tiller-test-\(UUID().uuidString.prefix(8)).sock"
    let server = ControlServer(socketPath: path) { .success(id: $0.id) }
    try server.start()
    defer { server.stop() }
    // Raw client: send garbage, expect one JSON error line back.
    let fd = socket(AF_UNIX, SOCK_STREAM, 0)
    defer { close(fd) }
    var addr = sockaddr_un(); addr.sun_family = sa_family_t(AF_UNIX)
    withUnsafeMutableBytes(of: &addr.sun_path) { raw in
        path.utf8CString.withUnsafeBytes { raw.copyMemory(from: UnsafeRawBufferPointer(rebasing: $0.prefix(raw.count))) }
    }
    let len = socklen_t(MemoryLayout<sockaddr_un>.size)
    let connected = withUnsafePointer(to: &addr) {
        $0.withMemoryRebound(to: sockaddr.self, capacity: 1) { connect(fd, $0, len) }
    }
    #expect(connected == 0)
    _ = "not json\n".withCString { write(fd, $0, strlen($0)) }
    var buffer = [UInt8](repeating: 0, count: 4096)
    let n = read(fd, &buffer, buffer.count)
    #expect(n > 0)
    let decoded = try ControlFraming.decodeResponse(line: Data(buffer[0..<n]).dropLast())
    #expect(!decoded.ok)
}

@Test func longSocketPathThrows() {
    let path = NSTemporaryDirectory() + String(repeating: "x", count: 200)
    let server = ControlServer(socketPath: path) { .success(id: $0.id) }
    #expect(throws: ControlServerError.self) {
        try server.start()
    }
    #expect(throws: ControlClientError.self) {
        try ControlClient.roundTrip(socketPath: path, request: ControlRequest(id: "t1", method: "ping", params: [:]))
    }
}

@Test func pipelinedResponsesArriveInRequestOrder() throws {
    // Regression test for Step 1: a "slow" request must not let a "fast"
    // request's response overtake it when pipelined on the same connection.
    let path = NSTemporaryDirectory() + "tiller-test-\(UUID().uuidString.prefix(8)).sock"
    let server = ControlServer(socketPath: path) { request in
        if request.method == "slow" {
            try? await Task.sleep(for: .milliseconds(300))
        }
        return .success(id: request.id)
    }
    try server.start()
    defer { server.stop() }

    let fd = socket(AF_UNIX, SOCK_STREAM, 0)
    defer { close(fd) }
    var addr = sockaddr_un(); addr.sun_family = sa_family_t(AF_UNIX)
    withUnsafeMutableBytes(of: &addr.sun_path) { raw in
        path.utf8CString.withUnsafeBytes { raw.copyMemory(from: UnsafeRawBufferPointer(rebasing: $0.prefix(raw.count))) }
    }
    let len = socklen_t(MemoryLayout<sockaddr_un>.size)
    let connected = withUnsafePointer(to: &addr) {
        $0.withMemoryRebound(to: sockaddr.self, capacity: 1) { connect(fd, $0, len) }
    }
    #expect(connected == 0)

    // Write both request lines in a single send.
    let reqA = try ControlFraming.encodeLine(ControlRequest(id: "a", method: "slow", params: [:]))
    let reqB = try ControlFraming.encodeLine(ControlRequest(id: "b", method: "fast", params: [:]))
    let combined = reqA + reqB
    _ = combined.withUnsafeBytes { write(fd, $0.baseAddress!, $0.count) }

    // Read until we have two complete response lines.
    var accumulator = Data()
    while true {
        var buf = [UInt8](repeating: 0, count: 4096)
        let n = read(fd, &buf, buf.count)
        #expect(n > 0, "EOF before receiving both responses")
        accumulator.append(Data(buf[0..<n]))
        let newlines = accumulator.reduce(0) { $1 == 0x0A ? $0 + 1 : $0 }
        if newlines >= 2 { break }
    }

    let parts = accumulator.split(separator: 0x0A, maxSplits: 2, omittingEmptySubsequences: true)
    let r1 = try ControlFraming.decodeResponse(line: Data(parts[0]))
    let r2 = try ControlFraming.decodeResponse(line: Data(parts[1]))
    #expect(r1.id == "a")
    #expect(r2.id == "b")
}

@Test func oversizedLineGetsErrorAndDisconnect() throws {
    let path = NSTemporaryDirectory() + "tiller-test-\(UUID().uuidString.prefix(8)).sock"
    let server = ControlServer(socketPath: path) { .success(id: $0.id) }
    try server.start()
    defer { server.stop() }

    let fd = socket(AF_UNIX, SOCK_STREAM, 0)
    defer { close(fd) }
    // Suppress SIGPIPE: the server closes the connection mid-write when
    // it detects the buffer overflow, which could kill the process.
    var noSigpipe: Int32 = 1
    setsockopt(fd, SOL_SOCKET, SO_NOSIGPIPE, &noSigpipe, socklen_t(MemoryLayout<Int32>.size))
    var addr = sockaddr_un(); addr.sun_family = sa_family_t(AF_UNIX)
    withUnsafeMutableBytes(of: &addr.sun_path) { raw in
        path.utf8CString.withUnsafeBytes { raw.copyMemory(from: UnsafeRawBufferPointer(rebasing: $0.prefix(raw.count))) }
    }
    let len = socklen_t(MemoryLayout<sockaddr_un>.size)
    let connected = withUnsafePointer(to: &addr) {
        $0.withMemoryRebound(to: sockaddr.self, capacity: 1) { connect(fd, $0, len) }
    }
    #expect(connected == 0)

    // Send >1 MiB without a newline to trigger the buffer cap.
    let bigData = Data([UInt8](repeating: 0x61, count: 2 * 1024 * 1024))
    _ = bigData.withUnsafeBytes { write(fd, $0.baseAddress!, $0.count) }

    // Should get an error response back.
    var buf = [UInt8](repeating: 0, count: 4096)
    let n = read(fd, &buf, buf.count)
    #expect(n > 0)
    let response = try ControlFraming.decodeResponse(line: Data(buf[0..<n]).dropLast())
    #expect(!response.ok)

    // Connection should be closed — subsequent read returns 0 (EOF).
    let n2 = read(fd, &buf, buf.count)
    #expect(n2 == 0)
}

@Test func partialFrameAcrossWritesStillParses() throws {
    let path = NSTemporaryDirectory() + "tiller-test-\(UUID().uuidString.prefix(8)).sock"
    let server = ControlServer(socketPath: path) { .success(id: $0.id) }
    try server.start()
    defer { server.stop() }

    let fd = socket(AF_UNIX, SOCK_STREAM, 0)
    defer { close(fd) }
    var addr = sockaddr_un(); addr.sun_family = sa_family_t(AF_UNIX)
    withUnsafeMutableBytes(of: &addr.sun_path) { raw in
        path.utf8CString.withUnsafeBytes { raw.copyMemory(from: UnsafeRawBufferPointer(rebasing: $0.prefix(raw.count))) }
    }
    let len = socklen_t(MemoryLayout<sockaddr_un>.size)
    let connected = withUnsafePointer(to: &addr) {
        $0.withMemoryRebound(to: sockaddr.self, capacity: 1) { connect(fd, $0, len) }
    }
    #expect(connected == 0)

    // Send half a request line, sleep, then the rest.
    let request = ControlRequest(id: "t1", method: "ping", params: ["msg": "hello"])
    let fullData = try ControlFraming.encodeLine(request)
    let midPoint = fullData.count / 2
    _ = Data(fullData[0..<midPoint]).withUnsafeBytes { write(fd, $0.baseAddress!, $0.count) }
    Thread.sleep(forTimeInterval: 0.05)
    _ = Data(fullData[midPoint...]).withUnsafeBytes { write(fd, $0.baseAddress!, $0.count) }

    // Read response — should succeed.
    var buf = [UInt8](repeating: 0, count: 4096)
    let n = read(fd, &buf, buf.count)
    #expect(n > 0)
    let response = try ControlFraming.decodeResponse(line: Data(buf[0..<n]).dropLast())
    #expect(response.ok)
    #expect(response.id == "t1")
}
