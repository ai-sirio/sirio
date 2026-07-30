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
    public static func resolve(
        pointInGroup: CGPoint,
        groupBounds: CGRect,
        tabStripHeight: CGFloat,
        tabFrames: [CGRect],
        draggedTab: WorkspaceTabID,
        sourceGroup: PaneGroupID,
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
            return .tabStrip(sourceGroup, insertionIndex: insertionIndex(
                for: pointInGroup.x, in: tabFrames
            ))
        }

        if groupTabCount == 1 {
            return .center(sourceGroup)
        }

        let distances: [(EdgePlacement, CGFloat, CGFloat)] = [
            (.left, pointInGroup.x - groupBounds.minX, groupBounds.width * WorkspaceMetrics.edgeBandFraction),
            (.right, groupBounds.maxX - pointInGroup.x, groupBounds.width * WorkspaceMetrics.edgeBandFraction),
            (.bottom, pointInGroup.y - groupBounds.minY, groupBounds.height * WorkspaceMetrics.edgeBandFraction),
            (.top, groupBounds.maxY - pointInGroup.y, groupBounds.height * WorkspaceMetrics.edgeBandFraction)
        ]
        guard let nearest = distances.min(by: { $0.1 < $1.1 }), nearest.1 <= nearest.2 else {
            return .center(sourceGroup)
        }
        return .edge(sourceGroup, placement: nearest.0)
    }

    private static func insertionIndex(for x: CGFloat, in frames: [CGRect]) -> Int {
        for (index, frame) in frames.enumerated() where x < frame.midX {
            return index
        }
        return frames.count
    }
}
