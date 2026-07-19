import Foundation

/// Agent-initiated traffic surfaced to the session layer.
public enum ACPIncoming: Sendable {
    case notification(method: String, params: JSONValue?)
    case request(id: JSONRPCID, method: String, params: JSONValue?)
}

public enum ACPClientError: Error, Sendable {
    case transportClosed
    case agentError(JSONRPCError)
}

/// JSON-RPC endpoint over an ACPTransport: correlates our requests with the
/// agent's responses, and forwards agent-initiated requests/notifications to
/// the `incoming` stream (single consumer: ACPSession).
public actor ACPClient {
    private let transport: any ACPTransport
    private var nextRequestId = 0
    private var pending: [JSONRPCID: CheckedContinuation<JSONValue?, Error>] = [:]
    private var readTask: Task<Void, Never>?
    private let incomingContinuation: AsyncStream<ACPIncoming>.Continuation
    public nonisolated let incoming: AsyncStream<ACPIncoming>

    public init(transport: any ACPTransport) {
        self.transport = transport
        (incoming, incomingContinuation) = AsyncStream.makeStream(of: ACPIncoming.self)
    }

    public func start() async throws {
        try await transport.start()
        readTask = Task { [weak self, transport] in
            do {
                for try await line in transport.lines() {
                    await self?.handle(line: line)
                }
            } catch {}
            await self?.closed()
        }
    }

    public func stop() async {
        readTask?.cancel()
        await transport.terminate()
        closed()
    }

    public func request<R: Decodable>(_ method: String,
                                      params: (some Encodable)? = Optional<JSONValue>.none,
                                      as type: R.Type) async throws -> R {
        nextRequestId += 1
        let id = JSONRPCID.number(nextRequestId)
        let paramsValue = try params.map { try JSONValue.encoding($0) }
        let line = try JSONRPCMessage.request(id: id, method: method, params: paramsValue)
            .encodedLine()
        let result: JSONValue? = try await withCheckedThrowingContinuation { continuation in
            pending[id] = continuation
            Task {
                do { try await transport.send(line: line) }
                catch { self.fail(id: id, error: ACPClientError.transportClosed) }
            }
        }
        return try (result ?? .null).decoded(R.self)
    }

    public func notify(_ method: String, params: some Encodable) async throws {
        let line = try JSONRPCMessage.notification(
            method: method, params: JSONValue.encoding(params)).encodedLine()
        try await transport.send(line: line)
    }

    public func respond(to id: JSONRPCID, result: some Encodable) async throws {
        let line = try JSONRPCMessage.response(
            id: id, result: JSONValue.encoding(result), error: nil).encodedLine()
        try await transport.send(line: line)
    }

    public func respondError(to id: JSONRPCID, code: Int, message: String) async throws {
        let line = try JSONRPCMessage.response(
            id: id, result: nil, error: JSONRPCError(code: code, message: message))
            .encodedLine()
        try await transport.send(line: line)
    }

    private func handle(line: Data) {
        guard let message = try? JSONRPCMessage.decode(line) else { return }
        switch message {
        case .response(let id, let result, let error):
            guard let continuation = pending.removeValue(forKey: id) else { return }
            if let error {
                continuation.resume(throwing: ACPClientError.agentError(error))
            } else {
                continuation.resume(returning: result)
            }
        case .notification(let method, let params):
            incomingContinuation.yield(.notification(method: method, params: params))
        case .request(let id, let method, let params):
            incomingContinuation.yield(.request(id: id, method: method, params: params))
        }
    }

    private func fail(id: JSONRPCID, error: Error) {
        pending.removeValue(forKey: id)?.resume(throwing: error)
    }

    private func closed() {
        for continuation in pending.values {
            continuation.resume(throwing: ACPClientError.transportClosed)
        }
        pending.removeAll()
        incomingContinuation.finish()
    }
}
