# Codex OAuth Usage Fetcher Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace Tiller's `codex app-server` subprocess/JSON-RPC usage fetcher with a direct OAuth HTTP client that reads `~/.codex/auth.json`, refreshes tokens when needed, and calls the same usage endpoint the `codex` CLI itself uses.

**Architecture:** All new logic lives in the `TillerCore` package (mirrors `ClaudeUsageFetcher`'s pattern in `TillerTerminal`): a credentials loader/saver for `auth.json`, a token refresher, and an orchestrating `CodexUsageFetcher` with an injectable HTTP transport seam for tests. `App/UsageStore.swift` needs zero changes — it already calls `CodexUsageFetcher.fetch()` via `import TillerCore`.

**Tech Stack:** Swift 6, Foundation (`URLSession`, `JSONSerialization`/`JSONDecoder`), Swift Testing (`@Test`, `#expect`).

## Global Constraints

- OAuth only — no `OPENAI_API_KEY`/apikey auth mode support. Missing `tokens` in `auth.json` → treated as logged out.
- Auto-refresh threshold: 8 days since `last_refresh`.
- Refresh endpoint: `POST https://auth.openai.com/oauth/token`, client id `app_EMoamEEZ73f0CkXaXp7hrann`, body `{"client_id", "grant_type": "refresh_token", "refresh_token", "scope": "openid profile email"}`.
- Usage endpoint: `GET https://chatgpt.com/backend-api/wham/usage`, headers `Authorization: Bearer <access_token>` and `ChatGPT-Account-Id: <account_id>`.
- Full replacement — no CLI-subprocess fallback path.
- Everything lives in `Packages/TillerCore/Sources/TillerCore/`; no new file in `App/`.
- State mapping (`UsageFetchOutcome`): missing/tokenless `auth.json` → `.unavailable(.loggedOut)`; refresh expired/revoked/reused → `.unavailable(.loggedOut)`; 401/403 from usage endpoint → `.unavailable(.loggedOut)`; network timeout → `.timedOut`; other network/decode error → `.unavailable(.error)`; success → `.success(ProviderUsage)`.
- Refreshed tokens persisted back to `auth.json` by merging only `tokens` + `last_refresh` keys into the existing file, written atomically.

---

### Task 1: `CodexHTTPTransport` — injectable HTTP seam

**Files:**
- Create: `Packages/TillerCore/Sources/TillerCore/CodexHTTPTransport.swift`

**Interfaces:**
- Produces: `public protocol CodexHTTPTransport: Sendable { func send(_ request: URLRequest) async throws -> (Data, HTTPURLResponse) }` and `public struct URLSessionCodexHTTPTransport: CodexHTTPTransport` (production adapter, `init(session: URLSession = .shared)`).

This is a thin real-network adapter with no branching logic — like `PtyProcessUsageAdapter` in `ClaudeUsageFetcher.swift`, it is exercised indirectly through the fake used by later tasks' tests, not tested directly.

- [ ] **Step 1: Write the file**

```swift
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
```

- [ ] **Step 2: Build to verify it compiles**

Run: `swift build --package-path Packages/TillerCore`
Expected: Build succeeds with no errors.

- [ ] **Step 3: Commit**

```bash
git add Packages/TillerCore/Sources/TillerCore/CodexHTTPTransport.swift
git commit -m "feat: add injectable HTTP transport seam for Codex OAuth"
```

---

### Task 2: `FakeCodexHTTPTransport` — shared test double

**Files:**
- Create: `Packages/TillerCore/Tests/TillerCoreTests/FakeCodexHTTPTransport.swift`

**Interfaces:**
- Consumes: `CodexHTTPTransport` protocol from Task 1.
- Produces: `final class FakeCodexHTTPTransport: CodexHTTPTransport, @unchecked Sendable` with `init(results: [FakeCodexHTTPTransport.Result])`, nested `enum Result { case success(Data, Int); case failure(Error) }`, and `private(set) var requests: [URLRequest]` for assertions in later tasks.

This is test-support infrastructure (a fake), not production code — it is not itself unit tested, matching `FakePty` in `ClaudeUsageFetcherTests.swift`.

- [ ] **Step 1: Write the file**

```swift
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
```

- [ ] **Step 2: Build to verify it compiles**

Run: `swift build --package-path Packages/TillerCore --build-tests`
Expected: Build succeeds with no errors.

- [ ] **Step 3: Commit**

```bash
git add Packages/TillerCore/Tests/TillerCoreTests/FakeCodexHTTPTransport.swift
git commit -m "test: add FakeCodexHTTPTransport double for Codex OAuth tests"
```

---

### Task 3: `CodexOAuthCredentials` — model + `auth.json` loader/saver

**Files:**
- Create: `Packages/TillerCore/Sources/TillerCore/CodexOAuthCredentials.swift`
- Test: `Packages/TillerCore/Tests/TillerCoreTests/CodexOAuthCredentialsTests.swift`

**Interfaces:**
- Produces:
  - `public struct CodexOAuthCredentials: Sendable, Equatable` with fields `accessToken: String`, `refreshToken: String`, `idToken: String?`, `accountId: String?`, `lastRefresh: Date?`, and computed `var needsRefresh: Bool`.
  - `public enum CodexOAuthCredentialsError: Error, Equatable { case fileNotFound, invalidJSON, missingTokens }`
  - `public enum CodexOAuthCredentialsStore` with `static func authFilePath(environment: [String: String] = ProcessInfo.processInfo.environment) -> URL`, `static func load(from url: URL) throws -> CodexOAuthCredentials`, `static func save(_ credentials: CodexOAuthCredentials, to url: URL) throws`.

- [ ] **Step 1: Write the failing tests**

```swift
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
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `swift test --package-path Packages/TillerCore --filter CodexOAuthCredentials`
Expected: FAIL — `CodexOAuthCredentials`, `CodexOAuthCredentialsError`, `CodexOAuthCredentialsStore` don't exist yet (compile error).

- [ ] **Step 3: Write the implementation**

```swift
import Foundation

public struct CodexOAuthCredentials: Sendable, Equatable {
    public let accessToken: String
    public let refreshToken: String
    public let idToken: String?
    public let accountId: String?
    public let lastRefresh: Date?

    public init(
        accessToken: String, refreshToken: String,
        idToken: String? = nil, accountId: String? = nil, lastRefresh: Date? = nil
    ) {
        self.accessToken = accessToken
        self.refreshToken = refreshToken
        self.idToken = idToken
        self.accountId = accountId
        self.lastRefresh = lastRefresh
    }

    /// `codex` itself refreshes every 8 days; Tiller mirrors that cadence so
    /// a long-idle Tiller doesn't keep presenting a token codex would
    /// already consider due for renewal.
    public var needsRefresh: Bool {
        guard let lastRefresh else { return true }
        return Date().timeIntervalSince(lastRefresh) > 8 * 24 * 3600
    }
}

public enum CodexOAuthCredentialsError: Error, Equatable {
    case fileNotFound
    case invalidJSON
    case missingTokens
}

public enum CodexOAuthCredentialsStore {
    /// `~/.codex/auth.json`, or `$CODEX_HOME/auth.json` when set — same
    /// precedence `codex` itself uses (and `AppModel` already reads
    /// elsewhere in Tiller for other Codex integrations).
    public static func authFilePath(
        environment: [String: String] = ProcessInfo.processInfo.environment
    ) -> URL {
        if let codexHome = environment["CODEX_HOME"], !codexHome.isEmpty {
            return URL(fileURLWithPath: codexHome).appendingPathComponent("auth.json")
        }
        return FileManager.default.homeDirectoryForCurrentUser
            .appendingPathComponent(".codex/auth.json")
    }

    public static func load(from url: URL) throws -> CodexOAuthCredentials {
        guard let data = try? Data(contentsOf: url) else {
            throw CodexOAuthCredentialsError.fileNotFound
        }
        guard let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else {
            throw CodexOAuthCredentialsError.invalidJSON
        }
        guard
            let tokens = json["tokens"] as? [String: Any],
            let accessToken = tokens["access_token"] as? String,
            let refreshToken = tokens["refresh_token"] as? String
        else {
            throw CodexOAuthCredentialsError.missingTokens
        }
        return CodexOAuthCredentials(
            accessToken: accessToken,
            refreshToken: refreshToken,
            idToken: tokens["id_token"] as? String,
            accountId: tokens["account_id"] as? String,
            lastRefresh: (json["last_refresh"] as? String).flatMap(parseISO8601)
        )
    }

    /// Merges refreshed tokens into the existing file rather than
    /// overwriting it, since `codex` itself may store other fields Tiller
    /// doesn't know about (config, install id, etc.).
    public static func save(_ credentials: CodexOAuthCredentials, to url: URL) throws {
        var json: [String: Any] = [:]
        if let data = try? Data(contentsOf: url),
           let existing = try? JSONSerialization.jsonObject(with: data) as? [String: Any] {
            json = existing
        }
        var tokens: [String: Any] = [
            "access_token": credentials.accessToken,
            "refresh_token": credentials.refreshToken
        ]
        if let idToken = credentials.idToken { tokens["id_token"] = idToken }
        if let accountId = credentials.accountId { tokens["account_id"] = accountId }
        json["tokens"] = tokens
        json["last_refresh"] = ISO8601DateFormatter().string(from: Date())

        let data = try JSONSerialization.data(withJSONObject: json, options: [.prettyPrinted, .sortedKeys])
        try data.write(to: url, options: .atomic)
    }

    private static func parseISO8601(_ string: String) -> Date? {
        let withFractional = ISO8601DateFormatter()
        withFractional.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        return withFractional.date(from: string) ?? ISO8601DateFormatter().date(from: string)
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `swift test --package-path Packages/TillerCore --filter CodexOAuthCredentials`
Expected: PASS — all 10 tests green.

- [ ] **Step 5: Commit**

```bash
git add Packages/TillerCore/Sources/TillerCore/CodexOAuthCredentials.swift \
        Packages/TillerCore/Tests/TillerCoreTests/CodexOAuthCredentialsTests.swift
git commit -m "feat: add CodexOAuthCredentials model and auth.json loader/saver"
```

---

### Task 4: `CodexTokenRefresher` — refresh flow + error classification

**Files:**
- Create: `Packages/TillerCore/Sources/TillerCore/CodexTokenRefresher.swift`
- Test: `Packages/TillerCore/Tests/TillerCoreTests/CodexTokenRefresherTests.swift`

**Interfaces:**
- Consumes: `CodexOAuthCredentials` (Task 3), `CodexHTTPTransport` (Task 1), `FakeCodexHTTPTransport` (Task 2, tests only).
- Produces: `public enum CodexTokenRefreshError: Error, Equatable { case expired, revoked, reused, network, invalidResponse }` and `public enum CodexTokenRefresher { static func refresh(_ credentials: CodexOAuthCredentials, transport: any CodexHTTPTransport) async throws -> CodexOAuthCredentials }`.

- [ ] **Step 1: Write the failing tests**

```swift
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
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `swift test --package-path Packages/TillerCore --filter CodexTokenRefresher`
Expected: FAIL — `CodexTokenRefreshError` and `CodexTokenRefresher` don't exist yet (compile error).

- [ ] **Step 3: Write the implementation**

```swift
import Foundation

public enum CodexTokenRefreshError: Error, Equatable {
    case expired
    case revoked
    case reused
    case network
    case invalidResponse
}

public enum CodexTokenRefresher {
    private static let refreshURL = URL(string: "https://auth.openai.com/oauth/token")!
    private static let clientId = "app_EMoamEEZ73f0CkXaXp7hrann"

    public static func refresh(
        _ credentials: CodexOAuthCredentials,
        transport: any CodexHTTPTransport
    ) async throws -> CodexOAuthCredentials {
        var request = URLRequest(url: refreshURL)
        request.httpMethod = "POST"
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        request.httpBody = try? JSONSerialization.data(withJSONObject: [
            "client_id": clientId,
            "grant_type": "refresh_token",
            "refresh_token": credentials.refreshToken,
            "scope": "openid profile email"
        ])

        let data: Data
        let response: HTTPURLResponse
        do {
            (data, response) = try await transport.send(request)
        } catch {
            throw CodexTokenRefreshError.network
        }

        if response.statusCode == 401 {
            throw classify401(data)
        }
        guard
            response.statusCode == 200,
            let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
            let accessToken = json["access_token"] as? String
        else {
            throw CodexTokenRefreshError.invalidResponse
        }

        return CodexOAuthCredentials(
            accessToken: accessToken,
            refreshToken: json["refresh_token"] as? String ?? credentials.refreshToken,
            idToken: json["id_token"] as? String ?? credentials.idToken,
            accountId: credentials.accountId,
            lastRefresh: Date()
        )
    }

    private static func classify401(_ data: Data) -> CodexTokenRefreshError {
        guard
            let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
            let code = (json["error"] as? [String: Any])?["code"] as? String
                ?? json["error"] as? String
        else {
            return .expired
        }
        switch code.lowercased() {
        case "refresh_token_reused": return .reused
        case "refresh_token_invalidated": return .revoked
        default: return .expired
        }
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `swift test --package-path Packages/TillerCore --filter CodexTokenRefresher`
Expected: PASS — all 6 tests green.

- [ ] **Step 5: Commit**

```bash
git add Packages/TillerCore/Sources/TillerCore/CodexTokenRefresher.swift \
        Packages/TillerCore/Tests/TillerCoreTests/CodexTokenRefresherTests.swift
git commit -m "feat: add CodexTokenRefresher with expired/revoked/reused classification"
```

---

### Task 5: `CodexUsageFetcher` — orchestration + response mapping

**Files:**
- Create: `Packages/TillerCore/Sources/TillerCore/CodexUsageFetcher.swift`
- Test: `Packages/TillerCore/Tests/TillerCoreTests/CodexUsageFetcherTests.swift`

**Interfaces:**
- Consumes: `CodexOAuthCredentialsStore` / `CodexOAuthCredentials` (Task 3), `CodexTokenRefresher` / `CodexTokenRefreshError` (Task 4), `CodexHTTPTransport` / `URLSessionCodexHTTPTransport` (Task 1), `FakeCodexHTTPTransport` (Task 2, tests only), `UsageFetchOutcome` / `ProviderUsage` / `UsageWindow` (existing, `ProviderUsage.swift`).
- Produces: `public enum CodexUsageFetcher { public static func fetch() async -> UsageFetchOutcome }` plus an internal testable overload `static func fetch(transport: any CodexHTTPTransport, authFileURL: URL) async -> UsageFetchOutcome`. This is the exact name and public signature of the type being deleted in Task 6, so `App/UsageStore.swift:75`'s `await CodexUsageFetcher.fetch()` keeps compiling unchanged.

- [ ] **Step 1: Write the failing tests**

```swift
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
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `swift test --package-path Packages/TillerCore --filter CodexUsageFetcher`
Expected: FAIL — `CodexUsageFetcher` (the new `TillerCore` type) doesn't exist yet (compile error). Note: `App/CodexUsageFetcher.swift` still exists at this point (deleted in Task 6), so there is no naming collision — the `App` target and `TillerCore` package build independently.

- [ ] **Step 3: Write the implementation**

```swift
import Foundation

public enum CodexUsageFetcher {
    private static let usageURL = URL(string: "https://chatgpt.com/backend-api/wham/usage")!

    public static func fetch() async -> UsageFetchOutcome {
        await fetch(
            transport: URLSessionCodexHTTPTransport(),
            authFileURL: CodexOAuthCredentialsStore.authFilePath()
        )
    }

    static func fetch(transport: any CodexHTTPTransport, authFileURL: URL) async -> UsageFetchOutcome {
        guard var credentials = try? CodexOAuthCredentialsStore.load(from: authFileURL) else {
            return .unavailable(.loggedOut)
        }

        if credentials.needsRefresh, !credentials.refreshToken.isEmpty {
            guard let refreshed = try? await CodexTokenRefresher.refresh(credentials, transport: transport) else {
                return .unavailable(.loggedOut)
            }
            credentials = refreshed
            try? CodexOAuthCredentialsStore.save(credentials, to: authFileURL)
        }

        var request = URLRequest(url: usageURL, timeoutInterval: 15)
        request.setValue("Bearer \(credentials.accessToken)", forHTTPHeaderField: "Authorization")
        request.setValue("application/json", forHTTPHeaderField: "Accept")
        request.setValue("Tiller", forHTTPHeaderField: "User-Agent")
        if let accountId = credentials.accountId {
            request.setValue(accountId, forHTTPHeaderField: "ChatGPT-Account-Id")
        }

        let data: Data
        let response: HTTPURLResponse
        do {
            (data, response) = try await transport.send(request)
        } catch let error as URLError where error.code == .timedOut {
            return .timedOut
        } catch {
            return .unavailable(.error)
        }

        if response.statusCode == 401 || response.statusCode == 403 {
            return .unavailable(.loggedOut)
        }
        guard (200..<300).contains(response.statusCode), let usage = parseUsage(data) else {
            return .unavailable(.error)
        }
        return .success(usage)
    }

    private struct UsageWindowWire: Decodable {
        let usedPercent: Int
        let resetAt: Double?
        enum CodingKeys: String, CodingKey {
            case usedPercent = "used_percent"
            case resetAt = "reset_at"
        }
    }

    private struct RateLimitWire: Decodable {
        let primaryWindow: UsageWindowWire?
        let secondaryWindow: UsageWindowWire?
        enum CodingKeys: String, CodingKey {
            case primaryWindow = "primary_window"
            case secondaryWindow = "secondary_window"
        }
    }

    private struct UsageResponseWire: Decodable {
        let rateLimit: RateLimitWire?
        enum CodingKeys: String, CodingKey {
            case rateLimit = "rate_limit"
        }
    }

    private static func parseUsage(_ data: Data) -> ProviderUsage? {
        guard let decoded = try? JSONDecoder().decode(UsageResponseWire.self, from: data) else {
            return nil
        }
        let session = decoded.rateLimit?.primaryWindow.map {
            UsageWindow(label: "5h", usedPercent: $0.usedPercent, resetsAt: $0.resetAt.map(Date.init(timeIntervalSince1970:)))
        }
        let weekly = decoded.rateLimit?.secondaryWindow.map {
            UsageWindow(label: "wk", usedPercent: $0.usedPercent, resetsAt: $0.resetAt.map(Date.init(timeIntervalSince1970:)))
        }
        guard session != nil || weekly != nil else { return nil }
        return ProviderUsage(session: session, weekly: weekly)
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `swift test --package-path Packages/TillerCore --filter CodexUsageFetcher`
Expected: PASS — all 7 tests green.

- [ ] **Step 5: Run the full TillerCore test suite**

Run: `swift test --package-path Packages/TillerCore`
Expected: PASS — no regressions in existing `TillerCore` tests (including the still-present `CodexRateLimitParserTests`, removed in Task 6).

- [ ] **Step 6: Commit**

```bash
git add Packages/TillerCore/Sources/TillerCore/CodexUsageFetcher.swift \
        Packages/TillerCore/Tests/TillerCoreTests/CodexUsageFetcherTests.swift
git commit -m "feat: add TillerCore CodexUsageFetcher backed by direct OAuth HTTP"
```

---

### Task 6: Delete the old subprocess fetcher and wire up the App target

**Files:**
- Delete: `App/CodexUsageFetcher.swift`
- Delete: `Packages/TillerCore/Sources/TillerCore/CodexRateLimitParser.swift`
- Delete: `Packages/TillerCore/Tests/TillerCoreTests/CodexRateLimitParserTests.swift`
- Verify (no edits expected): `App/UsageStore.swift:75`

**Interfaces:**
- Consumes: `CodexUsageFetcher.fetch() async -> UsageFetchOutcome` from Task 5 (same name/signature as the deleted `App/CodexUsageFetcher.swift`, resolved now via `TillerCore` since `App/UsageStore.swift` already has `import TillerCore`).

- [ ] **Step 1: Confirm nothing else references the old symbols**

Run: `grep -rln "CodexRateLimitParser" App Packages --include="*.swift" | grep -v ".build/"`
Expected: No output (only the three files being deleted reference it; already confirmed during design — this is a re-check before deleting).

- [ ] **Step 2: Delete the three files**

```bash
git rm App/CodexUsageFetcher.swift \
       Packages/TillerCore/Sources/TillerCore/CodexRateLimitParser.swift \
       Packages/TillerCore/Tests/TillerCoreTests/CodexRateLimitParserTests.swift
```

- [ ] **Step 3: Build the App target**

Run: `xcodebuild -project Tiller.xcodeproj -scheme Tiller -configuration Debug -destination "platform=macOS" build`
Expected: `** BUILD SUCCEEDED **` — `App/UsageStore.swift:75`'s `await CodexUsageFetcher.fetch()` now resolves to the `TillerCore` type from Task 5 with no source change needed.

- [ ] **Step 4: Run the full TillerCore test suite once more**

Run: `swift test --package-path Packages/TillerCore`
Expected: PASS — `CodexRateLimitParserTests` is gone (deleted), all remaining tests (including the new Codex OAuth tests from Tasks 3–5) pass.

- [ ] **Step 5: Commit**

```bash
git commit -m "refactor: remove codex app-server subprocess fetcher, replaced by direct OAuth"
```

- [ ] **Step 6: Manual smoke check (requires a real Codex ChatGPT login)**

With `codex login` done via "Sign in with ChatGPT" (not API key) on the dev machine: launch Tiller, open the usage bar / AI Providers settings, confirm the Codex row shows real usage percentages instead of "Timed out" or "Error". If logged out or using apikey mode instead, confirm the row shows "Not logged in — run `codex login`." rather than a generic error.
