import SwiftUI
import AppKit

enum TitlebarControlSlot: Hashable {
    case sidebar
    case rightPanel
    case split
    case permissions
}

struct TitlebarControlProbeRecord: Hashable {
    let slot: TitlebarControlSlot
    let accessibilityIdentifier: String?
}

@MainActor
final class TitlebarAccessoryOwnershipToken {}

@MainActor
final class TitlebarAccessoryHost {
    private(set) var leadingController: NSTitlebarAccessoryViewController?
    private(set) var trailingController: NSTitlebarAccessoryViewController?
    private(set) var leadingHostingView: NSHostingView<AnyView>?
    private(set) var trailingHostingView: NSHostingView<AnyView>?
    private(set) var renderedControlProbes: Set<TitlebarControlProbeRecord> = []
    private(set) var updateCount = 0
    private weak var ownerWindow: NSWindow?
    private var ownerToken: ObjectIdentifier?

    func install(on window: NSWindow, token: TitlebarAccessoryOwnershipToken? = nil) {
        if let ownerWindow, ownerWindow !== window {
            remove(from: ownerWindow)
        }
        if let token {
            ownerToken = ObjectIdentifier(token)
        }
        ownerWindow = window

        if leadingController.map({ window.titlebarAccessoryViewControllers.contains($0) }) != true {
            leadingController = nil
            leadingHostingView = nil
        }
        if trailingController.map({ window.titlebarAccessoryViewControllers.contains($0) }) != true {
            trailingController = nil
            trailingHostingView = nil
        }
        if leadingController == nil {
            let leading = makeController(layoutAttribute: .left, width: TitlebarGeometry.controlFrame.width)
            window.addTitlebarAccessoryViewController(leading.controller)
            leadingController = leading.controller
            leadingHostingView = leading.hostingView
        }
        if trailingController == nil {
            let trailing = makeController(
                layoutAttribute: .right,
                width: 3 * TitlebarGeometry.controlFrame.width + 2 * TitlebarGeometry.controlSpacing)
            window.addTitlebarAccessoryViewController(trailing.controller)
            trailingController = trailing.controller
            trailingHostingView = trailing.hostingView
        }
    }

    func update(leading: AnyView, trailing: AnyView) {
        updateCount = min(updateCount + 1, 1)
        leadingHostingView?.rootView = leading
        trailingHostingView?.rootView = trailing

        leadingHostingView?.setFrameSize(CGSize(
            width: TitlebarGeometry.controlFrame.width,
            height: TitlebarGeometry.accessoryHeight))
        trailingHostingView?.setFrameSize(CGSize(
            width: 3 * TitlebarGeometry.controlFrame.width + 2 * TitlebarGeometry.controlSpacing,
            height: TitlebarGeometry.accessoryHeight))
    }

    func recordRenderedControl(slot: TitlebarControlSlot, accessibilityIdentifier: String?) {
        renderedControlProbes.insert(
            TitlebarControlProbeRecord(
                slot: slot,
                accessibilityIdentifier: accessibilityIdentifier))
    }

    func remove(from window: NSWindow, token: TitlebarAccessoryOwnershipToken? = nil) {
        guard ownerWindow === window else { return }
        if let token, ownerToken != ObjectIdentifier(token) { return }

        for controller in [leadingController, trailingController].compactMap({ $0 }) {
            guard let index = window.titlebarAccessoryViewControllers.firstIndex(of: controller) else {
                continue
            }
            window.removeTitlebarAccessoryViewController(at: index)
        }

        leadingController = nil
        trailingController = nil
        leadingHostingView = nil
        trailingHostingView = nil
        renderedControlProbes.removeAll(keepingCapacity: false)
        updateCount = 0
        ownerWindow = nil
        ownerToken = nil
    }

    func removeFromOwnerWindow(token: TitlebarAccessoryOwnershipToken? = nil) {
        guard let ownerWindow else { return }
        remove(from: ownerWindow, token: token)
    }

    private func makeController(
        layoutAttribute: NSLayoutConstraint.Attribute,
        width: CGFloat
    ) -> (controller: NSTitlebarAccessoryViewController, hostingView: NSHostingView<AnyView>) {
        let controller = NSTitlebarAccessoryViewController()
        controller.layoutAttribute = layoutAttribute
        let hostingView = NSHostingView(rootView: AnyView(EmptyView()))
        hostingView.frame = CGRect(
            x: 0,
            y: 0,
            width: width,
            height: TitlebarGeometry.accessoryHeight)
        controller.view = hostingView
        controller.view.frame = hostingView.frame
        return (controller, hostingView)
    }
}

struct TitlebarControlProbe: NSViewRepresentable {
    let slot: TitlebarControlSlot
    let accessibilityIdentifier: String?
    let host: TitlebarAccessoryHost

    func makeNSView(context: Context) -> NSView {
        let view = NSView(frame: .zero)
        host.recordRenderedControl(slot: slot, accessibilityIdentifier: accessibilityIdentifier)
        return view
    }

    func updateNSView(_ nsView: NSView, context: Context) {
        host.recordRenderedControl(slot: slot, accessibilityIdentifier: accessibilityIdentifier)
    }
}

struct WindowChromeConfigurator: NSViewRepresentable {
    final class Coordinator {
        let host: TitlebarAccessoryHost
        let token = TitlebarAccessoryOwnershipToken()

        init(host: TitlebarAccessoryHost) {
            self.host = host
        }
    }

    let configuration: @MainActor () -> (leading: AnyView, trailing: AnyView)
    let host: TitlebarAccessoryHost

    func makeCoordinator() -> Coordinator {
        Coordinator(host: host)
    }

    func makeNSView(context: Context) -> NSView {
        let view = NSView()
        let token = context.coordinator.token
        DispatchQueue.main.async {
            guard let window = view.window else { return }
            window.styleMask.insert(.fullSizeContentView)
            window.titlebarAppearsTransparent = true
            window.isOpaque = false
            window.backgroundColor = .clear
            window.installHideOnClose()
            host.install(on: window, token: token)
            let content = configuration()
            host.update(leading: content.leading, trailing: content.trailing)
        }
        return view
    }

    func updateNSView(_ nsView: NSView, context: Context) {
        guard let window = nsView.window else { return }
        host.install(on: window, token: context.coordinator.token)
        let content = configuration()
        host.update(leading: content.leading, trailing: content.trailing)
    }

    static func dismantleNSView(_ nsView: NSView, coordinator: Coordinator) {
        coordinator.host.removeFromOwnerWindow(token: coordinator.token)
    }
}

extension View {
    func configuresWindowChrome(
        configuration: @escaping @MainActor () -> (leading: AnyView, trailing: AnyView),
        host: TitlebarAccessoryHost
    ) -> some View {
        background(WindowChromeConfigurator(configuration: configuration, host: host))
    }
}
