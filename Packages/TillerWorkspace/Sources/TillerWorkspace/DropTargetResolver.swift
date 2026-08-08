import CoreGraphics
import TillerCore

public enum DropTarget: Equatable, Sendable {
    case tabStrip(PaneGroupID, insertionIndex: Int)
    case center(PaneGroupID)
    case edge(PaneGroupID, placement: EdgePlacement)
    case none
}

public enum EdgePlacement: Equatable, Sendable {
    case left
    case right
    case top
    case bottom
}

public enum DropTargetResolver {
    /// All geometry is in workspace-root flipped coordinates: the origin is the
    /// top-left of the workspace and `y` grows downward, so the tab strip
    /// occupies the band starting at `groupBounds.minY` and the edge nearest
    /// the strip is `.top`.
    ///
    /// `sourceGroup` is where the dragged tab came from; `hoveredGroup` is the
    /// group under the pointer. They differ on every cross-pane drag, and
    /// conflating them makes a one-tab pane reject splits it should accept.
    public static func resolve(
        pointInGroup: CGPoint,
        groupBounds: CGRect,
        tabStripHeight: CGFloat,
        tabFrames: [CGRect],
        draggedTab: WorkspaceTabID?,
        sourceGroup: PaneGroupID?,
        hoveredGroup: PaneGroupID,
        groupTabCount: Int
    ) -> DropTarget {
        _ = draggedTab

        guard groupBounds.contains(pointInGroup) else { return .none }

        let strip = CGRect(
            x: groupBounds.minX,
            y: groupBounds.minY,
            width: groupBounds.width,
            height: min(max(0, tabStripHeight), groupBounds.height)
        )
        if strip.contains(pointInGroup) {
            return .tabStrip(hoveredGroup, insertionIndex: insertionIndex(
                for: pointInGroup.x, in: tabFrames
            ))
        }

        // Rule 7: only the tab's *own* group is barred, and only when taking
        // that tab out would leave the group empty.
        if let sourceGroup, hoveredGroup == sourceGroup, groupTabCount == 1 {
            return .center(hoveredGroup)
        }

        let horizontalBand = groupBounds.width * WorkspaceMetrics.edgeBandFraction
        let verticalBand = groupBounds.height * WorkspaceMetrics.edgeBandFraction
        let distances: [(EdgePlacement, CGFloat, CGFloat)] = [
            (.left, pointInGroup.x - groupBounds.minX, horizontalBand),
            (.right, groupBounds.maxX - pointInGroup.x, horizontalBand),
            (.top, pointInGroup.y - groupBounds.minY, verticalBand),
            (.bottom, groupBounds.maxY - pointInGroup.y, verticalBand)
        ]
        guard let nearest = distances.min(by: { $0.1 < $1.1 }), nearest.1 <= nearest.2 else {
            return .center(hoveredGroup)
        }
        return .edge(hoveredGroup, placement: nearest.0)
    }

    private static func insertionIndex(for x: CGFloat, in frames: [CGRect]) -> Int {
        for (index, frame) in frames.enumerated() where x < frame.midX {
            return index
        }
        return frames.count
    }
}
