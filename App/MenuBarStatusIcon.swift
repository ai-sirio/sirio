import SwiftUI
import TillerCore

/// Menu-bar glyph reflecting the aggregate agent status across all
/// active-agent worktrees. Spins while any worktree is `.running`.
struct MenuBarStatusIcon: View {
    let status: AgentStatus?

    @State private var isSpinning = false

    var body: some View {
        Image("AppIcon")
            .resizable()
            .scaledToFit()
            .frame(width: 18, height: 18)
            .rotationEffect(.degrees(isSpinning ? 360 : 0))
            .animation(
                status == .running
                    ? .linear(duration: 1).repeatForever(autoreverses: false)
                    : .default,
                value: isSpinning
            )
            .onAppear { isSpinning = status == .running }
            .onChange(of: status) { _, newStatus in isSpinning = newStatus == .running }
    }
}
