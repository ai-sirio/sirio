import Foundation
import Testing

@testable import TillerWorkspace

/// The divider band is deliberately wider than the line drawn inside it: the
/// band is the grab target, the hairline is what the eye reads. Without a
/// drawn line two panes are indistinguishable, which is the bug these pin.
@Suite("WorkspaceDividerTests")
struct WorkspaceDividerTests {
    @Test func aVerticalDividerDrawsAHairlineCenteredInItsBand() {
        let band = CGRect(x: 100, y: 0, width: WorkspaceMetrics.dividerThickness, height: 400)

        let hairline = WorkspaceMetrics.dividerHairline(in: band, isVertical: true)

        #expect(hairline.width == WorkspaceMetrics.dividerHairlineThickness)
        #expect(hairline.height == 400)
        #expect(hairline.midX == band.midX)
    }

    @Test func aHorizontalDividerDrawsAHairlineCenteredInItsBand() {
        let band = CGRect(x: 0, y: 50, width: 800, height: WorkspaceMetrics.dividerThickness)

        let hairline = WorkspaceMetrics.dividerHairline(in: band, isVertical: false)

        #expect(hairline.height == WorkspaceMetrics.dividerHairlineThickness)
        #expect(hairline.width == 800)
        #expect(hairline.midY == band.midY)
    }

    @Test func theHairlineNeverEscapesItsBand() {
        let vertical = CGRect(x: 12, y: 0, width: WorkspaceMetrics.dividerThickness, height: 100)
        let horizontal = CGRect(x: 0, y: 12, width: 100, height: WorkspaceMetrics.dividerThickness)

        #expect(vertical.contains(WorkspaceMetrics.dividerHairline(in: vertical, isVertical: true)))
        #expect(
            horizontal.contains(
                WorkspaceMetrics.dividerHairline(in: horizontal, isVertical: false)))
    }

    /// A band thinner than the hairline must still paint something, otherwise
    /// the divider silently disappears again at small sizes.
    @Test func aBandThinnerThanTheHairlineStillDrawsTheWholeBand() {
        let band = CGRect(x: 0, y: 0, width: 0.5, height: 100)

        let hairline = WorkspaceMetrics.dividerHairline(in: band, isVertical: true)

        #expect(hairline.width == 0.5)
        #expect(hairline.height == 100)
    }
}
