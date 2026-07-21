import AppKit
import SwiftUI

/// Rasterized AgentIcon images for native menu items. macOS menus render
/// only Text/Image labels, so the custom vector logos (OpenCode, Pi, omp)
/// must become NSImages to appear at all. Rendered per agent and appearance
/// (Color.primary must resolve white in dark menus, black in light ones),
/// then cached.
@MainActor
enum AgentMenuIconCache {
    private static var cache: [String: NSImage] = [:]

    /// Returns the cached menu icon for the agent, rendering it on first
    /// use. Nil when rasterization fails — callers fall back to a
    /// text-only label.
    static func image(for agentId: String) -> NSImage? {
        let isDark = NSApp.effectiveAppearance
            .bestMatch(from: [.darkAqua, .aqua]) == .darkAqua
        let key = "\(agentId)#\(isDark ? "dark" : "light")"
        if let cached = cache[key] { return cached }
        let renderer = ImageRenderer(
            content: AgentIcon(agentId: agentId, size: 14)
                .environment(\.colorScheme, isDark ? .dark : .light))
        renderer.scale = 2
        guard let image = renderer.nsImage else {
            NSLog("[AgentMenuIconCache] rasterization failed for %@", agentId)
            return nil
        }
        image.size = NSSize(width: 14, height: 14)
        cache[key] = image
        return image
    }
}
