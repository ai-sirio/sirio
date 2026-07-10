import SwiftUI
import TillerCore
import TillerTerminal
import TillerControl

struct ContentView: View {
    var model: AppModel
    var updater: UpdaterModel
    @AppStorage("usage.claude.showInBar") private var showClaudeInBar = true
    @AppStorage("usage.codex.showInBar") private var showCodexInBar = true
    @AppStorage("usage.opencodeGo.showInBar") private var showOpencodeGoInBar = false
    @AppStorage("usage.ollamaCloud.showInBar") private var showOllamaCloudInBar = false
    @AppStorage("hasSeenPermissionsOnboarding") private var hasSeenPermissionsOnboarding = false
    private let menuProvider: TerminalContextMenuProvider

    private var showUsageBar: Bool {
        showClaudeInBar || showCodexInBar || showOpencodeGoInBar || showOllamaCloudInBar
    }

    init(model: AppModel, updater: UpdaterModel) {
        self.model = model
        self.updater = updater
        self.menuProvider = TerminalContextMenuProvider(model: model)
    }

    var body: some View {
        Group {
            switch model.route {
            case .workspace: workspaceView
            case .settings: SettingsSurface(model: model, updater: updater)
            }
        }
        .frame(minWidth: 900, minHeight: 560)
        .configuresWindowChrome()
        .task { await model.bootstrap() }
        .alert(
            "Error",
            isPresented: .init(
                get: { model.lastError != nil },
                set: { if !$0 { model.lastError = nil } }
            )
        ) {
            Button("OK") { model.lastError = nil }
        } message: {
            Text(model.lastError ?? "")
        }
        .sheet(isPresented: .init(
            get: { !hasSeenPermissionsOnboarding },
            set: { if !$0 { hasSeenPermissionsOnboarding = true } }
        )) {
            PermissionsOnboardingSheet { hasSeenPermissionsOnboarding = true }
        }
        .overlay(alignment: .bottomTrailing) {
            UpdateToastView(updater: updater)
                .padding(16)
        }
    }

    private var workspaceView: some View {
        VStack(spacing: 0) {
            TopBarView(model: model)
            NavigationSplitView {
                SidebarView(model: model)
                    .navigationSplitViewColumnWidth(min: 200, ideal: 240, max: 400)
            } detail: {
                VStack(spacing: 0) {
                    if let selected = model.selectedWorktree {
                        TabBarView(model: model, worktree: selected)
                    }
                    terminalStack
                }
                .frame(minWidth: 320, maxWidth: .infinity, minHeight: 160, maxHeight: .infinity)
                .background(AppTheme.background)
                .dropDestination(for: URL.self) { urls, _ in
                    guard let worktree = model.selectedWorktree,
                          let url = urls.first(where: { MarkdownFileLink.isMarkdown($0) }) else { return false }
                    model.openMarkdownTab(fileURL: url, in: worktree)
                    return true
                }
            }
            if showUsageBar {
                Divider()
                UsageBarView(store: model.usage, worktree: model.selectedWorktree)
            }
        }
    }

    @ViewBuilder
    private var terminalStack: some View {
        ZStack {
            if model.openWorktreeIds.isEmpty {
                ContentUnavailableView(
                    "No worktree selected",
                    systemImage: "terminal",
                    description: Text("Add a project, then select a worktree.")
                )
            }
            ForEach(model.openWorktreeIds, id: \.self) { worktreeId in
                if let worktree = model.worktree(byId: worktreeId) {
                    let isSelected = model.selectedWorktree?.id == worktreeId
                    ForEach(model.tabs[worktreeId] ?? []) { tab in
                        let isVisible = isSelected
                            && model.activeTab(for: worktreeId)?.id == tab.id
                        Group {
                            switch tab.content {
                            case .terminal(let tree):
                                TerminalSplitHost(
                                    tree: tree,
                                    workingDirectory: worktree.path,
                                    extraEnvironment: [
                                        "TILLER_ENV": "1",
                                        "TILLER_SOCKET": ControlSocket.defaultPath(),
                                        "TILLER_WORKTREE_ID": worktreeId.uuidString
                                    ],
                                    paneContext: (
                                        initial: { model.loadScrollback(paneId: $0) },
                                        onClose: { id, data in
                                            await model.saveScrollback(worktreeId: worktreeId, paneId: id, data: data)
                                            await MainActor.run {
                                                model.agentActivity.paneClosed(paneId: id)
                                                model.paneCommands[id] = nil
                                            }
                                        },
                                        command: { model.paneCommand(paneId: $0) },
                                        onTitleChange: { id, title in model.handleTitleChange(paneId: id, title: title) },
                                        onContentSignal: { id, tail in model.handleContentSignal(paneId: id, tailText: tail) },
                                        onOpenURL: { _, url in model.handleTerminalOpenURL(url, in: worktree) }
                                    ),
                                    menuProvider: { paneId, proxy in
                                        menuProvider.items(for: paneId, proxy: proxy)
                                    },
                                    onMenuAction: { action, paneId, proxy in
                                        menuProvider.handle(action, paneId: paneId, proxy: proxy)
                                    }
                                )
                            case .markdown:
                                if let doc = model.markdownDocument(for: tab) {
                                    MarkdownEditorTabView(document: doc)
                                } else {
                                    ContentUnavailableView(
                                        "File non trovato",
                                        systemImage: "doc.questionmark",
                                        description: Text(tab.markdownFileURL?.path ?? "")
                                    )
                                }
                            }
                        }
                        .opacity(isVisible ? 1 : 0)
                        .allowsHitTesting(isVisible)
                        .accessibilityHidden(!isVisible)
                    }
                }
            }
        }
    }
}
