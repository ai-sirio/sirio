import AppKit
import TillerCore

@MainActor
public final class WorkspaceSplitController: NSSplitViewController {
    public let id: SplitID
    public private(set) var axis: WorkspaceSplitAxis
    public private(set) var preferredFraction: Double

    public init(id: SplitID, axis: WorkspaceSplitAxis, preferredFraction: Double) {
        self.id = id
        self.axis = axis
        self.preferredFraction = preferredFraction
        super.init(nibName: nil, bundle: nil)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override public func loadView() {
        splitView = WorkspaceNativeSplitView()
        view = splitView
    }

    func update(
        axis: WorkspaceSplitAxis,
        preferredFraction: Double,
        first: NSViewController,
        second: NSViewController
    ) {
        self.axis = axis
        self.preferredFraction = preferredFraction
        splitView.isVertical = axis == .horizontal
        removeAllChildren()
        add(first, minimumThickness: minimumThickness(for: axis))
        add(second, minimumThickness: minimumThickness(for: axis))
    }

    /// The AppKit fraction that can currently be displayed without making a
    /// group smaller than its preferred usable size. Core's preferred value is
    /// intentionally left untouched when this value is clamped.
    public var effectiveFraction: Double? {
        guard splitView.bounds.width > 0, splitView.bounds.height > 0 else { return nil }
        let total = splitView.isVertical ? splitView.bounds.width : splitView.bounds.height
        let available = total - splitView.dividerThickness
        guard available > 0 else { return nil }

        let minimum = minimumThickness(for: axis)
        let lower = Double(minimum / available)
        let upper = 1 - lower
        guard lower <= upper else { return 0.5 }
        return min(max(preferredFraction, lower), upper)
    }

    private func removeAllChildren() {
        for item in splitViewItems {
            let child = item.viewController
            removeSplitViewItem(item)
            child.removeFromParent()
            child.viewIfLoaded?.removeFromSuperview()
        }
    }

    private func add(_ controller: NSViewController, minimumThickness: CGFloat) {
        controller.removeFromParent()
        controller.viewIfLoaded?.removeFromSuperview()
        let item = NSSplitViewItem(viewController: controller)
        item.minimumThickness = minimumThickness
        item.holdingPriority = NSLayoutConstraint.Priority(250)
        addSplitViewItem(item)
    }

    private func minimumThickness(for axis: WorkspaceSplitAxis) -> CGFloat {
        axis == .horizontal ? 240 : 160
    }
}

private final class WorkspaceNativeSplitView: NSSplitView {
    override var dividerThickness: CGFloat { 6 }
}
