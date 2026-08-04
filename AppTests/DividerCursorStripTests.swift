import AppKit
import Testing

@testable import Tiller

@Suite("DividerCursorStrip")
@MainActor
struct DividerCursorStripTests {
    /// The strip sits on top of the split divider. If it answered the click,
    /// it would swallow the mouse-down and the divider would stop dragging —
    /// the cursor would look right and the resize would be dead.
    @Test func clicksFallThroughToTheDivider() {
        #expect(DividerCursorHitPolicy.acceptsHit(eventType: .leftMouseDown) == false)
        #expect(DividerCursorHitPolicy.acceptsHit(eventType: .leftMouseDragged) == false)
        #expect(DividerCursorHitPolicy.acceptsHit(eventType: .rightMouseDown) == false)
        #expect(DividerCursorHitPolicy.acceptsHit(eventType: nil) == false)
    }

    /// AppKit routes `cursorUpdate` through `hitTest`, so refusing hover events
    /// would silently cost the strip its cursor rect — the original bug.
    @Test func hoverIsAnsweredSoTheCursorRectApplies() {
        #expect(DividerCursorHitPolicy.acceptsHit(eventType: .mouseMoved))
        #expect(DividerCursorHitPolicy.acceptsHit(eventType: .cursorUpdate))
        #expect(DividerCursorHitPolicy.acceptsHit(eventType: .mouseEntered))
    }

    @Test func stripRefusesClicksWithNoEventInFlight() {
        let strip = DividerCursorStripView(cursor: .resizeLeftRight)
        strip.frame = NSRect(x: 0, y: 0, width: 10, height: 400)

        #expect(strip.hitTest(NSPoint(x: 5, y: 200)) == nil)
    }
}
