import AppKit
import TillerCore

/// AppKit container for one pane group. The group owns the local tab chrome in
/// the full renderer; Phase 4 mounts only its active content host.
@MainActor
public final class PaneGroupController: NSViewController {
    public let id: PaneGroupID
    public private(set) var mountedTabID: WorkspaceTabID?

    private weak var mountedHost: WorkspaceContentHost?

    public init(id: PaneGroupID) {
        self.id = id
        super.init(nibName: nil, bundle: nil)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override public func loadView() {
        view = NSView()
    }

    func update(group: PaneGroup, hostProvider: WorkspaceHostProvider) {
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
        view.addSubview(controller.view)
        NSLayoutConstraint.activate([
            controller.view.leadingAnchor.constraint(equalTo: view.leadingAnchor),
            controller.view.trailingAnchor.constraint(equalTo: view.trailingAnchor),
            controller.view.topAnchor.constraint(equalTo: view.topAnchor),
            controller.view.bottomAnchor.constraint(equalTo: view.bottomAnchor)
        ])
    }
}
