import SwiftUI
import TillerCore

/// Tab bar orizzontale sopra l'area contenuto: le tab del worktree
/// selezionato, sempre in sync con la sidebar (stessa source of truth:
/// AppModel.tabs / activeTabId). Sempre visibile quando un worktree è
/// selezionato; con zero tab mostra solo il "+".
struct TabBarView: View {
    @Bindable var model: AppModel
    let worktree: Worktree

    var body: some View {
        HStack(spacing: 4) {
            ScrollViewReader { proxy in
                ScrollView(.horizontal, showsIndicators: false) {
                    HStack(spacing: 2) {
                        ForEach(model.tabs[worktree.id] ?? []) { tab in
                            TabBarItem(model: model, worktree: worktree, tab: tab)
                                .id(tab.id)
                        }
                    }
                    .padding(.leading, 6)
                }
                .onChange(of: model.activeTabId[worktree.id]) { _, newValue in
                    guard let id = newValue else { return }
                    withAnimation(.easeInOut(duration: 0.15)) { proxy.scrollTo(id) }
                }
            }

            Spacer(minLength: 0)

            Menu {
                NewTabMenuItems(model: model, worktree: worktree)
            } label: {
                Image(systemName: "plus")
                    .font(.system(size: 11))
                    .foregroundStyle(AppTheme.meta)
            }
            .buttonStyle(.plain)
            .menuIndicator(.hidden)
            .help("Nuova tab (⌘T)")
            .padding(.trailing, 8)
        }
        .frame(height: 32)
    }
}

/// Una tab nella bar: icona tipo, titolo, dirty dot markdown, indicatore
/// attività agente, × on-hover. Click = attiva.
struct TabBarItem: View {
    @Bindable var model: AppModel
    let worktree: Worktree
    let tab: WorkspaceTab
    @State private var hovering = false

    private var isActive: Bool {
        model.activeTab(for: worktree.id)?.id == tab.id
    }

    private var status: AgentStatus? {
        model.agentActivity.statusForWorktree(paneIds: tab.activityPaneIds)
    }

    private var agentId: String? {
        tab.chatAgentId
            ?? tab.leafIds.compactMap { model.agentActivity.paneAgents[$0] }.first
    }

    var body: some View {
        HStack(spacing: 6) {
            WorkspaceTabIcon(model: model, tab: tab)
                .frame(width: 14)

            Text(tab.title)
                .font(.system(size: 12))
                .foregroundStyle(isActive ? AppTheme.titleSelected : AppTheme.subtitle)
                .lineLimit(1)
                .truncationMode(.tail)

            if model.markdownDocuments[tab.id]?.isDirty == true {
                Circle().fill(.secondary).frame(width: 5, height: 5)
            }

            if status != nil {
                WorktreeStatusGlyph(status: status, agentId: agentId)
                    .scaleEffect(0.8)
                    .frame(width: 14, height: 14)
            }

            if hovering {
                Button {
                    model.closeTab(tab.id, in: worktree)
                } label: {
                    Image(systemName: "xmark")
                        .font(.system(size: 9, weight: .bold))
                        .foregroundStyle(AppTheme.meta)
                }
                .buttonStyle(HoverIconButtonStyle())
                .help("Chiudi tab (⌘W)")
            } else {
                // Placeholder della × per evitare che la tab cambi larghezza
                // in hover (jitter durante il mouse-over).
                Color.clear.frame(width: 16, height: 16)
            }
        }
        .padding(.horizontal, 9)
        .padding(.vertical, 5)
        .contentShape(Rectangle())
        .background {
            if isActive {
                RoundedRectangle(cornerRadius: 7)
                    .fill(AppTheme.selectionFill)
                    .overlay(
                        RoundedRectangle(cornerRadius: 7)
                            .stroke(AppTheme.selectionRing, lineWidth: 1)
                    )
            } else if hovering {
                RoundedRectangle(cornerRadius: 7).fill(AppTheme.rowHover)
            }
        }
        .onHover { hovering = $0 }
        .onTapGesture { model.activateTab(tab.id, in: worktree.id) }
    }
}
