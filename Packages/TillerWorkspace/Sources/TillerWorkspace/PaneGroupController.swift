import AppKit
import TillerCore

/// AppKit container for one pane group. The group owns its local tab chrome and
/// mounts the active tab's content host below it.
@MainActor
public final class PaneGroupController: NSViewController {
    public let id: PaneGroupID
    public private(set) var mountedTabID: WorkspaceTabID?
    public private(set) var accessibilityPane: WorkspacePaneAccessibility?
    /// The tabs this group shows, in order. Drives the chrome and is the value
    /// tests assert against rather than reaching into the view hierarchy.
    public private(set) var tabEntries: [TabMenuEntry] = []

    private weak var mountedHost: WorkspaceContentHost?
    private weak var intentSink: WorkspaceIntentSink?
    private let stripModel = PaneTabStripModel()
    private let stripFactory: PaneTabStripFactory?
    private let contentContainer = NSView()

    public init(
        id: PaneGroupID,
        intentSink: WorkspaceIntentSink? = nil,
        stripFactory: PaneTabStripFactory? = nil
    ) {
        self.id = id
        self.intentSink = intentSink
        self.stripFactory = stripFactory
        super.init(nibName: nil, bundle: nil)
        stripModel.onActivate = { [weak self] in self?.activateTab($0) }
        stripModel.onClose = { [weak self] in self?.closeTab($0) }
        stripModel.onNewTab = { [weak self] in self?.requestNewTab() }
        stripModel.onActivateGroup = { [weak self] in self?.activateGroup() }
    }

    /// The strip's model. Exposed so the collector and the drag wiring can read
    /// the tab rectangles the app-side view measured.
    public var strip: PaneTabStripModel { stripModel }

    /// This group's geometry in `space`'s coordinates with a top-left origin.
    /// AppKit's default is bottom-left; the flip happens here, once, so nothing
    /// downstream has to think about it.
    public func hitFrame(in space: NSView) -> PaneGroupHitFrame? {
        guard isViewLoaded, view.superview != nil else { return nil }
        let converted = view.convert(view.bounds, to: space)
        let flipped = CGRect(
            x: converted.minX,
            y: space.bounds.height - converted.maxY,
            width: converted.width,
            height: converted.height
        )
        let tabFrames = stripModel.orderedTabFrames.map {
            $0.offsetBy(dx: flipped.minX, dy: flipped.minY)
        }
        return PaneGroupHitFrame(
            id: id, bounds: flipped, tabFrames: tabFrames, tabCount: tabEntries.count
        )
    }

    /// The strip lives in the app target and speaks in screen points; the
    /// coordinator owns the rest of the gesture.
    public func connectDrag(to coordinator: WorkspaceDragCoordinator) {
        stripModel.onDragChanged = { [weak self, weak coordinator] tab, screenPoint in
            guard let self, let coordinator else { return }
            if !coordinator.isPressing {
                coordinator.pressBegan(
                    tab: tab, in: self.id, atScreenPoint: screenPoint,
                    tabFrame: self.stripModel.tabFrames[tab] ?? .zero,
                    title: self.tabEntries.first { $0.tabID == tab }?.title ?? ""
                )
            }
            coordinator.pointerMoved(toScreenPoint: screenPoint)
        }
        stripModel.onDragEnded = { [weak coordinator] _ in coordinator?.released() }
        stripModel.onDragCancelled = { [weak coordinator] in
            coordinator?.cancel(reason: .pointerCancelled)
        }
    }

    public func activateTab(_ tabID: WorkspaceTabID) {
        intentSink?.send(.activateTab(tabID))
    }

    public func closeTab(_ tabID: WorkspaceTabID) {
        intentSink?.send(.requestClose(tabID))
    }

    public func requestNewTab() {
        intentSink?.send(.requestNewTab(into: id))
    }

    public func activateGroup() {
        intentSink?.send(.activateGroup(id))
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override public func loadView() {
        view = NSView()
        view.setAccessibilityElement(true)
        view.setAccessibilityRole(.group)

        contentContainer.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(contentContainer)

        guard let strip = stripFactory?(stripModel) else {
            NSLayoutConstraint.activate([
                contentContainer.leadingAnchor.constraint(equalTo: view.leadingAnchor),
                contentContainer.trailingAnchor.constraint(equalTo: view.trailingAnchor),
                contentContainer.topAnchor.constraint(equalTo: view.topAnchor),
                contentContainer.bottomAnchor.constraint(equalTo: view.bottomAnchor)
            ])
            return
        }
        strip.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(strip)

        NSLayoutConstraint.activate([
            strip.leadingAnchor.constraint(equalTo: view.leadingAnchor),
            strip.trailingAnchor.constraint(equalTo: view.trailingAnchor),
            strip.topAnchor.constraint(equalTo: view.topAnchor),
            strip.heightAnchor.constraint(equalToConstant: WorkspaceMetrics.tabStripHeight),
            contentContainer.leadingAnchor.constraint(equalTo: view.leadingAnchor),
            contentContainer.trailingAnchor.constraint(equalTo: view.trailingAnchor),
            contentContainer.topAnchor.constraint(equalTo: strip.bottomAnchor),
            contentContainer.bottomAnchor.constraint(equalTo: view.bottomAnchor)
        ])
    }

    public var accessibilityPaneActions: [WorkspacePaneAccessibilityAction] {
        WorkspaceAccessibility.paneActions
    }

    public func setAccessibilityPosition(
        position: Int,
        total: Int,
        activeTabTitle: String
    ) {
        let pane = WorkspacePaneAccessibility(
            id: id, position: position, total: total, activeTabTitle: activeTabTitle
        )
        accessibilityPane = pane
        if isViewLoaded {
            view.setAccessibilityLabel(pane.label)
        }
    }

    public func accessibilityLabel() -> String {
        accessibilityPane?.label ?? ""
    }

    func update(group: PaneGroup, hostProvider: WorkspaceHostProvider) {
        tabEntries = PaneTabStripView.overflowMenuItems(for: group)
        stripModel.entries = tabEntries

        let nextTabID = group.activeTabID
        let nextHost = nextTabID.flatMap { hostProvider.host(for: $0) }

        // Re-mounting a host that is already mounted takes its view out of the
        // window and puts it straight back, and a terminal surface reads that
        // round trip as losing focus. Reconciles are not rare — a divider drag
        // runs one per frame — so an unchanged mount has to be a no-op.
        if let nextTabID, let nextHost, mountedTabID == nextTabID,
           nextHost.viewController.viewIfLoaded?.superview === contentContainer {
            mountedHost = nextHost
            return
        }

        if let mountedHost {
            mountedHost.setVisible(false)
        }
        detachMountedHost()

        guard let nextHost, let nextTabID else { return }
        attach(nextHost)
        mountedTabID = nextTabID
        mountedHost = nextHost
        nextHost.setVisible(true)
    }

    private func detachMountedHost() {
        if let mountedHost {
            let controller = mountedHost.viewController
            controller.removeFromParent()
            self.mountedHost = nil
        }
        contentContainer.subviews.forEach { $0.removeFromSuperview() }
        mountedTabID = nil
    }

    private func attach(_ host: WorkspaceContentHost) {
        let controller = host.viewController
        controller.removeFromParent()
        controller.viewIfLoaded?.removeFromSuperview()
        addChild(controller)
        controller.view.translatesAutoresizingMaskIntoConstraints = false
        contentContainer.addSubview(controller.view)
        NSLayoutConstraint.activate([
            controller.view.leadingAnchor.constraint(equalTo: contentContainer.leadingAnchor),
            controller.view.trailingAnchor.constraint(equalTo: contentContainer.trailingAnchor),
            controller.view.topAnchor.constraint(equalTo: contentContainer.topAnchor),
            controller.view.bottomAnchor.constraint(equalTo: contentContainer.bottomAnchor)
        ])
    }
}
