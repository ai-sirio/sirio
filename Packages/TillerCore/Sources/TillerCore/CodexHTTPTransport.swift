import Foundation

/// Injectable seam for Codex OAuth HTTP calls (token refresh + usage fetch),
/// so tests never hit the real network.
public protocol CodexHTTPTransport: Sendable {
    func send(_ request: URLRequest) async throws -> (Data, HTTPURLResponse)
}

/// Production adapter backed by `URLSession`.
public struct URLSessionCodexHTTPTransport: CodexHTTPTransport {
    private let session: URLSession

    public init(session: URLSession = .shared) {
        self.session = session
    }

    public func send(_ request: URLRequest) async throws -> (Data, HTTPURLResponse) {
        let (data, response) = try await session.data(for: request)
        guard let http = response as? HTTPURLResponse else {
            throw URLError(.badServerResponse)
        }
        return (data, http)
    }
}
