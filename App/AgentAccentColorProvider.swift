import SwiftUI
import TillerCore

/// Reads one agent's stored accent-color hex (`@AppStorage`, live-updating
/// when Settings changes it) and hands the resolved `Color` to `content`.
/// Isolates the dynamic-storage-key `@AppStorage` pattern — which needs a
/// custom `init` — away from `ChatComposerView`, which already has enough
/// property-wrapper state of its own.
struct AgentAccentColorProvider<Content: View>: View {
    let agentId: String
    @AppStorage private var hex: String
    let content: (Color) -> Content

    init(agentId: String, store: UserDefaults = .standard,
         @ViewBuilder content: @escaping (Color) -> Content) {
        self.agentId = agentId
        self.content = content
        _hex = AppStorage(
            wrappedValue: AgentAccentColor.defaultHex(for: agentId),
            AppSettings.agentColorKey(for: agentId),
            store: store)
    }

    var body: some View {
        content(AgentAccentColor.color(for: agentId, storedValue: hex))
    }
}
