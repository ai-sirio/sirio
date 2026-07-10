import SwiftUI
import TillerAgents
import TillerCore

/// Tab strip stile Orca per i terminali del worktree selezionato.
/// Doppio click sul titolo → rinomina inline; × chiude; + apre un menu (terminale o agente).
struct TabBarView: View {
    var model: AppModel
    let worktree: Worktree

    @State private var renamingTabId: UUID?
    @State private var draftTitle = ""
    @FocusState private var renameFieldFocused: Bool

    var body: some View {
        TillerGlassContainer(spacing: 8) {
            HStack(spacing: 2) {
                ForEach(model.tabs[worktree.id] ?? []) { tab in
                    tabItem(tab)
                }
                Menu {
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
                } label: {
                    Image(systemName: "plus")
                        .padding(.horizontal, 6)
                        .padding(.vertical, 4)
                }
                .buttonStyle(.plain)
                .menuIndicator(.hidden)
                .tillerGlass(in: .capsule)
                .help("Nuova tab (⌘T)")
                Spacer(minLength: 0)
            }
            .padding(.horizontal, 6)
            .padding(.vertical, 4)
        }
        .background(AppTheme.background)
    }

    @ViewBuilder
    private func tabItem(_ tab: WorkspaceTab) -> some View {
        let isActive = model.activeTab(for: worktree.id)?.id == tab.id
        HStack(spacing: 5) {
            if let agentId = tab.leafIds.compactMap({ model.agentActivity.paneAgents[$0] }).first {
                AgentIcon(agentId: agentId, size: 12)
            }
            if renamingTabId == tab.id {
                TextField("", text: $draftTitle)
                    .textFieldStyle(.plain)
                    .frame(width: 120)
                    .focused($renameFieldFocused)
                    .onSubmit {
                        model.renameTab(tab.id, in: worktree.id, to: draftTitle)
                        renamingTabId = nil
                    }
                    .onExitCommand { renamingTabId = nil }
            } else {
                Text(tab.title)
                    .lineLimit(1)
                    .truncationMode(.tail)
                    .frame(maxWidth: 160)
                    .fixedSize(horizontal: true, vertical: false)
            }
            Button {
                model.closeTab(tab.id, in: worktree)
            } label: {
                Image(systemName: "xmark")
                    .font(.system(size: 8, weight: .bold))
            }
            .buttonStyle(.plain)
            .opacity(isActive ? 0.9 : 0.65)
            .help("Chiudi tab (⌘W)")
        }
        .padding(.horizontal, 10)
        .padding(.vertical, 4)
        .tillerTabBackground(isActive: isActive)
        .contentShape(Rectangle())
        .onTapGesture(count: 2) {
            draftTitle = tab.title
            renamingTabId = tab.id
            renameFieldFocused = true
        }
        .onTapGesture { model.activateTab(tab.id, in: worktree.id) }
    }
}
