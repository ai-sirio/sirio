import AppKit

public enum WorkspaceMetrics {
    public static let preferredGroupSize = CGSize(width: 240, height: 160)
    public static let dividerThickness: CGFloat = 10
    public static let dividerHairlineThickness: CGFloat = 1
    public static let tabStripHeight: CGFloat = 32
    public static let edgeBandFraction: CGFloat = 0.22
    public static let dragThreshold: CGFloat = 4

    /// The visible line inside the draggable divider band. The band is
    /// `dividerThickness` wide so it stays easy to grab, while only a hairline
    /// is painted — a full-width band would read as a gutter rather than a seam.
    /// `isVertical` follows `NSSplitView.isVertical`: a vertical divider
    /// separates side-by-side panes, so the line itself runs top to bottom.
    public static func dividerHairline(in band: CGRect, isVertical: Bool) -> CGRect {
        let axisLength = isVertical ? band.width : band.height
        guard axisLength > dividerHairlineThickness else { return band }
        let inset = (axisLength - dividerHairlineThickness) / 2
        return isVertical
            ? CGRect(x: band.minX + inset, y: band.minY,
                     width: dividerHairlineThickness, height: band.height)
            : CGRect(x: band.minX, y: band.minY + inset,
                     width: band.width, height: dividerHairlineThickness)
    }
}
