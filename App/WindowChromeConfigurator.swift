import SwiftUI
import AppKit

@MainActor
final class TitlebarAccessoryHost {
    private(set) var leadingController: NSTitlebarAccessoryViewController?
    private(set) var trailingController: NSTitlebarAccessoryViewController?
    private(set) var leadingHostingView: NSHostingView<AnyView>?
    private(set) var trailingHostingView: NSHostingView<AnyView>?
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
        leadingHostingView?.rootView = leading
        trailingHostingView?.rootView = trailing

        leadingHostingView?.setFrameSize(CGSize(
            width: TitlebarGeometry.controlFrame.width,
            height: TitlebarGeometry.accessoryHeight))
        trailingHostingView?.setFrameSize(CGSize(
            width: 3 * TitlebarGeometry.controlFrame.width + 2 * TitlebarGeometry.controlSpacing,
            height: TitlebarGeometry.accessoryHeight))
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

@MainActor
private final class WindowChromeView: NSView {
    var onWindowChanged: ((NSWindow?) -> Void)?

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        onWindowChanged?(window)
    }
}

/// Makes the titlebar transparent and lets the canvas extend under it. The
/// window is non-opaque so the canvas material at reduced opacity
/// (`CanvasBackground.backgroundOpacity`) lets the desktop show through.
private struct WindowChromeConfigurator<LeadingAccessory: View, TrailingAccessory: View>: NSViewRepresentable {
    private let leadingAccessory: LeadingAccessory
    private let trailingAccessory: TrailingAccessory

    init(@ViewBuilder leadingAccessory: () -> LeadingAccessory,
         @ViewBuilder trailingAccessory: () -> TrailingAccessory) {
        self.leadingAccessory = leadingAccessory()
        self.trailingAccessory = trailingAccessory()
    }

    @MainActor
    final class Coordinator {
        let host = TitlebarAccessoryHost()
        var leading: AnyView?
        var trailing: AnyView?
        weak var window: NSWindow?

        func updateWindow(_ window: NSWindow?) {
            guard let previousWindow = self.window, previousWindow !== window else {
                self.window = window
                guard let window else { return }
                install(on: window)
                return
            }
            host.remove(from: previousWindow)
            self.window = window
            guard let window else { return }
            install(on: window)
        }

        func install(on window: NSWindow) {
            window.styleMask.insert(.fullSizeContentView)
            window.titlebarAppearsTransparent = true
            window.isOpaque = false
            window.backgroundColor = .clear
            window.installHideOnClose()
            host.install(on: window)
            guard let leading, let trailing else { return }
            host.update(leading: leading, trailing: trailing)
        }

        func remove() {
            guard let window else { return }
            host.remove(from: window)
            self.window = nil
        }
    }

    func makeCoordinator() -> Coordinator {
        Coordinator()
    }

    func makeNSView(context: Context) -> NSView {
        let view = WindowChromeView()
        view.onWindowChanged = { [weak coordinator = context.coordinator] window in
            coordinator?.updateWindow(window)
        }
        return view
    }

    func updateNSView(_ nsView: NSView, context: Context) {
        context.coordinator.leading = AnyView(leadingAccessory)
        context.coordinator.trailing = AnyView(trailingAccessory)
        guard let window = nsView.window else { return }
        context.coordinator.updateWindow(window)
    }
}

extension View {
    func configuresWindowChrome() -> some View {
        background(WindowChromeConfigurator(leadingAccessory: {}, trailingAccessory: {}))
    }
}
