import SwiftUI

struct ComposerLayoutMetrics {
    static let preferredWidthFraction: CGFloat = 0.84
    static let maximumWidth: CGFloat = 1_440
    static let minimumHorizontalInset: CGFloat = 16
    static let editorMinimumHeight: CGFloat = 56
    static let editorMaximumHeight: CGFloat = 128

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

struct ChatBottomOverlayMetrics {
    static let fadeExtension: CGFloat = 36
    static let bottomClearance: CGFloat = 16

    static func fadeHeight(for overlayHeight: CGFloat) -> CGFloat {
        max(0, overlayHeight) + fadeExtension
    }

    static func contentInset(for overlayHeight: CGFloat) -> CGFloat {
        max(0, overlayHeight) + bottomClearance
    }
}

struct ChatBottomOverlayHeightPreferenceKey: PreferenceKey {
    static let defaultValue: CGFloat = 0

    static func reduce(value: inout CGFloat, nextValue: () -> CGFloat) {
        value = max(value, nextValue())
    }
}

struct TranscriptViewportWidthPreferenceKey: PreferenceKey {
    static let defaultValue: CGFloat = 0

    static func reduce(value: inout CGFloat, nextValue: () -> CGFloat) {
        value = nextValue()
    }
}

extension View {
    func captureChatBottomOverlayHeight() -> some View {
        background {
            GeometryReader { proxy in
                Color.clear.preference(
                    key: ChatBottomOverlayHeightPreferenceKey.self,
                    value: proxy.size.height
                )
            }
        }
    }
}

struct CenteredComposerLayout: Layout {
    func sizeThatFits(
        proposal: ProposedViewSize,
        subviews: Subviews,
        cache: inout ()
    ) -> CGSize {
        guard let subview = subviews.first else { return .zero }

        let availableWidth = max(
            0,
            proposal.width ?? subview.sizeThatFits(.unspecified).width
        )
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
