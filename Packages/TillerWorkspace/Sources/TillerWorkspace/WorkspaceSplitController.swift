import AppKit
import TillerCore

@MainActor
public final class WorkspaceSplitController: NSSplitViewController {
    public let id: SplitID
    public private(set) var axis: WorkspaceSplitAxis
    public private(set) var preferredFraction: Double
    public private(set) var accessibilityDivider: WorkspaceDividerAccessibility?
    private var didApplyInitialPosition = false
    private var tracking: DividerTracking?

    /// The sink arrives after construction: the reconciler builds split
    /// controllers while walking a layout it has not finished applying.
    public func connectDivider(sink: WorkspaceIntentSink) {
        tracking = DividerTracking(sink: sink)
    }

    override public func splitViewDidResizeSubviews(_ notification: Notification) {
        super.splitViewDidResizeSubviews(notification)
        guard let tracking, splitView.subviews.count >= 2 else { return }

        let step = DividerCommitPolicy.step(for: NSApp.currentEvent?.type)
        guard step != .ignore else { return }

        let isVertical = splitView.isVertical
        let total = isVertical ? splitView.bounds.width : splitView.bounds.height
        let position = DividerPosition.readingOrder(
            first: splitView.subviews[0].frame,
            second: splitView.subviews[1].frame,
            bounds: splitView.bounds,
            isVertical: isVertical
        )

        // `began` resets the gesture, so restarting it on every move keeps the
        // tracker in step with a drag whose start we never observed.
        tracking.began(
            split: id,
            total: total,
            minimum: minimumThickness(for: axis),
            thickness: splitView.dividerThickness
        )
        tracking.moved(to: position)
        if step == .commit { tracking.ended() }
    }

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
        splitView.setAccessibilityElement(true)
        splitView.setAccessibilityRole(.splitter)
    }

    func update(
        axis: WorkspaceSplitAxis,
        preferredFraction: Double,
        first: NSViewController,
        second: NSViewController
    ) {
        // Rebuilding the items detaches both panes from the window, which is
        // how a terminal loses its surface focus mid divider drag. Nothing but
        // the fraction changes on most reconciles, so keep the children in
        // place and re-apply the position only when it actually moved.
        let keepsChildren = axis == self.axis
            && splitViewItems.count == 2
            && splitViewItems[0].viewController === first
            && splitViewItems[1].viewController === second
        let fractionMoved = preferredFraction != self.preferredFraction

        self.axis = axis
        self.preferredFraction = preferredFraction
        accessibilityDivider = nil

        if keepsChildren {
            if fractionMoved { didApplyInitialPosition = false }
            return
        }

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

    public func setAccessibilityDivider(
        firstPaneLabel: String,
        secondPaneLabel: String,
        fraction: Double
    ) {
        let divider = WorkspaceAccessibility.divider(
            axis: axis,
            firstPaneLabel: firstPaneLabel,
            secondPaneLabel: secondPaneLabel,
            fraction: fraction
        )
        accessibilityDivider = divider
        if isViewLoaded {
            splitView.setAccessibilityLabel(divider.label)
        }
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

    // AppKit's default drawing for a thin style paints nothing that survives
    // the translucent surface both panes share, which left two split panes
    // visually continuous.
    override func drawDivider(in rect: NSRect) {
        NSColor.separatorColor.setFill()
        WorkspaceMetrics.dividerHairline(in: rect, isVertical: isVertical).fill()
    }

    override func layout() {
        super.layout()
        exchangeContentFramesIfNeeded()
        window?.invalidateCursorRects(for: self)
        // The overlay measures its bands from this geometry, so a divider drag
        // has to invalidate its rects too, not only ours.
        if let overlay = enclosingDragOverlay {
            window?.invalidateCursorRects(for: overlay)
            overlay.refreshDividerTracking()
        }
    }

    private var enclosingDragOverlay: WorkspaceDragOverlay? {
        var node: NSView? = self
        while let current = node {
            if let overlay = current.superview?.subviews
                .compactMap({ $0 as? WorkspaceDragOverlay }).first {
                return overlay
            }
            node = current.superview
        }
        return nil
    }

    /// Declares the resize cursor over each divider band. AppKit infers its own
    /// divider rects from subview order, which the frame exchange below breaks,
    /// so the bands are measured from the laid-out geometry instead.
    override func resetCursorRects() {
        super.resetCursorRects()
        let cursor: NSCursor = isVertical ? .resizeLeftRight : .resizeUpDown
        for rect in DividerCursorRects.rects(
            subviewFrames: subviews.map(\.frame), bounds: bounds, isVertical: isVertical) {
            addCursorRect(rect, cursor: cursor)
        }
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
