import Foundation

/// Una tab di terminale dentro un worktree: contiene il proprio albero di
/// split. L'identità della tab è stabile; title e tree sono mutabili
/// (rinomina, split).
public struct TerminalTab: Identifiable, Equatable, Sendable {
    public let id: UUID
    public var title: String
    public var tree: SplitTree

    public init(id: UUID, title: String, tree: SplitTree) {
        self.id = id; self.title = title; self.tree = tree
    }

    /// Titolo default per una nuova shell manuale: "Terminale N", dove N
    /// conta le tab shell esistenti (prefisso "Terminale") + 1. Le tab
    /// agente (titolo = displayName) non incrementano il contatore.
    public static func nextShellTitle(existing: [TerminalTab]) -> String {
        let count = existing.filter { $0.title.hasPrefix("Terminale") }.count
        return "Terminale \(count + 1)"
    }
}
