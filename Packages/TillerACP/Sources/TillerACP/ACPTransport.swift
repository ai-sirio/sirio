import Foundation

/// Byte transport carrying newline-delimited JSON-RPC lines to/from an agent.
/// `lines()` yields complete lines without the trailing newline and must be
/// consumed by a single reader. The stream finishes when the peer closes.
public protocol ACPTransport: Sendable {
    func start() async throws
    func send(line: Data) async throws
    func lines() -> AsyncThrowingStream<Data, Error>
    func terminate() async
}
