import SwiftUI
import AppKit

/// Makes the native titlebar transparent and paints the window background
/// with `AppTheme.background`, so the titlebar strip (traffic lights + app
/// title) matches the sidebar/detail pane instead of the default system
/// titlebar material.
private struct WindowChromeConfigurator: NSViewRepresentable {
    func makeNSView(context: Context) -> NSView {
        let view = NSView()
        DispatchQueue.main.async {
            guard let window = view.window else { return }
            window.titlebarAppearsTransparent = true
            window.backgroundColor = NSColor(AppTheme.background)
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
