import Testing
@testable import TillerCore

@Test func chromeMatchesApprovedDarkAndLightValues() {
    #expect(AppSurfaceColor.red == 8.0 / 255.0)
    #expect(AppSurfaceColor.green == 9.0 / 255.0)
    #expect(AppSurfaceColor.blue == 10.0 / 255.0)
    #expect(AppSurfaceColor.hex == "08090A")
    #expect(AppSurfaceColor.lightHex == "E9EAED")
}

@Test func chatSurfaceMatchesApprovedDarkAndLightValues() {
    #expect(AppSurfaceColor.chatRed == 16.0 / 255.0)
    #expect(AppSurfaceColor.chatGreen == 17.0 / 255.0)
    #expect(AppSurfaceColor.chatBlue == 18.0 / 255.0)
    #expect(AppSurfaceColor.chatHex == "101112")
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
