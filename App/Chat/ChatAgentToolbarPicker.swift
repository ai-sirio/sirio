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

/// Window-toolbar agent indicator for the active chat tab: agent icon with
/// fused status dot plus display name. Read-only — the agent is chosen at
/// chat creation and cannot change afterwards.
struct ChatAgentToolbarPicker: View {
    let controller: ChatController
    let appModel: AppModel

    var body: some View {
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
        }
        .fixedSize()
    }
}
