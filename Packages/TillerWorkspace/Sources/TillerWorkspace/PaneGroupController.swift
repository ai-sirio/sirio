import AppKit
import SwiftUI
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
    private var tabStrip: NSHostingView<PaneTabBar>?
    private let contentContainer = NSView()

    public init(id: PaneGroupID, intentSink: WorkspaceIntentSink? = nil) {
        self.id = id
        self.intentSink = intentSink
        super.init(nibName: nil, bundle: nil)
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

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override public func loadView() {
        view = NSView()
        view.setAccessibilityElement(true)
        view.setAccessibilityRole(.group)

        let strip = NSHostingView(rootView: makeTabBar())
        strip.translatesAutoresizingMaskIntoConstraints = false
        contentContainer.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(strip)
        view.addSubview(contentContainer)
        tabStrip = strip

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

    private func makeTabBar() -> PaneTabBar {
        PaneTabBar(
            entries: tabEntries,
            onActivate: { [weak self] in self?.activateTab($0) },
            onClose: { [weak self] in self?.closeTab($0) },
            onNewTab: { [weak self] in self?.requestNewTab() })
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
        if isViewLoaded { tabStrip?.rootView = makeTabBar() }

        let nextTabID = group.activeTabID
        let nextHost = nextTabID.flatMap { hostProvider.host(for: $0) }

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
        guard let mountedHost else {
            mountedTabID = nil
            return
        }
        let controller = mountedHost.viewController
        controller.removeFromParent()
        controller.viewIfLoaded?.removeFromSuperview()
        self.mountedHost = nil
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
