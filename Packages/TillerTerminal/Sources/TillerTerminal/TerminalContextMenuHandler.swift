import AppKit
import Foundation
import TillerCore
import GhosttyTerminal

/// Intercepts right-mouse-down events inside a terminal split host,
/// determines which pane was clicked, and presents an AppKit context menu.
@MainActor
final class TerminalContextMenuHandler {
    private weak var coordinator: TerminalSplitHost.Coordinator?
    private let paneCache: TerminalPaneCache?
    private let provider: (UUID, TerminalSurfaceProxy) -> [TerminalContextMenuItem]
    private let onAction: (TerminalContextMenuAction, UUID, TerminalSurfaceProxy) -> Void
    private weak var hostView: NSView?
    private var monitor: Any?

    init(
        coordinator: TerminalSplitHost.Coordinator,
        paneCache: TerminalPaneCache?,
        provider: @escaping (UUID, TerminalSurfaceProxy) -> [TerminalContextMenuItem],
        onAction: @escaping (TerminalContextMenuAction, UUID, TerminalSurfaceProxy) -> Void
    ) {
        self.coordinator = coordinator
        self.paneCache = paneCache
        self.provider = provider
        self.onAction = onAction
    }

    func uninstall() {
        if let monitor {
            NSEvent.removeMonitor(monitor)
            self.monitor = nil
        }
    }

    func install(on view: NSView) {
        self.hostView = view
        monitor = NSEvent.addLocalMonitorForEvents(matching: .rightMouseDown) { [weak self] event in
            guard let self,
                  let hostView = self.hostView,
                  hostView.window != nil,
                  let paneId = self.paneId(for: event, hostView: hostView) else {
                return event
            }
            self.showMenu(for: paneId, with: event, in: hostView)
            return nil
        }
    }

    func paneId(for event: NSEvent, hostView: NSView) -> UUID? {
        guard hostView.window != nil else { return nil }
        let pointInWindow = event.locationInWindow
        let pointInHost = hostView.convert(pointInWindow, from: nil)
        guard let hitView = hostView.hitTest(pointInHost) else { return nil }
        return Self.paneId(forHit: hitView, in: Self.controllers(coordinator: coordinator, paneCache: paneCache))
    }

    /// Leaf controllers live in `paneCache.controllers` when a worktree-shared
    /// cache is in play (the default in production — see TerminalPaneCache),
    /// and in `coordinator.leafControllers` only when no cache was supplied.
    /// Must mirror TerminalSplitHost.cachedController's precedence, or hit
    /// testing here silently misses every pane and the menu never shows.
    static func controllers(
        coordinator: TerminalSplitHost.Coordinator?,
        paneCache: TerminalPaneCache?
    ) -> [UUID: NSViewController] {
        paneCache?.controllers ?? coordinator?.leafControllers ?? [:]
    }

    /// Resolves which pane owns a hit view. The hit view is the deepest view
    /// under the click, so it must be a descendant of (or equal to) the pane's
    /// container — `isDescendant(of:)` on the hit view, not the container.
    static func paneId(forHit hitView: NSView, in leafControllers: [UUID: NSViewController]) -> UUID? {
        for (id, controller) in leafControllers where hitView.isDescendant(of: controller.view) {
            return id
        }
        return nil
    }

    private func showMenu(for paneId: UUID, with event: NSEvent, in view: NSView) {
        guard let proxy = PaneProxyRegistry.shared.proxy(for: paneId) else { return }
        if let controller = coordinator?.leafControllers[paneId] {
            proxy.view = firstTerminalView(in: controller.view)
        }
        let presenter = TerminalContextMenuPresenter(
            paneId: paneId,
            proxy: proxy,
            provider: provider,
            onAction: onAction
        )
        let menu = presenter.menu()
        NSMenu.popUpContextMenu(menu, with: event, for: view)
    }

    private func firstTerminalView(in view: NSView) -> TerminalView? {
        if let terminalView = view as? TerminalView {
            return terminalView
        }
        for subview in view.subviews {
            if let found = firstTerminalView(in: subview) {
                return found
            }
        }
        return nil
    }
}
