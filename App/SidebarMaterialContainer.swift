import SwiftUI
import AppKit

/// One native dark-glass surface, lightly tinted toward Tiller's navy palette.
/// Blends behind the window so the desktop shows through, like native sidebars.
struct SidebarMaterialContainer: View {
    var body: some View {
        SidebarMaterialView()
            .overlay(AppTheme.chromeTint.opacity(0.25))
    }
}

private struct SidebarMaterialView: NSViewRepresentable {
    func makeNSView(context: Context) -> NSVisualEffectView {
        let view = NSVisualEffectView()
        view.material = .sidebar
        view.blendingMode = .behindWindow
        view.state = .followsWindowActiveState
        view.appearance = NSAppearance(named: .darkAqua)
        return view
    }

    func updateNSView(_: NSVisualEffectView, context: Context) {}
}
