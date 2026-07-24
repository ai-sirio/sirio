import Foundation

/// One process in a pane's shell subtree (captured by the App-layer libproc
/// walk; assembled here so the tree logic stays testable in TillerCore).
public struct ProcessNode: Equatable, Sendable {
    public let pid: Int32
    public let name: String
    public let children: [ProcessNode]

    public init(pid: Int32, name: String, children: [ProcessNode]) {
        self.pid = pid
        self.name = name
        self.children = children
    }
}

/// A subagent task extracted from a chat transcript (Task-type tool call).
public struct ChatSubagentInput: Equatable, Sendable, Identifiable {
    public let id: String       // toolCallId
    public let title: String
    public let status: AgentStatus

    public init(id: String, title: String, status: AgentStatus) {
        self.id = id
        self.title = title
        self.status = status
    }
}

/// A node of the Agents panel tree: a top-level agent (chat tab or terminal
/// pane) or a subagent spawned by it.
public struct AgentNode: Identifiable, Equatable, Sendable {
    public enum Kind: Equatable, Sendable {
        case chat(tabId: UUID)
        case terminal(paneId: UUID)
        case subagent
    }

    public let id: String
    public let agentId: String
    public let title: String
    public let kind: Kind
    public let status: AgentStatus
    public let children: [AgentNode]

    public init(id: String, agentId: String, title: String, kind: Kind,
                status: AgentStatus, children: [AgentNode] = []) {
        self.id = id
        self.agentId = agentId
        self.title = title
        self.kind = kind
        self.status = status
        self.children = children
    }
}

/// Pure assembly of the Agents panel tree from snapshots of existing state.
/// Chat tabs first, then terminal panes, both in `tabs` order.
public enum AgentTreeBuilder {
    public static func build(
        tabs: [WorkspaceTab],
        agentStatus: [UUID: AgentStatus],
        paneAgents: [UUID: String],
        chatSubagents: [UUID: [ChatSubagentInput]],
        processTrees: [UUID: [ProcessNode]],
        catalogIds: [String],
        displayNames: [String: String]
    ) -> [AgentNode] {
        var chatNodes: [AgentNode] = []
        var terminalNodes: [AgentNode] = []

        for tab in tabs {
            switch tab.content {
            case .chat:
                guard let status = agentStatus[tab.id],
                      let agentId = paneAgents[tab.id] else { continue }
                let children = (chatSubagents[tab.id] ?? []).map { sub in
                    AgentNode(
                        id: "chat:\(tab.id.uuidString):tool:\(sub.id)",
                        agentId: agentId,
                        title: sub.title,
                        kind: .subagent,
                        status: sub.status)
                }
                // Il titolo segue la tab (l'auto-rename lo aggiorna); l'id
                // agente delle chat è canonico ("claude-acp") e non risolve
                // in displayNames, che restano per i nodi processo.
                chatNodes.append(AgentNode(
                    id: "chat:\(tab.id.uuidString)",
                    agentId: agentId,
                    title: tab.title,
                    kind: .chat(tabId: tab.id),
                    status: status,
                    children: children))
            case .terminal:
                for paneId in tab.leafIds {
                    guard let status = agentStatus[paneId],
                          let agentId = paneAgents[paneId] else { continue }
                    let children = subagentNodes(
                        paneId: paneId,
                        forest: processTrees[paneId] ?? [],
                        catalogIds: catalogIds,
                        displayNames: displayNames,
                        paneStatus: status)
                    terminalNodes.append(AgentNode(
                        id: "term:\(paneId.uuidString)",
                        agentId: agentId,
                        title: displayNames[agentId] ?? agentId,
                        kind: .terminal(paneId: paneId),
                        status: status,
                        children: children))
                }
            case .markdown:
                continue
            }
        }
        return chatNodes + terminalNodes
    }

    /// Descendants of the first catalog-matched process that themselves match
    /// the catalog become subagent nodes. Intermediate non-catalog processes
    /// (shim shells, ripgrep, node) are skipped but their subtrees are still
    /// searched.
    private static func subagentNodes(
        paneId: UUID,
        forest: [ProcessNode],
        catalogIds: [String],
        displayNames: [String: String],
        paneStatus: AgentStatus
    ) -> [AgentNode] {
        let ids = Set(catalogIds)
        guard let root = firstMatch(in: forest, ids: ids) else { return [] }
        return collectMatches(in: root.children, ids: ids).map { proc in
            AgentNode(
                id: "term:\(paneId.uuidString):pid:\(proc.pid)",
                agentId: proc.name,
                title: displayNames[proc.name] ?? proc.name,
                kind: .subagent,
                status: paneStatus,
                children: [])
        }
    }

    private static func firstMatch(in forest: [ProcessNode], ids: Set<String>) -> ProcessNode? {
        for node in forest {
            if ids.contains(node.name) { return node }
            if let found = firstMatch(in: node.children, ids: ids) { return found }
        }
        return nil
    }

    private static func collectMatches(in forest: [ProcessNode], ids: Set<String>) -> [ProcessNode] {
        var result: [ProcessNode] = []
        for node in forest {
            if ids.contains(node.name) {
                result.append(node)
            }
            result.append(contentsOf: collectMatches(in: node.children, ids: ids))
        }
        return result
    }
}
