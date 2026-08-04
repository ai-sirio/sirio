import CoreGraphics
import TillerCore

public enum SplitEligibility {
    public enum Reason: Error, Equatable, Sendable {
        case insufficientWidth(available: CGFloat, required: CGFloat)
        case insufficientHeight(available: CGFloat, required: CGFloat)
        case soleTabOfItsOwnGroup
    }

    /// `isSplittingItsOwnSoleGroup` means the dragged tab is the only tab of the
    /// very group being split — the one case rule 7 bars. A one-tab group is a
    /// legal target for any tab that came from elsewhere.
    public static func check(
        groupSize: CGSize,
        placement: SplitPlacementSide,
        isSplittingItsOwnSoleGroup: Bool
    ) -> Result<Void, Reason> {
        if isSplittingItsOwnSoleGroup {
            return .failure(.soleTabOfItsOwnGroup)
        }

        switch placement.axis {
        case .horizontal:
            let available = (groupSize.width - WorkspaceMetrics.dividerThickness) / 2
            guard available >= WorkspaceMetrics.preferredGroupSize.width else {
                return .failure(.insufficientWidth(
                    available: available, required: WorkspaceMetrics.preferredGroupSize.width
                ))
            }
        case .vertical:
            let available = (groupSize.height - WorkspaceMetrics.dividerThickness) / 2
            guard available >= WorkspaceMetrics.preferredGroupSize.height else {
                return .failure(.insufficientHeight(
                    available: available, required: WorkspaceMetrics.preferredGroupSize.height
                ))
            }
        }

        return .success(())
    }
}
