import Testing

@testable import Tiller

@Suite("ComposerControlBar")
@MainActor
struct ComposerControlBarTests {
    @Test func sendPresentationDefinesTheInteractiveActionChrome() {
        let presentation = ComposerControlBar.primaryActionPresentation(for: .ready)

        #expect(presentation.kind == .send)
        #expect(presentation.footprintSize == 30)
        #expect(presentation.shape == .circle)
        #expect(presentation.accessibilityLabel == "Send message")
        #expect(presentation.systemImage == "arrow.up")
        #expect(presentation.inactiveFillOpacity == 0.18)
    }

    @Test func loadingPresentationKeepsTheInactiveCircularFootprint() {
        let presentation = ComposerControlBar.primaryActionPresentation(for: .connecting)

        #expect(presentation.kind == .loading)
        #expect(presentation.footprintSize == 30)
        #expect(presentation.shape == .circle)
        #expect(presentation.accessibilityLabel == "Starting agent")
        #expect(presentation.systemImage == nil)
        #expect(presentation.inactiveFillOpacity == 0.18)
    }

    @Test func stopPresentationDefinesTheStopActionChrome() {
        let presentation = ComposerControlBar.primaryActionPresentation(for: .prompting)

        #expect(presentation.kind == .stop)
        #expect(presentation.footprintSize == 30)
        #expect(presentation.shape == .circle)
        #expect(presentation.accessibilityLabel == "Stop response")
        #expect(presentation.systemImage == "stop.fill")
        #expect(presentation.inactiveFillOpacity == nil)
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
