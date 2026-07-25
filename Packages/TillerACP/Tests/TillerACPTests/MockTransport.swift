import Foundation
@testable import TillerACP

/// In-memory transport for driving ACPClient/ACPSession tests: records what
/// the client sends, lets the test emit agent lines.
actor MockTransport: ACPTransport {
    private(set) var sent: [Data] = []
    private var continuation: AsyncThrowingStream<Data, Error>.Continuation?
    private var pendingLines: [Data] = []
    private var closeRequested = false

    func start() {}

    func send(line: Data) {
        sent.append(line)
    }

    nonisolated func lines() -> AsyncThrowingStream<Data, Error> {
        AsyncThrowingStream { continuation in
            Task { await self.attach(continuation) }
        }
    }

    private func attach(_ continuation: AsyncThrowingStream<Data, Error>.Continuation) {
        self.continuation = continuation
        for line in pendingLines { continuation.yield(line) }
        pendingLines = []
        if closeRequested { continuation.finish() }
    }

    /// Emits one agent → client line (a JSON string, no trailing newline).
    func emit(_ json: String) {
        let data = Data(json.utf8)
        if let continuation { continuation.yield(data) } else { pendingLines.append(data) }
    }

    func close() {
        if let continuation {
            continuation.finish()
        } else {
            closeRequested = true
        }
    }

    func terminate() {
        close()
    }

    /// Polls until the client has sent at least `count` lines.
    func waitForSent(count: Int) async throws -> [Data] {
        for _ in 0..<500 {
            if sent.count >= count { return sent }
            try await Task.sleep(for: .milliseconds(5))
        }
        return sent
    }

    /// Decodes the n-th sent line as a JSON-RPC message.
    func sentMessage(_ index: Int) throws -> JSONRPCMessage {
        try JSONRPCMessage.decode(sent[index].last == UInt8(ascii: "\n")
                                  ? sent[index].dropLast() : sent[index])
    }
}
