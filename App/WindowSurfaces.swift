import SwiftUI
import AppKit
import TillerCore

/// The window canvas: one surface behind every floating card, reaching all four
/// window edges. With translucency on it is the only layer the desktop shows
/// through, so the desktop appears in the frame around the cards and not inside
/// the terminal.
struct CanvasBackground: View {
    /// Canvas translucency: < 1 lets the raw desktop show through the blur.
    /// Requires the non-opaque window set up in `WindowChromeConfigurator`.
    static let backgroundOpacity = AppSurfaceColor.translucentSurfaceOpacity
    static let tintOpacity = 0.30
    @AppStorage(AppSettings.translucencyEnabledKey) private var translucencyEnabled = false

    var body: some View {
        if translucencyEnabled {
            SidebarMaterialView()
                .overlay(AppTheme.canvas.opacity(Self.tintOpacity))
                .opacity(Self.backgroundOpacity)
        } else {
            AppTheme.canvas
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
