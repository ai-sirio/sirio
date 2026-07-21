import SwiftUI
import TillerACP
import TillerAgents
import TillerCore

/// A whole chat tab: header (agent identity + state + new conversation),
/// transcript, composer. State banners cover auth/disconnect/npx failures.
struct ChatPaneView: View {
    let controller: ChatController
    let worktree: Worktree
    let appModel: AppModel

    var body: some View {
        VStack(spacing: 0) {
            header
            Divider()
            switch controller.state {
            case .needsAuth:
                banner(
                    "Authentication required",
                    detail: "Log in from the CLI (e.g. `claude /login`) in a terminal, then restart the agent.",
                    actionTitle: "Retry") {
                    Task { await controller.start() }
                }
            case .disconnected(let message):
                banner(
                    "Agent disconnected",
                    detail: message ?? "The process has terminated.",
                    actionTitle: "Restart agent") {
                    Task { await controller.start() }
                }
            default:
                EmptyView()
            }
            if let promptError = controller.promptError {
                banner(
                    "Turn error",
                    detail: promptError,
                    actionTitle: "OK") {
                    controller.promptError = nil
                }
            }
            if let mcpWarning = controller.mcpWarning {
                banner(
                    "MCP configuration",
                    detail: mcpWarning,
                    actionTitle: "OK") {
                    controller.mcpWarning = nil
                }
            }
            TranscriptView(controller: controller, worktree: worktree,
                           appModel: appModel)
            Divider()
            ChatComposerView(controller: controller, worktreePath: worktree.path)
        }
        .task {
            controller.onFollowLocation = { [weak appModel] path in
                appModel?.requestChatFollow(path: path, worktreeId: worktree.id)
            }
            await controller.start()
        }
    }

    private var header: some View {
        HStack(spacing: 8) {
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
                HStack(spacing: 4) {
                    AgentIcon(agentId: controller.agentId, size: 14)
                    Image(systemName: "chevron.down")
                        .font(.system(size: 8, weight: .semibold))
                        .foregroundStyle(.secondary)
                }
            }
            .menuStyle(.borderlessButton)
            .fixedSize()
            .help("Switch agent for this conversation")
            Spacer()
            stateChip
            Button {
                controller.isFollowing.toggle()
            } label: {
                Label("Follow agent",
                      systemImage: controller.isFollowing ? "eye.fill" : "eye")
                    .font(.caption)
            }
            .buttonStyle(.borderless)
            .foregroundStyle(controller.isFollowing ? Color.accentColor : .secondary)
            .help("Opens the files the agent is editing in the right panel")
            Button {
                Task { await controller.newConversation() }
            } label: {
                Label("New conversation", systemImage: "plus.bubble")
                    .font(.caption)
            }
            .buttonStyle(.borderless)
        }
        .padding(.horizontal, 12).padding(.vertical, 6)
    }

    @ViewBuilder
    private var stateChip: some View {
        switch controller.state {
        case .connecting:
            Label("connecting…", systemImage: "circle.dotted")
                .font(.caption).foregroundStyle(.secondary)
        case .ready:
            Label("ready", systemImage: "circle.fill")
                .font(.caption).foregroundStyle(.green)
        case .prompting:
            Label("working", systemImage: "circle.fill")
                .font(.caption).foregroundStyle(.orange)
        case .needsAuth, .disconnected, .idle:
            Label("disconnected", systemImage: "circle")
                .font(.caption).foregroundStyle(.secondary)
        }
    }

    private func banner(_ title: String, detail: String,
                        actionTitle: String,
                        action: @escaping () -> Void) -> some View {
        HStack {
            VStack(alignment: .leading, spacing: 2) {
                Text(title).font(.callout.weight(.semibold))
                Text(detail).font(.caption).foregroundStyle(.secondary)
            }
            Spacer()
            Button(actionTitle, action: action).controlSize(.small)
        }
        .padding(10)
        .background(.yellow.opacity(0.12))
    }
}
