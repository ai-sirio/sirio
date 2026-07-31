import Foundation
import TillerACP
import TillerAgents
import TillerCore

/// Assembles the Agents panel tree for a worktree from live AppModel state.
@MainActor
enum AgentsPanelModel {
    static func nodes(appModel: AppModel, worktree: Worktree) -> [AgentNode] {
        guard let layout = appModel.workspaceCoordinator.layouts[worktree.id] else { return [] }
        let tabs = layout.allTabs
        var livePaneIds: [WorkspaceTabID: UUID] = [:]
        var chatSubagents: [UUID: [ChatSubagentInput]] = [:]
        for tab in tabs {
            switch tab.content {
            case .terminal(let contentID):
                if let paneId = appModel.workspaceCoordinator.liveControlPaneId(
                    contentID: contentID, in: worktree.id) {
                    livePaneIds[tab.id] = paneId
                }
            case .chat:
                let tabId = tab.id.rawValue
                guard let controller = appModel.chatControllers[tabId] else { continue }
                chatSubagents[tabId] = controller.presentationSnapshot.activeSubagentTasks.map {
                    ChatSubagentInput(id: $0.toolCallId, title: $0.title,
                                      status: agentStatus(for: $0.status))
                }
            case .document:
                continue
            }
        }
        return AgentTreeBuilder.build(
            tabs: tabs,
            livePaneIds: livePaneIds,
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
