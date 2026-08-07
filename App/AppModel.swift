import SwiftUI
import Observation
import TillerPersistence
import TillerCore
import TillerGit
import TillerTerminal
import TillerControl
import TillerAgents
import TillerACP
import TillerCode
import AppKit
import UserNotifications
import OSLog

@MainActor
@Observable
final class AppModel {
    let workspaceCoordinator: WorkspaceCoordinator
    private let workspacePersistenceBridge: WorkspacePersistenceBridge

    var projects: [Project] = []
    var worktrees: [UUID: [Worktree]] = [:]

    /// Progetto selezionato (non persistito).
    var selectedProjectId: UUID? = nil

    /// Progetti espansi (non persistito).
    var expandedProjectIds: Set<UUID> = []

    /// Riga attualmente trascinata, per il riordino con anteprima dal vivo.
    var draggingRow: DraggedRow?

    var selectedWorktree: Worktree? {
        didSet {
            let sid = SignpostMetrics.makeSignpostID()
            let state = SignpostMetrics.beginInterval("worktreeSwitch", id: sid)
            defer { SignpostMetrics.endInterval("worktreeSwitch", state) }
            selectedProjectId = selectedWorktree?.projectId
            UserDefaults.standard.set(selectedWorktree?.id.uuidString, forKey: AppSettings.selectedWorktreeIdKey)
            guard let worktree = selectedWorktree else { return }
            if !openWorktreeIds.contains(worktree.id) { openWorktreeIds.append(worktree.id) }
            // The worktree mounts with zero tabs. The user opens a tab
            // explicitly via ⌘T or the sidebar "+" menu.
            // ensureTabs(for:) is available for other callers if needed.
            evictIdleWorktreesIfNeeded()
        }
    }

    /// Unmounts idle, non-selected worktrees down to the user-configured
    /// cap (off by default — see AppSettings.maxMountedWorktreesKey). No-op
    /// unless the user opted in, since unmounting terminates a worktree's
    /// PTYs.
    private func evictIdleWorktreesIfNeeded() {
        let cap = UserDefaults.standard.integer(forKey: AppSettings.maxMountedWorktreesKey)
        let evicted = WorktreeMountPolicy.idsToEvict(
            openWorktreeIds: openWorktreeIds,
            selectedWorktreeId: selectedWorktree?.id,
            cap: cap,
            status: { [weak self] id in
                self?.worktree(byId: id).flatMap { self?.statusForWorktree($0) } ?? nil
            },
            hasUnsavedWork: { [weak self] id in
                self?.workspaceTabs(for: id).contains {
                    self?.isDocumentDirty(tabId: $0.id) == true
                } == true
            }
        )
        guard !evicted.isEmpty else { return }
        openWorktreeIds.removeAll { evicted.contains($0) }
    }

    /// Worktrees whose terminal hosts stay mounted (PTYs alive) across
    /// selection changes. Removal unmounts the host, which fires
    /// onDisappear and terminates its panes. Persisted so bootstrap() can
    /// reopen the same worktrees after a quit/relaunch.
    var openWorktreeIds: [UUID] = [] {
        didSet {
            UserDefaults.standard.set(openWorktreeIds.map(\.uuidString), forKey: AppSettings.openWorktreeIdsKey)
        }
    }

    func worktree(byId id: UUID) -> Worktree? {
        worktrees.values.flatMap { $0 }.first { $0.id == id }
    }

    func isProjectExpanded(_ project: Project) -> Bool {
        expandedProjectIds.contains(project.id)
    }

    func toggleProjectExpanded(_ project: Project) {
        if expandedProjectIds.contains(project.id) {
            expandedProjectIds.remove(project.id)
        } else {
            expandedProjectIds.insert(project.id)
            selectedProjectId = project.id
        }
    }

    func selectProjectHeader(_ project: Project) {
        selectedProjectId = project.id
    }

    /// Highest-priority agent status among all worktrees of a project.
    /// Priority: error > needs-input > running > done.
    func statusForProject(_ project: Project) -> AgentStatus? {
        let statuses = (worktrees[project.id] ?? []).compactMap { statusForWorktree($0) }
        return AgentStatus.highestPriority(in: statuses)
    }

    /// Agent lifecycle status, pane→agent mapping, and title-derived detection.
    var agentActivity = AgentActivityModel()
    private var processScanCoordinator = ProcessScanCoordinator()

    typealias ForegroundProcessScanner = @Sendable (
        UUID
    ) async -> ForegroundProcessScanResult?
    private let foregroundProcessScanner: ForegroundProcessScanner

    private static func makeForegroundProcessScanner(
        paneRegistry: PaneRegistry
    ) -> ForegroundProcessScanner {
        { paneId in
            guard let pid = await paneRegistry.shellPid(paneId: paneId) else {
                return nil
            }
            let agentId = ForegroundProcessAgent.identify(shellPid: pid)
            let processTree = agentId == nil
                ? []
                : ForegroundProcessAgent.processTree(shellPid: pid)
            return ForegroundProcessScanResult(
                agentId: agentId,
                processTree: processTree
            )
        }
    }


    /// Highest-priority agent status among all panes in a worktree's tree.
    /// Priority: error > needs-input > running > done. Returns nil if no agent panes.
    func statusForWorktree(_ worktree: Worktree) -> AgentStatus? {
        let paneIds = workspaceTabs(for: worktree.id)
            .flatMap { $0.activityPaneIds }
        return agentActivity.statusForWorktree(paneIds: paneIds)
    }

    /// Worktrees with at least one agent pane reporting a live status — the
    /// set surfaced in the menu-bar roster. Unsorted; callers apply
    /// `AttentionSort` for display order (same pattern as `SidebarView`).
    var activeAgentWorktrees: [Worktree] {
        worktrees.values.flatMap { $0 }.filter { statusForWorktree($0) != nil }
    }

    /// Worst status among all active-agent worktrees — drives the menu-bar
    /// icon (spin on `.running`, tint on `.needsInput`/`.error`).
    var menuBarAggregateStatus: AgentStatus? {
        AgentStatus.highestPriority(in: activeAgentWorktrees.compactMap { statusForWorktree($0) })
    }

    /// Tab whose panes report the worst status within `worktree` — the tab
    /// the menu-bar roster switches to when jumping back into a worktree.
    func worstStatusTab(in worktree: Worktree) -> LegacyWorkspaceTab? {
        AttentionSort.sorted(workspaceTabs(for: worktree.id)) { tab in
            agentActivity.statusForWorktree(paneIds: tab.leafIds)
        }.first
    }

    let usage = UsageStore()

    /// Top-level window route and the selected settings category.
    var route: AppRoute = .workspace
    var settingsCategory: SettingsCategory = .aiProviders

    func openSettings() { route = .settings }
    func openAgentsSettings() {
        settingsCategory = .agents
        route = .settings
    }
    func closeSettings() { route = .workspace }
    /// Adapter id of the most relevant agent pane in a worktree (same
    /// priority order as statusForWorktree), nil if no agent panes.
    func agentIdForWorktree(_ worktree: Worktree) -> String? {
        let paneIds = workspaceTabs(for: worktree.id).flatMap { $0.leafIds }
        return agentActivity.agentIdForWorktree(paneIds: paneIds)
    }

    /// Distinct agent ids currently .running in a worktree, ordered by
    /// AgentCatalog.all for stable left-to-right icon order in the
    /// worktree row's trailing running-agents badge.
    func runningAgentIds(for worktree: Worktree) -> [String] {
        let paneIds = workspaceTabs(for: worktree.id)
            .flatMap { $0.activityPaneIds }
        return agentActivity.runningAgentIds(paneIds: paneIds, catalogIds: AgentCatalog.all.map(\.id))
    }
    private let notifier = AgentNotifier()

    private var controlServer: ControlServer?
    typealias ControlTabPersister = @MainActor (
        UUID, [LegacyWorkspaceTab], UUID?
    ) async throws -> Void

    private let paneRegistry: PaneRegistry
    private let registrationTimeoutMs: Int
    private let paneIdGenerator: @MainActor () -> UUID
    private let activateApplication: @MainActor () -> Void
    private let controlTabPersister: ControlTabPersister?

    private struct ControlMountLease {
        let worktreeId: UUID
    }

    private struct ControlMountState {
        var leaseCount: Int
        let addedByControl: Bool
        var keepMounted: Bool
    }

    /// Iniettabile per i test: il bundle AppTests è ospitato dentro Tiller.app,
    /// quindi UserDefaults.standard lì è il dominio reale dell'utente.
    private let defaults: UserDefaults

    private var controlMountStates: [UUID: ControlMountState] = [:]
    private var tabPersistenceTasks: [UUID: Task<Void, Never>] = [:]
    private var controlLifecycleTasks: [UUID: Task<ControlResponse, Never>] = [:]

    init(
        paneRegistry: PaneRegistry = .shared,
        registrationTimeoutMs: Int = 5_000,
        paneIdGenerator: @escaping @MainActor () -> UUID = { UUID() },
        activateApplication: @escaping @MainActor () -> Void = {
            NSApp.activate(ignoringOtherApps: true)
            NSApp.windows.first?.makeKeyAndOrderFront(nil)
        },
        controlTabPersister: ControlTabPersister? = nil,
        defaults: UserDefaults = .standard,
        foregroundProcessScanner: ForegroundProcessScanner? = nil,
        persistenceCoordinator: PersistenceCoordinator? = nil,
        workspaceCoordinator: WorkspaceCoordinator? = nil,
        workspacePersistenceBridge: WorkspacePersistenceBridge? = nil
    ) {
        self.paneRegistry = paneRegistry
        self.registrationTimeoutMs = registrationTimeoutMs
        self.paneIdGenerator = paneIdGenerator
        self.activateApplication = activateApplication
        self.controlTabPersister = controlTabPersister
        self.defaults = defaults
        self.persistenceCoordinator = persistenceCoordinator
        let bridge = workspacePersistenceBridge ?? WorkspacePersistenceBridge()
        self.workspacePersistenceBridge = bridge
        if let workspaceCoordinator {
            self.workspaceCoordinator = workspaceCoordinator
        } else {
            let registry = WorkspaceContentRegistry()
            let adapters: [WorkspaceContentKind: any WorkspaceContentAdapter] = [
                .terminal: TerminalContentAdapter(),
                .chat: ChatContentAdapter(),
                .document: DocumentContentAdapter(),
                .browser: BrowserContentAdapter()
            ]
            self.workspaceCoordinator = WorkspaceCoordinator(
                persistence: bridge, registry: registry, adapters: adapters)
        }
        self.foregroundProcessScanner = foregroundProcessScanner
            ?? Self.makeForegroundProcessScanner(paneRegistry: paneRegistry)
        let installStore = AgentInstallStore(
            rootDirectory: FileManager.default.urls(
                for: .applicationSupportDirectory, in: .userDomainMask)[0]
                .appendingPathComponent("Tiller/acp-agents", isDirectory: true))
        self.agentInstallStore = installStore
        self.agentCenter = AcpAgentCenter(installStore: installStore)
        // Late-bound: the adapters are built above, before `self` exists, and
        // chatStore itself only arrives once the database is open.
        let chatAdapter = self.workspaceCoordinator.adapters[.chat] as? ChatContentAdapter
        chatAdapter?.makeSession = { [weak self] worktreeID, agentID in
            guard let store = self?.chatStore else { throw ContentAdapterError.noChatStore }
            self?.rememberChatAgent(agentID)
            return try store.createSession(
                worktreeId: worktreeID.uuidString, agentId: agentID).id
        }
        chatAdapter?.makeContentViewController = { [weak self] tab, worktree, isFresh in
            guard let self,
                  let controller = self.chatController(
                    for: tab, in: worktree,
                    startNewConversation: isFresh, startDetached: !isFresh)
            else { return nil }
            return NSHostingController(
                rootView: ChatPaneView(controller: controller, worktree: worktree, appModel: self))
        }
        chatAdapter?.releaseContent = { [weak self] tabID in
            self?.teardownChatController(tabId: tabID.rawValue)
        }
        (self.workspaceCoordinator.adapters[.terminal] as? TerminalContentAdapter)?
            .resumeCommandProvider = { [weak self] contentID, worktree, paneId in
                // The surface builds its root view on the main actor, so this
                // provider is only ever called there; the closure's type is
                // nonisolated because it crosses the TillerTerminal boundary.
                MainActor.assumeIsolated {
                    self?.resumeCommand(contentID: contentID, worktree: worktree, paneId: paneId)
                }
            }
        (self.workspaceCoordinator.adapters[.terminal] as? TerminalContentAdapter)?
            .onAgentTerminalPrepared = { [weak self] contentID, agentId in
                self?.pendingAgentLaunch[contentID] = agentId
            }
        (self.workspaceCoordinator.adapters[.terminal] as? TerminalContentAdapter)?
            .agentDisplayName = { agentId in
                AgentCatalog.all.first { $0.id == agentId }?.displayName ?? agentId
            }
        (self.workspaceCoordinator.adapters[.browser] as? BrowserContentAdapter)?
            .onPageChange = { [weak self] tabID, page in
                guard let self else { return }
                guard let worktree = self.worktrees.values.flatMap({ $0 }).first(where: {
                    self.workspaceCoordinator.layouts[$0.id]?.tab(tabID) != nil
                }) else { return }
                Task { @MainActor [weak self] in
                    await self?.workspaceCoordinator.updateBrowserPage(
                        tabID: tabID, url: page.url.absoluteString,
                        title: page.title, in: worktree)
                }
            }
        chatAdapter?.resolveTitle = { [weak self] contentID in
            let stored = self?.chatSession(id: contentID.rawValue)?.title?
                .trimmingCharacters(in: .whitespacesAndNewlines)
            return (stored?.isEmpty == false) ? stored : nil
        }
    }
    /// Projects whose root currently contains a `.git` entry. Derived at
    /// runtime (bootstrap, add, in-app git init) — never persisted, so an
    /// external `git init` is picked up on the next launch.
    var gitProjectIds: Set<UUID> = []

    /// Richiesta della chat di mostrare un file nel right panel (following /
    /// click su un file della card riepilogo). ContentView la osserva.
    struct ChatFollowRequest: Equatable {
        let path: String
        let worktreeId: UUID
        let ordinal: Int
    }
    var chatFollowRequest: ChatFollowRequest?
    private var chatFollowOrdinal = 0

    func requestChatFollow(path: String, worktreeId: UUID) {
        chatFollowOrdinal += 1
        chatFollowRequest = ChatFollowRequest(
            path: path, worktreeId: worktreeId, ordinal: chatFollowOrdinal)
    }

    func isGitProject(_ project: Project) -> Bool { gitProjectIds.contains(project.id) }
    func isGitProject(id: UUID) -> Bool { gitProjectIds.contains(id) }

    private func refreshGitDetection() {
        gitProjectIds = Set(
            projects.filter { GitRepoDetection.isGitRepository(path: $0.rootPath) }.map(\.id)
        )
    }
    var lastError: String?
    /// Short-lived bottom-right message (drop rejections today). A newer
    /// message replaces the older one and restarts the timer; the token is
    /// what stops a stale timer from clearing a fresh message.
    var transientMessage: String?
    private var transientMessageToken = 0
    static let transientMessageSeconds = 4

    func showTransientMessage(_ text: String) {
        transientMessage = text
        transientMessageToken += 1
        let token = transientMessageToken
        Task { [weak self] in
            try? await Task.sleep(for: .seconds(AppModel.transientMessageSeconds))
            guard let self, self.transientMessageToken == token else { return }
            self.transientMessage = nil
        }
    }
    /// State as loaded at launch — the target of the manual
    /// "Restore Previous Launch" action. In-memory only.
    struct LaunchSnapshot {
        let tabs: [UUID: [LegacyWorkspaceTab]]
        let paneCommands: [UUID: String]
        let openWorktreeIds: [UUID]
    }
    private(set) var launchSnapshot: LaunchSnapshot?

    private var store: ProjectStore?
    private var database: AppDatabase?
    private var persistenceCoordinator: PersistenceCoordinator?
    var agentAccounts: AgentAccountStore?

    private let sessionRestoreLogger = Logger(subsystem: "dev.tiller", category: "session-restore")

    func bootstrap() async {
        let sid = SignpostMetrics.makeSignpostID()
        let state = SignpostMetrics.beginInterval("bootstrapInteractive", id: sid)
        var bootstrapEnded = false
        defer {
            if !bootstrapEnded {
                SignpostMetrics.endInterval(
                    "bootstrapInteractive", state,
                    message: "projects: \(projects.count), worktrees: \(worktrees.values.reduce(0) { $0 + $1.count })")
            }
        }
        do {
            let dir = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
                .appendingPathComponent("Tiller", isDirectory: true)
            try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
            refreshTillerctlShim()
            let db = try AppDatabase(
                path: dir.appendingPathComponent("tiller.sqlite").path, upTo: "v16")
            try SQLiteWorkspacePersistence.migrateV15IfNeeded(
                database: db, backupDirectory: dir.appendingPathComponent("backups", isDirectory: true))
            let store = ProjectStore(database: db)
            workspacePersistenceBridge.install(SQLiteWorkspacePersistence(
                database: db,
                recoveryDirectory: dir.appendingPathComponent("recovery", isDirectory: true)))
            self.store = store
            self.database = db
            self.agentAccounts = AgentAccountStore(database: db)
            let chatStore = ChatSessionStore(database: db)
            self.chatStore = chatStore
            self.persistenceCoordinator = PersistenceCoordinator(
                transcriptWriter: { sessionId, items in
                    let sid = SignpostMetrics.makeSignpostID()
                    let state = SignpostMetrics.beginInterval(
                        "transcriptPersist", id: sid)
                    defer {
                        SignpostMetrics.endInterval(
                            "transcriptPersist", state,
                            message: "items: \(items.count)")
                    }
                    try chatStore.saveTranscript(sessionId: sessionId, items: items)
                },
                scrollbackWriter: { worktreeId, paneId, data in
                    try db.write { database in
                        try PaneScrollbackRecord(
                            paneId: paneId.uuidString,
                            worktreeId: worktreeId.uuidString,
                            data: data,
                            updatedAt: Date()
                        ).save(database)
                    }
                })
            projects = try await store.loadAll()
            let ctl = tillerctlPath()
            // Publish the sidebar tree first: one DB read per project, and
            // resolving the stored selection needs the worktrees present.
            for project in projects {
                worktrees[project.id] = try await store.worktrees(of: project.id)
            }
            pruneChatHistory()
            let storedOpenIds = (UserDefaults.standard.stringArray(forKey: AppSettings.openWorktreeIdsKey) ?? [])
                .compactMap(UUID.init)
            let storedSelectedId = UserDefaults.standard.string(forKey: AppSettings.selectedWorktreeIdKey)
                .flatMap(UUID.init)
            let order = BootstrapRestoreOrder.partition(
                worktrees: projects.flatMap { worktrees[$0.id] ?? [] },
                openWorktreeIds: storedOpenIds,
                selectedWorktreeId: storedSelectedId
            )
            for worktree in order.priority {
                await restoreWorktree(worktree, tillerctlPath: ctl)
            }
            openWorktreeIds = storedOpenIds.filter { id in
                worktree(byId: id) != nil
            }
            if let storedSelectedId, openWorktreeIds.contains(storedSelectedId) {
                selectedWorktree = worktree(byId: storedSelectedId)
            } else {
                selectedWorktree = openWorktreeIds.first.flatMap { worktree(byId: $0) }
            }
            bootstrapEnded = true
            SignpostMetrics.endInterval(
                "bootstrapInteractive", state,
                message: "projects: \(projects.count), worktrees: \(worktrees.values.reduce(0) { $0 + $1.count })")
            // Worktrees nobody is waiting on: restored after the first paint,
            // so time-to-interactive tracks the open panes, not the sidebar size.
            Task { @MainActor [weak self] in
                guard let self else { return }
                for worktree in order.deferred {
                    await self.restoreWorktree(worktree, tillerctlPath: ctl)
                }
                self.launchSnapshot = LaunchSnapshot(
                    tabs: self.workspaceCoordinator.legacyStore.tabs,
                    paneCommands: self.paneCommands,
                    openWorktreeIds: self.openWorktreeIds
                )
            }
            refreshGitDetection()
            if AppSettings.controlSocketEnabled(
                defaultsValue: UserDefaults.standard.object(forKey: AppSettings.controlSocketEnabledKey) as? Bool,
                env: ProcessInfo.processInfo.environment
            ) {
                startControlServer()
            }
            UNUserNotificationCenter.current().delegate = notifier
            notifier.onActivatePane = { [weak self] id in
                Task { await self?.focusPane(paneId: id) }
            }
            usage.updatePolling()
        } catch {
            lastError = "Database error: \(error)"
        }
    }

    /// Restores one worktree's tabs, chat agent ids and agent sessions.
    /// A worktree the user already populated is left alone, so a deferred
    /// restore never clobbers panes created while it was still in flight.
    private func restoreWorktree(_ worktree: Worktree, tillerctlPath ctl: String) async {
        guard let store else { return }
        if !workspaceCoordinator.legacyTabs(for: worktree.id).isEmpty { return }
        // Repair hook configs frozen on a tillerctl path that no
        // longer exists (pre-shim builds, cleaned DerivedData).
        ClaudeHookMigrator.migrateFile(
            atPath: worktree.path + "/.claude/settings.local.json",
            tillerctlPath: ctl
        )
        let loaded: (tabs: [LegacyWorkspaceTab], activeTabId: UUID?)?
        do {
            loaded = try await store.loadTabs(of: worktree.id)
        } catch {
            // terminalTab is renamed away by the v17 migration (see
            // WorkspaceMigrationV15/AppDatabase), so this throws on every
            // worktree once a database has migrated — that must not also
            // block workspaceCoordinator.restore() below, which is the only
            // thing populating the universal engine's layout for this
            // worktree. Legacy-tab bookkeeping is simply skipped for it.
            sessionRestoreLogger.warning("restore: loadTabs failed for worktree \(worktree.id.uuidString, privacy: .public): \(String(describing: error), privacy: .public)")
            loaded = nil
        }
        if let loaded {
            // Tab markdown il cui file è sparito tra le sessioni: scartate in silenzio.
            let restoredTabs = loaded.tabs.filter { tab in
                guard let url = tab.markdownFileURL else { return true }
                return FileManager.default.fileExists(atPath: url.path)
            }
            workspaceCoordinator.setLegacyTabs(restoredTabs, for: worktree.id)
            workspaceCoordinator.setLegacyActiveTabID(loaded.activeTabId.flatMap { active in
                restoredTabs.contains { $0.id == active } ? active : nil
            } ?? restoredTabs.first?.id, for: worktree.id)
            for tab in restoredTabs {
                guard let chatAgentId = tab.chatAgentId else { continue }
                agentActivity.registerAgentId(paneId: tab.id, agentId: chatAgentId)
            }
            if !WorkspaceEngineGate.isEnabled {
                await restoreAgentSessions(
                    for: worktree,
                    contentIDs: Set(restoredTabs.flatMap { $0.leafIds }.map(TerminalContentID.init))
                )
            }
        }
        if WorkspaceEngineGate.isEnabled {
            await workspaceCoordinator.restore(worktree: worktree)
            registerRestoredChatAgents(in: worktree.id)
            // After restore, so the layout exists: pruning against an empty
            // layout would delete every ref. Before any host is built, so the
            // parked resumes are in place when the first surface spawns —
            // hosts are created lazily on mount, not by restore.
            await restoreAgentSessions(
                for: worktree, contentIDs: restoredTerminalContentIDs(in: worktree.id))
        }
    }

    private func restoredTerminalContentIDs(in worktreeId: UUID) -> Set<TerminalContentID> {
        var ids: Set<TerminalContentID> = []
        for tab in workspaceCoordinator.layouts[worktreeId]?.allTabs ?? [] {
            if case .terminal(let contentID) = tab.content { ids.insert(contentID) }
        }
        return ids
    }

    /// Identity for restored chat tabs, so the Agents panel and badges know
    /// which agent a tab belongs to before it is ever viewed (viewing it
    /// builds a controller, which registers identity on its own).
    func registerRestoredChatAgents(in worktreeId: UUID) {
        for tab in workspaceCoordinator.layouts[worktreeId]?.allTabs ?? [] {
            guard case .chat(let contentID) = tab.content,
                  let record = chatSession(id: contentID.rawValue) else { continue }
            agentActivity.registerAgentId(
                paneId: tab.id.rawValue, agentId: AgentIdMigration.canonical(record.agentId))
        }
    }

    // MARK: - Control socket

    /// Idempotent. Starts a Unix-domain-socket control server so tillerctl (and
    /// agent hooks) can create/write/read/wait panels and push status updates.
    func startControlServer() {
        guard controlServer == nil else { return }
        let path = ControlSocket.defaultPath()
        try? FileManager.default.createDirectory(
            atPath: (path as NSString).deletingLastPathComponent,
            withIntermediateDirectories: true
        )
        let server = ControlServer(socketPath: path) { [weak self] request in
            return await Task { @MainActor [weak self] in
                guard let self else { return .failure(id: request.id, error: "app shutting down") }
                return await self.handleControl(request)
            }.value
        }
        do { try server.start(); controlServer = server }
        catch { lastError = "Control server failed to start: \(error)" }
    }
    /// Live toggle from Settings. Stopping kills the listener — agent hooks
    /// (Layer A) stop reporting until re-enabled.
    func setControlSocketEnabled(_ enabled: Bool) {
        if enabled {
            startControlServer()
        } else {
            controlServer?.stop()
            controlServer = nil
        }
    }

    /// Dispatch an incoming control request. MainActor-isolated; called via
    /// the @Sendable ControlServer.Handler bridge (Task { @MainActor in … }).
    func handleControl(_ request: ControlRequest) async -> ControlResponse {
        switch request.method {
        case "browser.open", "browser.navigate", "browser.get", "browser.screenshot":
            return await handleBrowserControl(request)
        case "panel.create":
            guard let selector = request.params["worktree"],
                  let worktree = resolveWorktree(selector) else {
                return .failure(id: request.id, error: "unknown worktree")
            }
            return await serializeControlLifecycle(for: worktree.id) {
                if WorkspaceEngineGate.isEnabled {
                    guard let group = await self.workspaceCoordinator.ensureGroup(
                        for: worktree) else {
                        return .failure(id: request.id, error: "workspace layout unavailable")
                    }
                    let mountLease = self.acquireControlMount(for: worktree.id)
                    let tabsBefore = Set(
                        self.workspaceCoordinator.layouts[worktree.id]?.allTabs.map(\.id) ?? [])
                    await self.workspaceCoordinator.requestNewTab(
                        into: group,
                        choice: .newTerminal(command: request.params["cmd"]),
                        in: worktree)
                    let insertedTab = self.workspaceCoordinator.layouts[worktree.id]?.allTabs
                        .first(where: { !tabsBefore.contains($0.id) })
                    guard let universalTabID = insertedTab,
                          let contentID = self.workspaceCoordinator.terminalContentID(
                              for: universalTabID.id, in: worktree.id) else {
                        if let insertedTab {
                            await self.workspaceCoordinator.closeTab(insertedTab.id, in: worktree)
                        }
                        self.releaseControlMount(mountLease, success: false)
                        return .failure(id: request.id, error: "panel did not register before timeout")
                    }
                    let startedAt = Date()
                    var livePaneId = self.workspaceCoordinator.liveControlPaneId(
                        contentID: contentID, in: worktree.id)
                    while livePaneId == nil
                        && Date().timeIntervalSince(startedAt) * 1_000
                            < Double(self.registrationTimeoutMs) {
                        livePaneId = self.workspaceCoordinator.liveControlPaneId(
                            contentID: contentID, in: worktree.id)
                        if livePaneId == nil {
                            try? await Task.sleep(for: .milliseconds(1))
                        }
                    }
                    guard let livePaneId else {
                        await self.workspaceCoordinator.closeTab(universalTabID.id, in: worktree)
                        self.releaseControlMount(mountLease, success: false)
                        return .failure(
                            id: request.id, error: "panel did not register before timeout"
                        )
                    }
                    let elapsedMs = Int(Date().timeIntervalSince(startedAt) * 1_000)
                    let remainingMs = max(0, self.registrationTimeoutMs - elapsedMs)
                    guard await self.paneRegistry.waitUntilRegistered(
                        paneId: livePaneId, timeoutMs: remainingMs
                    ) else {
                        await self.paneRegistry.cancelRegistration(paneId: livePaneId)
                        await self.workspaceCoordinator.closeTab(universalTabID.id, in: worktree)
                        self.releaseControlMount(mountLease, success: false)
                        return .failure(
                            id: request.id, error: "panel did not register before timeout"
                        )
                    }
                    self.releaseControlMount(mountLease, success: true)
                    return .success(id: request.id, result: ["id": livePaneId.uuidString])
                }

                let paneId = self.paneIdGenerator()
                if let command = request.params["cmd"] { self.paneCommands[paneId] = command }
                let mountLease = self.acquireControlMount(for: worktree.id)
                let title = request.params["cmd"]?
                    .split(separator: " ").first.map(String.init) ?? "Panel"
                let tab = self.openTab(
                    paneId: paneId, title: title, in: worktree,
                    activate: false, persist: false
                )

                guard await self.paneRegistry.waitUntilRegistered(
                    paneId: paneId, timeoutMs: self.registrationTimeoutMs
                ) else {
                    await self.paneRegistry.cancelRegistration(paneId: paneId)
                    self.workspaceRollbackCreatedPane(paneId, tabId: tab.id, in: worktree.id)
                    self.paneCommands[paneId] = nil
                    self.releaseControlMount(mountLease, success: false)
                    return .failure(
                        id: request.id, error: "panel did not register before timeout"
                    )
                }
                guard self.workspaceTabContaining(paneId: paneId) != nil else {
                    await self.paneRegistry.cancelRegistration(paneId: paneId)
                    self.paneCommands[paneId] = nil
                    self.releaseControlMount(mountLease, success: false)
                    return .failure(
                        id: request.id, error: "panel was closed before registration completed"
                    )
                }
                do {
                    try await self.persistControlTabs(for: worktree.id)
                } catch {
                    await self.paneRegistry.cancelRegistration(paneId: paneId)
                    self.workspaceRollbackCreatedPane(paneId, tabId: tab.id, in: worktree.id)
                    self.paneCommands[paneId] = nil
                    self.releaseControlMount(mountLease, success: false)
                    return .failure(
                        id: request.id, error: "panel persistence failed: \(error)"
                    )
                }
                self.releaseControlMount(mountLease, success: true)
                return .success(id: request.id, result: ["id": paneId.uuidString])
            }

        case "panel.split":
            guard let sourceId = request.params["from"].flatMap(UUID.init(uuidString:)) else {
                return .failure(id: request.id, error: "unknown source panel")
            }
            let axis: SplitAxis
            let placement: SplitPlacement
            let splitPlacementSide: SplitPlacementSide
            switch request.params["direction"] {
            case "left": axis = .horizontal; placement = .before; splitPlacementSide = .left
            case "right": axis = .horizontal; placement = .after; splitPlacementSide = .right
            case "up": axis = .vertical; placement = .before; splitPlacementSide = .above
            case "down": axis = .vertical; placement = .after; splitPlacementSide = .below
            default:
                return .failure(
                    id: request.id, error: "invalid direction (left|right|up|down)"
                )
            }
            if WorkspaceEngineGate.isEnabled {
                guard await paneRegistry.isRegistered(paneId: sourceId),
                      let target = universalControlTarget(paneId: sourceId),
                      let groupID = workspaceCoordinator.layouts[target.worktree.id]?
                          .groupContaining(tab: target.tab.id) else {
                    return .failure(id: request.id, error: "unknown source panel")
                }
                return await serializeControlLifecycle(for: target.worktree.id) {
                    let mountLease = self.acquireControlMount(for: target.worktree.id)
                    let tabsBefore = Set(
                        self.workspaceCoordinator.layouts[target.worktree.id]?.allTabs.map(\.id) ?? [])
                    await self.workspaceCoordinator.requestSplit(
                        anchor: groupID, placement: splitPlacementSide,
                        choice: .newTerminal(command: request.params["cmd"]), in: target.worktree)
                    let insertedTab = self.workspaceCoordinator.layouts[target.worktree.id]?.allTabs
                        .first(where: { !tabsBefore.contains($0.id) })
                    guard let universalTabID = insertedTab,
                          let contentID = self.workspaceCoordinator.terminalContentID(
                              for: universalTabID.id, in: target.worktree.id) else {
                        if let insertedTab {
                            await self.workspaceCoordinator.closeTab(insertedTab.id, in: target.worktree)
                        }
                        self.releaseControlMount(mountLease, success: false)
                        return .failure(id: request.id, error: "panel did not register before timeout")
                    }
                    let startedAt = Date()
                    var livePaneId = self.workspaceCoordinator.liveControlPaneId(
                        contentID: contentID, in: target.worktree.id)
                    while livePaneId == nil
                        && Date().timeIntervalSince(startedAt) * 1_000
                            < Double(self.registrationTimeoutMs) {
                        livePaneId = self.workspaceCoordinator.liveControlPaneId(
                            contentID: contentID, in: target.worktree.id)
                        if livePaneId == nil {
                            try? await Task.sleep(for: .milliseconds(1))
                        }
                    }
                    guard let livePaneId else {
                        await self.workspaceCoordinator.closeTab(universalTabID.id, in: target.worktree)
                        self.releaseControlMount(mountLease, success: false)
                        return .failure(
                            id: request.id, error: "panel did not register before timeout"
                        )
                    }
                    let elapsedMs = Int(Date().timeIntervalSince(startedAt) * 1_000)
                    let remainingMs = max(0, self.registrationTimeoutMs - elapsedMs)
                    guard await self.paneRegistry.waitUntilRegistered(
                        paneId: livePaneId, timeoutMs: remainingMs
                    ) else {
                        await self.paneRegistry.cancelRegistration(paneId: livePaneId)
                        await self.workspaceCoordinator.closeTab(universalTabID.id, in: target.worktree)
                        self.releaseControlMount(mountLease, success: false)
                        return .failure(
                            id: request.id, error: "panel did not register before timeout"
                        )
                    }
                    self.releaseControlMount(mountLease, success: true)
                    return .success(id: request.id, result: ["id": livePaneId.uuidString])
                }
            }
            guard let source = workspaceTabContaining(paneId: sourceId) else {
                return .failure(id: request.id, error: "unknown source panel")
            }
            return await serializeControlLifecycle(for: source.worktree.id) {
                let paneId = self.paneIdGenerator()
                if let command = request.params["cmd"] { self.paneCommands[paneId] = command }
                let mountLease = self.acquireControlMount(for: source.worktree.id)
                guard self.workspaceSplit(
                    paneId: sourceId, axis: axis, newPaneId: paneId, placement: placement,
                    persist: false
                ) else {
                    await self.paneRegistry.cancelRegistration(paneId: paneId)
                    self.paneCommands[paneId] = nil
                    self.releaseControlMount(mountLease, success: false)
                    return .failure(id: request.id, error: "source panel disappeared")
                }

                guard await self.paneRegistry.waitUntilRegistered(
                    paneId: paneId, timeoutMs: self.registrationTimeoutMs
                ) else {
                    await self.paneRegistry.cancelRegistration(paneId: paneId)
                    self.workspaceRollbackCreatedLeaf(paneId)
                    self.paneCommands[paneId] = nil
                    self.releaseControlMount(mountLease, success: false)
                    return .failure(
                        id: request.id, error: "panel did not register before timeout"
                    )
                }
                guard self.workspaceTabContaining(paneId: paneId) != nil else {
                    await self.paneRegistry.cancelRegistration(paneId: paneId)
                    self.paneCommands[paneId] = nil
                    self.releaseControlMount(mountLease, success: false)
                    return .failure(
                        id: request.id, error: "panel was closed before registration completed"
                    )
                }
                do {
                    try await self.persistControlTabs(for: source.worktree.id)
                } catch {
                    await self.paneRegistry.cancelRegistration(paneId: paneId)
                    self.workspaceRollbackCreatedLeaf(paneId)
                    self.paneCommands[paneId] = nil
                    self.releaseControlMount(mountLease, success: false)
                    return .failure(
                        id: request.id, error: "panel persistence failed: \(error)"
                    )
                }
                self.releaseControlMount(mountLease, success: true)
                return .success(id: request.id, result: ["id": paneId.uuidString])
            }

        case "panel.list":
            guard let selector = request.params["worktree"],
                  let worktree = resolveWorktree(selector) else {
                return .failure(id: request.id, error: "unknown worktree")
            }
            let rows: [[String: String]]
            if WorkspaceEngineGate.isEnabled {
                guard let layout = self.workspaceCoordinator.layouts[worktree.id] else {
                    return .success(id: request.id, result: ["panels": ControlRows.encode([])])
                }
                rows = layout.orderedGroupIDs.flatMap { groupID in
                    guard let group = layout.group(groupID) else { return [[String: String]]() }
                    return group.tabs.compactMap { tab -> [String: String]? in
                        guard case .terminal(let contentID) = tab.content,
                              let paneId = self.workspaceCoordinator.liveControlPaneId(
                                  contentID: contentID, in: worktree.id) else { return nil }
                        return [
                            "id": paneId.uuidString,
                            "tab": tab.title,
                            "title": self.paneTitles[paneId] ?? "",
                            "agent": self.agentActivity.paneAgents[paneId] ?? "",
                            "active": groupID == layout.activeGroupID
                                && tab.id == group.activeTabID ? "true" : "false",
                        ]
                    }
                }
            } else {
                let legacyTabs = self.workspaceCoordinator.legacyTabs(for: worktree.id)
                let activeTabID = self.workspaceCoordinator.legacyActiveTabID(for: worktree.id)
                rows = ControlListing.paneRows(legacyTabs.flatMap { tab in
                    tab.leafIds.map { paneId in
                        [
                            "id": paneId.uuidString,
                            "tab": tab.title,
                            "title": self.paneTitles[paneId] ?? "",
                            "agent": self.agentActivity.paneAgents[paneId] ?? "",
                            "active": tab.id == activeTabID ? "true" : "false",
                        ]
                    }
                })
            }
            return .success(
                id: request.id, result: ["panels": ControlRows.encode(rows)]
            )

        case "panel.write":
            guard let paneId = request.params["id"].flatMap(UUID.init(uuidString:)),
                  let input = request.params["input"] else {
                return .failure(id: request.id, error: "missing/invalid id or input")
            }
            let wrote = await paneRegistry.write(
                paneId: paneId, data: Data(input.utf8)
            )
            return wrote ? .success(id: request.id)
                         : .failure(id: request.id, error: "unknown panel")

        case "panel.key":
            guard let paneId = request.params["id"].flatMap(UUID.init(uuidString:)) else {
                return .failure(id: request.id, error: "missing/invalid id")
            }
            guard let key = request.params["key"].flatMap(TerminalKey.init(rawValue:)) else {
                let names = TerminalKey.allCases.map(\.rawValue).joined(separator: "|")
                return .failure(id: request.id, error: "invalid key (\(names))")
            }
            let wrote = await paneRegistry.write(paneId: paneId, data: key.bytes)
            return wrote ? .success(id: request.id)
                         : .failure(id: request.id, error: "unknown panel")

        case "panel.read":
            guard let paneId = request.params["id"].flatMap(UUID.init(uuidString:)) else {
                return .failure(id: request.id, error: "missing/invalid id")
            }
            guard let data = await paneRegistry.snapshot(paneId: paneId) else {
                return .failure(id: request.id, error: "unknown panel")
            }
            return .success(
                id: request.id, result: ["output": data.base64EncodedString()]
            )

        case "panel.wait":
            guard let paneId = request.params["id"].flatMap(UUID.init(uuidString:)) else {
                return .failure(id: request.id, error: "missing/invalid id")
            }
            let timeoutMs: Int?
            if let rawTimeout = request.params["timeoutMs"] {
                guard let parsed = Int(rawTimeout), parsed >= 0 else {
                    return .failure(id: request.id, error: "invalid timeoutMs")
                }
                timeoutMs = parsed
            } else {
                timeoutMs = nil
            }
            guard let code = await paneRegistry.waitExit(
                paneId: paneId, timeoutMs: timeoutMs
            ) else {
                return .failure(id: request.id, error: "unknown panel or timeout")
            }
            return .success(
                id: request.id, result: ["exitCode": String(code)]
            )

        case "panel.focus":
            if WorkspaceEngineGate.isEnabled {
                guard let paneId = request.params["id"].flatMap(UUID.init(uuidString:)),
                      await paneRegistry.isRegistered(paneId: paneId),
                      let target = universalControlTarget(paneId: paneId) else {
                    return .failure(id: request.id, error: "unknown panel")
                }
                guard !Task.isCancelled else {
                    return .failure(id: request.id, error: "panel focus cancelled")
                }
                selectedWorktree = target.worktree
                activateApplication()
                await workspaceCoordinator.handle(
                    .activateTab(target.tab.id), in: target.worktree)
                return .success(id: request.id)
            }
            guard let paneId = request.params["id"].flatMap(UUID.init(uuidString:)),
                  workspaceTabContaining(paneId: paneId) != nil else {
                return .failure(id: request.id, error: "unknown panel")
            }
            switch await focusPane(paneId: paneId) {
            case .focused:
                return .success(id: request.id)
            case .cancelled:
                // No client is left to read this: the request was abandoned
                // mid-flight. It is a distinct outcome anyway, so that
                // "abandoned" is never silently reported as "failed".
                return .failure(id: request.id, error: "panel focus cancelled")
            case .notFocused:
                return .failure(id: request.id, error: "panel could not be focused")
            }

        case "panel.close":
            if WorkspaceEngineGate.isEnabled {
                guard let paneId = request.params["id"].flatMap(UUID.init(uuidString:)),
                      await paneRegistry.isRegistered(paneId: paneId),
                      let target = universalControlTarget(paneId: paneId) else {
                    return .failure(id: request.id, error: "unknown panel")
                }
                return await serializeControlLifecycle(for: target.worktree.id) {
                    guard await self.paneRegistry.isRegistered(paneId: paneId) else {
                        return .failure(id: request.id, error: "unknown panel")
                    }
                    // Resolved before the close: afterwards the tab is gone
                    // from the layout and the pane id maps to nothing.
                    let closedContentID: TerminalContentID? =
                        if case .terminal(let cid) = target.tab.content { cid } else { nil }
                    await self.workspaceCoordinator.closeTab(target.tab.id, in: target.worktree)
                    await self.paneRegistry.cancelRegistration(paneId: paneId)
                    self.deleteAgentSessionRefs(contentIDs: closedContentID.map { [$0] } ?? [])
                    self.paneCommands[paneId] = nil
                    return .success(id: request.id)
                }
            }
            guard let paneId = request.params["id"].flatMap(UUID.init(uuidString:)),
                  let target = workspaceTabContaining(paneId: paneId) else {
                return .failure(id: request.id, error: "unknown panel")
            }
            return await serializeControlLifecycle(for: target.worktree.id) {
                guard let removal = self.removePaneForControl(paneId) else {
                    return .failure(id: request.id, error: "unknown panel")
                }
                do {
                    try await self.persistControlTabs(for: removal.worktreeId)
                } catch {
                    self.workspaceCoordinator.setLegacyTabs(
                        removal.tabs, for: removal.worktreeId)
                    self.workspaceCoordinator.setLegacyActiveTabID(
                        removal.activeTabId, for: removal.worktreeId)
                    return .failure(
                        id: request.id, error: "panel persistence failed: \(error)"
                    )
                }
                await self.paneRegistry.cancelRegistration(paneId: paneId)
                self.deleteAgentSessionRefs(contentIDs: [TerminalContentID(paneId)])
                self.paneCommands[paneId] = nil
                self.workspaceCoordinator.legacyPaneCache(for: removal.worktreeId).prune(
                    keeping: self.workspaceLiveLeafIds(for: removal.worktreeId)
                )
                return .success(id: request.id)
            }

        case "notify":
            guard let sessionId = UUID(uuidString: request.params["session"] ?? ""),
                  let status = AgentStatus(rawValue: request.params["status"] ?? "") else {
                return .failure(id: request.id, error: "missing/invalid session/status")
            }
            let transition = agentActivity.notify(
                paneId: sessionId, status: status, now: Date()
            )
            notifyTransition(paneId: sessionId, from: transition.old, to: transition.new)
            if let ref = request.params["agentSession"], !ref.isEmpty {
                saveAgentSessionRef(paneId: sessionId, sessionRef: ref)
            }
            return .success(id: request.id)

        case "session.ref":
            guard let paneId = UUID(uuidString: request.params["session"] ?? ""),
                  let ref = request.params["ref"], !ref.isEmpty else {
                return .failure(id: request.id, error: "missing/invalid session/ref")
            }
            saveAgentSessionRef(paneId: paneId, sessionRef: ref)
            return .success(id: request.id)

        case "worktree.set":
            guard let selector = request.params["worktree"],
                  let comment = request.params["comment"], let store else {
                return .failure(id: request.id, error: "missing worktree/comment")
            }
            let target: Worktree?
            if let uuid = UUID(uuidString: selector) {
                target = worktrees.values.flatMap { $0 }.first { $0.id == uuid }
            } else {
                target = try? await store.worktree(byPath: selector)
            }
            guard let target else {
                return .failure(id: request.id, error: "unknown worktree")
            }
            do {
                try await store.setWorktreeComment(target.id, comment: comment)
                worktrees[target.projectId] = try await store.worktrees(of: target.projectId)
                resyncSelection(projectId: target.projectId)
                return .success(id: request.id)
            } catch {
                return .failure(id: request.id, error: "persist failed: \(error)")
            }

        default:
            return await handleCmuxControl(request)
        }
    }

    func addProject(at url: URL) async {
        guard let store else { return }
        do {
            let project = try await store.addProject(name: url.lastPathComponent, rootPath: url.path)
            // The repo's main checkout is itself the first "worktree" entry.
            // Non-git folders skip the git shell-out and keep the "main"
            // placeholder; the UI ignores it while the project is non-git.
            let isGit = GitRepoDetection.isGitRepository(path: url.path)
            let branch = isGit
                ? ((try? await GitWorktrees.list(repoPath: url.path).first?.branch) ?? nil)
                : nil
            let main = try await store.addWorktree(
                projectId: project.id, branch: branch ?? "main", path: url.path
            )
            projects.append(project)
            worktrees[project.id] = [main]
            refreshGitDetection()
        } catch {
            lastError = "Add project failed: \(error)"
        }
    }

    func cloneProject(
        url: String, into parentDir: String,
        onProgress: @escaping @Sendable (Double) -> Void
    ) async throws {
        let name = GitRemote.projectName(fromCloneURL: url)
        let path = (parentDir as NSString).appendingPathComponent(name)
        try FileManager.default.createDirectory(atPath: parentDir, withIntermediateDirectories: true)
        do {
            try await GitClone.clone(url: url, to: path, onProgress: onProgress)
        } catch {
            try? FileManager.default.removeItem(atPath: path)
            throw error
        }
        await addProject(at: URL(fileURLWithPath: path))
    }

    func createProject(name: String, in parentDir: String) async throws {
        let path = (parentDir as NSString).appendingPathComponent(name)
        try FileManager.default.createDirectory(atPath: path, withIntermediateDirectories: true)
        do {
            _ = try await GitRunner.run(["init"], in: path)
        } catch {
            try? FileManager.default.removeItem(atPath: path)
            throw error
        }
        await addProject(at: URL(fileURLWithPath: path))
    }
    /// Add-flow path: user chose "Inizializza git" for a non-git folder.
    /// Throws so the sheet can alert without adding the project.
    func initGitAndAddProject(at url: URL) async throws {
        _ = try await GitRunner.run(["init"], in: url.path)
        await addProject(at: url)
    }

    /// Context-menu path: converts an already-added non-git project.
    func initializeGitRepository(for project: Project) async {
        guard let store else { return }
        do {
            _ = try await GitRunner.run(["init"], in: project.rootPath)
            // Respect a non-"main" init.defaultBranch: re-read the real
            // branch and fix the placeholder stored at add time.
            if let branch = try? await GitWorktrees.list(repoPath: project.rootPath).first?.branch,
               let main = (worktrees[project.id] ?? []).first(where: { $0.path == project.rootPath }),
               main.branch != branch {
                try await store.setWorktreeBranch(main.id, branch: branch)
                worktrees[project.id] = try await store.worktrees(of: project.id)
                resyncSelection(projectId: project.id)
            }
            refreshGitDetection()
        } catch {
            lastError = "Git init failed: \(error)"
        }
    }

    func removeProject(_ project: Project) async {
        guard let store else { return }
        let projectWorktrees = worktrees[project.id] ?? []
        do {
            // DB first (same rationale as removeWorktree): a git failure must not strand DB rows.
            try await store.removeProject(project.id)
            projects.removeAll { $0.id == project.id }
            worktrees[project.id] = nil
            for worktree in projectWorktrees {
                let tabsBeingRemoved = workspaceCoordinator.legacyTabs(for: worktree.id)
                workspaceCoordinator.legacyStore.removeWorktree(worktree.id)
                for tab in tabsBeingRemoved {
                    teardownDocument(tabId: tab.id)
                    teardownChatController(tabId: tab.id)
                }
            }
            openWorktreeIds.removeAll { id in projectWorktrees.contains { $0.id == id } }
            if let sel = selectedWorktree, sel.projectId == project.id {
                selectedWorktree = nil
                selectedProjectId = nil
            }
            for worktree in projectWorktrees where worktree.path != project.rootPath {
                do {
                    try await GitWorktrees.remove(repoPath: project.rootPath, path: worktree.path)
                } catch {
                    lastError = "Removed project, but git worktree cleanup failed for \(worktree.branch): \(error)"
                }
            }
        } catch {
            lastError = "Remove project failed: \(error)"
        }
    }

    func setProjectDisplayName(_ project: Project, displayName: String?) async {
        guard let store else { return }
        do {
            try await store.setProjectDisplayName(project.id, displayName: displayName)
            if let idx = projects.firstIndex(where: { $0.id == project.id }) {
                projects[idx].displayName = displayName
            }
        } catch { lastError = "Update display name failed: \(error)" }
    }

    func setProjectColor(_ project: Project, colorHex: String?) async {
        guard let store else { return }
        do {
            try await store.setProjectColor(project.id, colorHex: colorHex)
            if let idx = projects.firstIndex(where: { $0.id == project.id }) {
                projects[idx].colorHex = colorHex
            }
        } catch { lastError = "Update color failed: \(error)" }
    }

    func setProjectIcon(_ project: Project, kind: IconKind, value: String?, avatarImage: Data?) async {
        guard let store else { return }
        do {
            try await store.setProjectIcon(project.id, kind: kind, value: value, avatarImage: avatarImage)
            if let idx = projects.firstIndex(where: { $0.id == project.id }) {
                projects[idx].iconKind = kind
                projects[idx].iconValue = value
                projects[idx].avatarImage = avatarImage
            }
        } catch { lastError = "Update icon failed: \(error)" }
    }

    func setProjectWorktreeBase(_ project: Project, branch: String?) async {
        guard let store else { return }
        do {
            try await store.setProjectWorktreeBase(project.id, branch: branch)
            if let idx = projects.firstIndex(where: { $0.id == project.id }) {
                projects[idx].defaultWorktreeBase = branch
            }
        } catch { lastError = "Update worktree base failed: \(error)" }
    }

    func setProjectWorktreeLocation(_ project: Project, path: String?) async {
        guard let store else { return }
        do {
            try await store.setProjectWorktreeLocation(project.id, path: path)
            if let idx = projects.firstIndex(where: { $0.id == project.id }) {
                projects[idx].worktreeLocationOverride = path
            }
        } catch { lastError = "Update worktree location failed: \(error)" }
    }

    func addWorktree(project: Project, branch: String) async {
        guard let store else { return }
        let parentDir = WorktreeDefaults.resolveParentDirectory(project: project)
        let path = "\(parentDir)/\(project.name)-\(branch)"
        let base = WorktreeDefaults.resolveBase(project: project, worktrees: worktrees[project.id] ?? [])
        do {
            try await GitWorktrees.add(repoPath: project.rootPath, branch: branch, at: path, base: base)
            let worktree = try await store.addWorktree(projectId: project.id, branch: branch, path: path)
            worktrees[project.id, default: []].append(worktree)
        } catch {
            lastError = "Add worktree failed: \(error)"
        }
    }

    func removeWorktree(_ worktree: Worktree) async {
        guard let store else { return }
        guard let project = projects.first(where: { $0.id == worktree.projectId }) else { return }
        do {
            // Remove DB row first so a git failure can't strand a DB row.
            try await store.removeWorktree(worktree.id)
            worktrees[worktree.projectId]?.removeAll { $0.id == worktree.id }
            let tabsBeingRemoved = workspaceCoordinator.legacyTabs(for: worktree.id)
            workspaceCoordinator.legacyStore.removeWorktree(worktree.id)
            for tab in tabsBeingRemoved {
                teardownDocument(tabId: tab.id)
                teardownChatController(tabId: tab.id)
            }
            openWorktreeIds.removeAll { $0 == worktree.id }
            if selectedWorktree?.id == worktree.id {
                selectedWorktree = nil
                selectedProjectId = nil
            }
            // The main checkout entry (path == rootPath) is DB-only.
            if worktree.path != project.rootPath {
                try await GitWorktrees.remove(repoPath: project.rootPath, path: worktree.path)
            }
        } catch {
            lastError = "Remove worktree failed: \(error)"
        }
    }

    func setPrimary(_ worktree: Worktree) async {
        guard let store else { return }
        do {
            try await store.setWorktreePrimary(worktree.id, isPrimary: !worktree.isPrimary)
            worktrees[worktree.projectId] = try await store.worktrees(of: worktree.projectId)
            resyncSelection(projectId: worktree.projectId)
        } catch { lastError = "Set primary failed: \(error)" }
    }


    /// Re-point selectedWorktree at the fresh instance after a refresh —
    /// Worktree equality is by value, so a comment/primary update would
    /// otherwise silently drop the List selection.
    private func resyncSelection(projectId: UUID) {
        guard let selected = selectedWorktree, selected.projectId == projectId else { return }
        selectedWorktree = worktrees[projectId]?.first { $0.id == selected.id }
    }
    func ensureTabs(for worktree: Worktree) {
        // No-op: empty worktrees are valid. Tabs are created explicitly
        // via newShellTab, spawnAgent, or the panel.create control command.
    }

    func activeTab(for worktreeId: UUID) -> LegacyWorkspaceTab? {
        let list = workspaceTabs(for: worktreeId)
        guard !list.isEmpty else { return nil }
        return list.first {
            $0.id == workspaceActiveTabID(for: worktreeId)
        } ?? list.first
    }

    func workspaceTabs(for worktreeID: UUID) -> [LegacyWorkspaceTab] {
        guard WorkspaceEngineGate.isEnabled,
              let layout = workspaceCoordinator.layouts[worktreeID] else {
            return workspaceCoordinator.legacyTabs(for: worktreeID)
        }
        return SidebarTabProjection.rows(
            layout: layout,
            livePaneID: { [weak self] contentID in
                self?.workspaceCoordinator.liveControlPaneId(
                    contentID: contentID, in: worktreeID)
            },
            chatAgentID: { [weak self] contentID in
                self?.chatSession(id: contentID.rawValue)
                    .map { AgentIdMigration.canonical($0.agentId) }
            })
    }

    func workspaceActiveTabID(for worktreeID: UUID) -> UUID? {
        guard WorkspaceEngineGate.isEnabled,
              let layout = workspaceCoordinator.layouts[worktreeID] else {
            return workspaceCoordinator.legacyActiveTabID(for: worktreeID)
        }
        return layout.group(layout.activeGroupID)?.activeTabID?.rawValue
    }

    /// Apre una nuova tab con un singolo pane e la attiva. Punto unico usato
    /// da shell manuali, spawnAgent e panel.create.
    @discardableResult
    func openTab(
        paneId: UUID,
        title: String,
        in worktree: Worktree,
        activate: Bool = true,
        persist: Bool = true
    ) -> LegacyWorkspaceTab {
        let tab = LegacyWorkspaceTab(id: UUID(), title: title, tree: .leaf(id: paneId))
        workspaceCoordinator.appendLegacyTab(tab, to: worktree.id, activate: activate)
        if persist { workspacePersistTabs(for: worktree.id) }
        return tab
    }

    func newShellTab(in worktree: Worktree) {
        if WorkspaceEngineGate.isEnabled {
            // Selecting first: the sidebar's context menu can target a
            // worktree that is not on screen, and a tab nobody switches to
            // reads as "the menu did nothing".
            selectedWorktree = worktree
            Task {
                guard let groupID = await workspaceCoordinator.ensureGroup(for: worktree) else {
                    return
                }
                await workspaceCoordinator.handle(
                    .requestNewTab(into: groupID), in: worktree)
            }
            return
        }
        let title = LegacyWorkspaceTab.nextShellTitle(
            existing: workspaceCoordinator.legacyTabs(for: worktree.id))
        openTab(paneId: UUID(), title: title, in: worktree)
    }

    func newShellTabInSelected() {
        guard let worktree = selectedWorktree else { return }
        newShellTab(in: worktree)
    }

    func newBrowserTab(in worktree: Worktree) {
        guard WorkspaceEngineGate.isEnabled else { return }
        selectedWorktree = worktree
        Task {
            guard let group = await workspaceCoordinator.ensureGroup(for: worktree) else { return }
            await workspaceCoordinator.requestNewTab(
                into: group, choice: .newBrowser(url: nil), in: worktree)
        }
    }

    func newBrowserTabInSelected() {
        guard let worktree = selectedWorktree else { return }
        newBrowserTab(in: worktree)
    }

    func focusBrowserAddressBar() {
        guard WorkspaceEngineGate.isEnabled,
              let worktree = selectedWorktree,
              let adapter = workspaceCoordinator.adapters[.browser] as? BrowserContentAdapter
        else { return }
        guard let layout = workspaceCoordinator.layouts[worktree.id],
              let tabID = layout.group(layout.activeGroupID)?.activeTabID,
              case .browser = layout.tab(tabID)?.content else { return }
        adapter.focusAddressBar(tabID: tabID)
    }

    /// Rimuove la tab (l'host smonta → onClose salva scrollback e pulisce i
    /// dizionari pane). When the tab list becomes empty the worktree shows
    /// the empty-state view; the user creates a new tab via ⌘T or the
    /// sidebar "+" menu.
    func closeTab(_ tabId: UUID, in worktree: Worktree) {
        if let document = markdownDocuments[tabId], document.isDirty {
            guard resolveDirtyClose(fileURL: document.fileURL, save: document.save) else { return }
        }
        if let document = codeDocuments[tabId], document.isDirty {
            guard resolveDirtyClose(fileURL: document.fileURL, save: document.save) else { return }
        }
        if WorkspaceEngineGate.isEnabled {
            let tabID = WorkspaceTabID(tabId)
            // Documents live in the adapter now, so the dirty-close prompt has
            // to ask it — otherwise closing a modified file drops the buffer
            // without a word.
            if let adapter = workspaceCoordinator.adapters[.document] as? DocumentContentAdapter,
               adapter.isDirty(tabID: tabID) {
                let dirty: (url: URL, save: () throws -> Void)? =
                    adapter.markdownDocument(for: tabID).map { ($0.fileURL, $0.save) }
                    ?? adapter.codeDocument(for: tabID).map { ($0.fileURL, $0.save) }
                if let dirty,
                   !resolveDirtyClose(fileURL: dirty.url, save: dirty.save) { return }
            }
            // The coordinator owns teardown: closeTab runs the content
            // adapter's close, which reaches teardownChatController through
            // ChatContentAdapter.releaseContent.
            Task { await workspaceCoordinator.closeTab(tabID, in: worktree) }
            return
        }
        var list = workspaceCoordinator.legacyTabs(for: worktree.id)
        guard !list.isEmpty else { return }
        if let closing = list.first(where: { $0.id == tabId }) {
            deleteAgentSessionRefs(contentIDs: closing.leafIds.map(TerminalContentID.init))
        }
        list.removeAll { $0.id == tabId }
        teardownDocument(tabId: tabId)
        teardownChatController(tabId: tabId)
        // Allow empty tab list — the worktree can have zero tabs.
        // The user creates a new tab via ⌘T or the sidebar "+" menu.
        workspaceCoordinator.setLegacyTabs(list, for: worktree.id)
        if !list.contains(where: {
            $0.id == workspaceCoordinator.legacyActiveTabID(for: worktree.id)
        }) {
            workspaceCoordinator.setLegacyActiveTabID(list.last?.id, for: worktree.id)
        }
        workspacePersistTabs(for: worktree.id)
    }

    func closeActiveTab() {
        guard let worktree = selectedWorktree, let tab = activeTab(for: worktree.id) else { return }
        closeTab(tab.id, in: worktree)
    }

    func activateTab(_ tabId: UUID, in worktreeId: UUID) {
        workspaceCoordinator.setLegacyActiveTabID(tabId, for: worktreeId)
        workspacePersistTabs(for: worktreeId)
    }

    func renameTab(_ tabId: UUID, in worktreeId: UUID, to title: String) {
        if WorkspaceEngineGate.isEnabled {
            let trimmed = title.trimmingCharacters(in: .whitespaces)
            guard !trimmed.isEmpty, let worktree = worktree(byId: worktreeId) else { return }
            Task {
                await workspaceCoordinator.renameTab(
                    WorkspaceTabID(tabId), title: trimmed, isAutoNamed: false, in: worktree)
            }
            return
        }
        var tabs = workspaceCoordinator.legacyTabs(for: worktreeId)
        guard let idx = tabs.firstIndex(where: { $0.id == tabId }) else { return }
        let trimmed = title.trimmingCharacters(in: .whitespaces)
        guard !trimmed.isEmpty else { return }
        tabs[idx].title = trimmed
        tabs[idx].titleIsAutoNamed = false
        workspaceCoordinator.setLegacyTabs(tabs, for: worktreeId)
        workspacePersistTabs(for: worktreeId)
    }

    /// Applica un titolo generato dall'auto-naming (Task 9). A differenza di
    /// `renameTab`, non tocca `titleIsAutoNamed`: resta eleggibile per il
    /// prossimo pass finché l'utente non rinomina manualmente.
    func applyAutoTitle(_ tabId: UUID, in worktreeId: UUID, title: String) {
        let trimmedTitle = title.trimmingCharacters(in: .whitespaces)
        if WorkspaceEngineGate.isEnabled {
            guard !trimmedTitle.isEmpty,
                  let worktree = worktree(byId: worktreeId),
                  let tab = workspaceCoordinator.layouts[worktreeId]?
                    .tab(WorkspaceTabID(tabId)),
                  tab.titleIsAutoNamed
            else { return }
            // The history row outlives the tab, so the title has to live on
            // the session too — no extra summarizer call, same string.
            if case .chat(let contentID) = tab.content {
                try? chatStore?.setTitle(trimmedTitle, sessionId: contentID.rawValue)
            }
            Task {
                await workspaceCoordinator.renameTab(
                    tab.id, title: trimmedTitle, isAutoNamed: true, in: worktree)
            }
            return
        }
        var tabs = workspaceCoordinator.legacyTabs(for: worktreeId)
        guard let idx = tabs.firstIndex(where: { $0.id == tabId }) else { return }
        let trimmed = title.trimmingCharacters(in: .whitespaces)
        guard !trimmed.isEmpty, tabs[idx].titleIsAutoNamed == true else { return }
        tabs[idx].title = trimmed
        // The history row outlives the tab, so the title has to live on the
        // session too — no extra summarizer call, same string.
        if let sessionId = tabs[idx].chatSessionId {
            try? chatStore?.setTitle(trimmed, sessionId: sessionId)
        }
        workspaceCoordinator.setLegacyTabs(tabs, for: worktreeId)
        workspacePersistTabs(for: worktreeId)
    }
    /// Riordina la tab prima di `targetId` (nil = in coda). Usato dal drag
    /// & drop di tab bar e sidebar; l'ordine persiste nello snapshot già
    /// serializzato da persistTabs.
    func workspaceMoveTab(_ tabId: UUID, before targetId: UUID?, in worktreeId: UUID) {
        guard previewMoveTab(tabId, before: targetId, in: worktreeId) else { return }
        workspacePersistTabs(for: worktreeId)
    }

    /// Applies the reorder to the live list without persisting, so the rows
    /// can shift under the pointer while the drag is still in flight.
    /// Returns whether the order actually changed.
    @discardableResult
    func previewMoveTab(_ tabId: UUID, before targetId: UUID?, in worktreeId: UUID) -> Bool {
        let list = workspaceCoordinator.legacyTabs(for: worktreeId)
        let moved = TabOrdering.moving(list, id: tabId, before: targetId)
        guard moved.map(\.id) != list.map(\.id) else { return false }
        workspaceCoordinator.setLegacyTabs(moved, for: worktreeId)
        return true
    }

    /// Reorders a project in the sidebar and persists the new manual order.
    @discardableResult
    func previewMoveProject(_ projectId: UUID, before targetId: UUID?) -> Bool {
        let moved = ManualOrder.moving(projects, id: projectId, before: targetId)
        guard moved.map(\.id) != projects.map(\.id) else { return false }
        projects = moved
        return true
    }

    func persistProjectOrder() {
        guard let store else { return }
        let order = projects.map(\.id)
        Task { try? await store.saveProjectOrder(order) }
    }

    /// Reorders a worktree inside its project. The stored array is the manual
    /// order; the sidebar floats urgent rows on top of it at render time.
    @discardableResult
    func previewMoveWorktree(_ worktreeId: UUID, before targetId: UUID?, in projectId: UUID) -> Bool {
        guard let list = worktrees[projectId] else { return false }
        let moved = ManualOrder.moving(list, id: worktreeId, before: targetId)
        guard moved.map(\.id) != list.map(\.id) else { return false }
        worktrees[projectId] = moved
        return true
    }

    /// Hover during a drag: moves the dragged row before `targetId` in the
    /// live model, so the list under the pointer already shows the outcome.
    /// A drag from another list (or onto itself) is ignored.
    func previewDrag(before targetId: UUID, in scope: ReorderScope) {
        guard let dragged = draggingRow, dragged.scope == scope, dragged.id != targetId else { return }
        switch scope {
        case .projects:
            previewMoveProject(dragged.id, before: targetId)
        case .worktrees(let projectId):
            previewMoveWorktree(dragged.id, before: targetId, in: projectId)
        case .tabs(let worktreeId):
            previewMoveTab(dragged.id, before: targetId, in: worktreeId)
        }
    }

    /// Hover past the last row of a list (the empty space in the tab bar):
    /// the dragged row previews at the end.
    func previewDragToEnd(in scope: ReorderScope) {
        guard let dragged = draggingRow, dragged.scope == scope else { return }
        switch scope {
        case .projects:
            previewMoveProject(dragged.id, before: nil)
        case .worktrees(let projectId):
            previewMoveWorktree(dragged.id, before: nil, in: projectId)
        case .tabs(let worktreeId):
            previewMoveTab(dragged.id, before: nil, in: worktreeId)
        }
    }

    /// Drop: the order is already applied, so this only writes it down.
    func endRowDrag() {
        defer { draggingRow = nil }
        guard let dragged = draggingRow else { return }
        switch dragged.scope {
        case .projects: persistProjectOrder()
        case .worktrees(let projectId): persistWorktreeOrder(for: projectId)
        case .tabs(let worktreeId): workspacePersistTabs(for: worktreeId)
        }
    }

    func persistWorktreeOrder(for projectId: UUID) {
        guard let store else { return }
        let order = (worktrees[projectId] ?? []).map(\.id)
        Task { try? await store.saveWorktreeOrder(projectId: projectId, order: order) }
    }
    /// Chiude tutte le tab del worktree tranne quella indicata. Passa dal
    /// percorso closeTab singolo: le conferme markdown-dirty appaiono una
    /// alla volta e un annulla lascia la tab aperta.
    func closeOtherTabs(_ tabId: UUID, in worktree: Worktree) {
        let ids = workspaceCoordinator.legacyTabs(for: worktree.id)
            .map(\.id).filter { $0 != tabId }
        for id in ids { closeTab(id, in: worktree) }
    }

    /// Chiude le tab a destra di quella indicata (stesso percorso singolo).
    func workspaceCloseTabsToRight(of tabId: UUID, in worktree: Worktree) {
        let list = workspaceCoordinator.legacyTabs(for: worktree.id)
        guard
              let index = list.firstIndex(where: { $0.id == tabId }) else { return }
        for id in list.suffix(from: index + 1).map(\.id) {
            closeTab(id, in: worktree)
        }
    }

    /// ⌘n: attiva la tab n del worktree selezionato (9 = ultima). Out of
    /// range = no-op.
    func workspaceSelectTab(number: Int) {
        guard let worktree = selectedWorktree,
              let index = TabOrdering.selectionIndex(
                  number: number,
                  count: workspaceCoordinator.legacyTabs(for: worktree.id).count
              )
        else { return }
        let list = workspaceCoordinator.legacyTabs(for: worktree.id)
        activateTab(list[index].id, in: worktree.id)
    }

    /// ⌃Tab / ⌃⇧Tab: cicla le tab del worktree selezionato con wrap-around.
    func workspaceCycleTab(forward: Bool) {
        guard let worktree = selectedWorktree else { return }
        let list = workspaceCoordinator.legacyTabs(for: worktree.id)
        let current = list.firstIndex { $0.id == activeTab(for: worktree.id)?.id }
        guard let index = TabOrdering.cycledIndex(
            current: current, forward: forward, count: list.count
        ) else { return }
        activateTab(list[index].id, in: worktree.id)
    }

    func workspaceSplitCurrent(_ axis: SplitAxis) {
        guard let worktree = selectedWorktree,
              let tab = activeTab(for: worktree.id),
              let target = tab.leafIds.first else { return }
        workspaceSplit(paneId: target, axis: axis)
    }

    /// Tab (se esiste) che contiene paneId in uno qualsiasi dei worktree aperti.
    func workspaceTabContaining(paneId: UUID) -> (worktree: Worktree, tab: LegacyWorkspaceTab, index: Int)? {
        for worktree in worktrees.values.flatMap({ $0 }) {
            let tabs = workspaceCoordinator.legacyTabs(for: worktree.id)
            if let idx = tabs.firstIndex(where: { $0.leafIds.contains(paneId) }) {
                return (worktree, tabs[idx], idx)
            }
        }
        return nil
    }

    func universalControlTarget(
        paneId: UUID
    ) -> (worktree: Worktree, tab: WorkspaceTab)? {
        for worktree in worktrees.values.flatMap({ $0 }) {
            guard let layout = workspaceCoordinator.layouts[worktree.id] else { continue }
            for tab in layout.allTabs {
                guard case .terminal(let contentID) = tab.content,
                      workspaceCoordinator.liveControlPaneId(
                          contentID: contentID, in: worktree.id) == paneId else { continue }
                return (worktree, tab)
            }
        }
        return nil
    }

    /// Splits the tab containing the given pane along the requested axis.
    @discardableResult
    func workspaceSplit(
        paneId: UUID,
        axis: SplitAxis,
        newPaneId: UUID = UUID(),
        placement: SplitPlacement = .after,
        persist: Bool = true
    ) -> Bool {
        guard let tuple = workspaceTabContaining(paneId: paneId),
              let tree = tuple.tab.terminalTree,
              tree.leafIds.contains(paneId) else { return false }
        var tabs = workspaceCoordinator.legacyTabs(for: tuple.worktree.id)
        tabs[tuple.index].content = .terminal(
            tree.splitting(leaf: paneId, axis: axis, newLeaf: newPaneId, placement: placement))
        workspaceCoordinator.setLegacyTabs(tabs, for: tuple.worktree.id)
        if persist { workspacePersistTabs(for: tuple.worktree.id) }
        return true
    }


    private func acquireControlMount(for worktreeId: UUID) -> ControlMountLease {
        if var state = controlMountStates[worktreeId] {
            state.leaseCount += 1
            controlMountStates[worktreeId] = state
        } else {
            let alreadyMounted = openWorktreeIds.contains(worktreeId)
            controlMountStates[worktreeId] = ControlMountState(
                leaseCount: 1,
                addedByControl: !alreadyMounted,
                keepMounted: false
            )
            if !alreadyMounted { openWorktreeIds.append(worktreeId) }
        }
        return ControlMountLease(worktreeId: worktreeId)
    }

    private func releaseControlMount(_ lease: ControlMountLease, success: Bool) {
        guard var state = controlMountStates[lease.worktreeId] else { return }
        state.keepMounted = state.keepMounted || success || selectedWorktree?.id == lease.worktreeId
        state.leaseCount -= 1
        guard state.leaseCount == 0 else {
            controlMountStates[lease.worktreeId] = state
            return
        }
        controlMountStates[lease.worktreeId] = nil
        if state.addedByControl && !state.keepMounted {
            openWorktreeIds.removeAll { $0 == lease.worktreeId }
        }
    }

    private func workspaceRollbackCreatedPane(_ paneId: UUID, tabId: UUID, in worktreeId: UUID) {
        var tabs = workspaceCoordinator.legacyTabs(for: worktreeId)
        guard let index = tabs.firstIndex(where: { $0.id == tabId }),
              let tree = tabs[index].terminalTree,
              tree.leafIds.contains(paneId) else { return }
        if let remaining = tree.removing(leaf: paneId) {
            tabs[index].content = .terminal(remaining)
        } else {
            tabs.remove(at: index)
            if workspaceCoordinator.legacyActiveTabID(for: worktreeId) == tabId {
                workspaceCoordinator.setLegacyActiveTabID(tabs.last?.id, for: worktreeId)
            }
        }
        workspaceCoordinator.setLegacyTabs(tabs, for: worktreeId)
        workspaceCoordinator.legacyPaneCache(for: worktreeId).prune(
            keeping: workspaceLiveLeafIds(for: worktreeId))
    }

    private func workspaceRollbackCreatedLeaf(_ paneId: UUID) {
        guard let target = workspaceTabContaining(paneId: paneId),
              let tree = target.tab.terminalTree else { return }
        if let remaining = tree.removing(leaf: paneId) {
            var tabs = workspaceCoordinator.legacyTabs(for: target.worktree.id)
            tabs[target.index].content = .terminal(remaining)
            workspaceCoordinator.setLegacyTabs(tabs, for: target.worktree.id)
        } else {
            var tabs = workspaceCoordinator.legacyTabs(for: target.worktree.id)
            tabs.remove(at: target.index)
            if workspaceCoordinator.legacyActiveTabID(for: target.worktree.id) == target.tab.id {
                workspaceCoordinator.setLegacyActiveTabID(tabs.last?.id, for: target.worktree.id)
            }
            workspaceCoordinator.setLegacyTabs(tabs, for: target.worktree.id)
        }
        workspaceCoordinator.legacyPaneCache(for: target.worktree.id).prune(
            keeping: workspaceLiveLeafIds(for: target.worktree.id)
        )
    }

    /// Chiude il pane rimuovendolo dallo split; il pane vicino riprende lo
    /// spazio. Il teardown (stop PTY, unregister proxy) avviene tramite
    /// SplitViewRenderer.pruneCache + PtyTerminalPane.onDisappear, la stessa
    /// via già usata quando si cambia tab — nessuna nuova logica qui.
    func workspaceClosePane(paneId: UUID) {
        guard let tuple = workspaceTabContaining(paneId: paneId),
              let tree = tuple.tab.terminalTree,
              let newTree = tree.removing(leaf: paneId) else { return }
        deleteAgentSessionRefs(contentIDs: [TerminalContentID(paneId)])
        var tabs = workspaceCoordinator.legacyTabs(for: tuple.worktree.id)
        tabs[tuple.index].content = .terminal(newTree)
        workspaceCoordinator.setLegacyTabs(tabs, for: tuple.worktree.id)
        workspacePersistTabs(for: tuple.worktree.id)
    }

    // MARK: - Pane controller caches (una per worktree)

    func workspacePaneCache(for worktreeId: UUID) -> TerminalPaneCache {
        workspaceCoordinator.legacyPaneCache(for: worktreeId)
    }

    /// Tutti i pane vivi nei tab del worktree — set di pruning per gli host.
    func workspaceLiveLeafIds(for worktreeId: UUID) -> Set<UUID> {
        Set(workspaceCoordinator.legacyTabs(for: worktreeId).flatMap(\.leafIds))
    }

    /// True se il pane può essere affiancato al terminale corrente: stesso
    /// worktree del tab attivo, non già nel tab attivo, tab attivo terminale.
    func workspaceCanAdoptPane(_ paneId: UUID) -> Bool {
        guard let worktree = selectedWorktree,
              let source = workspaceTabContaining(paneId: paneId),
              source.worktree.id == worktree.id,
              let active = activeTab(for: worktree.id),
              active.terminalTree != nil,
              !active.leafIds.contains(paneId) else { return false }
        return true
    }

    /// Sposta il pane in split orizzontale accanto al primo leaf del tab
    /// attivo. Il tab sorgente svuotato viene rimosso direttamente (niente
    /// closeTab: il pane è vivo altrove, i suoi session ref non vanno toccati).
    func adoptPane(_ paneId: UUID) {
        guard workspaceCanAdoptPane(paneId),
              let worktree = selectedWorktree,
              let source = workspaceTabContaining(paneId: paneId),
              let active = activeTab(for: worktree.id),
              let destTree = active.terminalTree,
              let anchor = destTree.leafIds.first,
              let sourceTree = source.tab.terminalTree else { return }

        if let remaining = sourceTree.removing(leaf: paneId) {
            var tabs = workspaceCoordinator.legacyTabs(for: worktree.id)
            tabs[source.index].content = .terminal(remaining)
            workspaceCoordinator.setLegacyTabs(tabs, for: worktree.id)
        } else {
            var tabs = workspaceCoordinator.legacyTabs(for: worktree.id)
            tabs.remove(at: source.index)
            workspaceCoordinator.setLegacyTabs(tabs, for: worktree.id)
        }
        var tabs = workspaceCoordinator.legacyTabs(for: worktree.id)
        guard let destIndex = tabs.firstIndex(where: { $0.id == active.id }) else { return }
        tabs[destIndex].content =
            .terminal(destTree.splitting(leaf: anchor, axis: .horizontal, newLeaf: paneId))
        workspaceCoordinator.setLegacyTabs(tabs, for: worktree.id)
        workspacePersistTabs(for: worktree.id)
    }

    /// Chiude un terminale: se è l'unico pane del tab chiude l'intera tab.
    func closeTerminal(paneId: UUID) {
        guard let target = workspaceTabContaining(paneId: paneId) else { return }
        if target.tab.leafIds.count <= 1 {
            closeTab(target.tab.id, in: target.worktree)
        } else {
            workspaceClosePane(paneId: paneId)
        }
    }

    /// Fire-and-forget: la persistenza tab non deve bloccare la UI; un
    /// fallimento lascia solo il layout non salvato (rimedio: prossima mutazione).
    private func removePaneForControl(_ paneId: UUID) -> (
        worktreeId: UUID, tabs: [LegacyWorkspaceTab], activeTabId: UUID?
    )? {
        guard let target = workspaceTabContaining(paneId: paneId),
              let tree = target.tab.terminalTree else { return nil }
        var currentTabs = workspaceCoordinator.legacyTabs(for: target.worktree.id)
        let previousActiveTabId = workspaceCoordinator.legacyActiveTabID(
            for: target.worktree.id)
        if target.tab.leafIds.count == 1 {
            currentTabs.remove(at: target.index)
            if previousActiveTabId == target.tab.id {
                workspaceCoordinator.setLegacyActiveTabID(
                    currentTabs.last?.id, for: target.worktree.id)
            }
        } else if let remaining = tree.removing(leaf: paneId) {
            currentTabs[target.index].content = .terminal(remaining)
        } else {
            return nil
        }
        workspaceCoordinator.setLegacyTabs(currentTabs, for: target.worktree.id)
        return (target.worktree.id, currentTabs, previousActiveTabId)
    }

    private func persistControlTabs(for worktreeId: UUID) async throws {
        let list = workspaceCoordinator.legacyTabs(for: worktreeId)
        let active = workspaceCoordinator.legacyActiveTabID(for: worktreeId)
        let previous = tabPersistenceTasks[worktreeId]
        let persister = controlTabPersister
        let store = store
        let operation = Task { @MainActor () -> Result<Void, Error> in
            _ = await previous?.value
            do {
                if let persister {
                    try await persister(worktreeId, list, active)
                } else if let store {
                    try await store.saveTabs(
                        worktreeId: worktreeId, tabs: list, activeTabId: active
                    )
                }
                return .success(())
            } catch {
                return .failure(error)
            }
        }
        tabPersistenceTasks[worktreeId] = Task { _ = await operation.value }
        try await operation.value.get()
    }

    private func serializeControlLifecycle(
        for worktreeId: UUID,
        _ operation: @escaping @MainActor () async -> ControlResponse
    ) async -> ControlResponse {
        let previous = controlLifecycleTasks[worktreeId]
        let task = Task { @MainActor in
            _ = await previous?.value
            return await operation()
        }
        controlLifecycleTasks[worktreeId] = task
        return await task.value
    }

    func workspacePersistTabs(for worktreeId: UUID) {
        workspaceCoordinator.legacyPaneCache(for: worktreeId).prune(
            keeping: workspaceLiveLeafIds(for: worktreeId))
        guard let store else { return }
        let list = workspaceCoordinator.legacyTabs(for: worktreeId)
        let active = workspaceCoordinator.legacyActiveTabID(for: worktreeId)
        let previous = tabPersistenceTasks[worktreeId]
        tabPersistenceTasks[worktreeId] = Task {
            _ = await previous?.value
            try? await store.saveTabs(
                worktreeId: worktreeId, tabs: list, activeTabId: active
            )
        }
    }

    // MARK: - Scrollback persistence

    func saveScrollback(worktreeId: UUID, paneId: UUID, data: Data) async {
        guard !data.isEmpty, let persistenceCoordinator else { return }
        await persistenceCoordinator.enqueueScrollback(
            worktreeId: worktreeId, paneId: paneId, data: data)
    }

    func loadScrollback(paneId: UUID) -> Data? {
        guard let database else { return nil }
        return try? database.read { db in
            try PaneScrollbackRecord.fetchOne(db, key: paneId.uuidString)?.data
        }
    }

    /// worktreeId -> id dei pane terminale attualmente live, risolti tramite
    /// il motore attivo. Sotto il motore universale un WorkspaceTab è già
    /// una singola foglia, quindi ogni tab terminale risolve al più un pane
    /// (via liveControlPaneId, mai cache: il generation id cambia a ogni
    /// relaunch/retry).
    private func liveScrollbackPaneIds() -> [UUID: [UUID]] {
        guard WorkspaceEngineGate.isEnabled else {
            return workspaceCoordinator.legacyStore.tabs.mapValues { $0.flatMap(\.leafIds) }
        }
        var result: [UUID: [UUID]] = [:]
        for (worktreeId, layout) in workspaceCoordinator.layouts {
            let ids = layout.allTabs.compactMap { tab -> UUID? in
                guard case .terminal(let contentID) = tab.content else { return nil }
                return workspaceCoordinator.liveControlPaneId(contentID: contentID, in: worktreeId)
            }
            if !ids.isEmpty { result[worktreeId] = ids }
        }
        return result
    }

    /// Snapshot di tutti i pane vivi verso il DB. Chiamato al quit
    /// (AppDelegate). I pane non registrati (già chiusi) tornano nil da
    /// snapshot e sono saltati; saveScrollback salta i blob vuoti.
    func flushLiveScrollback() async {
        for target in scrollbackFlushTargets(paneIds: liveScrollbackPaneIds()) {
            if let data = await paneRegistry.snapshot(paneId: target.paneId) {
                await saveScrollback(
                    worktreeId: target.worktreeId, paneId: target.paneId, data: data)
            }
        }
        for controller in chatControllers.values {
            controller.persist()
        }
        await withTaskGroup(of: Void.self) { group in
            for controller in chatControllers.values {
                group.addTask {
                    await controller.drainPendingPersistence()
                }
            }
        }
        await persistenceCoordinator?.flushAll()
    }

    /// Upserts the agent-native session ref for a pane. Fire-and-forget like
    /// persistTabs: a failed write only loses one restore opportunity.
    private func saveAgentSessionRef(paneId: UUID, sessionRef: String) {
        guard let store,
              let agentId = agentActivity.agentId(paneId: paneId),
              let key = agentSessionKey(paneId: paneId) else {
            sessionRestoreLogger.warning("session ref for unknown pane \(paneId.uuidString, privacy: .public) dropped")
            return
        }
        Task {
            try? await store.saveAgentSessionRef(
                contentID: key.contentID, worktreeId: key.worktreeId,
                agentId: agentId, sessionRef: sessionRef
            )
        }
    }

    /// The stable key a live pane's session ref is stored under.
    ///
    /// Under the universal engine the live pane id is a ResourceGenerationID,
    /// minted anew at every spawn, so it has to be resolved to the tab's
    /// TerminalContentID. Legacy pane ids are themselves persisted in the tab
    /// tree, so they are already stable and are wrapped as-is — both engines
    /// then share one key space.
    private func agentSessionKey(
        paneId: UUID
    ) -> (worktreeId: UUID, contentID: TerminalContentID)? {
        if WorkspaceEngineGate.isEnabled {
            guard let target = universalControlTarget(paneId: paneId),
                  case .terminal(let contentID) = target.tab.content else { return nil }
            return (target.worktree.id, contentID)
        }
        guard let tuple = workspaceTabContaining(paneId: paneId) else { return nil }
        return (tuple.worktree.id, TerminalContentID(paneId))
    }

    /// Deletes stored session refs for terminals the user closed explicitly —
    /// a deliberately ended pane must not resurrect its agent on relaunch.
    /// Takes content ids, not pane ids: callers close the tab first, after
    /// which a live pane id can no longer be resolved.
    private func deleteAgentSessionRefs<S: Sequence<TerminalContentID> & Sendable>(
        contentIDs: S
    ) {
        guard let store else { return }
        Task {
            for id in contentIDs { try? await store.deleteAgentSessionRef(contentID: id) }
        }
    }

    // MARK: - Pane commands & agent spawning

    var paneCommands: [UUID: String] = [:]

    /// Ultimo titolo PTY riportato da ogni pane (solo in memoria: al riavvio
    /// la shell lo rigenera). Alimenta le etichette dei nodi pane in sidebar.
    var paneTitles: [UUID: String] = [:]

    /// Layer-D recursive process snapshot per pane, feeding the Agents
    /// panel's terminal subagent tree. In-memory only.
    var paneProcessTrees: [UUID: [ProcessNode]] = [:]

    // MARK: - File editor

    /// Documenti aperti, keyed su tab.id. Vivono qui (non nella view) perché
    /// le view SwiftUI muoiono al cambio tab e perderebbero il buffer.
    var markdownDocuments: [UUID: MarkdownDocument] = [:]
    var codeDocuments: [UUID: CodeDocument] = [:]
    var chatControllers: [UUID: ChatController] = [:]
    var chatStore: ChatSessionStore?
    /// Tiller-managed ACP agent installs (Settings → Agents).
    let agentInstallStore: AgentInstallStore
    let agentCenter: AcpAgentCenter
    private var autoNamingThrottle: [UUID: AutoNamingThrottle] = [:]


    /// Select a worktree tab from the Agents panel. No-op when the tab is gone.
    func focusTab(tabId: UUID, in worktree: Worktree) {
        guard workspaceCoordinator.legacyTabs(for: worktree.id)
            .contains(where: { $0.id == tabId }) else { return }
        selectedWorktree = worktree
        workspaceCoordinator.setLegacyActiveTabID(tabId, for: worktree.id)
        workspacePersistTabs(for: worktree.id)
    }

    /// Focuses a tab from the Activity panel, handling both the workspace-engine
    /// and legacy paths. Selecting `selectedWorktree` also mounts the worktree
    /// and expands it in the sidebar.
    func focusActivityRow(tabId: UUID, worktreeId: UUID) {
        guard let worktree = worktree(byId: worktreeId) else { return }
        selectedWorktree = worktree
        if WorkspaceEngineGate.isEnabled {
            workspaceCoordinator.activateTabDirectly(WorkspaceTabID(tabId), in: worktreeId)
        } else {
            workspaceCoordinator.setLegacyActiveTabID(tabId, for: worktreeId)
            workspacePersistTabs(for: worktreeId)
        }
    }

    /// History rows for the worktree's chat menu, newest first.
    func chatHistory(for worktree: Worktree) -> [ChatHistoryRow] {
        guard let chatStore else { return [] }
        let sessions = (try? chatStore.sessions(worktreeId: worktree.id.uuidString)) ?? []
        return ChatHistoryRows.make(
            sessions: sessions,
            displayName: { [agentCenter] id in agentCenter.displayName(for: id) },
            timeFormatter: { date in
                date.formatted(date: .abbreviated, time: .shortened)
            })
    }

    /// Applies the retention limit once per launch. Missing preference means
    /// the default; 0 means unlimited and prune() returns immediately.
    private func pruneChatHistory() {
        guard let chatStore else { return }
        let stored = defaults.object(forKey: AppSettings.chatHistoryRetentionKey) as? Int
        let keeping = stored ?? AppSettings.defaultChatHistoryRetention
        for worktree in worktrees.values.flatMap({ $0 }) {
            try? chatStore.prune(worktreeId: worktree.id.uuidString, keeping: keeping)
        }
    }

    /// Focus the tab already showing this conversation, or open it in a new
    /// detached tab.
    func openExistingChatSession(sessionId: String, in worktree: Worktree) {
        if WorkspaceEngineGate.isEnabled {
            selectedWorktree = worktree
            if let existing = universalChatTab(sessionId: sessionId, in: worktree.id) {
                Task { await workspaceCoordinator.handle(.activateTab(existing.id), in: worktree) }
                return
            }
            guard chatSession(id: sessionId) != nil,
                  let group = workspaceCoordinator.activeOrFirstGroup(for: worktree.id)
            else { return }
            Task {
                await workspaceCoordinator.requestNewTab(
                    into: group, choice: .resumeChat(ChatContentID(sessionId)), in: worktree)
            }
            return
        }
        if let existing = workspaceCoordinator.legacyTabs(for: worktree.id).first(where: {
            $0.chatSessionId == sessionId
        }) {
            focusTab(tabId: existing.id, in: worktree)
            return
        }
        guard let chatStore, let record = try? chatStore.session(id: sessionId)
        else { return }
        let agentId = AgentIdMigration.canonical(record.agentId)
        let trimmed = record.title?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
        let tab = LegacyWorkspaceTab(
            id: UUID(),
            title: trimmed.isEmpty ? "Chat" : trimmed,
            content: .chat(agentId: agentId, sessionId: sessionId))
        selectedWorktree = worktree
        workspaceCoordinator.appendLegacyTab(tab, to: worktree.id, activate: true)
        // Identity without status: a detached chat has no process to report on.
        agentActivity.registerAgentId(paneId: tab.id, agentId: agentId)
        workspacePersistTabs(for: worktree.id)
        _ = chatController(for: tab, in: worktree, startDetached: true)
    }

    /// The open tab showing this conversation, if any. A chat tab's content
    /// id is its session id, so no side table is needed.
    func universalChatTab(sessionId: String, in worktreeId: UUID) -> WorkspaceTab? {
        workspaceCoordinator.layouts[worktreeId]?.allTabs.first {
            if case .chat(let id) = $0.content { return id.rawValue == sessionId }
            return false
        }
    }

    /// Deletes a conversation and closes the tab rendering it, if any.
    func deleteChatSession(sessionId: String, in worktree: Worktree) {
        if WorkspaceEngineGate.isEnabled {
            if let open = universalChatTab(sessionId: sessionId, in: worktree.id) {
                Task { await workspaceCoordinator.closeTab(open.id, in: worktree) }
            }
            try? chatStore?.deleteSession(id: sessionId)
            return
        }
        if let open = workspaceCoordinator.legacyTabs(for: worktree.id).first(where: {
            $0.chatSessionId == sessionId
        }) {
            closeTab(open.id, in: worktree)
        }
        try? chatStore?.deleteSession(id: sessionId)
    }

    @discardableResult
    func openChatTab(agentId: String, in worktree: Worktree) -> LegacyWorkspaceTab? {
        rememberChatAgent(agentId)
        if WorkspaceEngineGate.isEnabled {
            selectedWorktree = worktree
            // The session row (and therefore the tab's identity) is minted by
            // ChatContentAdapter.makeSession during preparation.
            Task {
                guard let group = await workspaceCoordinator.ensureGroup(for: worktree) else {
                    return
                }
                await workspaceCoordinator.requestNewTab(
                    into: group, choice: .newChat(agentID: agentId), in: worktree)
            }
            return nil
        }
        // The session row exists from the start so the tab knows which
        // conversation it owns even before the first turn is persisted.
        let sessionId = try? chatStore?.createSession(
            worktreeId: worktree.id.uuidString, agentId: agentId).id
        let tab = LegacyWorkspaceTab(id: UUID(), title: "Chat",
                               content: .chat(agentId: agentId, sessionId: sessionId))
        selectedWorktree = worktree
        workspaceCoordinator.appendLegacyTab(tab, to: worktree.id, activate: true)
        agentActivity.agentSpawned(paneId: tab.id, agentId: agentId, now: Date())
        workspacePersistTabs(for: worktree.id)
        _ = chatController(for: tab, in: worktree, startNewConversation: true)
        return tab
    }

    /// Called by ChatController wiring whenever a chat connects or switches.
    func rememberChatAgent(_ agentId: String) {
        defaults.set(agentId, forKey: "chat.lastAgentId")
    }

    /// Lazily builds the controller for a (restored) chat tab, mirroring
    /// markdownDocument(for:).
    func chatController(for tab: LegacyWorkspaceTab, in worktree: Worktree,
                     startNewConversation: Bool = false,
                     startDetached: Bool = false) -> ChatController? {
        guard let agentId = tab.chatAgentId else { return nil }
        if let controller = chatControllers[tab.id] { return controller }
        return makeChatController(
            tabId: tab.id, agentId: agentId, sessionId: tab.chatSessionId, in: worktree,
            startNewConversation: startNewConversation, startDetached: startDetached)
    }

    /// Universal-engine sibling of the overload above. The tab carries only a
    /// ChatContentID, which *is* the session id, so the agent comes from the
    /// session row — the workspace model never learns what an agent is.
    func chatController(for tab: WorkspaceTab, in worktree: Worktree,
                        startNewConversation: Bool = false,
                        startDetached: Bool = true) -> ChatController? {
        guard case .chat(let contentID) = tab.content else { return nil }
        let tabId = tab.id.rawValue
        if let controller = chatControllers[tabId] { return controller }
        guard let record = chatSession(id: contentID.rawValue) else { return nil }
        let agentId = AgentIdMigration.canonical(record.agentId)
        // Mirrors the legacy split: a fresh chat spawns, a resumed one only
        // declares identity — a detached chat has no process to report on.
        if startNewConversation {
            agentActivity.agentSpawned(paneId: tabId, agentId: agentId, now: Date())
        } else {
            agentActivity.registerAgentId(paneId: tabId, agentId: agentId)
        }
        return makeChatController(
            tabId: tabId, agentId: agentId,
            sessionId: contentID.rawValue, in: worktree,
            startNewConversation: startNewConversation, startDetached: startDetached)
    }

    func chatSession(id: String) -> ChatSessionRecord? {
        guard let chatStore else { return nil }
        return try? chatStore.session(id: id)
    }

    /// Single construction site for both tab models: two hand-copied
    /// ChatController(...) calls would drift.
    private func makeChatController(tabId: UUID, agentId: String, sessionId: String?,
                                    in worktree: Worktree,
                                    startNewConversation: Bool,
                                    startDetached: Bool) -> ChatController {
        let controller = ChatController(
            tabId: tabId, agentId: agentId, worktreeId: worktree.id,
            worktreePath: worktree.path, store: chatStore,
            installStore: agentInstallStore,
            sessionId: sessionId,
            startDetached: startDetached,
            persistenceCoordinator: persistenceCoordinator,
            startNewConversation: startNewConversation)
        controller.onStatusChange = { [weak self] status in
            guard let self else { return }
            let transition = self.agentActivity.notify(
                paneId: tabId, status: status, now: Date())
            self.notifyTransition(paneId: tabId,
                                  from: transition.old, to: transition.new)
        }
        chatControllers[tabId] = controller
        return controller
    }

    func paneClosed(paneId: UUID) {
        processScanCoordinator.paneClosed(paneId: paneId)
        agentActivity.paneClosed(paneId: paneId)
        paneProcessTrees[paneId] = nil
    }

    func teardownChatController(tabId: UUID) {
        guard let controller = chatControllers[tabId] else { return }
        chatControllers[tabId] = nil
        paneClosed(paneId: tabId)
        // A chat closed before its first turn leaves a row nothing can show.
        if let sessionId = controller.sessionId {
            try? chatStore?.deleteIfEmpty(sessionId: sessionId)
        }
        Task { await controller.stop() }
    }

    /// Best-effort transcript flush on app quit.
    func flushChatControllers() {
        for controller in chatControllers.values {
            Task { await controller.stop() }
        }
    }

    func isDocumentDirty(tabId: UUID) -> Bool {
        markdownDocuments[tabId]?.isDirty == true || codeDocuments[tabId]?.isDirty == true
    }

    /// Funnel unico per tutti i canali di apertura (cmd+click, drop, ⌘O).
    /// Dedup per fileURL: se il file è già aperto nel worktree attiva quella tab.
    @discardableResult
    func openDocument(fileURL: URL, in worktree: Worktree) -> LegacyWorkspaceTab? {
        let url = fileURL.standardizedFileURL
        if WorkspaceEngineGate.isEnabled {
            selectedWorktree = worktree
            let editor: DocumentEditorKind = MarkdownFileLink.isMarkdown(url) ? .markdown : .code
            let documentID = DocumentID.makeCanonical(worktreeID: worktree.id, path: url.path)
            if let existing = workspaceCoordinator.layouts[worktree.id]?.allTabs.first(where: {
                if case .document(let id, _) = $0.content { return id == documentID }
                return false
            }) {
                Task { await workspaceCoordinator.handle(.activateTab(existing.id), in: worktree) }
                return nil
            }
            guard let group = workspaceCoordinator.activeOrFirstGroup(for: worktree.id) else {
                return nil
            }
            Task {
                await workspaceCoordinator.requestNewTab(
                    into: group, choice: .openFile(url, editor: editor), in: worktree)
            }
            return nil
        }
        if let existing = workspaceCoordinator.legacyTabs(for: worktree.id).first(where: {
            $0.fileURL?.standardizedFileURL == url
        }) {
            selectedWorktree = worktree
            workspaceCoordinator.setLegacyActiveTabID(existing.id, for: worktree.id)
            workspacePersistTabs(for: worktree.id)
            return existing
        }
        do {
            let tab: LegacyWorkspaceTab
            if MarkdownFileLink.isMarkdown(url) {
                let document = try MarkdownDocument(fileURL: url)
                tab = LegacyWorkspaceTab(id: UUID(), title: url.lastPathComponent,
                                   content: .markdown(fileURL: url))
                markdownDocuments[tab.id] = document
            } else {
                let document = try CodeDocument(fileURL: url)
                tab = LegacyWorkspaceTab(id: UUID(), title: url.lastPathComponent,
                                   content: .code(fileURL: url))
                codeDocuments[tab.id] = document
            }
            selectedWorktree = worktree
            workspaceCoordinator.appendLegacyTab(tab, to: worktree.id, activate: true)
            workspacePersistTabs(for: worktree.id)
            return tab
        } catch {
            lastError = "Could not open \(url.lastPathComponent): \(error.localizedDescription)"
            NSWorkspace.shared.open(url)
            return nil
        }
    }

    @discardableResult
    func openMarkdownTab(fileURL: URL, in worktree: Worktree) -> LegacyWorkspaceTab? {
        openDocument(fileURL: fileURL, in: worktree)
    }

    func openFileReference(_ raw: String, in worktree: Worktree) {
        if let fileURL = FileLink.resolve(raw, worktreePath: worktree.path) {
            openDocument(fileURL: fileURL, in: worktree)
        } else if let url = URL(string: raw) {
            NSWorkspace.shared.open(url)
        }
    }

    /// Documento della tab; lo crea al volo per le tab ripristinate da sessione.
    func markdownDocument(for tab: LegacyWorkspaceTab) -> MarkdownDocument? {
        guard let url = tab.markdownFileURL else { return nil }
        if let doc = markdownDocuments[tab.id] { return doc }
        guard let doc = try? MarkdownDocument(fileURL: url) else { return nil }
        markdownDocuments[tab.id] = doc
        return doc
    }

    func codeDocument(for tab: LegacyWorkspaceTab) -> CodeDocument? {
        guard let url = tab.codeFileURL else { return nil }
        if let document = codeDocuments[tab.id] { return document }
        guard let document = try? CodeDocument(fileURL: url) else { return nil }
        codeDocuments[tab.id] = document
        return document
    }

    /// ⌘S: salva il documento della tab attiva, se è un documento aperto.
    func saveActiveDocument() {
        guard let worktree = selectedWorktree else { return }
        if WorkspaceEngineGate.isEnabled {
            guard let layout = workspaceCoordinator.layouts[worktree.id],
                  let tabID = layout.group(layout.activeGroupID)?.activeTabID,
                  let tab = layout.tab(tabID),
                  let documentAdapter = workspaceCoordinator.adapters[.document] as? DocumentContentAdapter
            else { return }
            do {
                try documentAdapter.save(tabID: tabID)
            } catch {
                lastError = "Could not save \(tab.title): " + error.localizedDescription
            }
            return
        }
        guard let tab = activeTab(for: worktree.id) else { return }
        do {
            if let document = markdownDocuments[tab.id] {
                try document.save()
            } else if let document = codeDocuments[tab.id] {
                try document.save()
            }
        } catch {
            lastError = "Could not save \(tab.fileURL?.lastPathComponent ?? tab.title): "
                + error.localizedDescription
        }
    }

    func saveActiveMarkdownDocument() {
        saveActiveDocument()
    }

    /// Link attivato (cmd+click) in un pane del terminale: file markdown →
    /// tab editor nel worktree del pane; tutto il resto → apertura di sistema.
    func handleTerminalOpenURL(_ raw: String, in worktree: Worktree) {
        openFileReference(raw, in: worktree)
    }

    /// File > Open File… (⌘O): NSOpenPanel in the selected worktree.
    func openFilePanel() {
        guard let worktree = selectedWorktree else { return }
        let panel = NSOpenPanel()
        panel.directoryURL = URL(fileURLWithPath: worktree.path)
        panel.canChooseDirectories = false
        panel.allowsMultipleSelection = false
        guard panel.runModal() == .OK, let url = panel.url else { return }
        openDocument(fileURL: url, in: worktree)
    }

    func openMarkdownFilePanel() {
        openFilePanel()
    }

    /// Alert modale per chiusura con modifiche non salvate.
    /// true = procedere con la chiusura.
    private func resolveDirtyClose(
        fileURL: URL,
        save: () throws -> Void
    ) -> Bool {
        let alert = NSAlert()
        alert.messageText = "Save changes to \(fileURL.lastPathComponent)?"
        alert.informativeText = "Your changes will be lost if you don't save them."
        alert.addButton(withTitle: "Save")
        alert.addButton(withTitle: "Don't Save")
        alert.addButton(withTitle: "Cancel")
        switch alert.runModal() {
        case .alertFirstButtonReturn:
            do { try save(); return true }
            catch {
                lastError = "Save failed: \(error.localizedDescription)"
                return false
            }
        case .alertSecondButtonReturn:
            return true
        default:
            return false
        }
    }

    private func teardownDocument(tabId: UUID) {
        markdownDocuments[tabId]?.stopWatching()
        markdownDocuments[tabId] = nil
        codeDocuments[tabId]?.stopWatching()
        codeDocuments[tabId] = nil
    }
    func paneCommand(paneId: UUID) -> String? { paneCommands[paneId] }

    /// Config-dir env override for the currently active account of the
    /// given agent id, or `nil` for "System default" (today's behavior:
    /// whatever's globally logged into that CLI on this machine).
    private func configDirOverride(forAgentId agentId: String) -> (envKey: String, path: String)? {
        switch agentId {
        case "claude":
            guard let path = agentAccounts?.activeClaudeConfigDirPath() else { return nil }
            return (envKey: "CLAUDE_CONFIG_DIR", path: path)
        case "codex":
            guard let path = agentAccounts?.activeCodexConfigDirPath() else { return nil }
            return (envKey: "CODEX_HOME", path: path)
        default:
            return nil
        }
    }

    /// POSIX single-quote escaping for embedding the config-dir path in the
    /// shell command string built above.
    private static func shellQuote(_ s: String) -> String {
        "'" + s.replacingOccurrences(of: "'", with: "'\\''") + "'"
    }

    private var resumeAgentSessionsEnabled: Bool {
        UserDefaults.standard.object(forKey: AppSettings.resumeAgentSessionsKey) as? Bool ?? true
    }

    /// Rewires restored panes that were running an agent so they relaunch
    /// with the agent's resume command instead of a fresh shell. Refs for
    /// panes no longer in the layout, or stale on disk, are pruned.
    private func restoreAgentSessions(
        for worktree: Worktree, contentIDs: Set<TerminalContentID>
    ) async {
        guard let store else { return }
        let refs = (try? await store.agentSessionRefs(of: worktree.id)) ?? []
        let plan = AgentSessionRestorePlan.plan(refs: refs, liveContentIDs: contentIDs)
        for ref in plan.prunable {
            try? await store.deleteAgentSessionRef(contentID: ref.contentID)
        }
        for ref in plan.resumable {
            guard resumeAgentSessionsEnabled,
                  let adapter = AgentCatalog.all.first(where: { $0.id == ref.agentId })
            else { continue }
            let claudeDir = agentAccounts?.activeClaudeConfigDirPath()
                ?? NSHomeDirectory() + "/.claude"
            let codexHome = agentAccounts?.activeCodexConfigDirPath()
                ?? ProcessInfo.processInfo.environment["CODEX_HOME"]
                ?? NSHomeDirectory() + "/.codex"
            let agentId = ref.agentId
            let sessionRef = ref.sessionRef
            let worktreePath = worktree.path
            let isLikelyValid = await Task.detached {
                AgentSessionValidator.isLikelyValid(
                    agentId: agentId, sessionRef: sessionRef,
                    worktreePath: worktreePath,
                    claudeConfigDir: claudeDir, codexHome: codexHome
                )
            }.value
            guard isLikelyValid else {
                sessionRestoreLogger.warning("restore: stale session ref for terminal \(ref.contentID.rawValue.uuidString, privacy: .public) (\(agentId, privacy: .public)) — pruning")
                try? await store.deleteAgentSessionRef(contentID: ref.contentID)
                continue
            }
            if WorkspaceEngineGate.isEnabled {
                // The command cannot be built yet: it must embed the pane id,
                // which the surface only mints when the host is created. Park
                // the ref; resumeCommand(contentID:worktree:paneId:) finishes
                // the job from TerminalContentAdapter's provider.
                pendingAgentResume[ref.contentID] = ref
                continue
            }
            // Legacy: the pane id is the content id and is already stable, so
            // the command can be built and parked eagerly.
            let paneId = ref.contentID.rawValue
            guard let command = resumeCommand(ref: ref, worktree: worktree, paneId: paneId)
            else { continue }
            paneCommands[paneId] = command
        }
    }

    /// Terminals whose next spawn should resume an agent instead of opening a
    /// shell, keyed by the identity that survives relaunch.
    private var pendingAgentResume: [TerminalContentID: AgentSessionRef] = [:]

    /// Terminals whose next spawn should launch a fresh agent CLI. Same shape
    /// as a parked resume and for the same reason: the agent's hook config
    /// embeds the pane id, which only exists once the surface mints one.
    private var pendingAgentLaunch: [TerminalContentID: String] = [:]

    /// Completes a parked resume once the pane id exists: writes the agent's
    /// hook config for that pane and returns the command that resumes it.
    /// Consumed once — a relaunch of the same terminal starts a fresh shell
    /// rather than replaying a conversation the user already resumed.
    private func resumeCommand(
        contentID: TerminalContentID, worktree: Worktree, paneId: UUID
    ) -> String? {
        if let ref = pendingAgentResume.removeValue(forKey: contentID) {
            return resumeCommand(ref: ref, worktree: worktree, paneId: paneId)
        }
        guard let agentId = pendingAgentLaunch.removeValue(forKey: contentID) else { return nil }
        return launchCommand(agentId: agentId, worktree: worktree, paneId: paneId)
    }

    /// Builds a fresh agent launch for a pane the surface has just minted:
    /// writes the worktree-local hook config, records the command, and starts
    /// the activity/exit bookkeeping the badges read.
    private func launchCommand(agentId: String, worktree: Worktree, paneId: UUID) -> String? {
        guard let adapter = AgentCatalog.all.first(where: { $0.id == agentId }) else { return nil }
        let hc = tillerctlPath()
        do {
            try adapter.prepare(
                worktreePath: worktree.path,
                paneId: paneId,
                tillerctlPath: hc,
                skillMarkdown: try TillerSkillResource.markdown.get()
            )
        } catch {
            lastError = "Spawn \(adapter.displayName) failed: \(error)"
            return nil
        }
        var command = adapter.command(
            worktreePath: worktree.path, paneId: paneId, tillerctlPath: hc)
        if let override = configDirOverride(forAgentId: adapter.id) {
            command = "\(override.envKey)=\(Self.shellQuote(override.path)) \(command)"
        }
        paneCommands[paneId] = command
        agentActivity.agentSpawned(paneId: paneId, agentId: adapter.id, now: Date())
        watchExit(paneId: paneId)
        return command
    }

    private func resumeCommand(
        ref: AgentSessionRef, worktree: Worktree, paneId: UUID
    ) -> String? {
        guard let adapter = AgentCatalog.all.first(where: { $0.id == ref.agentId })
        else { return nil }
        let hc = tillerctlPath()
        do {
            try adapter.prepare(
                worktreePath: worktree.path,
                paneId: paneId,
                tillerctlPath: hc,
                skillMarkdown: try TillerSkillResource.markdown.get()
            )
        } catch {
            sessionRestoreLogger.warning("restore: prepare failed for pane \(paneId.uuidString, privacy: .public): \(String(describing: error), privacy: .public) — falling back to fresh shell")
            return nil
        }
        guard var command = adapter.resumeCommand(
            worktreePath: worktree.path, paneId: paneId,
            tillerctlPath: hc, sessionRef: ref.sessionRef
        ) else { return nil }
        if let override = configDirOverride(forAgentId: adapter.id) {
            command = "\(override.envKey)=\(Self.shellQuote(override.path)) \(command)"
        }
        agentActivity.agentSpawned(paneId: paneId, agentId: adapter.id, now: Date())
        watchExit(paneId: paneId)
        return command
    }

    func spawnAgent(_ adapter: any AgentAdapter, in worktree: Worktree) async {
        Task { await notifier.ensureAuthorization() }
        if WorkspaceEngineGate.isEnabled {
            // The launch itself is parked by the terminal adapter's
            // onAgentTerminalPrepared hook and completed once the surface
            // mints a pane id: prepare/command need that id, and on this path
            // it does not exist until the tab is rendered.
            selectedWorktree = worktree
            guard let group = await workspaceCoordinator.ensureGroup(for: worktree) else { return }
            await workspaceCoordinator.requestNewTab(
                into: group, choice: .agentTerminal(agentID: adapter.id), in: worktree)
            return
        }
        do {
            let hc = tillerctlPath()
            let paneId = UUID()
            try adapter.prepare(
                worktreePath: worktree.path,
                paneId: paneId,
                tillerctlPath: hc,
                skillMarkdown: try TillerSkillResource.markdown.get()
            )
            var command = adapter.command(worktreePath: worktree.path, paneId: paneId, tillerctlPath: hc)
            if let override = configDirOverride(forAgentId: adapter.id) {
                command = "\(override.envKey)=\(Self.shellQuote(override.path)) \(command)"
            }
            paneCommands[paneId] = command
            agentActivity.agentSpawned(paneId: paneId, agentId: adapter.id, now: Date())
            selectedWorktree = worktree
            openTab(paneId: paneId, title: adapter.displayName, in: worktree)
            watchExit(paneId: paneId)
        } catch {
            lastError = "Spawn \(adapter.displayName) failed: \(error)"
        }
    }

    /// Fallback badge driver for agents without lifecycle hooks: wait for
    /// the pane's process to exit and map the code to done/error. A nil
    /// code means the pane closed — onClose already cleared the badge.
    func watchExit(paneId: UUID) {
        Task {
            for _ in 0..<100 {
                if await PaneRegistry.shared.isRegistered(paneId: paneId) { break }
                try? await Task.sleep(for: .milliseconds(50))
            }
            guard let code = await PaneRegistry.shared.waitExit(paneId: paneId, timeoutMs: nil) else { return }
            guard let t = agentActivity.applyExitResult(paneId: paneId, exitCode: code, now: Date()) else { return }
            notifyTransition(paneId: paneId, from: t.old, to: t.new)
        }
    }
    // MARK: - Title-based activity fallback & notifications

    func handleTitleChange(paneId: UUID, title: String) {
        paneTitles[paneId] = title
        guard let t = agentActivity.handleTitleChange(paneId: paneId, title: title, now: Date()) else { return }
        notifyTransition(paneId: paneId, from: t.old, to: t.new)
    }

    func handleContentSignal(paneId: UUID, tailText: String) {
        guard let agentId = agentActivity.agentId(paneId: paneId) else {
            // Unregistered pane: probe the shell's children for an agent
            // that emits no OSC title (Codex) — Layer D identification.
            checkForegroundAgent(paneId: paneId)
            return
        }
        // For process-owned panes this reconfirms that the agent is still
        // alive; for every registered pane it refreshes the child snapshot.
        // Content output means the agent is active and its child tree may
        // have changed.
        checkForegroundAgent(paneId: paneId)
        guard let status = ScreenManifest.detect(tailText: tailText, agentId: agentId) else { return }
        guard let t = agentActivity.applyContentSignal(paneId: paneId, status: status, now: Date()) else { return }
        notifyTransition(paneId: paneId, from: t.old, to: t.new)
    }

    /// Layer D driver: resolves the pane's shell child processes off-main
    /// and registers (or clears) the pane's agent accordingly.
    private func checkForegroundAgent(paneId: UUID) {
        guard case .start(let generation) = processScanCoordinator.requestScan(
            paneId: paneId
        ) else {
            return
        }

        let scanner = foregroundProcessScanner
        Task.detached {
            let result = await scanner(paneId)
            await MainActor.run { [weak self] in
                self?.completeForegroundAgentScan(
                    paneId: paneId,
                    generation: generation,
                    result: result
                )
            }
        }
    }

    private func completeForegroundAgentScan(
        paneId: UUID,
        generation: Int,
        result: ForegroundProcessScanResult?
    ) {
        guard !processScanCoordinator.isResultStale(
            paneId: paneId,
            generation: generation
        ) else {
            _ = processScanCoordinator.finishScan(
                paneId: paneId,
                generation: generation
            )
            return
        }

        if let result {
            let resolvedTree: [ProcessNode]? = result.agentId.map {
                _ in result.processTree
            }
            let agentChanged = agentActivity.agentId(paneId: paneId) != result.agentId
            let treeChanged = paneProcessTrees[paneId] != resolvedTree
            if agentChanged || treeChanged {
                if let agentId = result.agentId {
                    agentActivity.processIdentified(
                        paneId: paneId,
                        agentId: agentId,
                        now: Date()
                    )
                    paneProcessTrees[paneId] = result.processTree
                } else {
                    agentActivity.processGone(paneId: paneId)
                    paneProcessTrees[paneId] = nil
                }
            }
        }

        if processScanCoordinator.finishScan(
            paneId: paneId,
            generation: generation
        ) {
            checkForegroundAgent(paneId: paneId)
        }
    }

    private func notifyTransition(paneId: UUID, from old: AgentStatus?, to new: AgentStatus) {
        Task { @MainActor [weak self] in
            await self?.requestAutoRename(paneId: paneId, from: old, to: new)
        }
        let visible = isSelectedWorktreeContaining(paneId: paneId)
        guard NotificationPolicy.shouldNotify(
            old: old, new: new, appActive: NSApp.isActive, paneVisible: visible
        ) else { return }
        guard let payload = buildPayload(paneId: paneId, status: new) else { return }
        notifier.post(payload)
    }

    /// Innesca un pass di auto-naming quando un turno finisce (running → done
    /// o needsInput). Copre le chat tab ACP di qualunque agente del registry e le
    /// terminal tab claude/codex; il titolo lo genera l'agente summarizer scelto in Settings, con fallback all'agente della tab.
    private func requestAutoRename(
        paneId: UUID, from old: AgentStatus?, to new: AgentStatus
    ) async {
        guard old == .running, new == .done || new == .needsInput else { return }
        guard AppSettings.autoNamingEnabled(
            defaultsValue: defaults.object(
                forKey: AppSettings.autoNamingEnabledKey
            ) as? Bool
        ) else { return }
        guard let (worktree, tab) = activityOwner(of: paneId) else { return }
        guard tab.titleIsAutoNamed,
              let tabAgentId = agentActivity.paneAgents[paneId]
        else { return }
        let catalogAgentId = AgentIdMigration.catalogId(tabAgentId)

        let source: TranscriptSource?
        switch tab.content {
        case .chat:
            source = chatControllers[tab.id].map { ChatTranscriptSource(controller: $0) }
        case .terminal:
            source = await resolveFileTranscriptSource(
                paneId: paneId, worktree: worktree, agentId: catalogAgentId
            )
        case .markdown:
            source = nil
        case .code:
            source = nil
        }
        guard let source, let text = source.recentText() else { return }

        let throttle = autoNamingThrottle[paneId] ?? AutoNamingThrottle()
        let now = Date()
        guard throttle.shouldRun(transcriptLength: text.count, now: now) else { return }
        autoNamingThrottle[paneId] = throttle.recording(
            transcriptLength: text.count, now: now
        )

        let selectedId = AppSettings.summarizerAgentId(
            defaultsValue: defaults.string(forKey: AppSettings.summarizerAgentIdKey)
        )
        let adapters = SummarizerSelection.adapters(
            selectedId: selectedId, tabAgentId: tabAgentId
        )
        let worktreePath = worktree.path
        Task { [weak self] in
            for adapter in adapters {
                guard let title = await AutoNamer.summarize(
                    transcript: text, worktreePath: worktreePath, adapter: adapter
                ) else { continue }
                await MainActor.run {
                    self?.applyAutoTitle(tab.id, in: worktree.id, title: title)
                }
                return
            }
        }
    }

    /// Sorgente file-based per le terminal-tab adapter con hook nativi.
    private func resolveFileTranscriptSource(
        paneId: UUID, worktree: Worktree, agentId: String
    ) async -> TranscriptSource? {
        guard let store,
              let key = agentSessionKey(paneId: paneId),
              let refs = try? await store.agentSessionRefs(of: worktree.id),
              let ref = refs.first(where: { $0.contentID == key.contentID })
        else { return nil }
        switch agentId {
        case "claude":
            return ClaudeTranscriptSource(
                worktreePath: worktree.path, sessionRef: ref.sessionRef
            )
        case "codex":
            return CodexTranscriptSource(sessionRef: ref.sessionRef)
        default:
            return nil
        }
    }



    // Control-socket bridges to the private notifier.
    func postUserNotification(title: String, subtitle: String?, body: String) {
        Task { await notifier.ensureAuthorization() }
        notifier.postUser(title: title, subtitle: subtitle, body: body)
    }

    func deliveredNotificationRows() async -> [[String: String]] {
        await notifier.deliveredNotifications()
    }

    func clearDeliveredNotifications() { notifier.clearDelivered() }

    private func isSelectedWorktreeContaining(paneId: UUID) -> Bool {
        guard let sel = selectedWorktree else { return false }
        return workspaceTabs(for: sel.id)
            .contains { $0.leafIds.contains(paneId) }
    }

    /// Worktree and tab that own `paneId`, resolved through the tab list of
    /// whichever engine is active. Reading the legacy store directly is not
    /// equivalent: once the universal engine owns the layout that store stays
    /// empty, and every activity lookup built on it silently resolves nothing.
    func activityOwner(of paneId: UUID) -> (worktree: Worktree, tab: LegacyWorkspaceTab)? {
        for worktree in worktrees.values.flatMap({ $0 }) {
            guard let tab = workspaceTabs(for: worktree.id).first(where: {
                $0.activityPaneIds.contains(paneId)
            }) else { continue }
            if WorkspaceEngineGate.isEnabled,
               workspaceCoordinator.layouts[worktree.id]?.tab(WorkspaceTabID(tab.id))?.content.kind
                    == .browser {
                continue
            }
            return (worktree, tab)
        }
        return nil
    }

    private func worktreeContaining(paneId: UUID) -> Worktree? {
        activityOwner(of: paneId)?.worktree
    }

    private func buildPayload(paneId: UUID, status: AgentStatus) -> NotificationPayload? {
        guard let adapterId = agentActivity.paneAgents[paneId],
              let worktree = worktreeContaining(paneId: paneId) else { return nil }
        let displayName = AgentCatalog.all.first { $0.id == adapterId }?.displayName ?? adapterId
        return agentActivity.buildPayload(
            paneId: paneId, status: status,
            agentDisplayName: displayName,
            worktreeId: worktree.id, worktreeBranch: worktree.branch,
            projectName: projects.first(where: { $0.id == worktree.projectId })?.name,
            worktreeComment: worktree.comment
        )
    }

    /// Why a focus attempt ended. `cancelled` is reported separately from
    /// `notFocused` because the two are indistinguishable from the outside
    /// otherwise: both mean "no focus happened", and a caller — or a test —
    /// then has nothing but elapsed wall-clock time to tell them apart. That
    /// made the cancellation test a race against AppKit activation cost rather
    /// than a check of cancellation semantics.
    enum FocusPaneOutcome: Equatable {
        case focused
        case notFocused
        case cancelled
    }

    @discardableResult
    private func focusPane(paneId: UUID) async -> FocusPaneOutcome {
        guard let target = workspaceTabContaining(paneId: paneId) else { return .notFocused }
        // Cancellation is checked before the prologue, not just inside the wait
        // loop: selecting a worktree and calling activateApplication() steal the
        // user's window focus, and doing that for a request whose caller has
        // already gone away is both wrong and slow — AppKit activation can cost
        // ~1s in a fresh session, which is time spent before the first
        // in-loop cancellation check could ever run.
        guard !Task.isCancelled else { return .cancelled }
        selectedWorktree = target.worktree
        activateTab(target.tab.id, in: target.worktree.id)
        activateApplication()

        let clock = ContinuousClock()
        let deadline = clock.now.advanced(by: .milliseconds(registrationTimeoutMs))
        while clock.now < deadline {
            guard !Task.isCancelled else { return .cancelled }
            if workspacePaneCache(for: target.worktree.id).focus(paneId: paneId) { return .focused }
            do {
                try await Task.sleep(for: .milliseconds(25))
            } catch {
                return .cancelled // Task.sleep only throws on cancellation
            }
        }
        guard !Task.isCancelled else { return .cancelled }
        return workspacePaneCache(for: target.worktree.id).focus(paneId: paneId)
            ? .focused : .notFocused
    }

    /// tillerctl ships next to the app binary in DEBUG dev loops; fall back to PATH.
    /// Re-points the stable shim at this build's bundled tillerctl so hook
    /// configs embedding the shim path survive app updates and dev↔installed
    /// switches. Must run before anything calls tillerctlPath().
    func refreshTillerctlShim() {
        let bundled = Bundle.main.bundleURL
            .appendingPathComponent("Contents/MacOS/tillerctl").path
        guard FileManager.default.fileExists(atPath: bundled) else { return }
        do {
            try TillerctlShim.install(target: bundled, shimPath: TillerctlShim.defaultShimPath())
        } catch {
            sessionRestoreLogger.warning("tillerctl shim install failed: \(String(describing: error), privacy: .public)")
        }
    }

    func tillerctlPath() -> String {
        // fileExists follows symlinks: a dangling shim falls through.
        let shim = TillerctlShim.defaultShimPath()
        if FileManager.default.fileExists(atPath: shim) { return shim }
        let bundled = Bundle.main.bundleURL
            .appendingPathComponent("Contents/MacOS/tillerctl").path
        if FileManager.default.fileExists(atPath: bundled) { return bundled }
        let devBuild = FileManager.default.currentDirectoryPath + "/Tiller/Packages/TillerControl/.build/debug/tillerctl"
        if FileManager.default.fileExists(atPath: devBuild) { return devBuild }
        return "tillerctl"   // PATH resolution inside the pane's login shell
    }
}

extension AgentStatus {
    var badgeColor: Color {
        switch self {
        case .running: .blue
        case .needsInput: .orange
        case .done: .green
        case .error: .red
        }
    }
}
