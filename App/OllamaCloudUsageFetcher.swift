import Foundation
import TillerCore

/// Best-effort fetcher — see `OllamaCloudUsageParser`'s doc comment. Any
/// response the parser can't recognize becomes `.unavailable(.error)`,
/// never a crash or a hang.
enum OllamaCloudUsageFetcher {
    static let cookieKey = "ollama-cloud-cookie"

    private static let session: URLSession = {
        let config = URLSessionConfiguration.default
        config.timeoutIntervalForRequest = 12
        return URLSession(configuration: config)
    }()

    static func fetch() async -> UsageFetchOutcome {
        guard
            let rawCookie = KeychainCredentialStore.get(key: cookieKey),
            !rawCookie.trimmingCharacters(in: .whitespaces).isEmpty
        else {
            return .unavailable(.loggedOut)
        }
        guard let usageURL = URL(string: "https://ollama.com/settings") else {
            return .unavailable(.error)
        }
        var request = URLRequest(url: usageURL)
        request.setValue(rawCookie, forHTTPHeaderField: "Cookie")

        do {
            let (data, response) = try await session.data(for: request)
            guard
                let httpResponse = response as? HTTPURLResponse,
                (200..<300).contains(httpResponse.statusCode)
            else {
                return .unavailable(.error)
            }
            guard let usage = OllamaCloudUsageParser.extractUsage(from: String(decoding: data, as: UTF8.self)) else {
                return .unavailable(.error)
            }
            return .success(usage)
        } catch let error as URLError where error.code == .timedOut {
            return .timedOut
        } catch {
            return .unavailable(.error)
        }
    }
}
