import AppKit
import SwiftUI
import Testing
@testable import Tiller

/// Resolves a dynamic colour against an explicit appearance. Read outside a
/// view hierarchy, one of `AppTheme`'s colours follows whatever appearance the
/// test process happens to run under — so the same assertion passes on a Mac in
/// dark mode and fails on one in light. A test that depends on System Settings
/// is not a test.
@MainActor
private func resolved(_ color: Color, _ name: NSAppearance.Name) -> NSColor {
    var result = NSColor.black
    NSAppearance(named: name)!.performAsCurrentDrawingAppearance {
        result = NSColor(color).usingColorSpace(.sRGB) ?? .black
    }
    return result
}

/// The field is sunk into the composer chrome, so in dark it must be darker
/// than it — that contrast is the whole point of giving it a surface of its own.
@MainActor
@Test func composerFieldIsDarkerThanTheChromeInDarkAppearance() {
    let field = resolved(AppTheme.composerFieldFill, .darkAqua)
    let chrome = resolved(AppTheme.cardFill, .darkAqua)
    #expect(field.brightnessComponent < chrome.brightnessComponent)
}

/// In light the relationship inverts by design: the field is white, the way a
/// text field is on every other light surface. Asserting "darker than chrome"
/// in both appearances would assert a rule the design does not follow.
@MainActor
@Test func composerFieldIsWhiteInLightAppearance() {
    #expect(resolved(AppTheme.composerFieldFill, .aqua).brightnessComponent == 1.0)
}
