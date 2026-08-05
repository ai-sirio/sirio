import Testing
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
