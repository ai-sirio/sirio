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

@MainActor
private func resolved(_ color: NSColor, _ name: NSAppearance.Name) -> NSColor {
    var result = NSColor.black
    NSAppearance(named: name)!.performAsCurrentDrawingAppearance {
        result = color.usingColorSpace(.sRGB) ?? .black
    }
    return result
}

@Suite("ComposerStyle", .serialized)
@MainActor
struct ComposerStyleTests {
    @Test func composerSurfaceMatchesTheApprovedHexInEveryAppearance() {
        for appearance in [NSAppearance.Name.aqua, .darkAqua] {
            let surface = resolved(AppTheme.composerFill, appearance)
            #expect(abs(surface.redComponent - (32.0 / 255.0)) < 0.0001)
            #expect(abs(surface.greenComponent - (35.0 / 255.0)) < 0.0001)
            #expect(abs(surface.blueComponent - (45.0 / 255.0)) < 0.0001)
            #expect(abs(surface.alphaComponent - 1.0) < 0.0001)
        }
    }

    @Test func composerUsesDarkSemanticAppearanceOnAqua() {
        #expect(AppTheme.ComposerAppearance.colorScheme == .dark)
        #expect(AppTheme.ComposerAppearance.appKitAppearance == .darkAqua)

        let textView = ChatTextEditor.makeTextView()
        #expect(textView.appearance?.bestMatch(from: [.aqua, .darkAqua]) == .darkAqua)

        let surface = resolved(AppTheme.composerFill, .darkAqua)
        let text = resolved(textView.textColor ?? .black, .darkAqua)
        let caret = resolved(textView.insertionPointColor ?? .black, .darkAqua)
        #expect(text.brightnessComponent > surface.brightnessComponent)
        #expect(caret.brightnessComponent > surface.brightnessComponent)
    }
}
