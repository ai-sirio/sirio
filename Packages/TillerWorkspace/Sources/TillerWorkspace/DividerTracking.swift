import CoreGraphics
import Foundation
import TillerCore

@MainActor
public final class DividerTracking {
    public enum KeyboardDirection: Equatable, Sendable {
        case increase
        case decrease
    }

    private let sink: WorkspaceIntentSink
    private let announce: @MainActor (String) -> Void
    private var split: SplitID?
    private var total: CGFloat = 0
    private var minimum: CGFloat = 0
    private var thickness: CGFloat = WorkspaceMetrics.dividerThickness
    private var currentFraction: Double?
    private var announcedBoundary: Boundary?

    public init(
        sink: WorkspaceIntentSink,
        announcement: @escaping @MainActor (String) -> Void = { _ in }
    ) {
        self.sink = sink
        announce = announcement
    }

    public static func fraction(
        forPosition position: CGFloat,
        total: CGFloat,
        thickness: CGFloat
    ) -> Double {
        let available = total - thickness
        guard available > 0, available.isFinite else { return 0.5 }
        let clampedPosition = min(max(position, 0), available)
        return rounded(Double(clampedPosition / available))
    }

    public static func clamp(
        _ fraction: Double,
        total: CGFloat,
        minimum: CGFloat
    ) -> Double {
        guard total > 0, total.isFinite else { return 0.5 }
        let edge = min(max(Double(minimum / total), 0), 0.5)
        return min(max(fraction, edge), 1 - edge)
    }

    public func began(
        split: SplitID,
        total: CGFloat,
        minimum: CGFloat = WorkspaceMetrics.preferredGroupSize.width,
        thickness: CGFloat = WorkspaceMetrics.dividerThickness
    ) {
        self.split = split
        self.total = total
        self.minimum = minimum
        self.thickness = thickness
        currentFraction = nil
        announcedBoundary = nil
    }

    public func moved(to position: CGFloat) {
        guard split != nil else { return }
        // `total` is the available track length supplied by the split view;
        // the static helper takes the full length including the divider.
        let fullLength = total + thickness
        let fraction = Self.fraction(
            forPosition: position, total: fullLength, thickness: thickness
        )
        currentFraction = Self.clamp(fraction, total: total, minimum: minimum)
        announcedBoundary = nil
    }

    public func ended() {
        guard let split, let currentFraction else { return }
        sink.send(.setPreferredFraction(split, currentFraction))
        resetGesture()
    }

    public func cancelled() {
        resetGesture()
    }

    public func keyboardStep(
        split: SplitID,
        currentFraction: Double,
        direction: KeyboardDirection,
        option: Bool = false,
        total: CGFloat = 0,
        minimum: CGFloat = 0
    ) {
        let amount = option ? 0.01 : 0.05
        let delta = direction == .increase ? amount : -amount
        let proposed = currentFraction + delta
        let next = total > 0
            ? Self.clamp(proposed, total: total, minimum: minimum)
            : min(max(proposed, 0), 1)

        guard next != currentFraction else {
            let boundary: Boundary? = direction == .decrease ? .minimum : .maximum
            if announcedBoundary != boundary {
                announce("Minimum pane size")
                announcedBoundary = boundary
            }
            return
        }

        announcedBoundary = nil
        sink.send(.setPreferredFraction(split, next))
    }

    private func resetGesture() {
        split = nil
        total = 0
        minimum = 0
        currentFraction = nil
        announcedBoundary = nil
    }

    private static func rounded(_ value: Double) -> Double {
        (value * 1_000).rounded() / 1_000
    }

    private enum Boundary: Equatable {
        case minimum
        case maximum
    }
}
