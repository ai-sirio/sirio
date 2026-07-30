import AppKit
import TillerCore

@MainActor
public final class WorkspaceReconciler {
    public let rootViewController: NSViewController

    private let hostProvider: WorkspaceHostProvider
    public let focusCoordinator = WorkspaceFocusCoordinator()
    private var groupControllers: [PaneGroupID: PaneGroupController] = [:]
    private var splitControllers: [SplitID: WorkspaceSplitController] = [:]
    private var pendingFocusTask: Task<Void, Never>?

    public init(hostProvider: WorkspaceHostProvider) {
        self.hostProvider = hostProvider
        self.rootViewController = WorkspaceRootController()
    }

    /// Applies a new layout, reusing cached controllers by stable ID.
    public func reconcile(to layout: WorkspaceLayout, delta: WorkspaceLayoutDelta?) {
        reconcile(to: layout, delta: delta, focusIntent: focusIntent(for: layout, delta: delta))
    }

    /// Explicit focus is useful to the composition root when a transition's
    /// focus intent is available alongside its layout delta.
    public func reconcile(
        to layout: WorkspaceLayout,
        delta: WorkspaceLayoutDelta?,
        focusIntent: FocusIntent
    ) {
        let window = currentWindow
        let previousFirstResponder = window?.firstResponder as? NSView
        let newRoot = build(layout.root, in: layout)

        let root = rootViewController as! WorkspaceRootController
        root.setContent(newRoot)
        updateAccessibility(for: layout)
        prune(keepingGroups: Set(layout.groups.keys), keepingSplits: Set(layout.splitIDs()))
        restoreFocusIfNeeded(previousFirstResponder: previousFirstResponder, window: window)
        if delta?.activeGroupChanged != nil || focusCoordinator.activeGroupID == nil {
            let activeGroup = layout.activeGroupID
            let contentKind = layout.group(activeGroup)?.activeTabID
                .flatMap { layout.tab($0)?.content.kind } ?? .terminal
            _ = focusCoordinator.focusPane(activeGroup, contentKind: contentKind)
        }
        fulfill(focusIntent, in: layout)
    }

    public func groupController(_ id: PaneGroupID) -> PaneGroupController? {
        groupControllers[id]
    }

    public func splitController(_ id: SplitID) -> WorkspaceSplitController? {
        splitControllers[id]
    }

    public var mountedHostCount: Int {
        groupControllers.values.reduce(into: 0) { count, controller in
            if controller.mountedTabID != nil { count += 1 }
        }
    }

    private func build(_ node: LayoutNode, in layout: WorkspaceLayout) -> NSViewController {
        switch node {
        case .group(let id):
            let controller = groupControllers[id] ?? {
                let created = PaneGroupController(id: id)
                groupControllers[id] = created
                return created
            }()
            if let group = layout.group(id) {
                controller.update(group: group, hostProvider: hostProvider)
            }
            return controller

        case .split(let id, let axis, let fraction, let first, let second):
            let controller = splitControllers[id] ?? {
                let created = WorkspaceSplitController(
                    id: id, axis: axis, preferredFraction: fraction
                )
                splitControllers[id] = created
                return created
            }()
            let firstController = build(first, in: layout)
            let secondController = build(second, in: layout)
            controller.update(
                axis: axis,
                preferredFraction: fraction,
                first: firstController,
                second: secondController
            )
            return controller
        }
    }

    private func prune(keepingGroups: Set<PaneGroupID>, keepingSplits: Set<SplitID>) {
        groupControllers = groupControllers.filter { keepingGroups.contains($0.key) }
        splitControllers = splitControllers.filter { keepingSplits.contains($0.key) }
    }

    private func updateAccessibility(for layout: WorkspaceLayout) {
        let total = layout.orderedGroupIDs.count
        for (index, groupID) in layout.orderedGroupIDs.enumerated() {
            guard let controller = groupControllers[groupID], let group = layout.group(groupID) else {
                continue
            }
            controller.setAccessibilityPosition(
                position: index + 1,
                total: total,
                activeTabTitle: group.tabs.first { $0.id == group.activeTabID }?.title
                    ?? WorkspaceAnnouncements.emptyPaneTitle
            )
        }
    }

    private func focusIntent(for layout: WorkspaceLayout, delta: WorkspaceLayoutDelta?) -> FocusIntent {
        guard let delta else { return .none }
        if let groupID = delta.activeGroupChanged,
           let tabID = layout.group(groupID)?.activeTabID {
            return .focusTab(tabID)
        }
        for groupID in layout.orderedGroupIDs {
            if let tabID = delta.activeTabChanges[groupID] ?? nil {
                return .focusTab(tabID)
            }
        }
        return .none
    }

    private func fulfill(_ intent: FocusIntent, in layout: WorkspaceLayout) {
        guard case .focusTab(let tabID) = intent,
              let host = hostProvider.host(for: tabID),
              let groupID = layout.groupContaining(tab: tabID) else { return }

        let result = focusCoordinator.requestFocus(.tab(tabID), in: groupID) {
            host.fulfill(intent)
        }
        guard result == .pendingRetry else { return }

        pendingFocusTask?.cancel()
        pendingFocusTask = Task { @MainActor [weak self] in
            await Task.yield()
            guard !Task.isCancelled, let self else { return }
            _ = self.focusCoordinator.hostAttached()
            self.pendingFocusTask = nil
        }
    }

    private var currentWindow: NSWindow? {
        if let window = rootViewController.viewIfLoaded?.window { return window }
        if let window = groupControllers.values.compactMap({ $0.viewIfLoaded?.window }).first {
            return window
        }
        return splitControllers.values.compactMap({ $0.viewIfLoaded?.window }).first
    }

    private func restoreFocusIfNeeded(previousFirstResponder: NSView?, window: NSWindow?) {
        guard let previousFirstResponder, let window,
              previousFirstResponder.window === window,
              window.firstResponder !== previousFirstResponder else { return }
        window.makeFirstResponder(previousFirstResponder)
    }
}

@MainActor
private final class WorkspaceRootController: NSViewController {
    override func loadView() { view = NSView() }

    func setContent(_ child: NSViewController) {
        children.forEach { $0.removeFromParent() }
        view.subviews.forEach { $0.removeFromSuperview() }
        child.removeFromParent()
        child.viewIfLoaded?.removeFromSuperview()
        addChild(child)
        child.view.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(child.view)
        NSLayoutConstraint.activate([
            child.view.leadingAnchor.constraint(equalTo: view.leadingAnchor),
            child.view.trailingAnchor.constraint(equalTo: view.trailingAnchor),
            child.view.topAnchor.constraint(equalTo: view.topAnchor),
            child.view.bottomAnchor.constraint(equalTo: view.bottomAnchor)
        ])
    }
}
