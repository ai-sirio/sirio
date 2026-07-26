import Foundation

/// Single source of truth for the app's shared dark surface colors.
/// `AppTheme.background`/`AppTheme.chatSurface` (SwiftUI, App target) and the
/// terminal's Ghostty theme (TillerTerminal) all derive from these
/// components so the sidebar, chat pane, and terminal surface never drift
/// out of sync.
public enum AppSurfaceColor {
    /// Warm graphite chrome (#1A1A1E) for the sidebar/titlebar/usage bar —
    /// only a faint blue lean versus the neutral grays elsewhere in `AppTheme`,
    /// deliberately less indigo than the chat/terminal surfaces below so it
    /// reads as a different, warmer color family.
    public static let red: Double = 0.102
    public static let green: Double = 0.102
    public static let blue: Double = 0.118

    public static var hex: String {
        hexString(red: red, green: green, blue: blue)
    }

    /// Near-black chat surface (#121216) — the app's darkest, most neutral
    /// tone, one shade below the chrome above so the chat pane reads as the
    /// most recessive surface in the window.
    public static let chatRed: Double = 0.070
    public static let chatGreen: Double = 0.072
    public static let chatBlue: Double = 0.086

    public static var chatHex: String {
        hexString(red: chatRed, green: chatGreen, blue: chatBlue)
    }

    /// Terminal pane surface (#121216) — matches the chat surface above so
    /// terminal and chat panes are visually identical. Kept as its own named
    /// token because it's consumed by a different subsystem: Ghostty's theme
    /// config (`TillerTerminal`), not SwiftUI (`AppTheme`, App target).
    public static let terminalRed: Double = chatRed
    public static let terminalGreen: Double = chatGreen
    public static let terminalBlue: Double = chatBlue

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
