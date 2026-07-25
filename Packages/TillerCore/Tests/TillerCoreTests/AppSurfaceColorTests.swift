import Testing
@testable import TillerCore

@Test func chromeIsIndigoTinted121216() {
    #expect(AppSurfaceColor.red == 0.070)
    #expect(AppSurfaceColor.green == 0.072)
    #expect(AppSurfaceColor.blue == 0.086)
    #expect(AppSurfaceColor.hex == "121216")
}

/// The chrome carries the same blue lean as the terminal surface: both sit in
/// one color family, which a neutral gray chrome broke.
@Test func chromeAndTerminalSurfaceShareABlueLean() {
    #expect(AppSurfaceColor.blue > AppSurfaceColor.red)
    #expect(AppSurfaceColor.terminalBlue > AppSurfaceColor.terminalRed)
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
