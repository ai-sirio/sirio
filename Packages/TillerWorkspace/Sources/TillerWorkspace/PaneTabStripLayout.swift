import CoreGraphics

public enum PaneTabStripLayout {
    public static let inactiveMinimumWidth: CGFloat = 72
    public static let activeMinimumWidth: CGFloat = 118
    public static let maximumWidth: CGFloat = 160
    public static let closeControlWidth: CGFloat = 16
    public static let overflowTolerance: CGFloat = 1

    public static func minimumWidth(isActive: Bool) -> CGFloat {
        isActive ? activeMinimumWidth : inactiveMinimumWidth
    }

    public static func isOverflowing(
        contentWidth: CGFloat,
        viewportWidth: CGFloat
    ) -> Bool {
        contentWidth > viewportWidth + overflowTolerance
    }
}
