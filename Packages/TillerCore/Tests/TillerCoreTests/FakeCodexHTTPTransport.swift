import Foundation
@testable import TillerCore

/// Queue-based fake transport: each call to `send` pops the next canned
/// result. Tests queue exactly as many results as they expect calls.
final class FakeCodexHTTPTransport: CodexHTTPTransport, @unchecked Sendable {
    enum Result {
        case success(Data, Int)
        case failure(Error)
    }

    private var results: [Result]
    private(set) var requests: [URLRequest] = []

    init(results: [Result]) {
        self.results = results
    }

    func send(_ request: URLRequest) async throws -> (Data, HTTPURLResponse) {
        requests.append(request)
        guard !results.isEmpty else {
            fatalError("FakeCodexHTTPTransport ran out of canned results")
        }
        switch results.removeFirst() {
        case .success(let data, let statusCode):
            let response = HTTPURLResponse(
                url: request.url!, statusCode: statusCode,
                httpVersion: nil, headerFields: nil)!
            return (data, response)
        case .failure(let error):
            throw error
        }
    }
}
