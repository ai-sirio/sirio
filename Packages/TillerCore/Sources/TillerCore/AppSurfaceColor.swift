import Foundation

/// Single source of truth for the app's shared surface colors.
/// `AppTheme.background`/`AppTheme.chatSurface` (SwiftUI, App target) and the
/// terminal's Ghostty theme (TillerTerminal) all derive from these
/// components so the sidebar, chat pane, and terminal surface never drift
/// out of sync.
public enum AppSurfaceColor {
    /// Primary sidebar/chrome surface (#1B1C1F).
    public static let red: Double = 27.0 / 255.0
    public static let green: Double = 28.0 / 255.0
    public static let blue: Double = 31.0 / 255.0

    public static var hex: String {
        hexString(red: red, green: green, blue: blue)
    }

    /// Primary sidebar/chrome surface in light appearance (#E9EAED).
    public static let lightRed: Double = 233.0 / 255.0
    public static let lightGreen: Double = 234.0 / 255.0
    public static let lightBlue: Double = 237.0 / 255.0

    public static var lightHex: String {
        hexString(red: lightRed, green: lightGreen, blue: lightBlue)
    }

    /// Central chat surface (#28292C).
    public static let chatRed: Double = 40.0 / 255.0
    public static let chatGreen: Double = 41.0 / 255.0
    public static let chatBlue: Double = 44.0 / 255.0

    public static var chatHex: String {
        hexString(red: chatRed, green: chatGreen, blue: chatBlue)
    }

    /// Central chat surface in light appearance (#F6F6F8).
    public static let chatLightRed: Double = 246.0 / 255.0
    public static let chatLightGreen: Double = 246.0 / 255.0
    public static let chatLightBlue: Double = 248.0 / 255.0

    public static var chatLightHex: String {
        hexString(red: chatLightRed, green: chatLightGreen, blue: chatLightBlue)
    }

    /// Terminal pane surface (#28292C) — matches the chat surface above so
    /// terminal and chat panes are visually identical. Kept as its own named
    /// token because it's consumed by a different subsystem: Ghostty's theme
    /// config (`TillerTerminal`), not SwiftUI (`AppTheme`, App target).
    public static let terminalRed: Double = chatRed
    public static let terminalGreen: Double = chatGreen
    public static let terminalBlue: Double = chatBlue

    public static var terminalHex: String {
        hexString(red: terminalRed, green: terminalGreen, blue: terminalBlue)
    }

    /// Terminal pane surface in light appearance (#F6F6F8).
    public static let terminalLightRed: Double = chatLightRed
    public static let terminalLightGreen: Double = chatLightGreen
    public static let terminalLightBlue: Double = chatLightBlue

    public static var terminalLightHex: String {
        hexString(red: terminalLightRed, green: terminalLightGreen, blue: terminalLightBlue)
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
