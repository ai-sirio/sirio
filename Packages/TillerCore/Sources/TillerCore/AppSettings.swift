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
}
