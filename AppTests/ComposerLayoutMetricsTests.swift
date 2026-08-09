import AppKit
import SwiftUI
import Testing

@testable import Tiller

private final class LayoutProposalRecorder: @unchecked Sendable {
    var proposals: [ProposedViewSize] = []
}

private struct RecordingLayout: Layout {
    let recorder: LayoutProposalRecorder

    func sizeThatFits(
        proposal: ProposedViewSize,
        subviews: Subviews,
        cache: inout ()
    ) -> CGSize {
        recorder.proposals.append(proposal)
        return CGSize(
            width: proposal.width ?? 80,
            height: proposal.height ?? 24
        )
    }

    func placeSubviews(
        in bounds: CGRect,
        proposal: ProposedViewSize,
        subviews: Subviews,
        cache: inout ()
    ) {}
}

@Suite("ComposerLayoutMetrics")
@MainActor
struct ComposerLayoutMetricsTests {
    @Test func constantsMatchTheResponsiveComposerSpecification() {
        #expect(ComposerLayoutMetrics.preferredWidthFraction == 0.84)
        #expect(ComposerLayoutMetrics.maximumWidth == 1_440)
        #expect(ComposerLayoutMetrics.minimumHorizontalInset == 16)
    }

    @Test func compactComposerHeightsMatchTheApprovedDesign() {
        #expect(ComposerLayoutMetrics.editorMinimumHeight == 56)
        #expect(ComposerLayoutMetrics.editorMaximumHeight == 128)
    }

    @Test func compactWidthUsesThePreferredFraction() {
        let width = ComposerLayoutMetrics.contentWidth(for: 320)

        #expect(abs(width - 268.8) < 0.0001)
    }

    @Test func wideWidthIsCappedAtTheMaximum() {
        #expect(ComposerLayoutMetrics.contentWidth(for: 2_000) == 1_440)
    }

    @Test func horizontalInsetsAreSymmetric() {
        let availableWidth = 1_000.0
        let contentWidth = ComposerLayoutMetrics.contentWidth(for: availableWidth)
        let leadingInset = ComposerLayoutMetrics.horizontalOffset(for: availableWidth)
        let trailingInset = availableWidth - leadingInset - contentWidth

        #expect(abs(leadingInset - trailingInset) < 0.0001)
    }

    @Test func negativeAvailableWidthProducesZeroGeometry() {
        #expect(ComposerLayoutMetrics.contentWidth(for: -100) == 0)
        #expect(ComposerLayoutMetrics.horizontalOffset(for: -100) == 0)
    }

    @Test func bottomOverlayMetricsMatchTheApprovedFadeAndClearance() {
        #expect(ChatBottomOverlayMetrics.fadeExtension == 36)
        #expect(ChatBottomOverlayMetrics.bottomClearance == 16)
        #expect(ChatBottomOverlayMetrics.fadeHeight(for: 100) == 136)
        #expect(ChatBottomOverlayMetrics.contentInset(for: 100) == 116)
    }

    @Test func bottomOverlayMetricsClampNegativeMeasurements() {
        #expect(ChatBottomOverlayMetrics.fadeHeight(for: -20) == 36)
        #expect(ChatBottomOverlayMetrics.contentInset(for: -20) == 16)
    }

    @Test func finiteWidthLayoutAvoidsIntrinsicChildMeasurement() {
        let recorder = LayoutProposalRecorder()
        let host = NSHostingView(
            rootView: CenteredComposerLayout {
                RecordingLayout(recorder: recorder) {
                    Color.clear
                }
            }
        )
        host.frame = NSRect(x: 0, y: 0, width: 800, height: 600)
        host.layoutSubtreeIfNeeded()

        let hasUnspecifiedProposal = recorder.proposals.contains {
            $0.width == nil && $0.height == nil
        }
        #expect(
            !hasUnspecifiedProposal,
            "finite-width layout recorded proposals: \(recorder.proposals)"
        )
    }
}
