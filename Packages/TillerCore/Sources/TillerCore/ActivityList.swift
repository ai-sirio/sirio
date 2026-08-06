import Foundation

/// One row of the Activity panel: a terminal or chat tab belonging to an open
/// worktree.
public struct ActivityRow: Identifiable, Equatable, Sendable {
    public enum Kind: Equatable, Sendable {
        case terminal
        case chat
    }

    public let id: String
    public let worktreeId: UUID
    public let tabId: UUID
    public let kind: Kind
    public let title: String
    /// nil when no agent was ever identified in this tab — a bare shell.
    public let agentId: String?
    public let worktreeLabel: String
    public let status: ActivityStatus

    public init(worktreeId: UUID, tabId: UUID, kind: Kind, title: String,
                agentId: String?, worktreeLabel: String, status: ActivityStatus) {
        self.id = "\(worktreeId.uuidString):\(tabId.uuidString)"
        self.worktreeId = worktreeId
        self.tabId = tabId
        self.kind = kind
        self.title = title
        self.agentId = agentId
        self.worktreeLabel = worktreeLabel
        self.status = status
    }
}

/// Flattens the tabs of every open worktree into one list. Unlike the agent
/// tree it replaces, a tab whose agent was never identified is kept: the panel
/// answers "what do I have open?", not "which agents are running?".
public enum ActivityListBuilder {
    public struct WorktreeInput: Sendable {
        public let worktreeId: UUID
        public let label: String
        public let tabs: [WorkspaceTab]
        public let livePaneIds: [WorkspaceTabID: UUID]

        public init(worktreeId: UUID, label: String, tabs: [WorkspaceTab],
                    livePaneIds: [WorkspaceTabID: UUID]) {
            self.worktreeId = worktreeId
            self.label = label
            self.tabs = tabs
            self.livePaneIds = livePaneIds
        }
    }

    public static func build(
        worktrees: [WorktreeInput],
        agentStatus: [UUID: AgentStatus],
        paneAgents: [UUID: String]
    ) -> [ActivityRow] {
        worktrees.flatMap { worktree in
            worktree.tabs.compactMap { tab in
                row(tab: tab, worktree: worktree,
                    agentStatus: agentStatus, paneAgents: paneAgents)
            }
        }
    }

    private static func row(
        tab: WorkspaceTab,
        worktree: WorktreeInput,
        agentStatus: [UUID: AgentStatus],
        paneAgents: [UUID: String]
    ) -> ActivityRow? {
        let kind: ActivityRow.Kind
        // Chats register their activity under the tab id, terminals under the
        // live pane id. Everything downstream reads one key.
        let activityKey: UUID
        switch tab.content {
        case .chat:
            kind = .chat
            activityKey = tab.id.rawValue
        case .terminal:
            guard let paneId = worktree.livePaneIds[tab.id] else { return nil }
            kind = .terminal
            activityKey = paneId
        case .document:
            return nil
        }
        return ActivityRow(
            worktreeId: worktree.worktreeId,
            tabId: tab.id.rawValue,
            kind: kind,
            title: tab.title,
            agentId: paneAgents[activityKey],
            worktreeLabel: worktree.label,
            status: .from(agentStatus[activityKey]))
    }
}
