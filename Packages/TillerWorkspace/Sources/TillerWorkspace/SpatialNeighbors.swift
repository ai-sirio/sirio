import CoreGraphics
import TillerCore

public enum FocusDirection: Sendable {
    case left
    case right
    case up
    case down
}

public enum SpatialNeighbors {
    /// Resolves the nearest pane in a direction without wrapping at an edge.
    /// Candidates are ordered by orthogonal overlap, edge distance, and then
    /// the supplied visual reading order.
    public static func neighbor(
        of source: PaneGroupID,
        direction: FocusDirection,
        frames: [PaneGroupID: CGRect],
        readingOrder: [PaneGroupID]
    ) -> PaneGroupID? {
        guard let sourceFrame = frames[source] else { return nil }

        let orderedRanks = Dictionary(
            uniqueKeysWithValues: readingOrder.enumerated().map { ($1, $0) }
        )
        let candidates = frames.compactMap { id, frame -> Candidate? in
            guard id != source,
                  isInDirection(frame, from: sourceFrame, direction: direction) else {
                return nil
            }

            return Candidate(
                id: id,
                overlap: orthogonalOverlap(sourceFrame, frame, direction: direction),
                distance: edgeDistance(sourceFrame, frame, direction: direction),
                readingRank: orderedRanks[id] ?? Int.max
            )
        }

        return candidates.min { lhs, rhs in
            if lhs.overlap != rhs.overlap { return lhs.overlap > rhs.overlap }
            if lhs.distance != rhs.distance { return lhs.distance < rhs.distance }
            if lhs.readingRank != rhs.readingRank { return lhs.readingRank < rhs.readingRank }
            return String(describing: lhs.id) < String(describing: rhs.id)
        }?.id
    }

    private static func isInDirection(
        _ candidate: CGRect,
        from source: CGRect,
        direction: FocusDirection
    ) -> Bool {
        switch direction {
        case .left:
            candidate.maxX <= source.minX
        case .right:
            candidate.minX >= source.maxX
        case .up:
            candidate.minY >= source.maxY
        case .down:
            candidate.maxY <= source.minY
        }
    }

    private static func orthogonalOverlap(
        _ source: CGRect,
        _ candidate: CGRect,
        direction: FocusDirection
    ) -> CGFloat {
        switch direction {
        case .left, .right:
            max(0, min(source.maxY, candidate.maxY) - max(source.minY, candidate.minY))
        case .up, .down:
            max(0, min(source.maxX, candidate.maxX) - max(source.minX, candidate.minX))
        }
    }

    private static func edgeDistance(
        _ source: CGRect,
        _ candidate: CGRect,
        direction: FocusDirection
    ) -> CGFloat {
        switch direction {
        case .left:
            source.minX - candidate.maxX
        case .right:
            candidate.minX - source.maxX
        case .up:
            candidate.minY - source.maxY
        case .down:
            source.minY - candidate.maxY
        }
    }

    private struct Candidate {
        let id: PaneGroupID
        let overlap: CGFloat
        let distance: CGFloat
        let readingRank: Int
    }
}
