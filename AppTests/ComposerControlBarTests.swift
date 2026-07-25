import SwiftUI
import Testing

@testable import Tiller

@Suite("ComposerControlBar")
@MainActor
struct ComposerControlBarTests {
    @Test func focusedBorderUsesTheAccentColor() {
        let style = ComposerControlBar.borderStyle(isFocused: true)
        #expect(style.color == Color.accentColor)
        #expect(style.width == 1.5)
    }

    @Test func unfocusedBorderUsesTheSeparatorColor() {
        let style = ComposerControlBar.borderStyle(isFocused: false)
        #expect(style.color != Color.accentColor)
        #expect(style.width == 1)
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
}
