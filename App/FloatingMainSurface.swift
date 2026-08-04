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
            bottomLeadingRadius: AppTheme.mainSurfaceCornerRadius,
            bottomTrailingRadius: AppTheme.mainSurfaceCornerRadius,
            style: .continuous)
    }

    var body: some View {
        content()
            .background { MainSurfaceMaterial(tint: tint) }
            // The border is drawn 1pt taller than the surface so its top edge
            // falls outside the clip: at the window's top edge a hairline reads
            // as a seam under the traffic lights, not as the card's outline.
            .overlay {
                shape
                    .strokeBorder(AppTheme.mainSurfaceBorder, lineWidth: 1)
                    .padding(.top, -1)
            }
            .clipShape(shape)
            .shadow(
                color: .black.opacity(colorScheme == .dark ? 0.36 : 0.14),
                radius: AppTheme.mainSurfaceShadowRadius,
                y: AppTheme.mainSurfaceShadowYOffset)
    }
}
