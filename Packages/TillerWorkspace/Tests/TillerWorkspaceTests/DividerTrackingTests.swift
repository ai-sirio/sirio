import CoreGraphics
import Testing
import TillerCore
import TillerWorkspace

@Suite @MainActor
struct DividerTrackingTests {
    @Test
    func pointerTrackingEmitsExactlyOneIntentOnRelease() {
        let split = SplitID()
        let sink = RecordingDividerSink()
        let tracking = DividerTracking(sink: sink)

        tracking.began(split: split, total: 800)
        (0..<40).forEach { tracking.moved(to: CGFloat(300 + $0)) }

        #expect(sink.intents.isEmpty)
        tracking.ended()
        #expect(sink.intents == [.setPreferredFraction(split, 0.424)])
    }

    @Test
    func clampNeverWritesBackThePreferredValue() {
        let preferredFraction = 0.05
        let effectiveFraction = DividerTracking.clamp(
            preferredFraction, total: 800, minimum: 240
        )

        #expect(effectiveFraction == 0.3)
        #expect(preferredFraction == 0.05)
    }

    @Test
    func keyboardStepMovesFivePercentAndOptionStepOnePercent() {
        let split = SplitID()
        let sink = RecordingDividerSink()
        let tracking = DividerTracking(sink: sink)

        tracking.keyboardStep(split: split, currentFraction: 0.50, direction: .increase)
        tracking.keyboardStep(split: split, currentFraction: 0.55, direction: .decrease, option: true)

        #expect(sink.intents == [
            .setPreferredFraction(split, 0.55),
            .setPreferredFraction(split, 0.54)
        ])
    }

    @Test
    func furtherInputAtTheMinimumAnnouncesOnceAndDoesNotMutate() {
        let split = SplitID()
        let sink = RecordingDividerSink()
        var announcements: [String] = []
        let tracking = DividerTracking(sink: sink) { announcements.append($0) }

        tracking.keyboardStep(
            split: split, currentFraction: 0.3, direction: .decrease,
            total: 800, minimum: 240
        )
        tracking.keyboardStep(
            split: split, currentFraction: 0.3, direction: .decrease,
            total: 800, minimum: 240
        )

        #expect(sink.intents.isEmpty)
        #expect(announcements == ["Minimum pane size"])
    }
}

@MainActor
private final class RecordingDividerSink: WorkspaceIntentSink {
    private(set) var intents: [WorkspaceIntent] = []

    func send(_ intent: WorkspaceIntent) {
        intents.append(intent)
    }
}
