import AppKit
import SwiftUI
import Testing
import TillerACP

@testable import Tiller

@MainActor
private func resolved(_ color: Color, _ appearance: NSAppearance.Name) -> NSColor {
    var result = NSColor.black
    NSAppearance(named: appearance)!.performAsCurrentDrawingAppearance {
        result = NSColor(color).usingColorSpace(.sRGB) ?? .black
    }
    return result
}

@Suite("ComposerControlBar", .serialized)
@MainActor
struct ComposerControlBarTests {
    @Test func sendPresentationDefinesTheInteractiveActionChrome() {
        let presentation = ComposerControlBar.primaryActionPresentation(for: .ready)

        #expect(presentation.kind == .send)
        #expect(presentation.footprintSize == 30)
        #expect(presentation.shape == .circle)
        #expect(presentation.accessibilityLabel == "Send")
        #expect(presentation.accessibilityHelp == "Send")
        #expect(presentation.systemImage == "arrow.up")
    }

    @Test func loadingPresentationKeepsTheInactiveCircularFootprint() {
        let presentation = ComposerControlBar.primaryActionPresentation(for: .connecting)

        #expect(presentation.kind == .loading)
        #expect(presentation.footprintSize == 30)
        #expect(presentation.shape == .circle)
        #expect(presentation.accessibilityLabel == "Starting the agent")
        #expect(presentation.accessibilityHelp == "Starting the agent")
        #expect(presentation.systemImage == nil)
    }

    @Test func stopPresentationDefinesTheStopActionChrome() {
        let presentation = ComposerControlBar.primaryActionPresentation(for: .prompting)

        #expect(presentation.kind == .stop)
        #expect(presentation.footprintSize == 30)
        #expect(presentation.shape == .circle)
        #expect(presentation.accessibilityLabel == "Stop the turn")
        #expect(presentation.accessibilityHelp == "Stop the turn")
        #expect(presentation.systemImage == "stop.fill")
    }

    @Test func connectingShowsTheLoadingControl() {
        #expect(ComposerControlBar.trailingControl(for: .connecting) == .loading)
    }

    @Test func promptingShowsTheStopControl() {
        #expect(ComposerControlBar.trailingControl(for: .prompting) == .stop)
    }

    /// Every state that is neither connecting nor prompting keeps the send
    /// button in place, so the row never loses its trailing element.
    @Test func everyOtherStateShowsTheSendControl() {
        for state in [ChatController.ChatState.idle, .ready, .needsAuth,
                      .disconnected(message: nil)] {
            #expect(ComposerControlBar.trailingControl(for: state) == .send)
        }
    }

    @Test func inactiveActionFillMatchesSecondaryOpacityInEveryAppearance() {
        for appearance in [NSAppearance.Name.aqua, .darkAqua] {
            let fill = resolved(ComposerControlBar.inactiveActionFill, appearance)
            let expected = resolved(Color.secondary.opacity(0.18), appearance)
            let accent = resolved(Color.accentColor, appearance)
            let deltaFromAccent = abs(fill.redComponent - accent.redComponent)
                + abs(fill.greenComponent - accent.greenComponent)
                + abs(fill.blueComponent - accent.blueComponent)
                + abs(fill.alphaComponent - accent.alphaComponent)

            #expect(abs(fill.redComponent - expected.redComponent) < 0.0001)
            #expect(abs(fill.greenComponent - expected.greenComponent) < 0.0001)
            #expect(abs(fill.blueComponent - expected.blueComponent) < 0.0001)
            #expect(abs(fill.alphaComponent - expected.alphaComponent) < 0.0001)
            #expect(deltaFromAccent > 0.001)
        }
    }

    @Test func sendFillUsesTheAgentAccentColorWhenSendIsEnabled() {
        #expect(ComposerControlBar.sendFill(canSend: true, agentAccentColor: .red) == .red)
    }

    @Test func sendFillUsesTheInactiveFillWhenSendIsDisabled() {
        #expect(ComposerControlBar.sendFill(canSend: false, agentAccentColor: .red)
                == ComposerControlBar.inactiveActionFill)
    }

    @Test func sendGlyphUsesWhiteWhenSendIsEnabledAndAgentAccentWhenDisabled() {
        #expect(ComposerControlBar.sendGlyphColor(canSend: true, agentAccentColor: .orange)
                == .white)
        #expect(ComposerControlBar.sendGlyphColor(canSend: false, agentAccentColor: .orange)
                == .orange)
    }

    @Test func contextRingUsesAgentAccentNormallyAndRedForWarning() {
        #expect(ComposerControlBar.contextRingColor(warning: false, agentAccentColor: .orange)
                == .orange)
        #expect(ComposerControlBar.contextRingColor(warning: true, agentAccentColor: .orange)
                == .red)
    }

    @Test func contextUsageDetailOmitsCostAndBreakdownWhenAbsent() {
        let usage = ContextUsage(used: 1000, size: 200_000)
        let detail = ComposerControlBar.contextUsageDetail(usage)

        #expect(detail.percentLine == "1% of context used")
        #expect(detail.tokensLine == "1,000 / 200,000 tokens")
        #expect(detail.costLine == nil)
        #expect(detail.breakdownLine == nil)
    }

    @Test func contextUsageDetailIncludesCostAndBreakdownWhenPresent() {
        let usage = ContextUsage(used: 620_602, size: 1_000_000, costUsd: 0.0421,
                                  inputTokens: 4, outputTokens: 123,
                                  cacheCreationTokens: 512, cacheReadTokens: 83_967)
        let detail = ComposerControlBar.contextUsageDetail(usage)

        #expect(detail.percentLine == "62% of context used")
        #expect(detail.tokensLine == "620,602 / 1,000,000 tokens")
        #expect(detail.costLine == "Cost: $0.04")
        #expect(detail.breakdownLine == "Input: 4 · Output: 123 · Cache write: 512 · Cache read: 83,967")
    }
}
