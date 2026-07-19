import SwiftUI
import AppKit

/// One native glass surface for the chrome (sidebar, titlebar, usage bar).
/// Blends behind the window so the desktop shows through, like native sidebars.
struct SidebarMaterialContainer: View {
    /// Chrome translucency: < 1 lets the raw desktop show through the blur.
    /// Requires the non-opaque window set up in `WindowChromeConfigurator`.
    static let backgroundOpacity = 0.96
    /// Indigo tint for the sidebar/tab bar/usage bar material
    static let tintOpacity = 0.30

    var body: some View {
        SidebarMaterialView()
            .overlay(AppTheme.chromeTint.opacity(Self.tintOpacity))
            .opacity(Self.backgroundOpacity)
    }
}

private struct SidebarMaterialView: NSViewRepresentable {
    func makeNSView(context: Context) -> NSVisualEffectView {
        let view = NSVisualEffectView()
        view.material = .sidebar
        view.blendingMode = .behindWindow
        view.state = .followsWindowActiveState
        return view
    }

    func updateNSView(_: NSVisualEffectView, context: Context) {}
}

#Preview {
    SidebarMaterialContainer()
        .frame(width: 240, height: 400)
}
