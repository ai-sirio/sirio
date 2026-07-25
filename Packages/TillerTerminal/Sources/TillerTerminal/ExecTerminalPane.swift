import SwiftUI
import GhosttyTerminal
import TillerCore

/// Spike surface: libghostty spawns and owns the login shell itself
/// (`.exec` backend), exactly like Ghostty.app. Zero PTY code on our side —
/// this isolates "does the engine render/input correctly" from our PTY work.
public struct ExecTerminalPane: View {
    @StateObject private var state = TerminalViewState(theme: TillerTerminalTheme.current())
    @AppStorage(AppSettings.terminalFontSizeKey)
    private var terminalFontSize = AppSettings.defaultTerminalFontSize
    @AppStorage(AppSettings.translucencyEnabledKey) private var translucencyEnabled = false

    public init() {}

    public var body: some View {
        TerminalSurfaceView(context: state)
            .onAppear {
                state.configuration = TerminalSurfaceOptions(backend: .exec)
                applyTheme()
            }
            .onChange(of: terminalFontSize) { _, _ in applyTheme() }
            .onChange(of: translucencyEnabled) { _, _ in applyTheme() }
    }

    private func applyTheme() {
        let size = Float(AppSettings.clampTerminalFontSize(terminalFontSize))
        state.setTheme(TillerTerminalTheme.theme(fontSize: size, translucencyEnabled: translucencyEnabled))
    }
}
