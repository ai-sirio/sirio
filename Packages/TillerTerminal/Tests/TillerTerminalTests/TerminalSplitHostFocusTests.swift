import Testing
import AppKit
@testable import TillerTerminal

/// Regression for focus silently dropping out of a pane after splitting it:
/// TerminalSplitHost.node(_:coordinator:) unconditionally detaches cached
/// leaves via removeFromSuperview() to rebuild the tree, even when the leaf
/// stays in the same host. AppKit auto-resigns first responder on removal,
/// and nothing else in Tiller reclaims it — the pane the user was typing in
/// goes silently unresponsive until they click it again.
@MainActor
struct TerminalSplitHostFocusTests {
    @Test func restoresFirstResponderDroppedByRemoveFromSuperview() {
        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 200, height: 200),
            styleMask: [.titled],
            backing: .buffered,
            defer: false
        )
        let container = NSView(frame: NSRect(x: 0, y: 0, width: 200, height: 200))
        window.contentView = container
        let paneView = FocusableTestView(frame: NSRect(x: 0, y: 0, width: 100, height: 100))
        container.addSubview(paneView)
        #expect(window.makeFirstResponder(paneView) == true)

        // Same detach/reattach TerminalSplitHost.node(_:) does when reusing
        // a cached leaf during a tree rebuild (e.g. splitting a pane).
        paneView.removeFromSuperview()
        #expect(window.firstResponder !== paneView)
        container.addSubview(paneView)

        TerminalSplitHost.restoreFocusIfNeeded(previousFirstResponder: paneView, window: window)

        #expect(window.firstResponder === paneView)
    }
}

private final class FocusableTestView: NSView {
    override var acceptsFirstResponder: Bool { true }
}
