import Foundation
import Testing
@testable import TillerCore

private func writeAuthFile(_ json: [String: Any]) -> URL {
    let url = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString + ".json")
    let data = try! JSONSerialization.data(withJSONObject: json)
    try! data.write(to: url)
    return url
}

private let validAuthJSON: [String: Any] = [
    "tokens": [
        "access_token": "access-1",
        "refresh_token": "refresh-1",
        "account_id": "acct-1"
    ],
    "last_refresh": ISO8601DateFormatter().string(from: Date())
]

private let usageResponseJSON = """
{"rate_limit":{"primary_window":{"used_percent":26,"reset_at":1750000000},
"secondary_window":{"used_percent":53,"reset_at":1750600000}}}
"""

@Test func fetchReturnsSuccessWithMappedWindows() async throws {
    let authURL = writeAuthFile(validAuthJSON)
    defer { try? FileManager.default.removeItem(at: authURL) }
    let transport = FakeCodexHTTPTransport(results: [.success(Data(usageResponseJSON.utf8), 200)])

    let outcome = await CodexUsageFetcher.fetch(transport: transport, authFileURL: authURL)

    guard case .success(let usage) = outcome else {
        Issue.record("Expected .success, got \(outcome)")
        return
    }
    #expect(usage.session?.usedPercent == 26)
    #expect(usage.weekly?.usedPercent == 53)
    #expect(usage.session?.resetsAt == Date(timeIntervalSince1970: 1750000000))
}

@Test func fetchReturnsLoggedOutWhenAuthFileMissing() async throws {
    let authURL = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString + ".json")
    let transport = FakeCodexHTTPTransport(results: [])

    let outcome = await CodexUsageFetcher.fetch(transport: transport, authFileURL: authURL)

    #expect(outcome == .unavailable(.loggedOut))
}

@Test func fetchReturnsLoggedOutWhenAuthFileHasNoTokens() async throws {
    let authURL = writeAuthFile(["OPENAI_API_KEY": "sk-something"])
    defer { try? FileManager.default.removeItem(at: authURL) }
    let transport = FakeCodexHTTPTransport(results: [])

    let outcome = await CodexUsageFetcher.fetch(transport: transport, authFileURL: authURL)

    #expect(outcome == .unavailable(.loggedOut))
}

@Test func fetchReturnsLoggedOutOn401FromUsageEndpoint() async throws {
    let authURL = writeAuthFile(validAuthJSON)
    defer { try? FileManager.default.removeItem(at: authURL) }
    let transport = FakeCodexHTTPTransport(results: [.success(Data(), 401)])

    let outcome = await CodexUsageFetcher.fetch(transport: transport, authFileURL: authURL)

    #expect(outcome == .unavailable(.loggedOut))
}

@Test func fetchReturnsTimedOutOnNetworkTimeout() async throws {
    let authURL = writeAuthFile(validAuthJSON)
    defer { try? FileManager.default.removeItem(at: authURL) }
    let transport = FakeCodexHTTPTransport(results: [.failure(URLError(.timedOut))])

    let outcome = await CodexUsageFetcher.fetch(transport: transport, authFileURL: authURL)

    #expect(outcome == .timedOut)
}

@Test func fetchReturnsErrorOnMalformedUsageResponse() async throws {
    let authURL = writeAuthFile(validAuthJSON)
    defer { try? FileManager.default.removeItem(at: authURL) }
    let transport = FakeCodexHTTPTransport(results: [.success(Data("not json".utf8), 200)])

    let outcome = await CodexUsageFetcher.fetch(transport: transport, authFileURL: authURL)

    #expect(outcome == .unavailable(.error))
}

@Test func fetchRefreshesStaleTokenThenSucceeds() async throws {
    let staleJSON: [String: Any] = [
        "tokens": ["access_token": "stale-access", "refresh_token": "refresh-1", "account_id": "acct-1"],
        "last_refresh": ISO8601DateFormatter().string(from: Date().addingTimeInterval(-9 * 24 * 3600))
    ]
    let authURL = writeAuthFile(staleJSON)
    defer { try? FileManager.default.removeItem(at: authURL) }
    let refreshResponseJSON = try JSONSerialization.data(withJSONObject: ["access_token": "fresh-access"])
    let transport = FakeCodexHTTPTransport(results: [
        .success(refreshResponseJSON, 200),
        .success(Data(usageResponseJSON.utf8), 200)
    ])

    let outcome = await CodexUsageFetcher.fetch(transport: transport, authFileURL: authURL)

    guard case .success = outcome else {
        Issue.record("Expected .success after refresh, got \(outcome)")
        return
    }
    #expect(transport.requests.count == 2)
    let usageRequest = try #require(transport.requests.last)
    #expect(usageRequest.value(forHTTPHeaderField: "Authorization") == "Bearer fresh-access")

    let persisted = try CodexOAuthCredentialsStore.load(from: authURL)
    #expect(persisted.accessToken == "fresh-access")
}
