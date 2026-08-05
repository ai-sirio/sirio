import AppKit
import TillerCore

public struct TabMenuEntry: Equatable, Sendable {
    public let tabID: WorkspaceTabID
    public let title: String
    public let isActive: Bool
    public let content: WorkspaceContentRef?

    public init(
        tabID: WorkspaceTabID,
        title: String,
        isActive: Bool,
        content: WorkspaceContentRef? = nil
    ) {
        self.tabID = tabID
        self.title = title
        self.isActive = isActive
        self.content = content
    }
}

@MainActor
public final class PaneTabStripView: NSView {
    public let dragSession: DragSession
    public private(set) var contentOffset = CGPoint.zero

    private let horizontalScrollView: NSScrollView
    private var tabFrames: [WorkspaceTabID: CGRect] = [:]
    private var viewport = CGRect.zero
    private var contentWidth: CGFloat = 0

    public override init(frame frameRect: NSRect) {
        dragSession = DragSession()
        horizontalScrollView = NSScrollView(frame: .zero)
        super.init(frame: frameRect)
        horizontalScrollView.hasHorizontalScroller = true
        horizontalScrollView.hasVerticalScroller = false
        horizontalScrollView.autohidesScrollers = true
        horizontalScrollView.drawsBackground = false
        addSubview(horizontalScrollView)
        setAccessibilityElement(true)
        setAccessibilityRole(.group)
        setAccessibilityLabel(WorkspaceAnnouncements.tabStripLabel)
    }

    public convenience init() {
        self.init(frame: .zero)
    }

    public var accessibilityTabActions: [WorkspaceTabAccessibilityAction] {
        WorkspaceAccessibility.tabActions
    }

    /// Tabs remain local to this strip. The workspace rotor contains panes,
    /// never a second global copy of these tab elements.
    public var accessibilityRotorName: String? { nil }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    public static func overflowMenuItems(for group: PaneGroup) -> [TabMenuEntry] {
        group.tabs.map { tab in
            TabMenuEntry(
                tabID: tab.id,
                title: tab.title,
                isActive: tab.id == group.activeTabID,
                content: tab.content
            )
        }
    }

    public func setTabFrames(
        _ frames: [WorkspaceTabID: CGRect],
        viewport: CGRect
    ) {
        tabFrames = frames
        self.viewport = viewport
        contentWidth = frames.values.map(\.maxX).max() ?? 0
        let document = horizontalScrollView.documentView ?? NSView()
        horizontalScrollView.documentView = document
        document.frame = CGRect(x: 0, y: 0, width: contentWidth, height: viewport.height)
        contentOffset = clampedOffset(contentOffset)
    }

    public func activate(tab: WorkspaceTabID) {
        scrollToReveal(tab: tab, animated: false)
    }

    public func scrollToReveal(tab: WorkspaceTabID, animated: Bool) {
        _ = animated
        guard let frame = tabFrames[tab] else { return }
        let visible = CGRect(origin: contentOffset, size: viewport.size)
        var next = contentOffset
        if frame.minX < visible.minX {
            next.x = frame.minX
        } else if frame.maxX > visible.maxX {
            next.x = frame.maxX - visible.width
        }
        contentOffset = clampedOffset(next)
        horizontalScrollView.contentView.setBoundsOrigin(contentOffset)
    }

    public func isTabVisible(_ tab: WorkspaceTabID) -> Bool {
        guard let frame = tabFrames[tab] else { return false }
        return CGRect(origin: contentOffset, size: viewport.size).contains(frame)
    }

    /// Returns and applies a small horizontal scroll only while a drag is in
    /// progress. Pointer release and menu activation never call this path.
    @discardableResult
    public func edgeAutoscrollDelta(
        pointerX: CGFloat,
        viewport: CGRect,
        contentWidth: CGFloat
    ) -> CGFloat {
        guard dragSession.isActive else { return 0 }
        let edgeBand = min(24, viewport.width / 4)
        let delta: CGFloat
        if pointerX < viewport.minX + edgeBand {
            delta = -min(24, viewport.minX + edgeBand - pointerX)
        } else if pointerX > viewport.maxX - edgeBand {
            delta = min(24, pointerX - (viewport.maxX - edgeBand))
        } else {
            return 0
        }
        let nextX = min(max(contentOffset.x + delta, 0), max(0, contentWidth - viewport.width))
        let applied = nextX - contentOffset.x
        contentOffset.x = nextX
        horizontalScrollView.contentView.setBoundsOrigin(contentOffset)
        return applied
    }

    override public func layout() {
        super.layout()
        horizontalScrollView.frame = bounds
    }

    private func clampedOffset(_ offset: CGPoint) -> CGPoint {
        CGPoint(
            x: min(max(offset.x, 0), max(0, contentWidth - viewport.width)),
            y: 0
        )
    }
}
