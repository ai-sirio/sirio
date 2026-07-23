import SwiftUI

typealias ChatState = ChatController.ChatState

/// Maps the chat controller state to the status dot shown on the toolbar
/// agent picker (replaces the old ChatPaneView header state chip).
func agentStatusDotColor(for state: ChatState) -> Color {
    switch state {
    case .ready: .green
    case .prompting: .orange
    case .idle, .connecting, .needsAuth, .disconnected: .gray
    }
}

/// Window-toolbar replacement for the removed ChatPaneView header: agent
/// menu labeled with the agent icon (status dot fused onto it), name, and
/// chevron. Shown only while the active tab is a chat.
struct ChatAgentToolbarPicker: View {
    let controller: ChatController
    let appModel: AppModel

    var body: some View {
        Menu {
            ForEach(appModel.agentCenter.installedAgents) { agent in
                Button {
                    Task {
                        await controller.switchAgent(to: agent.id,
                                                     displayName: agent.name)
                        appModel.rememberChatAgent(agent.id)
                    }
                } label: {
                    if let image = AgentMenuIconCache.image(for: agent.id) {
                        Label { Text(agent.name) } icon: { Image(nsImage: image) }
                    } else {
                        Text(agent.name)
                    }
                }
                .disabled(agent.id == controller.agentId)
            }
            Divider()
            Button("Other agents…") { appModel.openAgentsSettings() }
        } label: {
            HStack(spacing: 5) {
                AgentIcon(agentId: controller.agentId, size: 14)
                    .overlay(alignment: .bottomTrailing) {
                        Circle()
                            .fill(agentStatusDotColor(for: controller.state))
                            .frame(width: 5, height: 5)
                            .offset(x: 1.5, y: 1.5)
                    }
                Text(appModel.agentCenter.displayName(for: controller.agentId))
                    .font(.caption)
                Image(systemName: "chevron.down")
                    .font(.system(size: 8, weight: .semibold))
                    .foregroundStyle(.secondary)
            }
        }
        .menuStyle(.borderlessButton)
        .fixedSize()
        .help("Switch agent for this conversation")
    }
}
