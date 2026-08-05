import SwiftUI

/// One panel resting on the window canvas. All three columns — sidebar,
/// central pane, right panel — are these; the canvas showing through the gaps
/// between them is what separates them, so the card draws no border of its own.
struct FloatingCard<Content: View>: View {
    @Environment(\.colorScheme) private var colorScheme
    private let tint: Color
    private let content: () -> Content

    init(tint: Color = AppTheme.background,
         @ViewBuilder content: @escaping () -> Content) {
        self.tint = tint
        self.content = content
    }

    var body: some View {
        content()
            .background { MainSurfaceMaterial(tint: tint) }
            .clipShape(RoundedRectangle(cornerRadius: AppTheme.cardCornerRadius,
                                        style: .continuous))
            .shadow(
                color: .black.opacity(colorScheme == .dark ? 0.36 : 0.14),
                radius: AppTheme.cardShadowRadius,
                y: AppTheme.cardShadowYOffset)
    }
}
