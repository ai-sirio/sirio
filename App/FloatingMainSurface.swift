import SwiftUI

struct FloatingMainSurface<Content: View>: View {
    @Environment(\.colorScheme) private var colorScheme
    private let tint: Color
    private let content: () -> Content

    init(tint: Color = AppTheme.terminalSurface,
         @ViewBuilder content: @escaping () -> Content) {
        self.tint = tint
        self.content = content
    }

    /// Square on top, rounded at the bottom: the surface is flush with the
    /// window's top edge, and a corner radius there would round away from the
    /// window's own square top, reading as a misalignment rather than a card.
    private var shape: UnevenRoundedRectangle {
        UnevenRoundedRectangle(
            bottomLeadingRadius: AppTheme.cardCornerRadius,
            bottomTrailingRadius: AppTheme.cardCornerRadius,
            style: .continuous)
    }

    var body: some View {
        content()
            .background { MainSurfaceMaterial(tint: tint) }
            .clipShape(shape)
            .shadow(
                color: .black.opacity(colorScheme == .dark ? 0.36 : 0.14),
                radius: AppTheme.cardShadowRadius,
                y: AppTheme.cardShadowYOffset)
    }
}
