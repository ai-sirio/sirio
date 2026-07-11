import Foundation

/// App-preference bounds and defaults. Foundation-only so the timer can never
/// be driven by a corrupt or out-of-range stored interval.
public enum AppSettings {
    public static let defaultRefreshSeconds = 300
    public static let refreshRange: ClosedRange<Int> = 60...3600

    /// Clamp a stored refresh interval (seconds) into the allowed range.
    public static func clampRefresh(_ seconds: Int) -> Int {
        min(max(seconds, refreshRange.lowerBound), refreshRange.upperBound)
    }
    
    /// UserDefaults key for the "resume agent sessions on launch" toggle.
    /// Missing value means enabled (default true).
    public static let resumeAgentSessionsKey = "resumeAgentSessions"

    /// UserDefaults key for the control-socket toggle. Missing value means
    /// enabled (default true) — disabling it also disables agent hooks.
    public static let controlSocketEnabledKey = "controlSocket.enabled"

    /// Resolve whether the control socket should start.
    /// TILLER_SOCKET_ENABLE (1/0, true/false, on/off — case-insensitive)
    /// overrides the stored preference; anything else falls through.
    public static func controlSocketEnabled(defaultsValue: Bool?, env: [String: String]) -> Bool {
        if let raw = env["TILLER_SOCKET_ENABLE"] {
            switch raw.lowercased() {
            case "1", "true", "on": return true
            case "0", "false", "off": return false
            default: break
            }
        }
        return defaultsValue ?? true
    }

    /// UserDefaults key for the app appearance (AppAppearance rawValue).
    /// Missing value means `.system`.
    public static let appearanceThemeKey = "appearance.theme"

    /// UserDefaults key for the terminal font size in points.
    public static let terminalFontSizeKey = "appearance.terminalFontSize"
    public static let defaultTerminalFontSize = 13
    public static let terminalFontSizeRange: ClosedRange<Int> = 9...24

    /// Clamp a stored terminal font size into the allowed range.
    public static func clampTerminalFontSize(_ size: Int) -> Int {
        min(max(size, terminalFontSizeRange.lowerBound), terminalFontSizeRange.upperBound)
    }
}
