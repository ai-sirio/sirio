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
