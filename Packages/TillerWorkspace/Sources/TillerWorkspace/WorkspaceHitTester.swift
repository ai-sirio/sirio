import CoreGraphics
import TillerCore

/// One pane's geometry, in workspace-root flipped coordinates, as the drag
/// machinery sees it. The collector builds these from live view frames; tests
/// build them by hand, which is why the drag logic never needs a window.
public struct PaneGroupHitFrame: Equatable, Sendable {
    public let id: PaneGroupID
    /// The whole pane, tab strip included.
    public let bounds: CGRect
    /// Tab rectangles in the same space, in visual order.
    public let tabFrames: [CGRect]
    public let tabCount: Int

    public init(id: PaneGroupID, bounds: CGRect, tabFrames: [CGRect], tabCount: Int) {
        self.id = id
        self.bounds = bounds
        self.tabFrames = tabFrames
        self.tabCount = tabCount
    }
}

public enum WorkspaceHitTester {
    /// Panes tile without overlapping, so the first frame containing the point
    /// is the only one that can.
    public static func target(
        at point: CGPoint,
        in frames: [PaneGroupHitFrame],
        draggedTab: WorkspaceTabID? = nil,
        sourceGroup: PaneGroupID? = nil,
        tabStripHeight: CGFloat = WorkspaceMetrics.tabStripHeight
    ) -> DropTarget {
        guard let frame = frames.first(where: { $0.bounds.contains(point) }) else {
            return .none
        }

        return DropTargetResolver.resolve(
            pointInGroup: point,
            groupBounds: frame.bounds,
            tabStripHeight: tabStripHeight,
            tabFrames: frame.tabFrames,
            draggedTab: draggedTab,
            sourceGroup: sourceGroup,
            hoveredGroup: frame.id,
            groupTabCount: frame.tabCount
        )
    }
}
