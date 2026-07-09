import GhosttyTerminal
import TillerCore

/// Terminal color theme matching the app's unified surface color family
/// (`AppTheme.background` in the App target) — a bit darker than the chrome
/// (see `AppSurfaceColor.terminalHex`) instead of falling back to Ghostty's
/// default "afterglow" background (#212121).
enum TillerTerminalTheme {
    static let theme = TerminalTheme(
        light: .alabaster,
        dark: .afterglow.appending(.background(AppSurfaceColor.terminalHex))
    )
}
