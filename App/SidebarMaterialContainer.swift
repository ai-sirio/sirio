import SwiftUI
import AppKit

/// One native dark-glass surface, corrected to Tiller's navy palette.
struct SidebarMaterialContainer: View {
    var body: some View {
        SidebarMaterialView()
            .overlay(AppTheme.background.opacity(0.86))
    }
}

private struct SidebarMaterialView: NSViewRepresentable {
    func makeNSView(context: Context) -> NSVisualEffectView {
        let view = NSVisualEffectView()
        view.material = .sidebar
        view.blendingMode = .withinWindow
        view.state = .followsWindowActiveState
        view.appearance = NSAppearance(named: .darkAqua)
        return view
    }

    func updateNSView(_: NSVisualEffectView, context: Context) {}
}
