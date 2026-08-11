import AppKit
import SwiftUI
import TillerWorkspace

/// A hover-only target sitting over an `HSplitView` divider.
///
/// The divider itself is a hairline: `NSSplitView` accepts a drag from a band
/// several points wide, but the cursor rect it installs is as thin as the line,
/// so the resize cursor only ever appeared once a drag was already under way.
/// This widens the band the cursor responds to, and reports hover for the seam
/// highlight, while letting clicks fall through to the divider — see
/// `DividerCursorHitPolicy`.
struct DividerCursorStrip: NSViewRepresentable {
    /// Wide enough to land on without aiming, narrow enough not to claim the
    /// cursor while the pointer is working inside either pane.
    static let width: CGFloat = 10

    var cursor: NSCursor = .resizeLeftRight
    var onHoverChange: (Bool) -> Void = { _ in }

    func makeNSView(context: Context) -> DividerCursorStripView {
        DividerCursorStripView(cursor: cursor)
    }

    func updateNSView(_ nsView: DividerCursorStripView, context: Context) {
        nsView.cursor = cursor
        nsView.onHoverChange = onHoverChange
    }
}

final class DividerCursorStripView: NSView {
    var cursor: NSCursor {
        didSet { window?.invalidateCursorRects(for: self) }
    }

    /// Reported from the tracking area rather than from `hitTest`: enter and
    /// exit are delivered straight to the owning view, so the strip can report
    /// hover while still refusing every click.
    var onHoverChange: (Bool) -> Void = { _ in }

    private var hoverTracking: NSTrackingArea?

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

    override func updateTrackingAreas() {
        super.updateTrackingAreas()
        if let hoverTracking { removeTrackingArea(hoverTracking) }
        // .inVisibleRect keeps the area sized to the strip as the divider
        // moves, which makes the passed rect irrelevant.
        let area = NSTrackingArea(
            rect: .zero,
            options: [.mouseEnteredAndExited, .activeInKeyWindow, .inVisibleRect],
            owner: self,
            userInfo: nil)
        addTrackingArea(area)
        hoverTracking = area
    }

    override func mouseEntered(with event: NSEvent) {
        onHoverChange(true)
    }

    override func mouseExited(with event: NSEvent) {
        onHoverChange(false)
    }

    // Hiding a side panel removes the strip without ever delivering an exit,
    // which would leave the seam lit the next time that panel is shown.
    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        if window == nil { onHoverChange(false) }
    }

    // The strip follows a divider that moves during a drag, and cursor rects
    // are cached against the old geometry until invalidated.
    override func layout() {
        super.layout()
        window?.invalidateCursorRects(for: self)
    }
}
