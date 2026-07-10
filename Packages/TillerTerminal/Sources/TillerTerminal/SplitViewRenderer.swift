import SwiftUI
import AppKit
import TillerCore
import GhosttyTerminal

/// AppKit-backed split renderer. Each SplitTree.split becomes an
/// NSSplitViewController (thick dividers, min pane size enforced by
/// holdingPriority + constraints); each leaf is an NSHostingController
/// wrapping PtyTerminalPane. Leaf controllers are cached by UUID in the
/// Coordinator so re-rendering after a tree mutation reuses live terminals
/// instead of respawning shells.
public struct TerminalSplitHost: NSViewControllerRepresentable {
    let tree: SplitTree
    let workingDirectory: String?
    let extraEnvironment: [String: String]
    let paneContext: (
        initial: (UUID) -> Data?,
        onClose: @Sendable (UUID, Data) async -> Void,
        command: (UUID) -> String?,
        onTitleChange: (UUID, String) -> Void,
        onContentSignal: (UUID, String) -> Void,
        onOpenURL: (UUID, String) -> Void
    )?
    let menuProvider: ((UUID, TerminalSurfaceProxy) -> [TerminalContextMenuItem])?
    let onMenuAction: ((TerminalContextMenuAction, UUID, TerminalSurfaceProxy) -> Void)?

    public init(
        tree: SplitTree,
        workingDirectory: String? = nil,
        extraEnvironment: [String: String] = [:],
        paneContext: (
            initial: (UUID) -> Data?,
            onClose: @Sendable (UUID, Data) async -> Void,
            command: (UUID) -> String?,
            onTitleChange: (UUID, String) -> Void,
            onContentSignal: (UUID, String) -> Void,
            onOpenURL: (UUID, String) -> Void
        )? = nil,
        menuProvider: ((UUID, TerminalSurfaceProxy) -> [TerminalContextMenuItem])? = nil,
        onMenuAction: ((TerminalContextMenuAction, UUID, TerminalSurfaceProxy) -> Void)? = nil
    ) {
        self.tree = tree
        self.workingDirectory = workingDirectory
        self.extraEnvironment = extraEnvironment
        self.paneContext = paneContext
        self.menuProvider = menuProvider
        self.onMenuAction = onMenuAction
    }

    public final class Coordinator {
        var leafControllers: [UUID: NSViewController] = [:]
        var lastTree: SplitTree?
        var resizeObserver: TerminalResizeSettleObserver?
        var menuHandler: TerminalContextMenuHandler?
    }

    public func makeCoordinator() -> Coordinator { Coordinator() }

    public func makeNSViewController(context: Context) -> NSViewController {
        let controller = build(tree, coordinator: context.coordinator)
        context.coordinator.resizeObserver = TerminalResizeSettleObserver(containerView: controller.view)
        if let menuProvider {
            let handler = TerminalContextMenuHandler(
                coordinator: context.coordinator,
                provider: menuProvider,
                onAction: onMenuAction ?? { _, _, _ in }
            )
            context.coordinator.menuHandler = handler
            handler.install(on: controller.view)
        }
        return controller
    }

    public func updateNSViewController(_ controller: NSViewController, context: Context) {
        guard context.coordinator.lastTree != tree else { return }
        context.coordinator.lastTree = tree
        // Structure changed: rebuild the container hierarchy. Leaves are
        // pulled from the cache, so their PTYs keep running.
        let newRoot = build(tree, coordinator: context.coordinator)
        guard let container = controller as? ContainerViewController else { return }
        context.coordinator.menuHandler?.uninstall()
        container.setContent(newRoot)
        if let menuProvider = menuProvider {
            let handler = TerminalContextMenuHandler(
                coordinator: context.coordinator,
                provider: menuProvider,
                onAction: onMenuAction ?? { _, _, _ in }
            )
            context.coordinator.menuHandler = handler
            handler.install(on: container.view)
        }
        pruneCache(context.coordinator, keeping: Set(tree.leafIds))
    }

    private func pruneCache(_ coordinator: Coordinator, keeping: Set<UUID>) {
        for key in coordinator.leafControllers.keys where !keeping.contains(key) {
            coordinator.leafControllers.removeValue(forKey: key)
        }
    }

    private func build(_ tree: SplitTree, coordinator: Coordinator) -> NSViewController {
        let root = ContainerViewController()
        root.setContent(node(tree, coordinator: coordinator))
        coordinator.lastTree = tree
        return root
    }

    private func node(_ tree: SplitTree, coordinator: Coordinator) -> NSViewController {
        switch tree {
        case .leaf(let id):
            if let cached = coordinator.leafControllers[id] { return cached }
            let hosting = NSHostingController(
                rootView: PtyTerminalPane(
                    workingDirectory: workingDirectory,
                    command: paneContext?.command(id),
                    paneId: id,
                    initialScrollback: paneContext?.initial(id),
                    extraEnvironment: extraEnvironment,
                    onScrollback: paneContext?.onClose,
                    onTitleChange: paneContext?.onTitleChange,
                    onContentSignal: paneContext?.onContentSignal,
                    onOpenURL: paneContext?.onOpenURL,
                    onContextMenu: menuProvider
                )
            )
            coordinator.leafControllers[id] = hosting
            return hosting
        case .split(let axis, let first, let second):
            let split = NSSplitViewController()
            split.splitView.isVertical = (axis == .horizontal)
            let a = NSSplitViewItem(viewController: node(first, coordinator: coordinator))
            let b = NSSplitViewItem(viewController: node(second, coordinator: coordinator))
            a.minimumThickness = 160
            b.minimumThickness = 160
            split.addSplitViewItem(a)
            split.addSplitViewItem(b)
            return split
        }
    }
}

/// Stable root controller: TerminalSplitHost's NSViewController identity
/// never changes across updates; only its single child is swapped.
final class ContainerViewController: NSViewController {
    override func loadView() { view = NSView() }

    func setContent(_ child: NSViewController) {
        children.forEach { $0.removeFromParent() }
        view.subviews.forEach { $0.removeFromSuperview() }
        addChild(child)
        child.view.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(child.view)
        NSLayoutConstraint.activate([
            child.view.topAnchor.constraint(equalTo: view.topAnchor),
            child.view.bottomAnchor.constraint(equalTo: view.bottomAnchor),
            child.view.leadingAnchor.constraint(equalTo: view.leadingAnchor),
            child.view.trailingAnchor.constraint(equalTo: view.trailingAnchor)
        ])
    }
}
