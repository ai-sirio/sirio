import SwiftUI
import AppKit

/// Makes the titlebar transparent and lets split-view backgrounds extend under
/// it. The window itself stays on `AppTheme.background`; the sidebar supplies
/// its own full-height navy-tinted material through its native column.
private struct WindowChromeConfigurator: NSViewRepresentable {
    func makeNSView(context: Context) -> NSView {
        let view = NSView()
        DispatchQueue.main.async {
            guard let window = view.window else { return }
            window.styleMask.insert(.fullSizeContentView)
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
