import Foundation

/// Single source of truth for the app's shared dark surface color.
/// `AppTheme.background` (SwiftUI, App target) and the terminal's Ghostty
/// theme (TillerTerminal) both derive from these components so the sidebar,
/// detail pane, and terminal surface never drift out of sync.
public enum AppSurfaceColor {
    /// Near-black chrome (#121216) carrying the same faint indigo tint as
    /// `terminal*` below — a mathematically neutral gray here read as a
    /// different color family than every other token in `AppTheme`.
    /// Luminance is unchanged from the previous #131313, so text contrast
    /// against the chrome is identical.
    public static let red: Double = 0.070
    public static let green: Double = 0.072
    public static let blue: Double = 0.086

    public static var hex: String {
        hexString(red: red, green: green, blue: blue)
    }

    /// Charcoal surface for terminal and chat panes (#1F1F26) — slightly
    /// blue-tinted, distinct from the neutral chrome above.
    public static let terminalRed: Double = 0.122
    public static let terminalGreen: Double = 0.122
    public static let terminalBlue: Double = 0.149

    public static var terminalHex: String {
        hexString(red: terminalRed, green: terminalGreen, blue: terminalBlue)
    }

    /// Shared perceived opacity for every translucent surface (sidebar
    /// material, Ghostty background-opacity, chat main pane).
    public static let translucentSurfaceOpacity: Double = 0.96

    public static func surfaceOpacity(translucencyEnabled: Bool) -> Double {
        translucencyEnabled ? translucentSurfaceOpacity : 1.0
    }

    private static func hexString(red: Double, green: Double, blue: Double) -> String {
        String(format: "%02X%02X%02X",
               Int((red * 255).rounded()),
               Int((green * 255).rounded()),
               Int((blue * 255).rounded()))
    }
}
