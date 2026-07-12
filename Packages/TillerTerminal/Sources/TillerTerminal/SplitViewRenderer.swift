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
    let paneCache: TerminalPaneCache?
    let liveLeafIds: (() -> Set<UUID>)?

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
        onMenuAction: ((TerminalContextMenuAction, UUID, TerminalSurfaceProxy) -> Void)? = nil,
        paneCache: TerminalPaneCache? = nil,
        liveLeafIds: (() -> Set<UUID>)? = nil
    ) {
        self.tree = tree
        self.workingDirectory = workingDirectory
        self.extraEnvironment = extraEnvironment
        self.paneContext = paneContext
        self.menuProvider = menuProvider
        self.onMenuAction = onMenuAction
        self.paneCache = paneCache
        self.liveLeafIds = liveLeafIds
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
                paneCache: paneCache,
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
                paneCache: paneCache,
                provider: menuProvider,
                onAction: onMenuAction ?? { _, _, _ in }
            )
            context.coordinator.menuHandler = handler
            handler.install(on: container.view)
        }
        pruneCache(context.coordinator, keeping: Set(tree.leafIds))
    }

    /// Con cache condivisa il set "keeping" è il set dei pane vivi in TUTTI
    /// i tab del worktree (liveLeafIds), non solo quelli di questo host: un
    /// pane appena spostato in un altro tab non va scartato.
    private func pruneCache(_ coordinator: Coordinator, keeping treeLeaves: Set<UUID>) {
        let keeping = liveLeafIds.map { $0() } ?? treeLeaves
        if let paneCache {
            paneCache.prune(keeping: keeping)
        } else {
            for key in coordinator.leafControllers.keys where !keeping.contains(key) {
                coordinator.leafControllers.removeValue(forKey: key)
            }
        }
    }

    private func cachedController(for id: UUID, _ coordinator: Coordinator) -> NSViewController? {
        if let paneCache { return paneCache.controllers[id] }
        return coordinator.leafControllers[id]
    }

    private func storeController(_ controller: NSViewController, for id: UUID, _ coordinator: Coordinator) {
        if let paneCache {
            paneCache.controllers[id] = controller
        } else {
            coordinator.leafControllers[id] = controller
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
            if let cached = cachedController(for: id, coordinator) {
                // Adozione cross-host: stacca il controller dalla gerarchia
                // del tab precedente prima di inserirlo qui (un VC ha un
                // solo parent). No-op nei rebuild interni allo stesso host.
                cached.removeFromParent()
                cached.view.removeFromSuperview()
                return cached
            }
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
            storeController(hosting, for: id, coordinator)
            return hosting
        case .split(let axis, let first, let second):
            let split = EqualSplitViewController()
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

/// NSSplitViewController leaves the divider wherever AppKit's default
/// layout puts it, which for hosting-controller-wrapped views with no
/// meaningful intrinsic size collapses the newer pane to its
/// minimumThickness at the trailing/bottom edge instead of a 50/50 split.
/// Force an even split once, the first time the view has real bounds.
final class EqualSplitViewController: NSSplitViewController {
    private var didSetInitialPosition = false

    override func viewDidLayout() {
        super.viewDidLayout()
        guard !didSetInitialPosition else { return }
        let size = splitView.isVertical ? splitView.bounds.width : splitView.bounds.height
        guard size > 0 else { return }
        didSetInitialPosition = true
        splitView.setPosition(size / 2, ofDividerAt: 0)
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
