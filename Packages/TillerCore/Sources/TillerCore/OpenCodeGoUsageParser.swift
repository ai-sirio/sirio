import Foundation

/// Pure parsing for OpenCode Go's cookie-scraped pages. Ported from Orca's
/// `opencode-go-page-scraper.ts` / `opencode-go-usage-fetcher.ts`.
public enum OpenCodeGoUsageParser {
    /// A bare token (no `=`) is wrapped as `auth=<token>`; anything already
    /// containing `=` (e.g. `auth=...` or `__Host-auth=...`) passes through.
    public static func normalizeCookie(_ raw: String) -> String {
        let trimmed = raw.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return trimmed }
        return trimmed.contains("=") ? trimmed : "auth=\(trimmed)"
    }

    /// Extracts the `wrk_...` workspace ID from the `/_server` response body.
    public static func extractWorkspaceId(from body: String) -> String? {
        guard let range = body.range(of: #"id:\s*"(wrk_[A-Za-z0-9]+)""#, options: .regularExpression) else {
            return nil
        }
        let matched = body[range]
        guard let idRange = matched.range(of: #"wrk_[A-Za-z0-9]+"#, options: .regularExpression) else {
            return nil
        }
        return String(matched[idRange])
    }

    /// Extracts session/weekly/monthly usage windows from the
    /// `/workspace/<id>/go` page body (React Flight wire format). The regex
    /// requires a `{...}` body after the key, so a bare `key:null`
    /// occurrence elsewhere on the page can never match — only the real
    /// data block can.
    public static func extractUsage(from body: String) -> ProviderUsage? {
        let session = window(in: body, key: "rollingUsage", label: "5h")
        let weekly = window(in: body, key: "weeklyUsage", label: "wk")
        let monthly = window(in: body, key: "monthlyUsage", label: "mo")
        let usage = ProviderUsage(session: session, weekly: weekly, monthly: monthly)
        return usage.hasAny ? usage : nil
    }

    /// Matches the `{...}` block after the key first, then extracts
    /// `usagePercent`/`resetInSec` from inside that block independently —
    /// order-agnostic, since real pages put `resetInSec` before
    /// `usagePercent` (unlike a naive single ordered regex would assume).
    private static func window(in body: String, key: String, label: String) -> UsageWindow? {
        let blockPattern = "\(key):(?:\\$R\\[\\d+\\]=)?\\{([^}]*)\\}"
        guard let blockRegex = try? NSRegularExpression(pattern: blockPattern) else { return nil }
        let nsBody = body as NSString
        guard
            let blockMatch = blockRegex.firstMatch(in: body, range: NSRange(location: 0, length: nsBody.length)),
            blockMatch.numberOfRanges >= 2
        else { return nil }
        let block = nsBody.substring(with: blockMatch.range(at: 1))

        guard
            let percent = firstInt(in: block, forKey: "usagePercent"),
            let resetInSec = firstInt(in: block, forKey: "resetInSec")
        else { return nil }
        let resetsAt = Date().addingTimeInterval(TimeInterval(resetInSec))
        return UsageWindow(label: label, usedPercent: percent, resetsAt: resetsAt)
    }

    private static func firstInt(in text: String, forKey key: String) -> Int? {
        let pattern = "\(key):(\\d+)"
        guard let regex = try? NSRegularExpression(pattern: pattern) else { return nil }
        let nsText = text as NSString
        guard
            let match = regex.firstMatch(in: text, range: NSRange(location: 0, length: nsText.length)),
            match.numberOfRanges >= 2
        else { return nil }
        return Int(nsText.substring(with: match.range(at: 1)))
    }
}
