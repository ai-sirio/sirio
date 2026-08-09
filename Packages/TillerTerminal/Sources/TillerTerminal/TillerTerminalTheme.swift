import Foundation
import GhosttyTerminal
import TillerCore

/// Terminal color theme matching the app's unified surface color family:
/// #28292C for dark mode and #F6F6F8 for light mode. Font size is injected
/// into both configurations.
enum TillerTerminalTheme {
    /// The terminal deliberately does *not* follow the interface font.
    /// Agent CLIs (Claude Code, Codex) draw box-drawing and Nerd Font glyphs
    /// in their TUIs; SF Mono has no coverage for those and renders tofu.
    ///
    /// Ghostty resolves this by name through CoreText, which substitutes
    /// silently rather than failing — an unavailable family here comes back
    /// as a proportional font, not an error. `TillerTerminalThemeTests`
    /// pins the name for that reason.
    static let fontFamily = "MesloLGS Nerd Font Mono"

    static func theme(fontSize: Float, translucencyEnabled: Bool = true) -> TerminalTheme {
        // Ghostty's surface scrollback defaults to 10MB/pane; Tiller keeps
        // many worktrees' panes mounted at once (openWorktreeIds), so that
        // multiplies fast. Matched to ScrollbackBuffer's own 256KB cap
        // (TillerTerminal/ScrollbackBuffer.swift) — no point the live view
        // holding more than what gets persisted across restarts.
        let scrollbackLimit = TerminalConfigCommand.custom(key: "scrollback-limit", value: "262144")
        var darkConfiguration = TerminalConfiguration.afterglow
            .appending(.background(AppSurfaceColor.terminalHex))
        if translucencyEnabled {
            // Translucent terminal surface matching the sidebar's perceived
            // opacity; blur radius is an empirical starting point to visually
            // match NSVisualEffectView's sidebar material.
            darkConfiguration = darkConfiguration
                .appending(TerminalConfigCommand.custom(
                    key: "background-opacity",
                    value: "\(AppSurfaceColor.surfaceOpacity(translucencyEnabled: true))"))
                .appending(TerminalConfigCommand.custom(
                    key: "background-blur-radius", value: "20"))
        }
        let lightConfiguration = TerminalConfiguration.alabaster
                .appending(.background(AppSurfaceColor.terminalLightHex))
                .appending(.fontSize(fontSize))
                .appending(scrollbackLimit)
        return TerminalTheme(
            light: lightConfiguration.appending(.fontFamily(fontFamily)),
            dark: darkConfiguration
                .appending(.fontSize(fontSize))
                .appending(scrollbackLimit)
                .appending(.fontFamily(fontFamily))
        )
    }

    /// Theme for the currently stored font size — used at pane creation,
    /// before SwiftUI property wrappers are available.
    static func current(defaults: UserDefaults = .standard) -> TerminalTheme {
        let stored = defaults.object(forKey: AppSettings.terminalFontSizeKey) as? Int
            ?? AppSettings.defaultTerminalFontSize
        let translucencyEnabled = AppSettings.translucencyEnabled(
            defaultsValue: defaults.object(forKey: AppSettings.translucencyEnabledKey) as? Bool)
        return theme(
            fontSize: Float(AppSettings.clampTerminalFontSize(stored)),
            translucencyEnabled: translucencyEnabled)
    }
}
