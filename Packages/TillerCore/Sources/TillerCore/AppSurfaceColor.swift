import Foundation

/// Single source of truth for the app's shared dark surface color.
/// `AppTheme.background` (SwiftUI, App target) and the terminal's Ghostty
/// theme (TillerTerminal) both derive from these components so the sidebar,
/// detail pane, and terminal surface never drift out of sync.
public enum AppSurfaceColor {
    public static let red: Double = 0.075
    public static let green: Double = 0.075
    public static let blue: Double = 0.075

    public static var hex: String {
        hexString(red: red, green: green, blue: blue)
    }

    /// A bit darker than `background` — the terminal surface intentionally
    /// reads as a recessed focus area rather than flush with the chrome.
    public static let terminalRed: Double = red * 0.7
    public static let terminalGreen: Double = green * 0.7
    public static let terminalBlue: Double = blue * 0.7

    public static var terminalHex: String {
        hexString(red: terminalRed, green: terminalGreen, blue: terminalBlue)
    }

    private static func hexString(red: Double, green: Double, blue: Double) -> String {
        String(format: "%02X%02X%02X", Int(red * 255), Int(green * 255), Int(blue * 255))
    }
}
