import Foundation

/// Contenuto di una tab di workspace: un albero di split terminale oppure
/// un editor markdown puntato a un file su disco.
public enum TabContent: Equatable, Sendable {
    case terminal(SplitTree)
    case markdown(fileURL: URL)
    case chat(agentId: String)
}

/// Una tab dentro un worktree. L'identità della tab è stabile; title e
/// content sono mutabili (rinomina, split).
public struct WorkspaceTab: Identifiable, Equatable, Sendable {
    public let id: UUID
    public var title: String
    public var content: TabContent
    /// True quando `title` è ancora eleggibile per l'auto-naming: placeholder
    /// di creazione o ultimo titolo scritto dall'auto-naming stesso. Un
    /// rename manuale (`AppModel.renameTab`) lo flippa a false per sempre.
    public var titleIsAutoNamed: Bool

    public init(id: UUID, title: String, content: TabContent, titleIsAutoNamed: Bool = true) {
        self.id = id; self.title = title; self.content = content
        self.titleIsAutoNamed = titleIsAutoNamed
    }

    /// Convenience per il caso terminale, il più comune nei call site.
    public init(id: UUID, title: String, tree: SplitTree, titleIsAutoNamed: Bool = true) {
        self.init(id: id, title: title, content: .terminal(tree), titleIsAutoNamed: titleIsAutoNamed)
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

    /// Agent id when this is a chat tab.
    public var chatAgentId: String? {
        if case .chat(let agentId) = content { return agentId }
        return nil
    }

    /// Pane ids that participate in agent-activity aggregation: terminal
    /// leaves for terminal tabs, the tab itself for chat tabs.
    public var activityPaneIds: [UUID] {
        switch content {
        case .terminal: leafIds
        case .markdown: []
        case .chat: [id]
        }
    }

    /// Titolo default per una nuova shell manuale: "Terminale N", dove N
    /// conta le tab shell esistenti (prefisso "Terminale") + 1. Le tab
    /// agente (titolo = displayName) e markdown non incrementano il contatore.
    public static func nextShellTitle(existing: [WorkspaceTab]) -> String {
        let count = existing.filter { $0.title.hasPrefix("Terminale") }.count
        return "Terminale \(count + 1)"
    }
}
