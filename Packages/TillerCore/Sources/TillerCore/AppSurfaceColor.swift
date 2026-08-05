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

    /// Window canvas behind the floating cards (#131417). Every panel is a card
    /// on top of this; it is the only surface that reaches the window edges.
    public static let canvasRed: Double = 19.0 / 255.0
    public static let canvasGreen: Double = 20.0 / 255.0
    public static let canvasBlue: Double = 23.0 / 255.0

    public static var canvasHex: String {
        hexString(red: canvasRed, green: canvasGreen, blue: canvasBlue)
    }

    /// Window canvas in light appearance (#DCDDE2). Darker than the card, not
    /// lighter: a lighter canvas makes the cards sink instead of rest.
    public static let canvasLightRed: Double = 220.0 / 255.0
    public static let canvasLightGreen: Double = 221.0 / 255.0
    public static let canvasLightBlue: Double = 226.0 / 255.0

    public static var canvasLightHex: String {
        hexString(red: canvasLightRed, green: canvasLightGreen, blue: canvasLightBlue)
    }

    /// Central chat surface — the same colour as the sidebar chrome. Every
    /// floating card shares one surface; the canvas behind them is what
    /// separates them, not a difference in fill. Kept as its own name because
    /// `TillerTerminal` and `AppTheme` reach for it under this name.
    public static let chatRed: Double = red
    public static let chatGreen: Double = green
    public static let chatBlue: Double = blue

    public static var chatHex: String {
        hexString(red: chatRed, green: chatGreen, blue: chatBlue)
    }

    /// Central chat surface in light appearance — see `chatRed`.
    public static let chatLightRed: Double = lightRed
    public static let chatLightGreen: Double = lightGreen
    public static let chatLightBlue: Double = lightBlue

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
