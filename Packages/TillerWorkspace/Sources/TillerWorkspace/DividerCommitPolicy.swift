import AppKit

/// `NSSplitView` reports subview resizes without telling you whether a pointer
/// gesture is still in flight. The current event does: a drag is mid-gesture, a
/// mouse-up is the release, anything else is layout doing its job.
public enum DividerCommitPolicy {
    public enum Step: Equatable, Sendable {
        case move
        case commit
        case ignore
    }

    public static func step(for eventType: NSEvent.EventType?) -> Step {
        switch eventType {
        case .leftMouseDragged: return .move
        case .leftMouseUp: return .commit
        default: return .ignore
        }
    }
}

/// Where the divider sits, measured in reading order — from the left for a
/// side-by-side split, from the top for a stacked one.
///
/// It reads the two content frames by position rather than by index because the
/// vertical axis exchanges them after native layout, so subview 0 is not
/// reliably the pane Core calls first.
public enum DividerPosition {
    public static func readingOrder(
        first: CGRect,
        second: CGRect,
        bounds: CGRect,
        isVertical: Bool
    ) -> CGFloat {
        isVertical
            ? min(first.maxX, second.maxX)
            : bounds.height - max(first.minY, second.minY)
    }
}
