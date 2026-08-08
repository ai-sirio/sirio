import SwiftUI

struct ComposerLayoutMetrics {
    static let preferredWidthFraction: CGFloat = 0.84
    static let maximumWidth: CGFloat = 1_440
    static let minimumHorizontalInset: CGFloat = 16

    static func contentWidth(for availableWidth: CGFloat) -> CGFloat {
        max(0, min(
            availableWidth - (2 * minimumHorizontalInset),
            availableWidth * preferredWidthFraction,
            maximumWidth
        ))
    }

    static func horizontalOffset(for availableWidth: CGFloat) -> CGFloat {
        max(0, (availableWidth - contentWidth(for: availableWidth)) / 2)
    }
}

struct CenteredComposerLayout: Layout {
    func sizeThatFits(
        proposal: ProposedViewSize,
        subviews: Subviews,
        cache: inout ()
    ) -> CGSize {
        guard let subview = subviews.first else { return .zero }

        let idealSize = subview.sizeThatFits(.unspecified)
        let availableWidth = max(0, proposal.width ?? idealSize.width)
        let contentWidth = ComposerLayoutMetrics.contentWidth(for: availableWidth)
        let contentSize = subview.sizeThatFits(
            ProposedViewSize(width: contentWidth, height: proposal.height)
        )

        return CGSize(width: availableWidth, height: contentSize.height)
    }

    func placeSubviews(
        in bounds: CGRect,
        proposal: ProposedViewSize,
        subviews: Subviews,
        cache: inout ()
    ) {
        guard let subview = subviews.first else { return }

        let contentWidth = ComposerLayoutMetrics.contentWidth(for: bounds.width)
        let contentProposal = ProposedViewSize(width: contentWidth, height: proposal.height)
        subview.place(
            at: CGPoint(
                x: bounds.minX + ComposerLayoutMetrics.horizontalOffset(for: bounds.width),
                y: bounds.minY
            ),
            anchor: .topLeading,
            proposal: contentProposal
        )
    }
}
