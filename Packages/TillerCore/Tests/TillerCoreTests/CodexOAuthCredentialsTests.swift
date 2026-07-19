import Foundation
import Testing
@testable import TillerCore

// MARK: - authFilePath

@Test func authFilePathUsesCodexHomeWhenSet() {
    let url = CodexOAuthCredentialsStore.authFilePath(environment: ["CODEX_HOME": "/tmp/custom-codex-home"])
    #expect(url.path == "/tmp/custom-codex-home/auth.json")
}

@Test func authFilePathFallsBackToHomeDirectory() {
    let url = CodexOAuthCredentialsStore.authFilePath(environment: [:])
    #expect(url.path == FileManager.default.homeDirectoryForCurrentUser.appendingPathComponent(".codex/auth.json").path)
}

// MARK: - load

private func writeTempAuthFile(_ json: [String: Any]) -> URL {
    let url = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString + ".json")
    let data = try! JSONSerialization.data(withJSONObject: json)
    try! data.write(to: url)
    return url
}

@Test func loadParsesTokensAndLastRefresh() throws {
    let url = writeTempAuthFile([
        "tokens": [
            "access_token": "access-1",
            "refresh_token": "refresh-1",
            "id_token": "id-1",
            "account_id": "acct-1"
        ],
        "last_refresh": "2026-07-01T00:00:00Z"
    ])
    defer { try? FileManager.default.removeItem(at: url) }

    let credentials = try CodexOAuthCredentialsStore.load(from: url)

    #expect(credentials.accessToken == "access-1")
    #expect(credentials.refreshToken == "refresh-1")
    #expect(credentials.idToken == "id-1")
    #expect(credentials.accountId == "acct-1")
    #expect(credentials.lastRefresh == Date(timeIntervalSince1970: 1_782_864_000))
}

@Test func loadThrowsMissingTokensForApiKeyOnlyFile() {
    let url = writeTempAuthFile(["OPENAI_API_KEY": "sk-something"])
    defer { try? FileManager.default.removeItem(at: url) }

    #expect(throws: CodexOAuthCredentialsError.missingTokens) {
        _ = try CodexOAuthCredentialsStore.load(from: url)
    }
}

@Test func loadThrowsFileNotFoundForMissingFile() {
    let url = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString + ".json")

    #expect(throws: CodexOAuthCredentialsError.fileNotFound) {
        _ = try CodexOAuthCredentialsStore.load(from: url)
    }
}

@Test func loadThrowsInvalidJSONForMalformedFile() {
    let url = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString + ".json")
    try! Data("not json".utf8).write(to: url)
    defer { try? FileManager.default.removeItem(at: url) }

    #expect(throws: CodexOAuthCredentialsError.invalidJSON) {
        _ = try CodexOAuthCredentialsStore.load(from: url)
    }
}

// MARK: - needsRefresh

@Test func needsRefreshIsTrueWhenLastRefreshIsNil() {
    let credentials = CodexOAuthCredentials(accessToken: "a", refreshToken: "r", lastRefresh: nil)
    #expect(credentials.needsRefresh)
}

@Test func needsRefreshIsTrueWhenOlderThanEightDays() {
    let nineDaysAgo = Date().addingTimeInterval(-9 * 24 * 3600)
    let credentials = CodexOAuthCredentials(accessToken: "a", refreshToken: "r", lastRefresh: nineDaysAgo)
    #expect(credentials.needsRefresh)
}

@Test func needsRefreshIsFalseWhenRecentlyRefreshed() {
    let oneDayAgo = Date().addingTimeInterval(-1 * 24 * 3600)
    let credentials = CodexOAuthCredentials(accessToken: "a", refreshToken: "r", lastRefresh: oneDayAgo)
    #expect(!credentials.needsRefresh)
}

// MARK: - save

@Test func saveMergesTokensPreservingUnrelatedFields() throws {
    let url = writeTempAuthFile(["some_other_config": "keep-me", "tokens": ["access_token": "stale"]])
    defer { try? FileManager.default.removeItem(at: url) }

    let refreshed = CodexOAuthCredentials(
        accessToken: "fresh-access", refreshToken: "fresh-refresh",
        idToken: "fresh-id", accountId: "acct-1", lastRefresh: Date())
    try CodexOAuthCredentialsStore.save(refreshed, to: url)

    let data = try Data(contentsOf: url)
    let json = try JSONSerialization.jsonObject(with: data) as? [String: Any]
    #expect(json?["some_other_config"] as? String == "keep-me")
    let tokens = json?["tokens"] as? [String: Any]
    #expect(tokens?["access_token"] as? String == "fresh-access")
    #expect(tokens?["refresh_token"] as? String == "fresh-refresh")
    #expect(json?["last_refresh"] != nil)
}
