import AppKit

/// Paints drag feedback above the pane tree. Flipped, so it shares the drag
/// machinery's coordinate space, and transparent to hit-testing, so the panes
/// underneath keep receiving events during and after a drag.
///
/// It draws directly and never animates: Reduce Motion must not change what the
/// preview says, so there is no motion to reduce.
@MainActor
public final class WorkspaceDragOverlay: NSView {
    public var previewRect: CGRect? {
        didSet { needsDisplay = true }
    }

    public var ghostRect: CGRect? {
        didSet { needsDisplay = true }
    }

    public var ghostTitle: String = "" {
        didSet { needsDisplay = true }
    }

    override public var isFlipped: Bool { true }

    public var isShowingFeedback: Bool {
        previewRect != nil || ghostRect != nil
    }

    public func clear() {
        previewRect = nil
        ghostRect = nil
        ghostTitle = ""
    }

    /// Divider pointer targets, in overlay coordinates, recomputed whenever the
    /// window resets cursor rects. The overlay is the only layer already above
    /// every pane, and hit testing runs front to back — so this is the one place
    /// a divider can claim the pointer no matter what a pane puts under it.
    /// Measured on demand rather than cached from `resetCursorRects`: hit
    /// testing runs before AppKit ever asks for cursor rects, and a band list
    /// that is still empty at that point loses the pointer to whatever is
    /// underneath.
    override public func hitTest(_ point: NSPoint) -> NSView? {
        guard DividerCursorHitPolicy.acceptsHit(eventType: NSApp?.currentEvent?.type) else {
            return nil
        }
        let local = convert(point, from: superview)
        return measureDividerBands().contains { $0.rect.contains(local) } ? self : nil
    }

    override public func resetCursorRects() {
        for band in measureDividerBands() {
            addCursorRect(band.rect, cursor: Self.cursor(isVertical: band.isVertical))
        }
    }

    /// Cursor rects alone are not enough here: measured in the running app, the
    /// overlay won the hit test over a divider and then never received a single
    /// `cursorUpdate`, so the pointer kept whatever the pane had set. Tracking
    /// areas raise those events themselves instead of waiting on the window's
    /// cursor-rect cycle.
    override public func updateTrackingAreas() {
        super.updateTrackingAreas()
        for area in trackingAreas where area.owner === self {
            removeTrackingArea(area)
        }
        for band in measureDividerBands() {
            addTrackingArea(NSTrackingArea(
                rect: band.rect,
                options: [.activeInKeyWindow, .cursorUpdate, .mouseEnteredAndExited],
                owner: self,
                userInfo: ["isVertical": band.isVertical]))
        }
    }

    /// Re-measures the bands after a divider drag, which moves them without
    /// changing the overlay's own frame.
    public func refreshDividerTracking() {
        updateTrackingAreas()
    }

    override public func mouseEntered(with event: NSEvent) {
        guard let isVertical = event.trackingArea?.userInfo?["isVertical"] as? Bool else {
            super.mouseEntered(with: event)
            return
        }
        Self.cursor(isVertical: isVertical).set()
    }

    /// Claiming the hit without an installed cursor rect would leave whatever
    /// the last pane set — an I-beam, in a terminal — sitting over the divider,
    /// so set the cursor outright rather than trusting the rects to be current.
    override public func cursorUpdate(with event: NSEvent) {
        let local = convert(event.locationInWindow, from: nil)
        guard let band = measureDividerBands().first(where: { $0.rect.contains(local) }) else {
            super.cursorUpdate(with: event)
            return
        }
        Self.cursor(isVertical: band.isVertical).set()
    }

    override public func layout() {
        super.layout()
        window?.invalidateCursorRects(for: self)
        updateTrackingAreas()
    }

    private static func cursor(isVertical: Bool) -> NSCursor {
        if #available(macOS 15.0, *) {
            return isVertical ? .columnResize : .rowResize
        }
        return isVertical ? .resizeLeftRight : .resizeUpDown
    }

    /// Walks the pane tree the overlay covers, since splits come and go with
    /// every reconcile and nothing else knows all of them at cursor-rect time.
    private func measureDividerBands() -> [(rect: CGRect, isVertical: Bool)] {
        guard let root = superview else { return [] }
        var bands: [(CGRect, Bool)] = []

        func visit(_ view: NSView) {
            if let split = view as? NSSplitView, split !== self {
                let measured = DividerCursorRects.rects(
                    subviewFrames: split.subviews.map(\.frame),
                    bounds: split.bounds,
                    isVertical: split.isVertical)
                for rect in measured {
                    bands.append((convert(rect, from: split), split.isVertical))
                }
            }
            for subview in view.subviews where subview !== self { visit(subview) }
        }

        visit(root)
        return bands
    }

    override public func draw(_ dirtyRect: NSRect) {
        if let previewRect {
            NSColor.controlAccentColor.withAlphaComponent(0.18).setFill()
            previewRect.fill()
            NSColor.controlAccentColor.setStroke()
            let border = NSBezierPath(rect: previewRect.insetBy(dx: 0.5, dy: 0.5))
            border.lineWidth = 1
            border.stroke()
        }

        guard let ghostRect else { return }
        NSColor.windowBackgroundColor.withAlphaComponent(0.9).setFill()
        let ghost = NSBezierPath(roundedRect: ghostRect, xRadius: 7, yRadius: 7)
        ghost.fill()
        NSColor.separatorColor.setStroke()
        ghost.lineWidth = 1
        ghost.stroke()

        guard !ghostTitle.isEmpty else { return }
        let attributes: [NSAttributedString.Key: Any] = [
            .font: NSFont.systemFont(ofSize: 12),
            .foregroundColor: NSColor.labelColor
        ]
        let size = ghostTitle.size(withAttributes: attributes)
        let origin = CGPoint(x: ghostRect.minX + 9, y: ghostRect.midY - size.height / 2)
        ghostTitle.draw(at: origin, withAttributes: attributes)
    }
}
