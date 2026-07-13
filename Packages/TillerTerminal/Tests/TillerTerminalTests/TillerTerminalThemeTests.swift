import Foundation
import Testing
import GhosttyTerminal
import TillerCore
@testable import TillerTerminal

@Test func themeAppliesFontSizeToBothConfigurations() {
    let theme = TillerTerminalTheme.theme(fontSize: 14)
    let scrollbackLimit = TerminalConfigCommand.custom(key: "scrollback-limit", value: "262144")
    #expect(theme.light == TerminalConfiguration.alabaster
        .appending(.fontSize(14))
        .appending(scrollbackLimit))
    #expect(theme.dark == TerminalConfiguration.afterglow
        .appending(.background(AppSurfaceColor.terminalHex))
        .appending(.fontSize(14))
        .appending(scrollbackLimit))
}

@Test func themeDiffersByFontSize() {
    #expect(TillerTerminalTheme.theme(fontSize: 12) != TillerTerminalTheme.theme(fontSize: 14))
}

@Test func currentReadsStoredFontSizeAndClamps() throws {
    let suiteName = "TillerTerminalThemeTests-\(UUID().uuidString)"
    let defaults = try #require(UserDefaults(suiteName: suiteName))
    defer { defaults.removePersistentDomain(forName: suiteName) }

    // No stored value -> default size.
    #expect(TillerTerminalTheme.current(defaults: defaults)
        == TillerTerminalTheme.theme(fontSize: Float(AppSettings.defaultTerminalFontSize)))

    // Stored value used.
    defaults.set(16, forKey: AppSettings.terminalFontSizeKey)
    #expect(TillerTerminalTheme.current(defaults: defaults)
        == TillerTerminalTheme.theme(fontSize: 16))

    // Out-of-range value clamped.
    defaults.set(99, forKey: AppSettings.terminalFontSizeKey)
    #expect(TillerTerminalTheme.current(defaults: defaults)
        == TillerTerminalTheme.theme(fontSize: 24))
}
