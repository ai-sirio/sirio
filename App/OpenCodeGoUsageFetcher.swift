import Foundation
import TillerCore

enum OpenCodeGoUsageFetcher {
    static let cookieKey = "opencode-go-cookie"
    private static let workspaceServerId = "def39973159c7f0483d8793a822b8dbb10d067e12c65455fcb4608459ba0234f"

    private static let session: URLSession = {
        let config = URLSessionConfiguration.default
        config.timeoutIntervalForRequest = 12
        return URLSession(configuration: config)
    }()

    static func fetch(workspaceIdOverride: String? = nil) async -> UsageFetchOutcome {
        guard
            let rawCookie = KeychainCredentialStore.get(key: cookieKey),
            !rawCookie.trimmingCharacters(in: .whitespaces).isEmpty
        else {
            return .unavailable(.loggedOut)
        }
        let cookie = OpenCodeGoUsageParser.normalizeCookie(rawCookie)

        let workspaceId: String
        if let override = workspaceIdOverride?.trimmingCharacters(in: .whitespaces), !override.isEmpty {
            workspaceId = override
        } else {
            guard let discovered = await discoverWorkspaceId(cookie: cookie) else {
                return .unavailable(.error)
            }
            workspaceId = discovered
        }

        guard let usageURL = URL(string: "https://opencode.ai/workspace/\(workspaceId)/go") else {
            return .unavailable(.error)
        }
        var usageRequest = URLRequest(url: usageURL)
        usageRequest.setValue(cookie, forHTTPHeaderField: "Cookie")

        do {
            let (usageData, usageResponse) = try await session.data(for: usageRequest)
            guard
                let usageHttp = usageResponse as? HTTPURLResponse,
                (200..<300).contains(usageHttp.statusCode)
            else {
                return .unavailable(.error)
            }
            guard let usage = OpenCodeGoUsageParser.extractUsage(from: String(decoding: usageData, as: UTF8.self)) else {
                return .unavailable(.error)
            }
            return .success(usage)
        } catch let error as URLError where error.code == .timedOut {
            return .timedOut
        } catch {
            return .unavailable(.error)
        }
    }

    /// Returns `nil` on any failure (network error, non-2xx, unparseable
    /// body) — callers translate that into `.unavailable(.error)`.
    private static func discoverWorkspaceId(cookie: String) async -> String? {
        guard let workspacesURL = URL(string: "https://opencode.ai/_server?id=\(workspaceServerId)") else {
            return nil
        }
        var workspacesRequest = URLRequest(url: workspacesURL)
        workspacesRequest.setValue(cookie, forHTTPHeaderField: "Cookie")
        workspacesRequest.setValue(workspaceServerId, forHTTPHeaderField: "X-Server-Id")

        guard
            let (workspacesData, workspacesResponse) = try? await session.data(for: workspacesRequest),
            let workspacesHttp = workspacesResponse as? HTTPURLResponse,
            (200..<300).contains(workspacesHttp.statusCode)
        else {
            return nil
        }
        return OpenCodeGoUsageParser.extractWorkspaceId(from: String(decoding: workspacesData, as: UTF8.self))
    }
}
