import Foundation
import GhosttyTerminal
import TillerCore

/// Terminal color theme matching the app's unified surface color family
/// (`AppTheme.background` in the App target) — a bit darker than the chrome
/// (see `AppSurfaceColor.terminalHex`) instead of falling back to Ghostty's
/// default "afterglow" background (#212121). Light mode uses the stock
/// alabaster preset. Font size is injected into both configurations.
enum TillerTerminalTheme {
    static func theme(fontSize: Float) -> TerminalTheme {
        // Ghostty's surface scrollback defaults to 10MB/pane; Tiller keeps
        // many worktrees' panes mounted at once (openWorktreeIds), so that
        // multiplies fast. Matched to ScrollbackBuffer's own 256KB cap
        // (TillerTerminal/ScrollbackBuffer.swift) — no point the live view
        // holding more than what gets persisted across restarts.
        let scrollbackLimit = TerminalConfigCommand.custom(key: "scrollback-limit", value: "262144")
        return TerminalTheme(
            light: TerminalConfiguration.alabaster
                .appending(.fontSize(fontSize))
                .appending(scrollbackLimit),
            dark: TerminalConfiguration.afterglow
                .appending(.background(AppSurfaceColor.terminalHex))
                .appending(.fontSize(fontSize))
                .appending(scrollbackLimit)
        )
    }

    /// Theme for the currently stored font size — used at pane creation,
    /// before SwiftUI property wrappers are available.
    static func current(defaults: UserDefaults = .standard) -> TerminalTheme {
        let stored = defaults.object(forKey: AppSettings.terminalFontSizeKey) as? Int
            ?? AppSettings.defaultTerminalFontSize
        return theme(fontSize: Float(AppSettings.clampTerminalFontSize(stored)))
    }
}
