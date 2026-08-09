import SwiftUI
import TillerCore
import Inject

/// Tab bar orizzontale sopra l'area contenuto: le tab del worktree
/// selezionato, sempre in sync con la sidebar (stessa source of truth:
/// AppModel.tabs / activeTabId). Sempre visibile quando un worktree è
/// selezionato; con zero tab mostra solo il "+".
struct TabBarView: View {
    @ObserveInjection private var inject

    @Bindable var model: AppModel
    let worktree: Worktree
    @State private var contentWidth: CGFloat = 0
    @State private var viewportWidth: CGFloat = 0

    private var isOverflowing: Bool { contentWidth > viewportWidth + 1 }

    var body: some View {
        HStack(spacing: 4) {
            ScrollViewReader { proxy in
                ScrollView(.horizontal, showsIndicators: false) {
                    HStack(spacing: 2) {
                        ForEach(model.workspaceTabs(for: worktree.id)) { tab in
                            TabBarItem(model: model, worktree: worktree, tab: tab)
                                .id(tab.id)
                        }
                    }
                    .padding(.leading, 6)
                    .onGeometryChange(for: CGFloat.self) { $0.size.width } action: {
                        contentWidth = $0
                    }
                    .onDrop(
                        of: [.tillerRowDrag],
                        isTargeted: Binding(
                            get: { false },
                            set: { targeted in
                                // Sorvolo dello spazio vuoto dopo l'ultima tab → in coda.
                                if targeted {
                                    model.previewDragToEnd(in: .tabs(worktreeId: worktree.id))
                                }
                            }
                        )
                    ) { _ in
                        model.endRowDrag()
                        return true
                    }
                }
                .onChange(of: model.workspaceActiveTabID(for: worktree.id)) { _, newValue in
                    guard let id = newValue else { return }
                    withAnimation(.easeInOut(duration: 0.15)) { proxy.scrollTo(id) }
                }
                .onGeometryChange(for: CGFloat.self) { $0.size.width } action: {
                    viewportWidth = $0
                }
            }

            Spacer(minLength: 0)
            if isOverflowing {
                Menu {
                    ForEach(model.workspaceTabs(for: worktree.id)) { tab in
                        Button {
                            model.activateTab(tab.id, in: worktree.id)
                        } label: {
                            if model.activeTab(for: worktree.id)?.id == tab.id {
                                Label(tab.title, systemImage: "checkmark")
                            } else {
                                Text(tab.title)
                            }
                        }
                    }
                } label: {
                    Image(systemName: "chevron.down")
                        .font(AppFont.system(size: 10))
                        .foregroundStyle(AppTheme.meta)
                }
                .buttonStyle(.plain)
                .menuIndicator(.hidden)
                .help("All tabs")
            }

            ChatHistoryMenu(model: model, worktree: worktree)

            Menu {
                NewTabMenuItems(model: model, worktree: worktree)
            } label: {
                Image(systemName: "plus")
                    .font(AppFont.system(size: 11))
                    .foregroundStyle(AppTheme.meta)
            }
            .buttonStyle(.plain)
            .menuIndicator(.hidden)
            .help("New tab (⌘T)")
            .padding(.trailing, 8)
        }
        .frame(height: 32)
        .background { MainSurfaceMaterial(tint: AppTheme.chatSurface) }
    .enableInjection()
    }
}

/// Una tab nella bar: icona tipo, titolo, dirty dot markdown, indicatore
/// attività agente, × on-hover. Click = attiva.
struct TabBarItem: View {
    @ObserveInjection private var inject

    @Bindable var model: AppModel
    let worktree: Worktree
    let tab: LegacyWorkspaceTab
    @State private var hovering = false
    @State private var renaming = false
    @State private var draftTitle = ""
    @FocusState private var renameFieldFocused: Bool

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

            if renaming {
                TextField("", text: $draftTitle)
                    .textFieldStyle(.plain)
                    .font(AppFont.system(size: 12))
                    .frame(width: 120)
                    .focused($renameFieldFocused)
                    .onSubmit {
                        model.renameTab(tab.id, in: worktree.id, to: draftTitle)
                        renaming = false
                    }
                    .onExitCommand { renaming = false }
            } else {
                Text(tab.title)
                    .font(AppFont.system(size: 12))
                    .foregroundStyle(isActive ? AppTheme.titleSelected : AppTheme.subtitle)
                    .lineLimit(1)
                    .truncationMode(.tail)
                    // Doppio click SOLO sul titolo (stesso pattern di TabRow):
                    // un count:2 sull'intera riga inghiottirebbe i click sulla ×.
                    .onTapGesture(count: 2) {
                        draftTitle = tab.title
                        renaming = true
                        renameFieldFocused = true
                    }
            }

            if model.isDocumentDirty(tabId: tab.id) {
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
                        .font(AppFont.system(size: 9, weight: .bold))
                        .foregroundStyle(AppTheme.meta)
                }
                .buttonStyle(HoverIconButtonStyle())
                .help("Close tab (⌘W)")
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
            RoundedRectangle(cornerRadius: 7)
                .fill(AppTheme.background)
                .overlay {
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
        }
        .onHover { hovering = $0 }
        .onTapGesture { model.activateTab(tab.id, in: worktree.id) }
        .contextMenu {
            Button("Rename") {
                draftTitle = tab.title
                renaming = true
                renameFieldFocused = true
            }
            Divider()
            Button("Close") { model.closeTab(tab.id, in: worktree) }
            Button("Close Others") { model.closeOtherTabs(tab.id, in: worktree) }
                .disabled(model.workspaceTabs(for: worktree.id).count <= 1)
            Button("Close Tabs to the Right") {
                model.workspaceCloseTabsToRight(of: tab.id, in: worktree)
            }
                .disabled(model.workspaceTabs(for: worktree.id).last?.id == tab.id)
    }
        .reorderable(model: model, id: tab.id, scope: .tabs(worktreeId: worktree.id))
    .enableInjection()
    }
}
