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
