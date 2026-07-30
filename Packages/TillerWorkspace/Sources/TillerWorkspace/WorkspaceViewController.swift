import AppKit
import TillerCore

@MainActor
public final class WorkspaceViewController: NSViewController {
    public private(set) var currentLayout: WorkspaceLayout = .empty()

    private let reconciler: WorkspaceReconciler
    private let intentSink: WorkspaceIntentSink

    public init(hostProvider: WorkspaceHostProvider, intentSink: WorkspaceIntentSink) {
        self.reconciler = WorkspaceReconciler(hostProvider: hostProvider)
        self.intentSink = intentSink
        super.init(nibName: nil, bundle: nil)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override public func loadView() {
        view = NSView()
    }

    public func update(layout: WorkspaceLayout, delta: WorkspaceLayoutDelta?) {
        _ = view
        attachRootIfNeeded()
        currentLayout = layout
        let reconcileSignpost = SignpostMetrics.beginInterval("workspaceReconcile")
        reconciler.reconcile(to: layout, delta: delta)
        SignpostMetrics.endInterval("workspaceReconcile", reconcileSignpost)
        view.needsLayout = true
    }

    public func effectiveFraction(for split: SplitID) -> Double? {
        reconciler.splitController(split)?.effectiveFraction
    }

    public func splitController(_ id: SplitID) -> WorkspaceSplitController? {
        reconciler.splitController(id)
    }

    public var focusCoordinator: WorkspaceFocusCoordinator {
        reconciler.focusCoordinator
    }

    private func attachRootIfNeeded() {
        let root = reconciler.rootViewController
        guard !children.contains(where: { $0 === root }) else { return }

        addChild(root)
        root.view.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(root.view)
        NSLayoutConstraint.activate([
            root.view.leadingAnchor.constraint(equalTo: view.leadingAnchor),
            root.view.trailingAnchor.constraint(equalTo: view.trailingAnchor),
            root.view.topAnchor.constraint(equalTo: view.topAnchor),
            root.view.bottomAnchor.constraint(equalTo: view.bottomAnchor)
        ])
    }
}
