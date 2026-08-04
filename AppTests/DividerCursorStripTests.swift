import AppKit
import Testing

@testable import Tiller

@Suite("DividerCursorStrip")
@MainActor
struct DividerCursorStripTests {
    /// The strip sits on top of the split divider. If it ever answered a hit
    /// test, it would swallow the mouse-down and the divider would stop
    /// dragging — the cursor would look right and the resize would be dead.
    @Test func stripIsTransparentToMouseEvents() {
        let strip = DividerCursorStripView(cursor: .resizeLeftRight)
        strip.frame = NSRect(x: 0, y: 0, width: 10, height: 400)

        #expect(strip.hitTest(NSPoint(x: 5, y: 200)) == nil)
    }
}
