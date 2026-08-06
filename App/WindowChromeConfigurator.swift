import SwiftUI
import AppKit

enum TitlebarControlSlot: Hashable {
    case sidebar
    case rightPanel
    case split
    case permissions
}

struct TitlebarControlProbeRecord: Equatable {
    let slot: TitlebarControlSlot
    let accessibilityIdentifier: String?
}

@MainActor
final class TitlebarAccessoryHost {
    private(set) var leadingController: NSTitlebarAccessoryViewController?
    private(set) var trailingController: NSTitlebarAccessoryViewController?
    private(set) var leadingHostingView: NSHostingView<AnyView>?
    private(set) var trailingHostingView: NSHostingView<AnyView>?
    private(set) var renderedControlProbes: [TitlebarControlProbeRecord] = []
    private(set) var updateCount = 0
    private weak var ownerWindow: NSWindow?

    func install(on window: NSWindow) {
        guard leadingController == nil, trailingController == nil else { return }
        ownerWindow = window

        let leading = NSTitlebarAccessoryViewController()
        leading.layoutAttribute = .left
        let trailing = NSTitlebarAccessoryViewController()
        trailing.layoutAttribute = .right
        let leadingHostingView = NSHostingView(rootView: AnyView(EmptyView()))
        let trailingHostingView = NSHostingView(rootView: AnyView(EmptyView()))
        leadingHostingView.frame = CGRect(
            x: 0,
            y: 0,
            width: TitlebarGeometry.controlFrame.width,
            height: TitlebarGeometry.accessoryHeight)
        trailingHostingView.frame = CGRect(
            x: 0,
            y: 0,
            width: 3 * TitlebarGeometry.controlFrame.width + 2 * TitlebarGeometry.controlSpacing,
            height: TitlebarGeometry.accessoryHeight)

        leading.view = leadingHostingView
        trailing.view = trailingHostingView
        leading.view.frame = leadingHostingView.frame
        trailing.view.frame = trailingHostingView.frame
        window.addTitlebarAccessoryViewController(leading)
        window.addTitlebarAccessoryViewController(trailing)

        leadingController = leading
        trailingController = trailing
        self.leadingHostingView = leadingHostingView
        self.trailingHostingView = trailingHostingView
    }

    func update(leading: AnyView, trailing: AnyView) {
        updateCount += 1
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
        renderedControlProbes.append(
            TitlebarControlProbeRecord(
                slot: slot,
                accessibilityIdentifier: accessibilityIdentifier))
    }

    func remove(from window: NSWindow) {
        guard ownerWindow === window else { return }

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
        ownerWindow = nil
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

private struct WindowChromeConfigurator: NSViewRepresentable {
    let configuration: @MainActor () -> (leading: AnyView, trailing: AnyView)
    let host: TitlebarAccessoryHost

    func makeNSView(context: Context) -> NSView {
        let view = NSView()
        DispatchQueue.main.async {
            guard let window = view.window else { return }
            window.styleMask.insert(.fullSizeContentView)
            window.titlebarAppearsTransparent = true
            window.isOpaque = false
            window.backgroundColor = .clear
            window.installHideOnClose()
            host.install(on: window)
            let content = configuration()
            host.update(leading: content.leading, trailing: content.trailing)
        }
        return view
    }

    func updateNSView(_ nsView: NSView, context: Context) {
        guard let window = nsView.window else { return }
        host.install(on: window)
        let content = configuration()
        host.update(leading: content.leading, trailing: content.trailing)
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
