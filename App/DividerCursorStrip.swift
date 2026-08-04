import AppKit
import SwiftUI

/// A hover-only resize cursor for an `HSplitView` divider.
///
/// The divider itself is a hairline: `NSSplitView` accepts a drag from a band
/// several points wide, but the cursor rect it installs is as thin as the line,
/// so the resize cursor only ever appeared once a drag was already under way.
/// This widens the band the cursor responds to, while letting clicks fall
/// through to the divider — see `DividerCursorHitPolicy`.
struct DividerCursorStrip: NSViewRepresentable {
    /// Wide enough to land on without aiming, narrow enough not to claim the
    /// cursor while the pointer is working inside either pane.
    static let width: CGFloat = 10

    var cursor: NSCursor = .resizeLeftRight

    func makeNSView(context: Context) -> DividerCursorStripView {
        DividerCursorStripView(cursor: cursor)
    }

    func updateNSView(_ nsView: DividerCursorStripView, context: Context) {
        nsView.cursor = cursor
    }
}

/// Which events the cursor strip answers.
///
/// AppKit dispatches `cursorUpdate` through `hitTest`, so a view that refuses
/// every hit test never shows its cursor rect either — measured, not assumed.
/// The strip therefore has to answer hover events, and must keep refusing
/// clicks so a mouse-down still reaches the divider underneath.
enum DividerCursorHitPolicy {
    static func acceptsHit(eventType: NSEvent.EventType?) -> Bool {
        switch eventType {
        case .mouseMoved, .cursorUpdate, .mouseEntered, .mouseExited: true
        default: false
        }
    }
}

final class DividerCursorStripView: NSView {
    var cursor: NSCursor {
        didSet { window?.invalidateCursorRects(for: self) }
    }

    init(cursor: NSCursor) {
        self.cursor = cursor
        super.init(frame: .zero)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func hitTest(_ point: NSPoint) -> NSView? {
        DividerCursorHitPolicy.acceptsHit(eventType: NSApp.currentEvent?.type)
            ? super.hitTest(point)
            : nil
    }

    override func resetCursorRects() {
        addCursorRect(bounds, cursor: cursor)
    }

    // The strip follows a divider that moves during a drag, and cursor rects
    // are cached against the old geometry until invalidated.
    override func layout() {
        super.layout()
        window?.invalidateCursorRects(for: self)
    }
}
