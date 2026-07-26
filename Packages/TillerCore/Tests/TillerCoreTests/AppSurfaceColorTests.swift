import Testing
@testable import TillerCore

@Test func chromeIsWarmGraphite1A1A1E() {
    #expect(AppSurfaceColor.red == 0.102)
    #expect(AppSurfaceColor.green == 0.102)
    #expect(AppSurfaceColor.blue == 0.118)
    #expect(AppSurfaceColor.hex == "1A1A1E")
}

@Test func chatSurfaceIsNearBlack121216() {
    #expect(AppSurfaceColor.chatRed == 0.070)
    #expect(AppSurfaceColor.chatGreen == 0.072)
    #expect(AppSurfaceColor.chatBlue == 0.086)
    #expect(AppSurfaceColor.chatHex == "121216")
}

/// The chrome carries the same blue lean as the terminal surface: both sit in
/// one color family, which a neutral gray chrome broke.
@Test func chromeAndTerminalSurfaceShareABlueLean() {
    #expect(AppSurfaceColor.blue > AppSurfaceColor.red)
    #expect(AppSurfaceColor.terminalBlue > AppSurfaceColor.terminalRed)
}

@Test func terminalSurfaceMatchesChatSurface121216() {
    #expect(AppSurfaceColor.terminalRed == AppSurfaceColor.chatRed)
    #expect(AppSurfaceColor.terminalGreen == AppSurfaceColor.chatGreen)
    #expect(AppSurfaceColor.terminalBlue == AppSurfaceColor.chatBlue)
    #expect(AppSurfaceColor.terminalHex == "121216")
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
