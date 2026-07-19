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
