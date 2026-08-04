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

    var body: some View {
        content()
            .background { MainSurfaceMaterial(tint: tint) }
            .clipShape(RoundedRectangle(
                cornerRadius: AppTheme.mainSurfaceCornerRadius,
                style: .continuous))
            .overlay {
                RoundedRectangle(
                    cornerRadius: AppTheme.mainSurfaceCornerRadius,
                    style: .continuous)
                    .stroke(AppTheme.mainSurfaceBorder, lineWidth: 1)
            }
            .shadow(
                color: .black.opacity(colorScheme == .dark ? 0.36 : 0.14),
                radius: AppTheme.mainSurfaceShadowRadius,
                y: AppTheme.mainSurfaceShadowYOffset)
    }
}
