import AppKit
import Foundation
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

/// Chat cards and the composer's surrounding chrome use the approved dark
/// surface color.
@MainActor
@Test func chatCardFillUsesTheApprovedDarkSurfaceColor() {
    let fill = resolved(AppTheme.cardFill, .darkAqua)
    #expect(abs(fill.redComponent - 44.0 / 255.0) < 0.0001)
    #expect(abs(fill.greenComponent - 47.0 / 255.0) < 0.0001)
    #expect(abs(fill.blueComponent - 57.0 / 255.0) < 0.0001)
}

@MainActor
private func resolved(_ color: NSColor, _ name: NSAppearance.Name) -> NSColor {
    var result = NSColor.black
    NSAppearance(named: name)!.performAsCurrentDrawingAppearance {
        result = color.usingColorSpace(.sRGB) ?? .black
    }
    return result
}

@MainActor
private final class ColorSchemeCapture {
    var value: ColorScheme?
}

private struct ColorSchemeProbe: NSViewRepresentable {
    @Environment(\.colorScheme) private var colorScheme
    let capture: ColorSchemeCapture

    func makeNSView(context: Context) -> NSView {
        capture.value = colorScheme
        return NSView(frame: .zero)
    }

    func updateNSView(_ nsView: NSView, context: Context) {
        capture.value = colorScheme
    }
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

    @Test func composerCardDarkAppearanceDoesNotLeakIntoQueuedContent() {
        let queuedCapture = ColorSchemeCapture()
        let cardCapture = ColorSchemeCapture()
        let root = VStack {
            ColorSchemeProbe(capture: queuedCapture)
            ColorSchemeProbe(capture: cardCapture)
                .composerCardAppearance()
        }
        .environment(\.colorScheme, .light)
        let host = NSHostingView(rootView: root)
        host.frame = NSRect(x: 0, y: 0, width: 300, height: 100)
        host.layoutSubtreeIfNeeded()

        #expect(queuedCapture.value == .light)
        #expect(cardCapture.value == .dark)
    }

    @Test func composerBodyUsesScopedCardAppearanceSeam() throws {
        let repositoryRoot = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()
            .deletingLastPathComponent()
        let sourceURL = repositoryRoot.appendingPathComponent("App/Chat/ChatComposerView.swift")
        let source = try String(contentsOf: sourceURL, encoding: .utf8)
        guard let bodyStart = source.range(of: "var body: some View"),
              let bodyEnd = source.range(of: "// MARK: - Card", range: bodyStart.upperBound..<source.endIndex)
        else {
            Issue.record("ChatComposerView body/card markers are missing")
            return
        }

        let bodySource = String(source[bodyStart.lowerBound..<bodyEnd.lowerBound])
        #expect(bodySource.contains("cardWithAppearance"))
        #expect(!bodySource.contains("card.composerCardAppearance()"))
        #expect(!bodySource.contains(".environment(\\.colorScheme"))

        guard let seamStart = source.range(of: "private var cardWithAppearance: some View"),
              let seamEnd = source.range(
                  of: "private var editor: some View",
                  range: seamStart.upperBound..<source.endIndex)
        else {
            Issue.record("ChatComposerView cardWithAppearance seam is missing")
            return
        }
        let seamSource = String(source[seamStart.lowerBound..<seamEnd.lowerBound])
        #expect(seamSource.contains("card.composerCardAppearance()"))
    }

    @Test func textViewNoLongerForcesADarkAppKitAppearance() {
        let textView = ChatTextEditor.makeTextView()
        #expect(textView.appearance == nil)
    }
}

@Suite("ComposerBorderView")
struct ComposerBorderViewTests {
    @Test func idleBorderIsTheNeutralHairlineAtFullOpacity() {
        #expect(ComposerBorderView.borderColor(isFocused: false, agentAccentColor: .red)
                == AppTheme.hairline)
    }

    @Test func focusedBorderIsTheAgentAccentColor() {
        #expect(ComposerBorderView.borderColor(isFocused: true, agentAccentColor: .red) == .red)
    }

    @Test func animatedGradientStartsAndEndsOnTheOpaqueAgentColor() {
        let colors = ComposerBorderView.animatedGradientColors(agentAccentColor: .red)
        #expect(colors.count == 5)
        #expect(colors.first == .red)
        #expect(colors.last == .red)
        #expect(colors[2] == Color.red.opacity(0))
    }
}
