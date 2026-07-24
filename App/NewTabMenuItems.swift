import SwiftUI
import TillerCore
import TillerAgents

/// Contenuto del menu "+" (nuova shell, spawn agenti terminale, chat ACP),
/// condiviso tra il "+" della sidebar e quello della tab bar. Il chiamante
/// fornisce il Menu wrapper.
struct NewTabMenuItems: View {
    @Bindable var model: AppModel
    let worktree: Worktree

    var body: some View {
        Button {
            model.newShellTab(in: worktree)
        } label: {
            Label("New Terminal", systemImage: "terminal")
        }
        Divider()
        ForEach(AgentCatalog.all, id: \.id) { adapter in
            Button {
                Task { await model.spawnAgent(adapter, in: worktree) }
            } label: {
                if let icon = AgentMenuIconCache.image(for: adapter.id) {
                    Label {
                        Text(adapter.displayName)
                    } icon: {
                        Image(nsImage: icon)
                    }
                } else {
                    Text(adapter.displayName)
                }
            }
        }
        Divider()
        NewChatMenuItems(model: model, worktree: worktree)
    }
}

/// Shared "New Chat" submenu listing installed ACP agents, used by the "+"
/// menu and the sidebar worktree context menu. Empty install list falls back
/// to a single item that opens the Agents settings.
struct NewChatMenuItems: View {
    @Bindable var model: AppModel
    let worktree: Worktree

    var body: some View {
        Menu("New Chat") {
            ForEach(model.agentCenter.installedAgents) { agent in
                Button {
                    model.openChatTab(agentId: agent.id, in: worktree)
                } label: {
                    if let icon = AgentMenuIconCache.image(for: agent.id) {
                        Label {
                            Text(agent.name)
                        } icon: {
                            Image(nsImage: icon)
                        }
                    } else {
                        Text(agent.name)
                    }
                }
            }
            if model.agentCenter.installedAgents.isEmpty {
                Button("Other agents…") { model.openAgentsSettings() }
            }
        }
    }
}
