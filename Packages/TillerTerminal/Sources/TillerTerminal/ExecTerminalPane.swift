import SwiftUI
import GhosttyTerminal
import TillerCore

/// Spike surface: libghostty spawns and owns the login shell itself
/// (`.exec` backend), exactly like Ghostty.app. Zero PTY code on our side —
/// this isolates "does the engine render/input correctly" from our PTY work.
public struct ExecTerminalPane: View {
    @StateObject private var state = TerminalViewState(theme: TillerTerminalTheme.theme)

    public init() {}

    public var body: some View {
        TerminalSurfaceView(context: state)
            .onAppear {
                state.configuration = TerminalSurfaceOptions(backend: .exec)
            }
    }
}
