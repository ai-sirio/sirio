import Foundation
import TillerACP
import TillerAgents
import TillerCore

/// Assembles the Agents panel tree for a worktree from live AppModel state.
@MainActor
enum AgentsPanelModel {
    static func nodes(appModel: AppModel, worktree: Worktree) -> [AgentNode] {
        let tabs = appModel.tabs[worktree.id] ?? []
        var chatSubagents: [UUID: [ChatSubagentInput]] = [:]
        for tab in tabs where tab.chatAgentId != nil {
            guard let controller = appModel.chatControllers[tab.id] else { continue }
            chatSubagents[tab.id] = controller.presentationSnapshot.activeSubagentTasks.map {
                ChatSubagentInput(id: $0.toolCallId, title: $0.title,
                                  status: agentStatus(for: $0.status))
            }
        }
        return AgentTreeBuilder.build(
            tabs: tabs,
            agentStatus: appModel.agentActivity.agentStatus,
            paneAgents: appModel.agentActivity.paneAgents,
            chatSubagents: chatSubagents,
            processTrees: appModel.paneProcessTrees,
            catalogIds: AgentCatalog.all.map(\.id),
            displayNames: Dictionary(uniqueKeysWithValues:
                AgentCatalog.all.map { ($0.id, $0.displayName) }))
    }

    static func agentStatus(for status: ToolCallStatus) -> AgentStatus {
        switch status {
        case .pending, .inProgress: .running
        case .completed: .done
        case .failed: .error
        }
    }
}
