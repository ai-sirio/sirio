import SwiftUI
import Inject

/// A panel resting on the window canvas: tinted, rounded, and shadowed.
///
/// `flushEdges` names the sides that sit against a neighbouring card with no
/// gap. Those sides lose both their corner rounding and their shadow bleed:
/// at a zero-width seam a rounded corner leaves a notch of bare canvas, and a
/// shadow spilling sideways reads as a black band down the seam.
struct FloatingCard<Content: View>: View {
    @ObserveInjection private var inject

    @Environment(\.colorScheme) private var colorScheme
    private let tint: Color
    private let flushEdges: Edge.Set
    private let content: () -> Content

    init(tint: Color = AppTheme.background,
         flushEdges: Edge.Set = [],
         @ViewBuilder content: @escaping () -> Content) {
        self.tint = tint
        self.flushEdges = flushEdges
        self.content = content
    }

    var body: some View {
        content()
            .background { MainSurfaceMaterial(tint: tint) }
            .clipShape(UnevenRoundedRectangle(
                cornerRadii: CardLayout.cardCorners(
                    flushEdges: flushEdges, radius: AppTheme.cardCornerRadius),
                style: .continuous))
            .compositingGroup()
            .shadow(
                color: .black.opacity(colorScheme == .dark ? 0.36 : 0.14),
                radius: AppTheme.cardShadowRadius,
                y: AppTheme.cardShadowYOffset)
            .clipShape(ShadowBleedRect(insets: CardLayout.shadowBleed(
                flushEdges: flushEdges, radius: AppTheme.cardShadowRadius)))
    .enableInjection()
    }
}

/// The card's own bounds grown by however far its shadow is allowed to reach.
///
/// `.compositingGroup()` flattens the card before the shadow is applied, so
/// clipping the shadowed result to this shape is what confines the shadow to
/// the sides that are not flush against a neighbour.
///
/// The insets are read straight off the rect's edges, which assumes a
/// left-to-right layout — the only direction this window is laid out in.
private struct ShadowBleedRect: Shape {
    let insets: EdgeInsets

    func path(in rect: CGRect) -> Path {
        Path(CGRect(
            x: rect.minX - insets.leading,
            y: rect.minY - insets.top,
            width: rect.width + insets.leading + insets.trailing,
            height: rect.height + insets.top + insets.bottom))
    }
}
