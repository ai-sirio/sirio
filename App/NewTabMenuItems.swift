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
            Label("Nuovo Terminale", systemImage: "terminal")
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
        Button("New Chat") {
            model.openChatTab(in: worktree)
        }
    }
}
