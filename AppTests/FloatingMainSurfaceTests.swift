import Testing
@testable import Tiller

@Test func centralSurfaceUsesApprovedFloatingCardGeometry() {
    #expect(AppTheme.mainSurfaceCornerRadius == 18)
    #expect(AppTheme.mainSurfaceHorizontalInset == 10)
    #expect(AppTheme.mainSurfaceVerticalInset == 12)
    #expect(AppTheme.mainSurfaceShadowRadius == 18)
    #expect(AppTheme.mainSurfaceShadowYOffset == 6)
}
