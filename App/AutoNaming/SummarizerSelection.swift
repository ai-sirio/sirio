import TillerACP
import TillerAgents
import TillerCore

/// Ordered summarize candidates for one auto-naming pass: the user-selected
/// agent first, then the tab's own agent as runtime fallback. Pure — no
/// defaults access, no process spawning — so the ordering rules stay
/// testable without touching AutoNamer.
enum SummarizerSelection {
    static func adapters(
        selectedId: String, tabAgentId: String
    ) -> [any AgentAdapter] {
        let catalog = AgentCatalog.all
        let primary = catalog.first { $0.id == selectedId }
            ?? catalog.first { $0.id == AppSettings.defaultSummarizerAgentId }
        let fallback = catalog.first {
            $0.id == AgentIdMigration.catalogId(tabAgentId)
        }
        var result: [any AgentAdapter] = []
        if let primary { result.append(primary) }
        if let fallback, fallback.id != primary?.id { result.append(fallback) }
        return result
    }
}
