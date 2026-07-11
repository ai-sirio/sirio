import SwiftUI
import AppKit

/// Makes the titlebar transparent and lets split-view backgrounds extend under
/// it. The window is non-opaque so the chrome material at reduced opacity
/// (`SidebarMaterialContainer.backgroundOpacity`) lets the desktop show
/// through; the terminal pane stays covered by the opaque `AppTheme.background`.
private struct WindowChromeConfigurator: NSViewRepresentable {
    func makeNSView(context: Context) -> NSView {
        let view = NSView()
        DispatchQueue.main.async {
            guard let window = view.window else { return }
            window.styleMask.insert(.fullSizeContentView)
            window.titlebarAppearsTransparent = true
            window.isOpaque = false
            window.backgroundColor = .clear
            window.installHideOnClose()
        }
        return view
    }

    func updateNSView(_ nsView: NSView, context: Context) {}
}

extension View {
    func configuresWindowChrome() -> some View {
        background(WindowChromeConfigurator())
    }
}
