import AppKit
import GhosttyTerminal

/// Tiller hides inactive tabs/worktrees with `.opacity(0)`, which Core
/// Animation applies at composite time — libghostty never sees it, so every
/// hidden pane keeps its per-surface display link ticking at full refresh
/// rate. This forwards the SwiftUI-level visibility down to ghostty's
/// occlusion API (`ghostty_surface_set_occlusion` + display-link stop),
/// the same mechanism Ghostty.app uses for occluded windows.
///
/// Seam for tests: `AppTerminalView` needs a live ghostty surface to
/// instantiate, so tests conform a plain NSView instead.
@MainActor
protocol SurfaceOcclusionApplying {
    func setSurfaceVisible(_ visible: Bool)
}

extension AppTerminalView: SurfaceOcclusionApplying {}

@MainActor
enum SurfaceVisibility {
    /// Applies `visible` to every terminal surface in `root`'s subtree.
    /// Idempotent and cheap when nothing changed: the coordinator behind
    /// `setSurfaceVisible` early-returns on an unchanged value.
    static func apply(_ visible: Bool, in root: NSView) {
        for subview in root.subviews {
            if let surface = subview as? SurfaceOcclusionApplying {
                surface.setSurfaceVisible(visible)
            }
            apply(visible, in: subview)
        }
    }
}
