import AppKit
import SwiftUI

/// Rasterized AgentIcon images for native menu items. macOS menus render
/// only Text/Image labels, so the custom vector logos (OpenCode, Pi, omp)
/// must become NSImages to appear at all. Rendered once per agent, cached.
@MainActor
enum AgentMenuIconCache {
    private static var cache: [String: NSImage] = [:]

    /// Returns the cached menu icon for the agent, rendering it on first
    /// use. Nil when rasterization fails — callers fall back to a
    /// text-only label.
    static func image(for agentId: String) -> NSImage? {
        if let cached = cache[agentId] { return cached }
        let renderer = ImageRenderer(content: AgentIcon(agentId: agentId, size: 14))
        renderer.scale = 2
        guard let image = renderer.nsImage else { return nil }
        image.size = NSSize(width: 14, height: 14)
        cache[agentId] = image
        return image
    }
}
