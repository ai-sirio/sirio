import AppKit
import CoreGraphics
import Testing
import SwiftUI
@testable import Tiller

@Test func cardsUseApprovedFloatingGeometry() {
    #expect(AppTheme.cardCornerRadius == 6)
    #expect(AppTheme.cardGap == 10)
    #expect(AppTheme.cardShadowRadius == 18)
    #expect(AppTheme.cardShadowYOffset == 6)
}

/// 28 is the height of the standard macOS titlebar — the clearance the traffic
/// lights need. Shrinking it clips them; growing it thickens the frame's top
/// band for nothing.
@Test func titleStripClearsTheTrafficLights() {
    #expect(AppTheme.titleStripHeight == 28)
    #expect(AppTheme.trafficLightInset == 78)
}

/// The strip's glyphs sit beside the 12pt window controls; a default-sized SF
/// Symbol reads larger and breaks the row.
@Test func titleStripGlyphsMatchTheTrafficLightScale() {
    #expect(AppTheme.titleStripIconSize == 13)
}

@Test func titlebarControlsShareOneFrameAndCenterline() {
    #expect(TitlebarGeometry.accessoryHeight == AppTheme.titleStripHeight)
    #expect(TitlebarGeometry.iconSize == AppTheme.titleStripIconSize)
    #expect(TitlebarGeometry.controlFrame == CGSize(width: 24, height: 24))
    #expect(TitlebarGeometry.verticalCenter(in: TitlebarGeometry.accessoryHeight) == 14)
}

@Test func titlebarUsesOneSharedSpacingAndNoPerIconCorrection() {
    #expect(TitlebarGeometry.controlSpacing == 2)
    #expect(TitlebarGeometry.sidebarVerticalCorrection == 0)
    #expect(TitlebarGeometry.trafficLightInset == AppTheme.trafficLightInset)
}

@Test func fourTitlebarControlsHaveBoundedCenteredFrames() {
    let bounds = CGRect(x: 0, y: 0, width: 4 * 24 + 3 * 2, height: 28)
    let frames = TitlebarGeometry.controlFrames(count: 4, in: bounds)

    #expect(frames.count == 4)
    #expect(frames.allSatisfy { $0.size == CGSize(width: 24, height: 24) })
    #expect(frames.allSatisfy { bounds.contains($0) })
    #expect(frames.allSatisfy { $0.midY == bounds.midY })
    #expect(frames.dropFirst().enumerated().allSatisfy { index, frame in
        frame.minX == frames[index].maxX + 2
    })
}

@Test func threeTrailingControlsUseIndependentBoundedSlots() {
    let bounds = CGRect(x: 0, y: 0, width: 3 * 24 + 2 * 2, height: 28)
    let frames = TitlebarGeometry.controlFrames(count: 3, in: bounds)

    #expect(frames.count == 3)
    #expect(frames.allSatisfy { $0.size == CGSize(width: 24, height: 24) })
    #expect(frames.allSatisfy { bounds.contains($0) })
    #expect(frames.allSatisfy { $0.midY == bounds.midY })
    #expect(frames.last?.maxX == bounds.maxX)
    #expect(frames.dropFirst().enumerated().allSatisfy { index, frame in
        frame.minX == frames[index].maxX + 2
    })
}

@MainActor @Test func threeTrailingControlsRenderAsIndependentSlots() {
    let host = NSHostingView(
        rootView: TitleStripGroup {
            TitlebarControlFrame { Color.clear }
            TitlebarControlFrame { Color.clear }
            TitlebarControlFrame { Color.clear }
        })

    host.layoutSubtreeIfNeeded()

    #expect(host.fittingSize == CGSize(width: 3 * 24 + 2 * 2, height: 28))
}

/// A flush edge squares both of its corners. Two rounded corners meeting at a
/// zero-width seam leave a notch of bare canvas above and below it.
@Test func cardCornersSquareTheFlushEdge() {
    let corners = CardLayout.cardCorners(flushEdges: .leading, radius: 6)

    #expect(corners.topLeading == 0)
    #expect(corners.bottomLeading == 0)
    #expect(corners.topTrailing == 6)
    #expect(corners.bottomTrailing == 6)
}

@Test func cardCornersStayRoundWithNoFlushEdge() {
    let corners = CardLayout.cardCorners(flushEdges: [], radius: 6)

    #expect(corners.topLeading == 6)
    #expect(corners.bottomLeading == 6)
    #expect(corners.topTrailing == 6)
    #expect(corners.bottomTrailing == 6)
}

/// The central card is flush on both sides whenever both side panels are open.
@Test func cardCornersSquareBothVerticalSeams() {
    let corners = CardLayout.cardCorners(flushEdges: [.leading, .trailing], radius: 6)

    #expect(corners.topLeading == 0)
    #expect(corners.bottomLeading == 0)
    #expect(corners.topTrailing == 0)
    #expect(corners.bottomTrailing == 0)
}

/// The sideways shadow spill is what makes a zero-width seam read as a black
/// band, so a flush edge gets no bleed at all.
@Test func shadowBleedStopsAtAFlushEdge() {
    let bleed = CardLayout.shadowBleed(flushEdges: .leading, radius: 18)

    #expect(bleed.leading == 0)
    #expect(bleed.trailing == 36)
    #expect(bleed.top == 36)
    #expect(bleed.bottom == 36)
}

@Test func shadowBleedIsUnboundedWithNoFlushEdge() {
    let bleed = CardLayout.shadowBleed(flushEdges: [], radius: 18)

    #expect(bleed.leading == 36)
    #expect(bleed.trailing == 36)
    #expect(bleed.top == 36)
    #expect(bleed.bottom == 36)
}

/// `dividerCenterX` needs no edit when a column's gap padding moves from both
/// sides to the outward side alone: the column measures half a gap narrower
/// and the seam moves by exactly that much, so the same expression keeps
/// landing on the seam. Both offsets in `ContentView` depend on this.
@Test func seamCentreFollowsThePaddingMovingOutward() {
    let cardWidth: CGFloat = 240
    let gap: CGFloat = 10

    let symmetric = CardLayout.dividerCenterX(columnWidth: cardWidth + gap, gap: gap)
    let outwardOnly = CardLayout.dividerCenterX(columnWidth: cardWidth + gap / 2, gap: gap)

    #expect(symmetric == cardWidth + gap * 1.5)
    #expect(outwardOnly == cardWidth + gap)
    #expect(symmetric - outwardOnly == gap / 2)
}
