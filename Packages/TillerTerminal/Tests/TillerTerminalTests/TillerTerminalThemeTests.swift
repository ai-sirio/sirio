import Foundation
import Testing
import GhosttyTerminal
import TillerCore
@testable import TillerTerminal

@Test func themeAppliesFontSizeToBothConfigurations() {
    let theme = TillerTerminalTheme.theme(fontSize: 14, translucencyEnabled: true)
    let scrollbackLimit = TerminalConfigCommand.custom(key: "scrollback-limit", value: "262144")
    #expect(AppSurfaceColor.terminalHex == "1B1C1F")
    #expect(AppSurfaceColor.terminalHex == AppSurfaceColor.hex)
    #expect(theme.light == TerminalConfiguration.alabaster
        .appending(.background(AppSurfaceColor.terminalLightHex))
        .appending(.fontSize(14))
        .appending(scrollbackLimit))
    #expect(theme.dark == TerminalConfiguration.afterglow
        .appending(.background(AppSurfaceColor.terminalHex))
        .appending(TerminalConfigCommand.custom(key: "background-opacity", value: "\(AppSurfaceColor.surfaceOpacity(translucencyEnabled: true))"))
        .appending(TerminalConfigCommand.custom(key: "background-blur-radius", value: "20"))
        .appending(.fontSize(14))
        .appending(scrollbackLimit))
}

@Test func themeDiffersByFontSize() {
    #expect(TillerTerminalTheme.theme(fontSize: 12, translucencyEnabled: true)
        != TillerTerminalTheme.theme(fontSize: 14, translucencyEnabled: true))
}

@Test func currentReadsStoredFontSizeAndClamps() throws {
    let suiteName = "TillerTerminalThemeTests-\(UUID().uuidString)"
    let defaults = try #require(UserDefaults(suiteName: suiteName))
    defer { defaults.removePersistentDomain(forName: suiteName) }
    defaults.set(true, forKey: AppSettings.translucencyEnabledKey)

    // No stored value -> default size.
    #expect(TillerTerminalTheme.current(defaults: defaults)
        == TillerTerminalTheme.theme(fontSize: Float(AppSettings.defaultTerminalFontSize), translucencyEnabled: true))

    // Stored value used.
    defaults.set(16, forKey: AppSettings.terminalFontSizeKey)
    #expect(TillerTerminalTheme.current(defaults: defaults)
        == TillerTerminalTheme.theme(fontSize: 16, translucencyEnabled: true))

    // Out-of-range value clamped.
    defaults.set(99, forKey: AppSettings.terminalFontSizeKey)
    #expect(TillerTerminalTheme.current(defaults: defaults)
        == TillerTerminalTheme.theme(fontSize: 24, translucencyEnabled: true))
}

@Test func themeOmitsOpacityAndBlurWhenTranslucencyDisabled() {
    let theme = TillerTerminalTheme.theme(fontSize: 13, translucencyEnabled: false)
    let expected = TerminalConfiguration.afterglow
        .appending(.background(AppSurfaceColor.terminalHex))
        .appending(.fontSize(13))
        .appending(TerminalConfigCommand.custom(key: "scrollback-limit", value: "262144"))
    #expect(theme.dark == expected)
}

@Test func currentReadsStoredTranslucencyPreference() {
    let suiteName = "TillerTerminalThemeTests-\(UUID().uuidString)"
    let defaults = UserDefaults(suiteName: suiteName)!
    defer { defaults.removePersistentDomain(forName: suiteName) }
    defaults.set(true, forKey: AppSettings.translucencyEnabledKey)

    let theme = TillerTerminalTheme.current(defaults: defaults)

    let expected = TerminalConfiguration.afterglow
        .appending(.background(AppSurfaceColor.terminalHex))
        .appending(TerminalConfigCommand.custom(key: "background-opacity", value: "0.96"))
        .appending(TerminalConfigCommand.custom(key: "background-blur-radius", value: "20"))
        .appending(.fontSize(Float(AppSettings.defaultTerminalFontSize)))
        .appending(TerminalConfigCommand.custom(key: "scrollback-limit", value: "262144"))
    #expect(theme.dark == expected)
}
