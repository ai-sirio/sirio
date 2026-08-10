import SwiftUI

/// Per-agent accent color for the chat composer: focus border, the
/// processing-turn animated border, and the send button. User-configurable
/// in Settings (`AppearanceSettingsView`); these are only the shipped
/// defaults. Unrelated to `AgentIcon.color(for:)`, which is a different,
/// pre-existing mapping used for icon-badge fallbacks elsewhere in the UI.
enum AgentAccentColor {
    static let defaultHexByAgentId: [String: String] = [
        "claude": "D97757",
        "codex": "0A84FF",
        "omp": "9B4DFF",
        "opencode": "FF9500",
        "pi": "34C759",
    ]

    /// Neutral gray for any agent id not in the table above (future
    /// adapters land here until given a real default).
    static let fallbackHex = "8E8E93"

    static func defaultHex(for agentId: String) -> String {
        defaultHexByAgentId[agentId] ?? fallbackHex
    }

    /// Prefers a validly-formatted stored hex string; falls back to the
    /// agent's default for a missing or malformed one.
    static func resolvedHex(for agentId: String, storedValue: String?) -> String {
        if let storedValue, Color(hex: storedValue) != nil {
            return storedValue
        }
        return defaultHex(for: agentId)
    }

    /// `storedValue` is read by the caller (see `AgentAccentColorProvider`,
    /// typically via `@AppStorage`) — this function stays pure and
    /// UserDefaults-free so it's trivially testable.
    static func color(for agentId: String, storedValue: String?) -> Color {
        // Force-unwrap is safe: `resolvedHex` only ever returns either a
        // value `Color(hex:)` already validated, or one of the hardcoded
        // hex strings in `defaultHexByAgentId`/`fallbackHex` above, all of
        // which are valid "#RRGGBB"/"RRGGBB" by construction.
        Color(hex: resolvedHex(for: agentId, storedValue: storedValue))!
    }
}
