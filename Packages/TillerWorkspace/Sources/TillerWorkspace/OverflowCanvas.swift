import AppKit
import CoreGraphics
import TillerCore

@MainActor
public final class OverflowCanvas: NSScrollView {
    private var canvasSize = CGSize.zero

    public override init(frame frameRect: NSRect) {
        super.init(frame: frameRect)
        hasHorizontalScroller = true
        hasVerticalScroller = true
        autohidesScrollers = true
        drawsBackground = false
        horizontalScrollElasticity = .none
        verticalScrollElasticity = .none
    }

    public convenience init() {
        self.init(frame: .zero)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    public static func minimumCanvasSize(for layout: WorkspaceLayout) -> CGSize {
        minimumCanvasSize(for: layout.root)
    }

    public static func isOverflowing(canvas: CGSize, viewport: CGSize) -> Bool {
        canvas.width > viewport.width || canvas.height > viewport.height
    }

    /// Returns the smallest content offset that makes the whole group frame
    /// visible. Frames and offsets are expressed in document coordinates.
    public static func minimumScrollOffset(
        for groupFrame: CGRect,
        viewport: CGRect,
        currentOffset: CGPoint
    ) -> CGPoint {
        let visible = CGRect(origin: currentOffset, size: viewport.size)
        var offset = currentOffset
        if groupFrame.minX < visible.minX {
            offset.x = groupFrame.minX
        } else if groupFrame.maxX > visible.maxX {
            offset.x = groupFrame.maxX - visible.width
        }
        if groupFrame.minY < visible.minY {
            offset.y = groupFrame.minY
        } else if groupFrame.maxY > visible.maxY {
            offset.y = groupFrame.maxY - visible.height
        }
        return CGPoint(x: max(0, offset.x), y: max(0, offset.y))
    }

    public func setCanvasSize(_ size: CGSize) {
        canvasSize = size
        let document = documentView ?? NSView()
        documentView = document
        document.frame = CGRect(origin: .zero, size: size)
    }

    public func reveal(groupFrame: CGRect, animated: Bool) {
        _ = animated
        let viewport = CGRect(origin: .zero, size: bounds.size)
        let next = Self.minimumScrollOffset(
            for: groupFrame,
            viewport: viewport,
            currentOffset: contentView.bounds.origin
        )
        contentView.setBoundsOrigin(next)
    }

    private static func minimumCanvasSize(for node: LayoutNode) -> CGSize {
        switch node {
        case .group:
            return WorkspaceMetrics.preferredGroupSize
        case .split(_, let axis, _, let first, let second):
            let firstSize = minimumCanvasSize(for: first)
            let secondSize = minimumCanvasSize(for: second)
            switch axis {
            case .horizontal:
                return CGSize(
                    width: firstSize.width + WorkspaceMetrics.dividerThickness + secondSize.width,
                    height: max(firstSize.height, secondSize.height)
                )
            case .vertical:
                return CGSize(
                    width: max(firstSize.width, secondSize.width),
                    height: firstSize.height + WorkspaceMetrics.dividerThickness + secondSize.height
                )
            }
        }
    }
}
