import Testing
@testable import TillerCore

@Test func chromeHexIsUnchanged() {
    #expect(AppSurfaceColor.hex == "131313")
}

@Test func terminalSurfaceIsCharcoal1F1F26() {
    #expect(AppSurfaceColor.terminalRed == 0.122)
    #expect(AppSurfaceColor.terminalGreen == 0.122)
    #expect(AppSurfaceColor.terminalBlue == 0.149)
    #expect(AppSurfaceColor.terminalHex == "1F1F26")
}

@Test func sharedSurfaceOpacityIs096() {
    #expect(AppSurfaceColor.translucentSurfaceOpacity == 0.96)
}

@Test func surfaceOpacityIsOpaqueWhenTranslucencyDisabled() {
    #expect(AppSurfaceColor.surfaceOpacity(translucencyEnabled: false) == 1.0)
}

@Test func surfaceOpacityMatchesLegacyValueWhenTranslucencyEnabled() {
    #expect(AppSurfaceColor.surfaceOpacity(translucencyEnabled: true) == 0.96)
    #expect(AppSurfaceColor.surfaceOpacity(translucencyEnabled: true) == AppSurfaceColor.translucentSurfaceOpacity)
}
