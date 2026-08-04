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

    override public func hitTest(_ point: NSPoint) -> NSView? { nil }

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
