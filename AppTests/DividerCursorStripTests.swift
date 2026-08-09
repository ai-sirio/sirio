import AppKit
import Testing

@testable import Tiller

@Suite("DividerCursorStrip", .serialized)
@MainActor
struct DividerCursorStripTests {
    /// The strip sits on top of the split divider. If it answered the click, it
    /// would swallow the mouse-down and the divider would stop dragging — the
    /// cursor would look right and the resize would be dead. The policy itself
    /// is covered in TillerWorkspace, which now owns it.
    @Test func stripRefusesClicksWithNoEventInFlight() {
        let strip = DividerCursorStripView(cursor: .resizeLeftRight)
        strip.frame = NSRect(x: 0, y: 0, width: 10, height: 400)

        #expect(strip.hitTest(NSPoint(x: 5, y: 200)) == nil)
    }
}
