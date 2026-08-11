import CoreGraphics
import SwiftUI

/// Where the split's divider sits, and how a card is shaped where it meets one.
///
/// The seam is built from paddings: half a gap on the outside of the split
/// container, plus whatever a column pads on its own outward side. A column's
/// measured width includes its own padding, so from the container's leading
/// edge the seam is the outer half-gap plus that width.
enum CardLayout {
    static func dividerCenterX(columnWidth: CGFloat, gap: CGFloat) -> CGFloat {
        gap / 2 + columnWidth
    }

    /// Corner radii for a card whose `flushEdges` sit against a neighbour with
    /// no gap between them.
    ///
    /// Two rounded corners meeting at a zero-width seam leave a notch of bare
    /// canvas above and below it, so a flush edge squares off both of its own
    /// corners.
    static func cardCorners(
        flushEdges: Edge.Set, radius: CGFloat
    ) -> RectangleCornerRadii {
        func radiusAt(_ first: Edge.Set, _ second: Edge.Set) -> CGFloat {
            flushEdges.contains(first) || flushEdges.contains(second) ? 0 : radius
        }

        return RectangleCornerRadii(
            topLeading: radiusAt(.leading, .top),
            bottomLeading: radiusAt(.leading, .bottom),
            bottomTrailing: radiusAt(.trailing, .bottom),
            topTrailing: radiusAt(.trailing, .top))
    }

    /// How far a card's shadow may reach past its own bounds on each side.
    ///
    /// A flush edge gets none. A shadow spilling sideways onto a neighbour is
    /// what makes a zero-width seam read as a black band — it, not the seam's
    /// width, is the dominant source of the dark stripe.
    static func shadowBleed(flushEdges: Edge.Set, radius: CGFloat) -> EdgeInsets {
        func bleedAt(_ edge: Edge.Set) -> CGFloat {
            flushEdges.contains(edge) ? 0 : radius * 2
        }

        return EdgeInsets(
            top: bleedAt(.top),
            leading: bleedAt(.leading),
            bottom: bleedAt(.bottom),
            trailing: bleedAt(.trailing))
    }
}
