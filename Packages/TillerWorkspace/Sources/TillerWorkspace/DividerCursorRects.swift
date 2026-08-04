import AppKit

/// The bands where a split's dividers sit, for installing resize cursors.
///
/// Measured from the gaps between laid-out subviews rather than from subview
/// order: `WorkspaceNativeSplitView` exchanges the two content frames after
/// layout, so index 0 is not reliably the leading pane. `DividerPosition`
/// already learned this lesson for divider tracking.
/// Which events a divider's pointer target answers.
///
/// AppKit dispatches `cursorUpdate` through `hitTest`, so a view that refuses
/// every hit test never shows its cursor rect either — measured, not assumed.
/// A divider target therefore has to answer hover events, and must keep
/// refusing clicks so a mouse-down still reaches the divider underneath.
public enum DividerCursorHitPolicy {
    public static func acceptsHit(eventType: NSEvent.EventType?) -> Bool {
        switch eventType {
        case .mouseMoved, .cursorUpdate, .mouseEntered, .mouseExited: true
        default: false
        }
    }
}

public enum DividerCursorRects {
    /// - Parameter minimumThickness: widens each band around the gap it was
    ///   measured from, leaving the painted hairline where it is. The pointer
    ///   target and the visible seam are deliberately different sizes.
    public static func rects(
        subviewFrames: [CGRect],
        bounds: CGRect,
        isVertical: Bool,
        minimumThickness: CGFloat = 0
    ) -> [CGRect] {
        guard subviewFrames.count >= 2 else { return [] }
        let ordered = subviewFrames.sorted {
            isVertical ? $0.minX < $1.minX : $0.minY < $1.minY
        }

        return zip(ordered, ordered.dropFirst()).compactMap { leading, trailing in
            if isVertical {
                let gap = trailing.minX - leading.maxX
                guard gap > 0 else { return nil }
                return widened(
                    CGRect(x: leading.maxX, y: bounds.minY, width: gap, height: bounds.height),
                    to: minimumThickness, isVertical: isVertical)
            }
            let gap = trailing.minY - leading.maxY
            guard gap > 0 else { return nil }
            return widened(
                CGRect(x: bounds.minX, y: leading.maxY, width: bounds.width, height: gap),
                to: minimumThickness, isVertical: isVertical)
        }
    }

    private static func widened(
        _ band: CGRect, to thickness: CGFloat, isVertical: Bool
    ) -> CGRect {
        let current = isVertical ? band.width : band.height
        guard current < thickness else { return band }
        let growth = (thickness - current) / 2
        return isVertical
            ? band.insetBy(dx: -growth, dy: 0)
            : band.insetBy(dx: 0, dy: -growth)
    }
}
