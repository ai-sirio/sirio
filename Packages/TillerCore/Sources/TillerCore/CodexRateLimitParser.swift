import Foundation

// MARK: - Wire types (mirror the Codex app-server `account/rateLimits/read` result)

private struct CodexRpcWindow: Decodable {
    let usedPercent: Double?
    let windowDurationMins: Int?
    let resetsAt: Double?
}

private struct CodexRpcRateLimits: Decodable {
    let primary: CodexRpcWindow?
    let secondary: CodexRpcWindow?
}

private struct CodexRpcResult: Decodable {
    let rateLimits: CodexRpcRateLimits?
}

// MARK: - Parser (pure)

/// Parses the `result` payload of an `account/rateLimits/read` JSON-RPC
/// response into `ProviderUsage`. Ported from Orca's `codex-fetcher.ts`
/// `mapRpcWindow`: `resetsAt` is Unix **seconds**, and a window with no
/// `usedPercent` maps to `nil` rather than a zero-percent window.
public enum CodexRateLimitParser {
    public static func parse(resultData: Data) -> ProviderUsage? {
        guard let decoded = try? JSONDecoder().decode(CodexRpcResult.self, from: resultData) else {
            return nil
        }
        let session = window(decoded.rateLimits?.primary, label: "5h")
        let weekly = window(decoded.rateLimits?.secondary, label: "wk")
        guard session != nil || weekly != nil else { return nil }
        return ProviderUsage(session: session, weekly: weekly)
    }

    private static func window(_ raw: CodexRpcWindow?, label: String) -> UsageWindow? {
        guard let raw, let percent = raw.usedPercent else { return nil }
        let resetsAt = raw.resetsAt.map { Date(timeIntervalSince1970: $0) }
        return UsageWindow(label: label, usedPercent: Int(percent.rounded()), resetsAt: resetsAt)
    }
}
