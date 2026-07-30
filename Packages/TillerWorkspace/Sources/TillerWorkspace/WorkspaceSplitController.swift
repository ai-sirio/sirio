import AppKit
import TillerCore

@MainActor
public final class WorkspaceSplitController: NSSplitViewController {
    public let id: SplitID
    public private(set) var axis: WorkspaceSplitAxis
    public private(set) var preferredFraction: Double
    private var didApplyInitialPosition = false

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
        let nativeSplitView = WorkspaceNativeSplitView()
        nativeSplitView.firstContentOnTop = axis == .vertical
        splitView = nativeSplitView
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
        didApplyInitialPosition = false
        removeAllChildren()
        add(first, minimumThickness: minimumThickness(for: axis))
        add(second, minimumThickness: minimumThickness(for: axis))
        // `NSSplitViewController` may normalize orientation while items are
        // being installed, so apply the domain axis after the item update.
        splitView.isVertical = axis == .horizontal
        (splitView as? WorkspaceNativeSplitView)?.firstContentOnTop = axis == .vertical
    }

    override public func viewDidLayout() {
        super.viewDidLayout()
        splitView.isVertical = axis == .horizontal
        guard !didApplyInitialPosition,
              splitView.bounds.width > 0,
              splitView.bounds.height > 0,
              let effectiveFraction else { return }
        let total = splitView.isVertical ? splitView.bounds.width : splitView.bounds.height
        let position = CGFloat(effectiveFraction) * (total - splitView.dividerThickness)
        splitView.setPosition(position, ofDividerAt: 0)
        didApplyInitialPosition = true
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
        axis == .horizontal
            ? WorkspaceMetrics.preferredGroupSize.width
            : WorkspaceMetrics.preferredGroupSize.height
    }
}

private final class WorkspaceNativeSplitView: NSSplitView {
    var firstContentOnTop = false
    private var isExchangingFrames = false

    override var dividerThickness: CGFloat { WorkspaceMetrics.dividerThickness }

    override func layout() {
        super.layout()
        exchangeContentFramesIfNeeded()
    }

    override func adjustSubviews() {
        super.adjustSubviews()
        exchangeContentFramesIfNeeded()
    }

    private func exchangeContentFramesIfNeeded() {
        guard firstContentOnTop, !isExchangingFrames, subviews.count >= 2 else { return }

        // AppKit lays out the first item below the divider for a horizontal
        // divider. Core's vertical axis is defined in reading order, so
        // exchange the two content frames after native layout.
        isExchangingFrames = true
        let first = subviews[0]
        let second = subviews[1]
        let firstFrame = first.frame
        first.frame = second.frame
        second.frame = firstFrame
        isExchangingFrames = false
    }
}
