import CoreGraphics
import TillerCore

@MainActor
public final class DragSession {
    public private(set) var isActive = false
    public private(set) var grabOffset = CGPoint.zero
    public private(set) var currentTarget: DropTarget = .none

    private var tab: WorkspaceTabID?
    private var pressPoint = CGPoint.zero
    private var isCancelled = false

    public init() {}

    public func pressBegan(tab: WorkspaceTabID, at point: CGPoint, inTabFrame frame: CGRect) {
        self.tab = tab
        pressPoint = point
        grabOffset = CGPoint(x: point.x - frame.minX, y: point.y - frame.minY)
        isActive = false
        currentTarget = .none
        isCancelled = false
    }

    public func pointerMoved(to point: CGPoint, hitTest: (CGPoint) -> DropTarget) {
        guard tab != nil, !isCancelled else { return }

        if !isActive {
            let movement = hypot(point.x - pressPoint.x, point.y - pressPoint.y)
            guard movement >= WorkspaceMetrics.dragThreshold else { return }
            isActive = true
        }
        currentTarget = hitTest(point)
    }

    public func cancel(reason: CancelReason) {
        _ = reason
        isCancelled = true
        isActive = false
        currentTarget = .none
    }

    /// Returns the semantic intent to send, or nil when the release is not a
    /// legal target. Resetting here makes a second release mutation-free.
    public func release() -> WorkspaceIntent? {
        defer { reset() }
        guard !isCancelled, let tab else { return nil }
        guard isActive else { return .activateTab(tab) }

        switch currentTarget {
        case .tabStrip(let groupID, let insertionIndex):
            return .requestMove(tab, to: .group(groupID, index: insertionIndex))
        case .center(let groupID):
            return .requestMove(tab, to: .group(groupID, index: 0))
        case .edge(let groupID, let placement):
            // One-to-one, not collapsed: a left-edge drag must split to the
            // anchor's left, not silently produce the same result as a
            // right-edge drag. `EdgePlacement` and `SplitPlacementSide` are
            // separate types (drag-preview geometry vs. Core command), but
            // this mapping only ever loses information if it is wrong.
            let splitPlacement: SplitPlacementSide
            switch placement {
            case .left: splitPlacement = .left
            case .right: splitPlacement = .right
            case .top: splitPlacement = .above
            case .bottom: splitPlacement = .below
            }
            return .requestMove(tab, to: .edgeSplit(anchor: groupID, placement: splitPlacement))
        case .none:
            return nil
        }
    }

    public enum CancelReason: Equatable, Sendable {
        case escape
        case pointerCancelled
        case focusLost
        case staleIdentity
    }

    private func reset() {
        isActive = false
        currentTarget = .none
        tab = nil
        pressPoint = .zero
        isCancelled = false
    }
}
