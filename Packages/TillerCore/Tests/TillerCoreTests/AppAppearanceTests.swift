import Testing
@testable import TillerCore

@Test func appAppearanceRawValuesAreStable() {
    #expect(AppAppearance.system.rawValue == "system")
    #expect(AppAppearance.light.rawValue == "light")
    #expect(AppAppearance.dark.rawValue == "dark")
}

@Test func appAppearanceCasesOrderIsSystemLightDark() {
    #expect(AppAppearance.allCases == [.system, .light, .dark])
}

@Test func appAppearanceTitles() {
    #expect(AppAppearance.system.title == "System")
    #expect(AppAppearance.light.title == "Light")
    #expect(AppAppearance.dark.title == "Dark")
}

@Test func appearanceSettingsKeys() {
    #expect(AppSettings.appearanceThemeKey == "appearance.theme")
    #expect(AppSettings.terminalFontSizeKey == "appearance.terminalFontSize")
}

@Test func defaultTerminalFontSizeIs13() {
    #expect(AppSettings.defaultTerminalFontSize == 13)
}

@Test func defaultInterfaceFontSizeMatchesSystemUI() {
    #expect(AppSettings.uiFontSizeKey == "appearance.uiFontSize")
    #expect(AppSettings.defaultUIFontSize == 13)
}

@Test func clampUIFontSizeClampsIntoRange() {
    #expect(AppSettings.clampUIFontSize(4) == 10)
    #expect(AppSettings.clampUIFontSize(99) == 20)
    #expect(AppSettings.clampUIFontSize(14) == 14)
    #expect(AppSettings.clampUIFontSize(10) == 10)
    #expect(AppSettings.clampUIFontSize(20) == 20)
}

@Test func clampTerminalFontSizeClampsIntoRange() {
    #expect(AppSettings.clampTerminalFontSize(4) == 9)
    #expect(AppSettings.clampTerminalFontSize(99) == 24)
    #expect(AppSettings.clampTerminalFontSize(13) == 13)
    #expect(AppSettings.clampTerminalFontSize(9) == 9)
    #expect(AppSettings.clampTerminalFontSize(24) == 24)
}
