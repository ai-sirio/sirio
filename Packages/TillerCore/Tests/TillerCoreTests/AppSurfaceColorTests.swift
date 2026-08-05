import Testing
@testable import TillerCore

@Test func chromeMatchesApprovedDarkAndLightValues() {
    #expect(AppSurfaceColor.red == 27.0 / 255.0)
    #expect(AppSurfaceColor.green == 28.0 / 255.0)
    #expect(AppSurfaceColor.blue == 31.0 / 255.0)
    #expect(AppSurfaceColor.hex == "1B1C1F")
    #expect(AppSurfaceColor.lightHex == "E9EAED")
}

@Test func chatSurfaceIsTheSameSurfaceAsTheChrome() {
    #expect(AppSurfaceColor.chatRed == AppSurfaceColor.red)
    #expect(AppSurfaceColor.chatGreen == AppSurfaceColor.green)
    #expect(AppSurfaceColor.chatBlue == AppSurfaceColor.blue)
    #expect(AppSurfaceColor.chatHex == AppSurfaceColor.hex)
    #expect(AppSurfaceColor.chatLightHex == AppSurfaceColor.lightHex)
}

@Test func canvasMatchesApprovedDarkAndLightValues() {
    #expect(AppSurfaceColor.canvasHex == "131417")
    #expect(AppSurfaceColor.canvasLightHex == "DCDDE2")
}

/// The relationship, not the values: whatever the palette becomes, a card that
/// is darker than what it sits on stops reading as resting on it.
@Test func canvasIsDarkerThanTheCardInBothAppearances() {
    #expect(AppSurfaceColor.canvasRed < AppSurfaceColor.red)
    #expect(AppSurfaceColor.canvasGreen < AppSurfaceColor.green)
    #expect(AppSurfaceColor.canvasBlue < AppSurfaceColor.blue)
    #expect(AppSurfaceColor.canvasLightRed < AppSurfaceColor.lightRed)
    #expect(AppSurfaceColor.canvasLightGreen < AppSurfaceColor.lightGreen)
    #expect(AppSurfaceColor.canvasLightBlue < AppSurfaceColor.lightBlue)
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
