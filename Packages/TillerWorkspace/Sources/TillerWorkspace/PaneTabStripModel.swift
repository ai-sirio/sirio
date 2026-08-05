import AppKit
import TillerCore

/// Supplied by the app target, which owns the theme the strip is drawn in.
public typealias PaneTabStripFactory = @MainActor (PaneTabStripModel) -> NSView

/// What a pane group's tab strip shows and what it can ask for. The strip's
/// appearance lives in the app target, which owns the theme; the package holds
/// only this model and hands it to whatever view the app supplies.
@MainActor
@Observable
public final class PaneTabStripModel {
    public internal(set) var entries: [TabMenuEntry] = []
    public internal(set) var isFocusedGroup = false

    public private(set) var contentWidth: CGFloat = 0
    public private(set) var viewportWidth: CGFloat = 0

    public var activeTabID: WorkspaceTabID? {
        entries.first(where: \.isActive)?.tabID
    }

    public var isOverflowing: Bool {
        PaneTabStripLayout.isOverflowing(
            contentWidth: contentWidth,
            viewportWidth: viewportWidth
        )
    }

    public func updateContentWidth(_ width: CGFloat) {
        contentWidth = max(0, width)
    }

    public func updateViewportWidth(_ width: CGFloat) {
        viewportWidth = max(0, width)
    }

    /// Tab rectangles in the strip view's own coordinate space, top-left origin.
    public private(set) var tabFrames: [WorkspaceTabID: CGRect] = [:]

    public var onActivate: (WorkspaceTabID) -> Void = { _ in }
    public var onClose: (WorkspaceTabID) -> Void = { _ in }
    public var onNewTab: () -> Void = {}

    /// Makes this strip's group the active one. The "+" menu's agent and chat
    /// entries are routed by the app through the active group, so the menu
    /// calls this first — otherwise it would open its tab in whichever pane
    /// happened to be active before the click.
    public var onActivateGroup: () -> Void = {}

    /// Called continuously while a tab is pressed. The point is in screen
    /// coordinates: a SwiftUI drag's local translation cannot address a pane it
    /// does not belong to, and screen space is what both worlds share.
    public var onDragChanged: (WorkspaceTabID, CGPoint) -> Void = { _, _ in }
    public var onDragEnded: (WorkspaceTabID) -> Void = { _ in }
    public var onDragCancelled: () -> Void = {}

    public init() {}

    public func setTabFrame(_ frame: CGRect, for tab: WorkspaceTabID) {
        tabFrames[tab] = frame
    }

    public func removeTabFrames(notIn tabs: Set<WorkspaceTabID>) {
        tabFrames = tabFrames.filter { tabs.contains($0.key) }
    }

    /// Only measured tabs appear. A missing frame means the view has not laid
    /// that tab out yet; inventing a zero rectangle would put a phantom
    /// insertion point at the left edge of the strip.
    public var orderedTabFrames: [CGRect] {
        entries.compactMap { tabFrames[$0.tabID] }
    }
}
