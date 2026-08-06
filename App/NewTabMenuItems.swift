import SwiftUI
import TillerCore
import TillerAgents
import Inject

/// Contenuto del menu "+" (nuova shell, spawn agenti terminale, chat ACP),
/// condiviso tra il "+" della sidebar e quello della tab bar. Il chiamante
/// fornisce il Menu wrapper.
struct NewTabMenuItems: View {
    @ObserveInjection private var inject

    @Bindable var model: AppModel
    let worktree: Worktree
    /// Run before every entry's action. The pane strip uses it to make its own
    /// group active first, since the AppModel routes below open into whichever
    /// group is active.
    var onBeforeAction: () -> Void = {}
    /// Overrides the plain-shell route. The pane strip supplies one that names
    /// its own group instead of the active one.
    var onNewTerminal: (() -> Void)?

    var body: some View {
        Button {
            onBeforeAction()
            if let onNewTerminal { onNewTerminal() } else { model.newShellTab(in: worktree) }
        } label: {
            Label("New Terminal", systemImage: "terminal")
        }
        Divider()
        ForEach(AgentCatalog.all, id: \.id) { adapter in
            Button {
                onBeforeAction()
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
        NewChatMenuItems(model: model, worktree: worktree, onBeforeAction: onBeforeAction)
    .enableInjection()
    }
}

/// Shared "New Chat" submenu listing installed ACP agents, used by the "+"
/// menu and the sidebar worktree context menu. Empty install list falls back
/// to a single item that opens the Agents settings.
struct NewChatMenuItems: View {
    @ObserveInjection private var inject

    @Bindable var model: AppModel
    let worktree: Worktree
    var onBeforeAction: () -> Void = {}

    var body: some View {
        Menu("New Chat") {
            ForEach(model.agentCenter.installedAgents) { agent in
                Button {
                    onBeforeAction()
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
    .enableInjection()
    }
}
