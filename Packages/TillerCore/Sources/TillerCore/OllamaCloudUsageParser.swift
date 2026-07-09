import Foundation

/// Best-effort parser for Ollama Cloud's usage page. **The real page
/// structure is unverified** (no public API, no logged-in account was
/// available while writing this) — see the spec's Sub-phase 3 for context.
/// Structured the same way as `OpenCodeGoUsageParser` so a future fix, once
/// someone can see the real page, is a parser-only change. Any unrecognized
/// shape returns `nil` — this is the expected degradation path, not a bug.
public enum OllamaCloudUsageParser {
    public static func extractUsage(from body: String) -> ProviderUsage? {
        guard let session = window(in: body, key: "usagePercent", label: "usage") else { return nil }
        return ProviderUsage(session: session, weekly: nil)
    }

    private static func window(in body: String, key: String, label: String) -> UsageWindow? {
        let pattern = "\(key)\\s*:\\s*(\\d+)"
        guard let regex = try? NSRegularExpression(pattern: pattern) else { return nil }
        let nsBody = body as NSString
        guard
            let match = regex.firstMatch(in: body, range: NSRange(location: 0, length: nsBody.length)),
            match.numberOfRanges >= 2,
            let percent = Int(nsBody.substring(with: match.range(at: 1)))
        else { return nil }
        return UsageWindow(label: label, usedPercent: percent)
    }
}
