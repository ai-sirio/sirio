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
    public static func rects(
        subviewFrames: [CGRect],
        bounds: CGRect,
        isVertical: Bool
    ) -> [CGRect] {
        guard subviewFrames.count >= 2 else { return [] }
        let ordered = subviewFrames.sorted {
            isVertical ? $0.minX < $1.minX : $0.minY < $1.minY
        }

        return zip(ordered, ordered.dropFirst()).compactMap { leading, trailing in
            if isVertical {
                let gap = trailing.minX - leading.maxX
                guard gap > 0 else { return nil }
                return CGRect(
                    x: leading.maxX, y: bounds.minY, width: gap, height: bounds.height)
            }
            let gap = trailing.minY - leading.maxY
            guard gap > 0 else { return nil }
            return CGRect(
                x: bounds.minX, y: leading.maxY, width: bounds.width, height: gap)
        }
    }
}
