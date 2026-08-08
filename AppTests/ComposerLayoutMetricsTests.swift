import Testing

@testable import Tiller

@Suite("ComposerLayoutMetrics")
struct ComposerLayoutMetricsTests {
    @Test func constantsMatchTheResponsiveComposerSpecification() {
        #expect(ComposerLayoutMetrics.preferredWidthFraction == 0.84)
        #expect(ComposerLayoutMetrics.maximumWidth == 1_440)
        #expect(ComposerLayoutMetrics.minimumHorizontalInset == 16)
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
}
