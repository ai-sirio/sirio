import CoreGraphics

/// Where the split's divider sits once the columns are padded apart.
///
/// The gap is built from three paddings: half a gap inside each column, plus
/// half a gap on the outside of the split container. A column's measured width
/// includes its own halves, so from the container's leading edge the divider
/// centre is the outer half-gap plus that width.
enum CardLayout {
    static func dividerCenterX(columnWidth: CGFloat, gap: CGFloat) -> CGFloat {
        gap / 2 + columnWidth
    }
}
