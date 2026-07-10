import Foundation

/// Contenuto di una tab di workspace: un albero di split terminale oppure
/// un editor markdown puntato a un file su disco.
public enum TabContent: Equatable, Sendable {
    case terminal(SplitTree)
    case markdown(fileURL: URL)
}

/// Una tab dentro un worktree. L'identità della tab è stabile; title e
/// content sono mutabili (rinomina, split).
public struct WorkspaceTab: Identifiable, Equatable, Sendable {
    public let id: UUID
    public var title: String
    public var content: TabContent

    public init(id: UUID, title: String, content: TabContent) {
        self.id = id; self.title = title; self.content = content
    }

    /// Convenience per il caso terminale, il più comune nei call site.
    public init(id: UUID, title: String, tree: SplitTree) {
        self.init(id: id, title: title, content: .terminal(tree))
    }

    /// Leaf del tree terminale; una tab markdown non ha pane.
    public var leafIds: [UUID] {
        if case .terminal(let tree) = content { return tree.leafIds }
        return []
    }

    public var terminalTree: SplitTree? {
        if case .terminal(let tree) = content { return tree }
        return nil
    }

    public var markdownFileURL: URL? {
        if case .markdown(let url) = content { return url }
        return nil
    }

    /// Titolo default per una nuova shell manuale: "Terminale N", dove N
    /// conta le tab shell esistenti (prefisso "Terminale") + 1. Le tab
    /// agente (titolo = displayName) e markdown non incrementano il contatore.
    public static func nextShellTitle(existing: [WorkspaceTab]) -> String {
        let count = existing.filter { $0.title.hasPrefix("Terminale") }.count
        return "Terminale \(count + 1)"
    }
}
