import SwiftUI
import TillerCore

/// Menu-bar glyph reflecting the aggregate agent status across all
/// active-agent worktrees. Spins while any worktree is `.running`.
///
/// Usa il glifo a raggi dell'app come immagine TEMPLATE (MenuBarIcon):
/// l'icona completa (`Image("AppIcon")`) ha lo sfondo quasi nero e sulla
/// menu bar scura spariva in un disco invisibile. Il template è la
/// convenzione macOS: il sistema lo colora da solo su barra chiara/scura.
struct MenuBarStatusIcon: View {
    let status: AgentStatus?

    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var isSpinning = false

    var body: some View {
        Image("MenuBarIcon")
            .resizable()
            .scaledToFit()
            .frame(width: 18, height: 18)
            .rotationEffect(.degrees(isSpinning && !reduceMotion ? 360 : 0))
            .animation(
                status == .running && !reduceMotion
                    ? .linear(duration: 1).repeatForever(autoreverses: false)
                    : .default,
                value: isSpinning
            )
            .onAppear { isSpinning = status == .running }
            .onChange(of: status) { _, newStatus in isSpinning = newStatus == .running }
    }
}
