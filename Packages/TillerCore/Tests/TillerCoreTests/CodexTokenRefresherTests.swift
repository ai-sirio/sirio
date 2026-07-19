import Foundation
import Testing
@testable import TillerCore

private let baseCredentials = CodexOAuthCredentials(
    accessToken: "old-access", refreshToken: "old-refresh",
    idToken: "old-id", accountId: "acct-1", lastRefresh: nil)

@Test func refreshSucceedsAndReturnsNewTokens() async throws {
    let responseJSON = try JSONSerialization.data(withJSONObject: [
        "access_token": "new-access",
        "refresh_token": "new-refresh",
        "id_token": "new-id"
    ])
    let transport = FakeCodexHTTPTransport(results: [.success(responseJSON, 200)])

    let refreshed = try await CodexTokenRefresher.refresh(baseCredentials, transport: transport)

    #expect(refreshed.accessToken == "new-access")
    #expect(refreshed.refreshToken == "new-refresh")
    #expect(refreshed.idToken == "new-id")
    #expect(refreshed.accountId == "acct-1")
}

@Test func refreshClassifiesExpiredToken() async throws {
    let errorJSON = try JSONSerialization.data(withJSONObject: ["error": "refresh_token_expired"])
    let transport = FakeCodexHTTPTransport(results: [.success(errorJSON, 401)])

    await #expect(throws: CodexTokenRefreshError.expired) {
        _ = try await CodexTokenRefresher.refresh(baseCredentials, transport: transport)
    }
}

@Test func refreshClassifiesRevokedToken() async throws {
    let errorJSON = try JSONSerialization.data(withJSONObject: ["error": "refresh_token_invalidated"])
    let transport = FakeCodexHTTPTransport(results: [.success(errorJSON, 401)])

    await #expect(throws: CodexTokenRefreshError.revoked) {
        _ = try await CodexTokenRefresher.refresh(baseCredentials, transport: transport)
    }
}

@Test func refreshClassifiesReusedToken() async throws {
    let errorJSON = try JSONSerialization.data(withJSONObject: ["error": "refresh_token_reused"])
    let transport = FakeCodexHTTPTransport(results: [.success(errorJSON, 401)])

    await #expect(throws: CodexTokenRefreshError.reused) {
        _ = try await CodexTokenRefresher.refresh(baseCredentials, transport: transport)
    }
}

@Test func refreshMapsNetworkFailureToNetworkError() async throws {
    let transport = FakeCodexHTTPTransport(results: [.failure(URLError(.notConnectedToInternet))])

    await #expect(throws: CodexTokenRefreshError.network) {
        _ = try await CodexTokenRefresher.refresh(baseCredentials, transport: transport)
    }
}

@Test func refreshSendsClientIdAndRefreshTokenInBody() async throws {
    let responseJSON = try JSONSerialization.data(withJSONObject: ["access_token": "new-access"])
    let transport = FakeCodexHTTPTransport(results: [.success(responseJSON, 200)])

    _ = try await CodexTokenRefresher.refresh(baseCredentials, transport: transport)

    let sentBody = try #require(transport.requests.first?.httpBody)
    let sentJSON = try JSONSerialization.jsonObject(with: sentBody) as? [String: Any]
    #expect(sentJSON?["client_id"] as? String == "app_EMoamEEZ73f0CkXaXp7hrann")
    #expect(sentJSON?["refresh_token"] as? String == "old-refresh")
    #expect(sentJSON?["grant_type"] as? String == "refresh_token")
}
