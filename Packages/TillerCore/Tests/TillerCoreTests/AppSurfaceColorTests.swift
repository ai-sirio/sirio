import Testing
@testable import TillerCore

@Test func chromeMatchesApprovedDarkAndLightValues() {
    #expect(AppSurfaceColor.red == 27.0 / 255.0)
    #expect(AppSurfaceColor.green == 28.0 / 255.0)
    #expect(AppSurfaceColor.blue == 31.0 / 255.0)
    #expect(AppSurfaceColor.hex == "1B1C1F")
    #expect(AppSurfaceColor.lightHex == "E9EAED")
}

@Test func chatSurfaceMatchesApprovedDarkAndLightValues() {
    #expect(AppSurfaceColor.chatRed == 40.0 / 255.0)
    #expect(AppSurfaceColor.chatGreen == 41.0 / 255.0)
    #expect(AppSurfaceColor.chatBlue == 44.0 / 255.0)
    #expect(AppSurfaceColor.chatHex == "28292C")
    #expect(AppSurfaceColor.chatLightHex == "F6F6F8")
}

@Test func terminalSurfaceMatchesChatSurfaceInBothAppearances() {
    #expect(AppSurfaceColor.terminalRed == AppSurfaceColor.chatRed)
    #expect(AppSurfaceColor.terminalGreen == AppSurfaceColor.chatGreen)
    #expect(AppSurfaceColor.terminalBlue == AppSurfaceColor.chatBlue)
    #expect(AppSurfaceColor.terminalHex == AppSurfaceColor.chatHex)
    #expect(AppSurfaceColor.terminalLightHex == AppSurfaceColor.chatLightHex)
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
