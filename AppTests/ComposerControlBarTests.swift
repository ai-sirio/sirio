import Testing

@testable import Tiller

@Suite("ComposerControlBar")
@MainActor
struct ComposerControlBarTests {
    @Test func primaryActionUsesAStableCircularFootprint() {
        #expect(ComposerControlBar.primaryActionSize == 30)
    }

    @Test func inactiveActionFillIsVisiblySubdued() {
        #expect(ComposerControlBar.inactiveActionFillOpacity == 0.18)
    }

    @Test func sendActionUsesTheArrowUpSymbol() {
        #expect(ComposerControlBar.sendSystemImage == "arrow.up")
    }

    @Test func primaryActionStatesHaveStableAccessibilityLabels() {
        #expect(ComposerControlBar.sendAccessibilityLabel == "Send message")
        #expect(ComposerControlBar.loadingAccessibilityLabel == "Starting agent")
        #expect(ComposerControlBar.stopAccessibilityLabel == "Stop response")
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
