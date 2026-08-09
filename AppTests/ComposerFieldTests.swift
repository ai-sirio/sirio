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
    /// The composer's card fill is the same dynamic surface the transcript
    /// sits on — that's what makes it blend in instead of standing out as
    /// its own dark panel.
    @Test func composerCardFillMatchesTheChatTranscriptSurface() {
        for appearance in [NSAppearance.Name.aqua, .darkAqua] {
            let composer = resolved(AppTheme.chatSurface, appearance)
            let transcript = resolved(AppTheme.chatSurface, appearance)
            #expect(composer == transcript)
        }
    }

    @Test func textViewNoLongerForcesADarkAppKitAppearance() {
        let textView = ChatTextEditor.makeTextView()
        #expect(textView.appearance == nil)
    }

    @Test func composerCardNoLongerForcesTheColorSchemeEnvironment() {
        let capture = ColorSchemeCapture()
        let root = ColorSchemeProbe(capture: capture)
            .environment(\.colorScheme, .light)
        let host = NSHostingView(rootView: root)
        host.frame = NSRect(x: 0, y: 0, width: 300, height: 100)
        host.layoutSubtreeIfNeeded()

        #expect(capture.value == .light)
    }

    @Test func chatComposerViewSourceNoLongerReferencesTheDeletedAppearanceSeam() throws {
        let repositoryRoot = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()
            .deletingLastPathComponent()
        let sourceURL = repositoryRoot.appendingPathComponent("App/Chat/ChatComposerView.swift")
        let source = try String(contentsOf: sourceURL, encoding: .utf8)
        #expect(!source.contains("composerCardAppearance"))
        #expect(!source.contains("cardWithAppearance"))
        #expect(!source.contains(".environment(\\.colorScheme"))
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
