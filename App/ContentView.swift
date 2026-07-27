import SwiftUI
import TillerCore
import TillerTerminal
import TillerControl

private struct RightPanelContext: Hashable {
    let worktreeId: UUID?
    let gitProject: Bool
    let visible: Bool
}

struct ContentView: View {
    var model: AppModel
    var updater: UpdaterModel
    @AppStorage("usage.claude.showInBar") private var showClaudeInBar = true
    @AppStorage("usage.codex.showInBar") private var showCodexInBar = true
    @AppStorage("usage.opencodeGo.showInBar") private var showOpencodeGoInBar = false
    @AppStorage("usage.ollamaCloud.showInBar") private var showOllamaCloudInBar = false
    @AppStorage("hasSeenPermissionsOnboarding") private var hasSeenPermissionsOnboarding = false
    @AppStorage("sidebar.visible") private var sidebarVisible = true
    @State private var sidebarWidth: CGFloat = 240
    @AppStorage(AppSettings.rightPanelVisibleKey)
    private var rightPanelVisible = AppSettings.defaultRightPanelVisible
    @AppStorage(AppSettings.rightPanelWidthKey)
    private var rightPanelWidth = AppSettings.defaultRightPanelWidth
    /// Live panel width driving the divider-cover overlay. The persisted
    /// value follows debounced: writing @AppStorage per geometry tick (every
    /// animation frame / divider-drag event) spammed UserDefaults and
    /// re-invalidated every @AppStorage reader in the window mid-animation.
    @State private var liveRightPanelWidth = CGFloat(AppSettings.defaultRightPanelWidth)
    @State private var persistRightPanelWidthTask: Task<Void, Never>?
    @AppStorage(AppSettings.rightPanelModeKey)
    private var rightPanelModeRaw = RightPanelMode.files.rawValue
    @State private var rightPanelModel = RightPanelModel()
    private let menuProvider: TerminalContextMenuProvider

    private var showUsageBar: Bool {
        showClaudeInBar || showCodexInBar || showOpencodeGoInBar || showOllamaCloudInBar
    }

    private var rightPanelContext: RightPanelContext {
        let worktree = model.selectedWorktree
        return RightPanelContext(
            worktreeId: worktree?.id,
            gitProject: worktree.map { model.isGitProject(id: $0.projectId) } ?? false,
            visible: rightPanelVisible)
    }

    init(model: AppModel, updater: UpdaterModel) {
        self.model = model
        self.updater = updater
        self.menuProvider = TerminalContextMenuProvider(model: model)
    }

    var body: some View {
        Group {
            ZStack {
                workspaceView
                    .opacity(model.route == .workspace ? 1 : 0)
                    .allowsHitTesting(model.route == .workspace)
                    .accessibilityHidden(model.route != .workspace)
                if model.route == .settings {
                    SettingsSurface(model: model, updater: updater)
                }
            }
        }
        .frame(minWidth: 900, minHeight: 560)
        .task(id: rightPanelContext) {
            guard rightPanelVisible else {
                rightPanelModel.deactivate()
                return
            }
            await rightPanelModel.activate(
                worktree: model.selectedWorktree,
                isGitRepository: rightPanelContext.gitProject)
        }
        .onChange(of: model.chatFollowRequest) {
            guard let request = model.chatFollowRequest,
                  let worktree = model.selectedWorktree,
                  worktree.id == request.worktreeId else { return }
            rightPanelVisible = true
            let root = URL(fileURLWithPath: worktree.path).standardizedFileURL.path
            let relative = request.path.hasPrefix(root + "/")
                ? String(request.path.dropFirst(root.count + 1))
                : request.path
            guard let entry = rightPanelModel.statusByPath[relative] else { return }
            rightPanelModeRaw = RightPanelMode.diff.rawValue
            Task { await rightPanelModel.selectDiff(entry) }
        }
        .onDisappear { rightPanelModel.deactivate() }
        .configuresWindowChrome()
        .toolbar {
            if model.route == .workspace {
                ToolbarItem(placement: .navigation) {
                    Button {
                        sidebarVisible.toggle()
                    } label: {
                        Image(systemName: "sidebar.left")
                    }
                    .help(sidebarVisible ? "Hide Sidebar (⌃⌘S)" : "Show Sidebar (⌃⌘S)")
                    .accessibilityLabel("Sidebar")
                }

                ToolbarItemGroup(placement: .primaryAction) {
                    Button {
                        rightPanelVisible.toggle()
                    } label: {
                        Image(systemName: "sidebar.right")
                    }
                    .help(rightPanelVisible
                          ? "Hide right panel (⌃⌘I)"
                          : "Show right panel (⌃⌘I)")
                    .accessibilityLabel("Right panel")

                    Button {
                        model.splitCurrent(.horizontal)
                    } label: {
                        Image(systemName: "square.split.1x2")
                    }
                    .help("Split terminal")
                    .accessibilityLabel("Split terminal")

                    Button {
                        model.settingsCategory = .permissions
                        model.openSettings()
                    } label: {
                        Image(systemName: "lock.shield")
                    }
                    .help("Permissions")
                    .accessibilityLabel("Permissions")
                }
            }
        }
        .toolbarBackgroundVisibility(.hidden, for: .windowToolbar)
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

    // HSplitView instead of NavigationSplitView: on macOS 26 the system sidebar
    // renders as an inset floating glass card with no opt-out; owning the split
    // lets the sidebar run edge-to-edge for the full window height.
    private var workspaceView: some View {
        // HSplitView (NSSplitView) clips each pane to its own bounds, so a
        // material background nested inside a pane can't ignoresSafeArea()
        // past that clip to reach under the transparent titlebar/toolbar.
        // Painting the material as the outermost layer, behind the whole
        // split view, lets it extend there unclipped.
        ZStack {
            SidebarMaterialContainer().ignoresSafeArea()
            splitContent
        }
    }

    private var splitContent: some View {
        HSplitView {
            if sidebarVisible {
                SidebarView(model: model)
                    .frame(minWidth: 200, idealWidth: 240, maxWidth: 400, maxHeight: .infinity)
                    .onGeometryChange(for: CGFloat.self) { $0.size.width } action: {
                        sidebarWidth = $0
                    }
            }
            VStack(spacing: 0) {
                if let worktree = model.selectedWorktree {
                    TabBarView(model: model, worktree: worktree)
                    Divider()
                }
                terminalStack
                if showUsageBar {
                    Divider()
                    UsageBarView(store: model.usage, worktree: model.selectedWorktree)
                }
            }
            .frame(minWidth: 320, maxWidth: .infinity, minHeight: 160, maxHeight: .infinity)
            // Translucent terminal/chat surface only below the titlebar; the
            // shared material behind the whole ZStack shows through above it,
            // so the titlebar matches the sidebar.
            .background { MainSurfaceMaterial() }
            .clipShape(RoundedRectangle(cornerRadius: 10))
            .dropDestination(for: URL.self) { urls, _ in
                guard let worktree = model.selectedWorktree,
                      let url = urls.first(where: { MarkdownFileLink.isMarkdown($0) }) else { return false }
                model.openMarkdownTab(fileURL: url, in: worktree)
                return true
            }
            if rightPanelVisible {
                RightPanelView(
                    appModel: model,
                    panelModel: rightPanelModel,
                    modeRaw: $rightPanelModeRaw,
                    isGitRepository: rightPanelContext.gitProject,
                    onClose: { rightPanelVisible = false })
                .frame(
                    minWidth: CGFloat(AppSettings.rightPanelWidthRange.lowerBound),
                    idealWidth: CGFloat(AppSettings.clampRightPanelWidth(rightPanelWidth)),
                    maxWidth: CGFloat(AppSettings.rightPanelWidthRange.upperBound),
                    maxHeight: .infinity)
                .onGeometryChange(for: CGFloat.self) { $0.size.width } action: { width in
                    let clamped = AppSettings.clampRightPanelWidth(Double(width))
                    liveRightPanelWidth = CGFloat(clamped)
                    persistRightPanelWidthTask?.cancel()
                    persistRightPanelWidthTask = Task { @MainActor in
                        try? await Task.sleep(for: .milliseconds(300))
                        guard !Task.isCancelled else { return }
                        rightPanelWidth = clamped
                    }
                }
            }
        }
        // HSplitView draws an opaque dark divider with no styling API; cover
        // it with the shared material so no seam shows between the columns.
        .overlay(alignment: .leading) {
            if sidebarVisible {
                SidebarMaterialContainer()
                    .frame(width: 2)
                    .offset(x: sidebarWidth)
                    .ignoresSafeArea()
                    .allowsHitTesting(false)
            }
        }
        .overlay(alignment: .trailing) {
            if rightPanelVisible {
                SidebarMaterialContainer()
                    .frame(width: 2)
                    .offset(x: -liveRightPanelWidth)
                    .ignoresSafeArea()
                    .allowsHitTesting(false)
            }
        }
        .animation(.easeInOut(duration: 0.2), value: sidebarVisible)
        .animation(.easeInOut(duration: 0.2), value: rightPanelVisible)
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
            } else if let worktree = model.selectedWorktree,
                      (model.tabs[worktree.id] ?? []).isEmpty {
                EmptyWorktreeView(onNewTerminal: { model.newShellTabInSelected() })
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
                                    isVisible: isVisible,
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
                                                model.paneClosed(paneId: id)
                                                model.paneCommands[id] = nil
                                                model.paneTitles[id] = nil
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
                                    },
                                    paneCache: model.paneCache(for: worktreeId),
                                    liveLeafIds: { model.liveLeafIds(for: worktreeId) }
                                )
                            case .markdown:
                                if let doc = model.markdownDocument(for: tab) {
                                    MarkdownEditorTabView(document: doc)
                                } else {
                                    ContentUnavailableView(
                                        "File not found",
                                        systemImage: "doc.questionmark",
                                        description: Text(tab.markdownFileURL?.path ?? "")
                                    )
                                }
                            case .chat:
                                // A chat that was opened once (controller exists) stays
                                // mounted like terminal/markdown tabs: remounting on every
                                // switch rebuilt the whole transcript view and read as a
                                // visible stall. Never-opened chats still mount only when
                                // visible, so their agent doesn't start eagerly.
                                if let controller = model.chatControllers[tab.id]
                                    ?? (isVisible ? model.chatController(for: tab, in: worktree) : nil) {
                                    ChatPaneView(controller: controller, worktree: worktree,
                                                 appModel: model)
                                } else if isVisible {
                                    ContentUnavailableView("Agent not available",
                                                           systemImage: "bubble.left")
                                } else {
                                    EmptyView()
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
        // Fill the detail column even when empty, so the usage bar stays
        // pinned to the window bottom instead of centering with the ZStack.
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}
