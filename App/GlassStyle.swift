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

    /// Sfondo della tab: capsula glass su 26+ (tint accent se attiva),
    /// flat legacy sotto — mai entrambi (doppio sfondo = incrostazione).
    @ViewBuilder
    func tillerTabBackground(isActive: Bool) -> some View {
        if #available(macOS 26.0, *) {
            glassEffect(isActive ? .regular.tint(.accentColor) : .regular, in: .capsule)
        } else {
            background(
                isActive ? Color.primary.opacity(0.12) : .clear,
                in: RoundedRectangle(cornerRadius: 5)
            )
        }
    }
}

/// GlassEffectContainer su 26+ (merge/morphing delle forme glass adiacenti),
/// passthrough sotto.
struct TillerGlassContainer<Content: View>: View {
    var spacing: CGFloat = 12
    @ViewBuilder var content: () -> Content

    var body: some View {
        if #available(macOS 26.0, *) {
            GlassEffectContainer(spacing: spacing, content: content)
        } else {
            content()
        }
    }
}
