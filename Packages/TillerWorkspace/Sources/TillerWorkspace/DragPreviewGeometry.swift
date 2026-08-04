import CoreGraphics
import TillerCore

/// Where the Edge Preview overlay paints, in workspace-root flipped
/// coordinates. Pure geometry, so what the user is promised and what the drop
/// actually does are decided by the same code, under test, without a view.
public enum DragPreviewGeometry {
    public static let insertionCaretWidth: CGFloat = 2

    /// The region the drop would occupy, or nil when the target is not a legal
    /// drop or names a pane that is no longer on screen.
    public static func previewRect(
        for target: DropTarget,
        in frames: [PaneGroupHitFrame],
        tabStripHeight: CGFloat = WorkspaceMetrics.tabStripHeight
    ) -> CGRect? {
        switch target {
        case .none:
            return nil

        case .center(let id):
            guard let frame = frames.first(where: { $0.id == id }) else { return nil }
            return content(of: frame, tabStripHeight: tabStripHeight)

        case .edge(let id, let placement):
            guard let frame = frames.first(where: { $0.id == id }) else { return nil }
            return half(of: content(of: frame, tabStripHeight: tabStripHeight), on: placement)

        case .tabStrip(let id, let insertionIndex):
            guard let frame = frames.first(where: { $0.id == id }) else { return nil }
            return caret(at: insertionIndex, in: frame, tabStripHeight: tabStripHeight)
        }
    }

    /// The strip is chrome, not a drop region: previewing over it would cover
    /// the very tabs the caret is meant to sit between.
    private static func content(of frame: PaneGroupHitFrame, tabStripHeight: CGFloat) -> CGRect {
        let strip = min(max(0, tabStripHeight), frame.bounds.height)
        return CGRect(
            x: frame.bounds.minX,
            y: frame.bounds.minY + strip,
            width: frame.bounds.width,
            height: frame.bounds.height - strip
        )
    }

    private static func half(of rect: CGRect, on placement: EdgePlacement) -> CGRect {
        switch placement {
        case .left:
            return CGRect(x: rect.minX, y: rect.minY, width: rect.width / 2, height: rect.height)
        case .right:
            return CGRect(x: rect.midX, y: rect.minY, width: rect.width / 2, height: rect.height)
        case .top:
            return CGRect(x: rect.minX, y: rect.minY, width: rect.width, height: rect.height / 2)
        case .bottom:
            return CGRect(x: rect.minX, y: rect.midY, width: rect.width, height: rect.height / 2)
        }
    }

    private static func caret(
        at index: Int,
        in frame: PaneGroupHitFrame,
        tabStripHeight: CGFloat
    ) -> CGRect {
        let strip = min(max(0, tabStripHeight), frame.bounds.height)
        let x: CGFloat
        if index <= 0 {
            x = frame.tabFrames.first?.minX ?? frame.bounds.minX
        } else if index >= frame.tabFrames.count {
            x = frame.tabFrames.last?.maxX ?? frame.bounds.minX
        } else {
            x = frame.tabFrames[index].minX
        }
        return CGRect(
            x: x - insertionCaretWidth / 2,
            y: frame.bounds.minY,
            width: insertionCaretWidth,
            height: strip
        )
    }
}
