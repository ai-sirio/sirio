import SwiftUI
import TillerCore
import TillerTerminal
import TillerControl
import TillerWorkspace

private struct RightPanelContext: Hashable {
    let worktreeId: UUID?
    let gitProject: Bool
    let visible: Bool
}

struct ContentView: View {
    var model: AppModel
    var updater: UpdaterModel
    @AppStorage("hasSeenPermissionsOnboarding") private var hasSeenPermissionsOnboarding = false
    @AppStorage("sidebar.visible") private var sidebarVisible = true
    @State private var sidebarWidth = CGFloat(AppSettings.defaultSidebarWidth)
    /// Titlebar height, reserved inside the centre surface. The surface itself
    /// runs to the window's top edge, but its content must not: the transparent
    /// `NSTitlebarContainerView` sits above the content view and keeps taking
    /// mouse events across the full width, so anything interactive drawn up
    /// there is visible and dead — pane tabs that neither switch nor close.
    @State private var titlebarInset: CGFloat = 0
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
    private let workspaceCoordinator: WorkspaceCoordinator
    private let workspaceEngineEnabled: Bool

    static func renderPath(gateEnabled: Bool) -> WorkspaceRenderPath {
        gateEnabled ? .workspace : .legacyTerminal
    }

    private var rightPanelContext: RightPanelContext {
        let worktree = model.selectedWorktree
        return RightPanelContext(
            worktreeId: worktree?.id,
            gitProject: worktree.map { model.isGitProject(id: $0.projectId) } ?? false,
            visible: rightPanelVisible)
    }

    init(model: AppModel, updater: UpdaterModel,
         workspaceCoordinator: WorkspaceCoordinator? = nil,
         workspaceEngineEnabled: Bool = WorkspaceEngineGate.isEnabled) {
        self.model = model
        self.updater = updater
        self.menuProvider = TerminalContextMenuProvider(model: model)
        self.workspaceCoordinator = workspaceCoordinator ?? model.workspaceCoordinator
        self.workspaceEngineEnabled = workspaceEngineEnabled
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
            rightPanelModeRaw = RightPanelMode.status.rawValue
            Task { await rightPanelModel.diffStore.expand(entry, repoPath: worktree.path) }
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

                    if workspaceEngineEnabled {
                        universalSplitMenu
                    } else {
                        Button {
                            model.workspaceSplitCurrent(.horizontal)
                        } label: {
                            Image(systemName: "square.split.1x2")
                        }
                        .help("Split terminal")
                        .accessibilityLabel("Split terminal")
                    }

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
        // A VStack, not `.safeAreaInset(edge: .bottom)`: that one leaves the
        // content at full height and only declares an inset, which an
        // NSSplitView-backed HSplitView never applies to itself — its columns
        // ran under the bar and the bar painted over them, clipping the
        // composer. Stacking actually proposes `height - bottomBarHeight`.
        VStack(spacing: 0) {
            ZStack {
                SidebarMaterialContainer().ignoresSafeArea()
                // Measured from a sibling, never from an ancestor of the split:
                // a GeometryReader wrapping the split pins it to the safe-area
                // origin and turns its `ignoresSafeArea` into a silent no-op.
                Color.clear
                    .onGeometryChange(for: CGFloat.self) { $0.safeAreaInsets.top } action: {
                        titlebarInset = $0
                    }
                splitContent
            }
            UsageBarView(
                store: model.usage,
                worktree: model.selectedWorktree,
                onOpenSettings: { model.openSettings() })
        }
    }

    private var splitContent: some View {
        // Two steps, both needed. This one makes the NSSplitView itself span the
        // full window instead of starting below the titlebar, which in turn gives
        // every column a full-height wrapper. Each column is its own NSHostingView
        // and re-derives the window's safe area on its own, so the sidebar and the
        // right panel still sit below the titlebar without being padded — only the
        // centre column opts out, where it's built.
        splitColumns()
            .ignoresSafeArea(.container, edges: .top)
            .overlay(alignment: .leading) {
                if sidebarVisible {
                    DividerCursorStrip()
                        .frame(width: DividerCursorStrip.width)
                        .offset(x: sidebarWidth - DividerCursorStrip.width / 2 + 1)
                }
            }
            .overlay(alignment: .trailing) {
                if rightPanelVisible {
                    SidebarMaterialContainer()
                        .frame(width: 2)
                        .offset(x: -liveRightPanelWidth)
                        .ignoresSafeArea()
                        .allowsHitTesting(false)
                    DividerCursorStrip()
                        .frame(width: DividerCursorStrip.width)
                        .offset(x: -liveRightPanelWidth + DividerCursorStrip.width / 2 - 1)
                }
            }
            .animation(.easeInOut(duration: 0.2), value: sidebarVisible)
            .animation(.easeInOut(duration: 0.2), value: rightPanelVisible)
    }

    private func splitColumns() -> some View {
        HSplitView {
            if sidebarVisible {
                SidebarView(model: model)
                    .frame(
                        minWidth: CGFloat(AppSettings.sidebarWidthRange.lowerBound),
                        idealWidth: CGFloat(AppSettings.defaultSidebarWidth),
                        maxWidth: CGFloat(AppSettings.sidebarWidthRange.upperBound),
                        maxHeight: .infinity)
                    .onGeometryChange(for: CGFloat.self) { $0.size.width } action: {
                        sidebarWidth = $0
                    }
            }
            ZStack {
                AppTheme.background
                FloatingCard {
                    VStack(spacing: 0) {
                        if let worktree = model.selectedWorktree {
                            if !workspaceEngineEnabled {
                                TabBarView(model: model, worktree: worktree)
                                Divider()
                            }
                        }
                        if workspaceEngineEnabled {
                            workspaceStack
                                .contextMenu { paneContextMenu }
                        } else {
                            terminalStack
                        }
                    }
                    // The surface reaches y=0; its content starts below the
                    // titlebar, which still owns the mouse up there.
                    .padding(.top, titlebarInset)
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                }
                .padding(.horizontal, AppTheme.cardGap)
                // No top inset: the surface runs to the window's top edge and the
                // toolbar controls float over its first ~52pt.
                .padding(.bottom, AppTheme.cardGap)
            }
            // Step two: this column's own hosting view still reserves the titlebar,
            // even though the split around it no longer does. Opting out here is
            // what actually lifts the surface to y=0.
            .ignoresSafeArea(.container, edges: .top)
            .frame(minWidth: 320, maxWidth: .infinity, minHeight: 160, maxHeight: .infinity)
            .dropDestination(for: URL.self) { urls, _ in
                guard let worktree = model.selectedWorktree,
                      let url = urls.first(where: {
                          guard $0.isFileURL else { return false }
                          return (try? $0.resourceValues(
                              forKeys: [.isRegularFileKey]).isRegularFile) ?? false
                      }) else { return false }
                model.openDocument(fileURL: url, in: worktree)
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
    }

    private func splitMenuModel(
        worktree: Worktree, layout: WorkspaceLayout
    ) -> SplitContentMenuModel {
        SplitContentMenuModel(
            worktreeID: worktree.id,
            sourceTabID: layout.group(layout.activeGroupID)?.activeTabID ?? WorkspaceTabID(),
            layout: layout,
            layoutsByWorktree: workspaceCoordinator.layouts,
            groupSize: CGSize(width: 800, height: 600),
            placement: .right,
            installedAgents: model.agentCenter.installedAgents.map {
                SplitMenuAgent(id: $0.id, name: $0.name)
            },
            resumedChats: model.chatHistory(for: worktree).map {
                SplitMenuChat(id: $0.id, title: $0.title)
            })
    }

    @ViewBuilder
    private var universalSplitMenu: some View {
        if let worktree = model.selectedWorktree,
           let layout = workspaceCoordinator.layouts[worktree.id] {
            SplitContentMenu(
                model: splitMenuModel(worktree: worktree, layout: layout),
                onAction: { action in
                    handleUniversalSplitAction(action, worktree: worktree, layout: layout)
                })
            .help("Split Right With…")
            .accessibilityLabel("Split Right With…")
        } else {
            Button("Split Right With…") {}
                .disabled(true)
        }
    }

    /// The same menu the toolbar's Split button shows, reachable by
    /// right-clicking the pane itself. It acts on the active pane, matching
    /// what the toolbar button does.
    @ViewBuilder
    private var paneContextMenu: some View {
        if let worktree = model.selectedWorktree,
           let layout = workspaceCoordinator.layouts[worktree.id] {
            SplitContentMenuItems(
                model: splitMenuModel(worktree: worktree, layout: layout),
                onAction: { action in
                    handleUniversalSplitAction(action, worktree: worktree, layout: layout)
                })
        }
    }

    private func handleUniversalSplitAction(
        _ action: SplitContentMenuAction,
        worktree: Worktree,
        layout: WorkspaceLayout
    ) {
        let anchor = layout.activeGroupID
        switch action {
        case .closeTab(let tabID):
            // Goes through AppModel rather than straight to the coordinator so
            // an unsaved document still gets its prompt.
            model.closeTab(tabID.rawValue, in: worktree)
        case .configureAgents:
            model.openAgentsSettings()
        case .openFile:
            let panel = NSOpenPanel()
            panel.directoryURL = URL(fileURLWithPath: worktree.path)
            panel.canChooseDirectories = false
            panel.allowsMultipleSelection = false
            guard panel.runModal() == .OK, let url = panel.url else { return }
            let editor: DocumentEditorKind = MarkdownFileLink.isMarkdown(url) ? .markdown : .code
            Task {
                await workspaceCoordinator.requestSplit(
                    anchor: anchor, placement: .right,
                    choice: .openFile(url, editor: editor), in: worktree)
            }
        case .newTerminal:
            requestUniversalSplit(.newTerminal(command: nil), anchor: anchor, worktree: worktree)
        case .agentTerminal(let agentID):
            requestUniversalSplit(.agentTerminal(agentID: agentID), anchor: anchor, worktree: worktree)
        case .newChat(let agentID):
            requestUniversalSplit(.newChat(agentID: agentID), anchor: anchor, worktree: worktree)
        case .resumeChat(let sessionID):
            requestUniversalSplit(
                .resumeChat(ChatContentID(sessionID)), anchor: anchor, worktree: worktree)
        case .moveExistingTab(let tabID):
            requestUniversalSplit(.moveExistingTab(tabID), anchor: anchor, worktree: worktree)
        }
    }

    private func requestUniversalSplit(
        _ choice: ContentChoice, anchor: PaneGroupID, worktree: Worktree
    ) {
        Task {
            await workspaceCoordinator.requestSplit(
                anchor: anchor, placement: .right, choice: choice, in: worktree)
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
            } else if let worktree = model.selectedWorktree,
                      model.workspaceTabs(for: worktree.id).isEmpty {
                EmptyWorktreeView(onNewTerminal: { model.newShellTabInSelected() })
            }
            ForEach(model.openWorktreeIds, id: \.self) { worktreeId in
                if let worktree = model.worktree(byId: worktreeId) {
                    let isSelected = model.selectedWorktree?.id == worktreeId
                    ForEach(model.workspaceTabs(for: worktreeId)) { tab in
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
                                        onOpenURL: { _, url in model.openFileReference(url, in: worktree) }
                                    ),
                                    menuProvider: { paneId, proxy in
                                        menuProvider.items(for: paneId, proxy: proxy)
                                    },
                                    onMenuAction: { action, paneId, proxy in
                                        menuProvider.handle(action, paneId: paneId, proxy: proxy)
                                    },
                                    paneCache: model.workspacePaneCache(for: worktreeId),
                                    liveLeafIds: { model.workspaceLiveLeafIds(for: worktreeId) }
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
                            case .code:
                                if let document = model.codeDocument(for: tab) {
                                    CodeEditorTabView(document: document)
                                } else {
                                    ContentUnavailableView(
                                        "File not readable",
                                        systemImage: "doc.questionmark",
                                        description: Text(tab.codeFileURL?.path ?? ""))
                                }
                            case .chat:
                                // A chat that was opened once (controller exists) stays
                                // mounted like terminal/markdown tabs: remounting on every
                                // switch rebuilt the whole transcript view and read as a
                                // visible stall. Never-opened chats still mount only when
                                // visible, so their agent doesn't start eagerly.
                                if let controller = model.chatControllers[tab.id]
                                    ?? (isVisible ? model.chatController(
                                            for: tab, in: worktree,
                                            startDetached: true) : nil) {
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

    @ViewBuilder
    private var workspaceStack: some View {
        let plan = WorkspaceMountPlan(
            openWorktreeIDs: model.openWorktreeIds,
            selectedWorktreeID: model.selectedWorktree?.id)
        ZStack {
            if plan.mountedWorktreeIDs.isEmpty {
                ContentUnavailableView(
                    "No worktree selected",
                    systemImage: "rectangle.split.3x1",
                    description: Text("Add a project, then select a worktree.")
                )
            }
            ForEach(plan.mountedWorktreeIDs, id: \.self) { worktreeID in
                if let worktree = model.worktree(byId: worktreeID),
                   let layout = workspaceCoordinator.layouts[worktreeID] {
                    let isSelected = plan.selectedWorktreeID == worktreeID
                    WorkspaceView(
                        layout: layout,
                        delta: isSelected ? workspaceCoordinator.lastSemanticDelta : nil,
                        hostProvider: workspaceCoordinator,
                        intentSink: WorkspaceIntentRouter(
                            coordinator: workspaceCoordinator, worktree: worktree),
                        stripFactory: { stripModel in
                            makePaneTabStrip(
                                stripModel,
                                appModel: model,
                                workspaceCoordinator: workspaceCoordinator,
                                worktree: worktree) {
                                NewTabMenuItems(
                                    model: model, worktree: worktree,
                                    onBeforeAction: stripModel.onActivateGroup,
                                    onNewTerminal: stripModel.onNewTab)
                            }
                        },
                        emptyStateFactory: { stripModel in
                            makePaneEmptyState(stripModel) {
                                NewTabMenuItems(
                                    model: model,
                                    worktree: worktree,
                                    onBeforeAction: stripModel.onActivateGroup,
                                    onNewTerminal: stripModel.onNewTab)
                            }
                        })
                        .focusedSceneValue(
                            \.workspaceMenuTarget,
                            WorkspaceMenuTarget(
                                worktreeID: worktreeID,
                                menuModel: WorkspaceMenuModel(layout: layout),
                                send: { action in
                                    Task { @MainActor in
                                        await workspaceCoordinator.handle(
                                            menuAction: action, in: worktree)
                                    }
                                }))
                        .opacity(isSelected ? 1 : 0)
                        .allowsHitTesting(isSelected)
                        .accessibilityHidden(!isSelected)
                }
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}
