import AppKit
import Testing
@testable import Tiller

/// The field must be darker than the composer chrome around it, or it stops
/// reading as sunk into it. The relationship, not the exact colour.
@Test func composerFieldIsDarkerThanTheComposerChrome() {
    let field = NSColor(AppTheme.composerFieldFill)
        .usingColorSpace(.sRGB)!
    let chrome = NSColor(AppTheme.cardFill)
        .usingColorSpace(.sRGB)!
    #expect(field.brightnessComponent < chrome.brightnessComponent)
}
