import SwiftUI
import AppKit
import TillerCore

/// One native glass surface for the chrome (sidebar, titlebar, usage bar).
/// Blends behind the window so the desktop shows through, like native sidebars.
struct SidebarMaterialContainer: View {
    /// Chrome translucency: < 1 lets the raw desktop show through the blur.
    /// Requires the non-opaque window set up in `WindowChromeConfigurator`.
    static let backgroundOpacity = AppSurfaceColor.translucentSurfaceOpacity
    /// Indigo tint for the sidebar/tab bar/usage bar material
    static let tintOpacity = 0.30
    @AppStorage(AppSettings.translucencyEnabledKey) private var translucencyEnabled = false

    var body: some View {
        if translucencyEnabled {
            SidebarMaterialView()
                .overlay(AppTheme.chromeTint.opacity(Self.tintOpacity))
                .opacity(Self.backgroundOpacity)
        } else {
            AppTheme.background
        }
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

/// Translucent surface for the terminal/chat main pane: the same
/// behind-window blur as the sidebar, tinted with `tint` at the shared
/// opacity so it visually matches the terminal's ghostty
/// `background-opacity`. Defaults to the terminal's charcoal surface;
/// `ChatPaneView` passes `AppTheme.chatSurface` instead.
struct MainSurfaceMaterial: View {
    var tint: Color = AppTheme.terminalSurface
    @AppStorage(AppSettings.translucencyEnabledKey) private var translucencyEnabled = false

    var body: some View {
        if translucencyEnabled {
            SidebarMaterialView()
                .overlay(tint.opacity(AppSurfaceColor.translucentSurfaceOpacity))
        } else {
            tint
        }
    }
}
