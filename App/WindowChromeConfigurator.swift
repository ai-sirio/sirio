import SwiftUI
import AppKit

/// Makes the titlebar transparent and lets the canvas extend under it. The
/// window is non-opaque so the canvas material at reduced opacity
/// (`CanvasBackground.backgroundOpacity`) lets the desktop show through.
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
