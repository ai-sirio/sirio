import SwiftUI

// Unico punto (insieme a StatusBadge in SidebarView) in cui vive
// #available(macOS 26.0, *): i call-site restano una riga e alzare il
// deployment target in futuro significa cancellare questo file.

extension View {
    /// Glass su macOS 26+, no-op sotto (aspetto attuale invariato).
    @ViewBuilder
    func tillerGlass(in shape: some Shape, tint: Color? = nil) -> some View {
        if #available(macOS 26.0, *) {
            glassEffect(tint.map { .regular.tint($0) } ?? .regular, in: shape)
        } else {
            self
        }
    }

    /// Stile bottone .glass su 26+, .bordered sotto.
    @ViewBuilder
    func tillerGlassButtonStyle() -> some View {
        if #available(macOS 26.0, *) {
            buttonStyle(.glass)
        } else {
            buttonStyle(.bordered)
        }
    }
}
