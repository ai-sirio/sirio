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
    #expect(AppSurfaceColor.surfaceOpacity == 0.96)
}
