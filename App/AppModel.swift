import SwiftUI
import Observation
import TillerPersistence
import TillerCore
import TillerGit
import TillerTerminal
import TillerControl
import TillerAgents
import AppKit
import UserNotifications
import OSLog
import UniformTypeIdentifiers

@MainActor
@Observable
final class AppModel {
    var projects: [Project] = []
    var worktrees: [UUID: [Worktree]] = [:]

    /// Progetto selezionato (non persistito).
    var selectedProjectId: UUID? = nil

    /// Progetti espansi (non persistito).
    var expandedProjectIds: Set<UUID> = []

    var selectedWorktree: Worktree? {
        didSet {
            selectedProjectId = selectedWorktree?.projectId
            guard let worktree = selectedWorktree else { return }
            if !openWorktreeIds.contains(worktree.id) { openWorktreeIds.append(worktree.id) }
            ensureTabs(for: worktree)
        }
    }

    /// Worktrees whose terminal hosts stay mounted (PTYs alive) across
    /// selection changes. Removal unmounts the host, which fires
    /// onDisappear and terminates its panes.
    var openWorktreeIds: [UUID] = []

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

    /// Highest-priority agent status among all panes in a worktree's tree.
    /// Priority: error > needs-input > running > done. Returns nil if no agent panes.
    func statusForWorktree(_ worktree: Worktree) -> AgentStatus? {
        let paneIds = (tabs[worktree.id] ?? []).flatMap { $0.leafIds }
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
    func worstStatusTab(in worktree: Worktree) -> WorkspaceTab? {
        AttentionSort.sorted(tabs[worktree.id] ?? []) { tab in
            agentActivity.statusForWorktree(paneIds: tab.leafIds)
        }.first
    }

    let usage = UsageStore()

    /// Top-level window route and the selected settings category.
    var route: AppRoute = .workspace
    var settingsCategory: SettingsCategory = .aiProviders

    func openSettings() { route = .settings }
    func closeSettings() { route = .workspace }
    /// Adapter id of the most relevant agent pane in a worktree (same
    /// priority order as statusForWorktree), nil if no agent panes.
    func agentIdForWorktree(_ worktree: Worktree) -> String? {
        let paneIds = (tabs[worktree.id] ?? []).flatMap { $0.leafIds }
        return agentActivity.agentIdForWorktree(paneIds: paneIds)
    }

    /// Distinct agent ids currently .running in a worktree, ordered by
    /// AgentCatalog.all for stable left-to-right icon order in the
    /// worktree row's trailing running-agents badge.
    func runningAgentIds(for worktree: Worktree) -> [String] {
        let paneIds = (tabs[worktree.id] ?? []).flatMap { $0.leafIds }
        return agentActivity.runningAgentIds(paneIds: paneIds, catalogIds: AgentCatalog.all.map(\.id))
    }
    private let notifier = AgentNotifier()

    private var controlServer: ControlServer?
    var tabs: [UUID: [WorkspaceTab]] = [:]
    var activeTabId: [UUID: UUID] = [:]
    var lastError: String?

    private var store: ProjectStore?
    private var database: AppDatabase?
    var agentAccounts: AgentAccountStore?

    private let sessionRestoreLogger = Logger(subsystem: "dev.tiller", category: "session-restore")

    func bootstrap() async {
        do {
            let dir = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
                .appendingPathComponent("Tiller", isDirectory: true)
            try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
            let db = try AppDatabase(path: dir.appendingPathComponent("tiller.sqlite").path)
            let store = ProjectStore(database: db)
            self.store = store
            self.database = db
            self.agentAccounts = AgentAccountStore(database: db)
            projects = try await store.loadAll()
            for project in projects {
                let list = try await store.worktrees(of: project.id)
                worktrees[project.id] = list
                for worktree in list {
                    let loaded = try await store.loadTabs(of: worktree.id)
                    // Tab markdown il cui file è sparito tra le sessioni: scartate in silenzio.
                    let restoredTabs = loaded.tabs.filter { tab in
                        guard let url = tab.markdownFileURL else { return true }
                        return FileManager.default.fileExists(atPath: url.path)
                    }
                    guard !restoredTabs.isEmpty else { continue }
                    tabs[worktree.id] = restoredTabs
                    activeTabId[worktree.id] = loaded.activeTabId.flatMap { active in
                        restoredTabs.contains { $0.id == active } ? active : nil
                    } ?? restoredTabs.first?.id
                    await restoreAgentSessions(
                        for: worktree,
                        paneIds: Set(restoredTabs.flatMap { $0.leafIds })
                    )
                }
            }
            if AppSettings.controlSocketEnabled(
                defaultsValue: UserDefaults.standard.object(forKey: AppSettings.controlSocketEnabledKey) as? Bool,
                env: ProcessInfo.processInfo.environment
            ) {
                startControlServer()
            }
            UNUserNotificationCenter.current().delegate = notifier
            notifier.onActivatePane = { [weak self] id in self?.focusPane(paneId: id) }
            usage.start()
        } catch {
            lastError = "Database error: \(error)"
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
        case "panel.create":
            guard let worktreeIdString = request.params["worktree"],
                  let worktreeId = UUID(uuidString: worktreeIdString),
                  let worktree = worktrees.values.flatMap({ $0 }).first(where: { $0.id == worktreeId })
            else {
                return .failure(id: request.id, error: "unknown worktree")
            }
            let paneId = UUID()
            if let cmd = request.params["cmd"] { paneCommands[paneId] = cmd }
            let title = request.params["cmd"]?
                .split(separator: " ").first.map(String.init) ?? "Panel"
            selectedWorktree = worktree
            openTab(paneId: paneId, title: title, in: worktree)
            NSApp.activate(ignoringOtherApps: false)
            for _ in 0..<100 {
                if await PaneRegistry.shared.isRegistered(paneId: paneId) {
                    return .success(id: request.id, result: ["panelId": paneId.uuidString])
                }
                try? await Task.sleep(for: .milliseconds(50))
            }
            return .failure(id: request.id, error: "panel did not start (worktree not visible?)")

        case "panel.write":
            guard let paneId = UUID(uuidString: request.params["id"] ?? ""),
                  let input = request.params["input"] else {
                return .failure(id: request.id, error: "missing id/input")
            }
            let ok = await PaneRegistry.shared.write(paneId: paneId, data: Data(input.utf8))
            return ok ? .success(id: request.id) : .failure(id: request.id, error: "unknown panel")

        case "panel.read":
            guard let paneId = UUID(uuidString: request.params["id"] ?? "") else {
                return .failure(id: request.id, error: "missing id")
            }
            guard let data = await PaneRegistry.shared.snapshot(paneId: paneId) else {
                return .failure(id: request.id, error: "unknown panel")
            }
            return .success(id: request.id, result: ["output": data.base64EncodedString()])

        case "panel.wait":
            guard let paneId = UUID(uuidString: request.params["id"] ?? "") else {
                return .failure(id: request.id, error: "missing id")
            }
            let timeoutMs = request.params["timeoutMs"].flatMap(Int.init)
            guard let code = await PaneRegistry.shared.waitExit(paneId: paneId, timeoutMs: timeoutMs) else {
                return .failure(id: request.id, error: "unknown panel or timeout")
            }
            return .success(id: request.id, result: ["exitCode": String(code)])

        case "notify":
            guard let sessionId = UUID(uuidString: request.params["session"] ?? ""),
                  let status = AgentStatus(rawValue: request.params["status"] ?? "") else {
                return .failure(id: request.id, error: "missing/invalid session/status")
            }
            let t = agentActivity.notify(paneId: sessionId, status: status, now: Date())
            notifyTransition(paneId: sessionId, from: t.old, to: t.new)
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
            guard let target else { return .failure(id: request.id, error: "unknown worktree") }
            do {
                try await store.setWorktreeComment(target.id, comment: comment)
                worktrees[target.projectId] = try await store.worktrees(of: target.projectId)
                resyncSelection(projectId: target.projectId)
                return .success(id: request.id)
            } catch {
                return .failure(id: request.id, error: "persist failed: \(error)")
            }

        default:
            return .failure(id: request.id, error: "unknown method \(request.method)")
        }
    }

    func addProject(at url: URL) async {
        guard let store else { return }
        do {
            let project = try await store.addProject(name: url.lastPathComponent, rootPath: url.path)
            // The repo's main checkout is itself the first "worktree" entry.
            let branch = (try? await GitWorktrees.list(repoPath: url.path).first?.branch) ?? nil
            let main = try await store.addWorktree(
                projectId: project.id, branch: branch ?? "main", path: url.path
            )
            projects.append(project)
            worktrees[project.id] = [main]
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

    func removeProject(_ project: Project) async {
        guard let store else { return }
        let projectWorktrees = worktrees[project.id] ?? []
        do {
            // DB first (same rationale as removeWorktree): a git failure must not strand DB rows.
            try await store.removeProject(project.id)
            projects.removeAll { $0.id == project.id }
            worktrees[project.id] = nil
            for worktree in projectWorktrees {
                let tabsBeingRemoved = tabs[worktree.id] ?? []
                tabs[worktree.id] = nil
                for tab in tabsBeingRemoved { teardownMarkdownDocument(tabId: tab.id) }
                activeTabId[worktree.id] = nil
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
            let tabsBeingRemoved = tabs[worktree.id] ?? []
            tabs[worktree.id] = nil
            paneCaches[worktree.id] = nil
            for tab in tabsBeingRemoved { teardownMarkdownDocument(tabId: tab.id) }
            activeTabId[worktree.id] = nil
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
        guard tabs[worktree.id, default: []].isEmpty else { return }
        let tab = WorkspaceTab(
            id: UUID(),
            title: WorkspaceTab.nextShellTitle(existing: []),
            tree: .leaf(id: worktree.id)
        )
        tabs[worktree.id] = [tab]
        activeTabId[worktree.id] = tab.id
        persistTabs(for: worktree.id)
    }

    func activeTab(for worktreeId: UUID) -> WorkspaceTab? {
        guard let list = tabs[worktreeId], !list.isEmpty else { return nil }
        return list.first { $0.id == activeTabId[worktreeId] } ?? list.first
    }

    /// Apre una nuova tab con un singolo pane e la attiva. Punto unico usato
    /// da shell manuali, spawnAgent e panel.create.
    @discardableResult
    func openTab(paneId: UUID, title: String, in worktree: Worktree) -> WorkspaceTab {
        let tab = WorkspaceTab(id: UUID(), title: title, tree: .leaf(id: paneId))
        tabs[worktree.id, default: []].append(tab)
        activeTabId[worktree.id] = tab.id
        persistTabs(for: worktree.id)
        return tab
    }

    func newShellTab(in worktree: Worktree) {
        let title = WorkspaceTab.nextShellTitle(existing: tabs[worktree.id] ?? [])
        openTab(paneId: UUID(), title: title, in: worktree)
    }

    func newShellTabInSelected() {
        guard let worktree = selectedWorktree else { return }
        newShellTab(in: worktree)
    }

    /// Rimuove la tab (l'host smonta → onClose salva scrollback e pulisce i
    /// dizionari pane). Ultima tab: ne crea subito una fresca con paneId
    /// NUOVO — riusare worktree.id riaggancerebbe il vecchio scrollback.
    func closeTab(_ tabId: UUID, in worktree: Worktree) {
        if let doc = markdownDocuments[tabId], doc.isDirty {
            guard resolveDirtyClose(doc) else { return }
        }
        guard var list = tabs[worktree.id] else { return }
        if let closing = list.first(where: { $0.id == tabId }) {
            deleteAgentSessionRefs(paneIds: closing.leafIds)
        }
        list.removeAll { $0.id == tabId }
        teardownMarkdownDocument(tabId: tabId)
        if list.isEmpty {
            list = [WorkspaceTab(
                id: UUID(),
                title: WorkspaceTab.nextShellTitle(existing: []),
                tree: .leaf(id: UUID())
            )]
        }
        tabs[worktree.id] = list
        if !list.contains(where: { $0.id == activeTabId[worktree.id] }) {
            activeTabId[worktree.id] = list.last?.id
        }
        persistTabs(for: worktree.id)
    }

    func closeActiveTab() {
        guard let worktree = selectedWorktree, let tab = activeTab(for: worktree.id) else { return }
        closeTab(tab.id, in: worktree)
    }

    func activateTab(_ tabId: UUID, in worktreeId: UUID) {
        activeTabId[worktreeId] = tabId
        persistTabs(for: worktreeId)
    }

    func renameTab(_ tabId: UUID, in worktreeId: UUID, to title: String) {
        guard let idx = tabs[worktreeId]?.firstIndex(where: { $0.id == tabId }) else { return }
        let trimmed = title.trimmingCharacters(in: .whitespaces)
        guard !trimmed.isEmpty else { return }
        tabs[worktreeId]?[idx].title = trimmed
        persistTabs(for: worktreeId)
    }

    func splitCurrent(_ axis: SplitAxis) {
        guard let worktree = selectedWorktree,
              let tab = activeTab(for: worktree.id),
              let target = tab.leafIds.first else { return }
        split(paneId: target, axis: axis)
    }

    /// Tab (se esiste) che contiene paneId in uno qualsiasi dei worktree aperti.
    func tabContaining(paneId: UUID) -> (worktree: Worktree, tab: WorkspaceTab, index: Int)? {
        for worktree in worktrees.values.flatMap({ $0 }) {
            if let idx = tabs[worktree.id]?.firstIndex(where: { $0.leafIds.contains(paneId) }) {
                return (worktree, tabs[worktree.id]![idx], idx)
            }
        }
        return nil
    }

    /// Splits the tab containing the given pane along the requested axis.
    func split(paneId: UUID, axis: SplitAxis) {
        guard let tuple = tabContaining(paneId: paneId) else { return }
        let idx = tuple.index
        guard let tree = tuple.tab.terminalTree else { return }
        tabs[tuple.worktree.id]?[idx].content = .terminal(tree.splitting(leaf: paneId, axis: axis, newLeaf: UUID()))
        persistTabs(for: tuple.worktree.id)
    }

    /// Chiude il pane rimuovendolo dallo split; il pane vicino riprende lo
    /// spazio. Il teardown (stop PTY, unregister proxy) avviene tramite
    /// SplitViewRenderer.pruneCache + PtyTerminalPane.onDisappear, la stessa
    /// via già usata quando si cambia tab — nessuna nuova logica qui.
    func closePane(paneId: UUID) {
        guard let tuple = tabContaining(paneId: paneId),
              let tree = tuple.tab.terminalTree,
              let newTree = tree.removing(leaf: paneId) else { return }
        deleteAgentSessionRefs(paneIds: [paneId])
        tabs[tuple.worktree.id]?[tuple.index].content = .terminal(newTree)
        persistTabs(for: tuple.worktree.id)
    }

    // MARK: - Pane controller caches (una per worktree)

    private var paneCaches: [UUID: TerminalPaneCache] = [:]

    func paneCache(for worktreeId: UUID) -> TerminalPaneCache {
        if let cache = paneCaches[worktreeId] { return cache }
        let cache = TerminalPaneCache()
        paneCaches[worktreeId] = cache
        return cache
    }

    /// Tutti i pane vivi nei tab del worktree — set di pruning per gli host.
    func liveLeafIds(for worktreeId: UUID) -> Set<UUID> {
        Set((tabs[worktreeId] ?? []).flatMap(\.leafIds))
    }

    /// True se il pane può essere affiancato al terminale corrente: stesso
    /// worktree del tab attivo, non già nel tab attivo, tab attivo terminale.
    func canAdoptPane(_ paneId: UUID) -> Bool {
        guard let worktree = selectedWorktree,
              let source = tabContaining(paneId: paneId),
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
        guard canAdoptPane(paneId),
              let worktree = selectedWorktree,
              let source = tabContaining(paneId: paneId),
              let active = activeTab(for: worktree.id),
              let destTree = active.terminalTree,
              let anchor = destTree.leafIds.first,
              let sourceTree = source.tab.terminalTree else { return }

        if let remaining = sourceTree.removing(leaf: paneId) {
            tabs[worktree.id]?[source.index].content = .terminal(remaining)
        } else {
            tabs[worktree.id]?.remove(at: source.index)
        }
        guard let destIndex = tabs[worktree.id]?.firstIndex(where: { $0.id == active.id }) else { return }
        tabs[worktree.id]?[destIndex].content =
            .terminal(destTree.splitting(leaf: anchor, axis: .horizontal, newLeaf: paneId))
        persistTabs(for: worktree.id)
    }

    /// Chiude un terminale: se è l'unico pane del tab chiude l'intera tab.
    func closeTerminal(paneId: UUID) {
        guard let target = tabContaining(paneId: paneId) else { return }
        if target.tab.leafIds.count <= 1 {
            closeTab(target.tab.id, in: target.worktree)
        } else {
            closePane(paneId: paneId)
        }
    }

    /// Fire-and-forget: la persistenza tab non deve bloccare la UI; un
    /// fallimento lascia solo il layout non salvato (rimedio: prossima mutazione).
    private func persistTabs(for worktreeId: UUID) {
        // Ogni mutazione tab passa di qui: il prune tiene la cache condivisa
        // allineata (un pane chiuso viene scartato → deinit → stop PTY).
        paneCaches[worktreeId]?.prune(keeping: liveLeafIds(for: worktreeId))
        guard let store else { return }
        let list = tabs[worktreeId] ?? []
        let active = activeTabId[worktreeId]
        Task { try? await store.saveTabs(worktreeId: worktreeId, tabs: list, activeTabId: active) }
    }

    // MARK: - Scrollback persistence

    func saveScrollback(worktreeId: UUID, paneId: UUID, data: Data) {
        guard let database, !data.isEmpty else { return }
        try? database.write { db in
            try PaneScrollbackRecord(
                paneId: paneId.uuidString, worktreeId: worktreeId.uuidString,
                data: data, updatedAt: Date()
            ).save(db)
        }
    }

    func loadScrollback(paneId: UUID) -> Data? {
        guard let database else { return nil }
        return try? database.read { db in
            try PaneScrollbackRecord.fetchOne(db, key: paneId.uuidString)?.data
        }
    }

    /// Snapshot di tutti i pane vivi verso il DB. Chiamato al quit
    /// (AppDelegate). I pane non registrati (già chiusi) tornano nil da
    /// snapshot e sono saltati; saveScrollback salta i blob vuoti.
    func flushLiveScrollback() async {
        for target in scrollbackFlushTargets(tabs: tabs) {
            if let data = await PaneRegistry.shared.snapshot(paneId: target.paneId) {
                saveScrollback(worktreeId: target.worktreeId, paneId: target.paneId, data: data)
            }
        }
    }

    /// Upserts the agent-native session ref for a pane. Fire-and-forget like
    /// persistTabs: a failed write only loses one restore opportunity.
    private func saveAgentSessionRef(paneId: UUID, sessionRef: String) {
        guard let store,
              let agentId = agentActivity.agentId(paneId: paneId),
              let tuple = tabContaining(paneId: paneId) else {
            sessionRestoreLogger.warning("session ref for unknown pane \(paneId.uuidString, privacy: .public) dropped")
            return
        }
        let worktreeId = tuple.worktree.id
        Task {
            try? await store.saveAgentSessionRef(
                paneId: paneId, worktreeId: worktreeId,
                agentId: agentId, sessionRef: sessionRef
            )
        }
    }

    /// Deletes stored session refs for panes the user closed explicitly —
    /// a deliberately ended pane must not resurrect its agent on relaunch.
    private func deleteAgentSessionRefs<S: Sequence<UUID> & Sendable>(paneIds: S) {
        guard let store else { return }
        Task {
            for id in paneIds { try? await store.deleteAgentSessionRef(paneId: id) }
        }
    }

    // MARK: - Pane commands & agent spawning

    var paneCommands: [UUID: String] = [:]

    /// Ultimo titolo PTY riportato da ogni pane (solo in memoria: al riavvio
    /// la shell lo rigenera). Alimenta le etichette dei nodi pane in sidebar.
    var paneTitles: [UUID: String] = [:]

    // MARK: - Markdown editor

    /// Documenti aperti, keyed su tab.id. Vivono qui (non nella view) perché
    /// le view SwiftUI muoiono al cambio tab e perderebbero il buffer.
    var markdownDocuments: [UUID: MarkdownDocument] = [:]

    /// Funnel unico per tutti i canali di apertura (cmd+click, drop, ⌘O).
    /// Dedup per fileURL: se il file è già aperto nel worktree attiva quella tab.
    @discardableResult
    func openMarkdownTab(fileURL: URL, in worktree: Worktree) -> WorkspaceTab? {
        let url = fileURL.standardizedFileURL
        if let existing = tabs[worktree.id]?.first(where: {
            $0.markdownFileURL?.standardizedFileURL == url
        }) {
            selectedWorktree = worktree
            activeTabId[worktree.id] = existing.id
            persistTabs(for: worktree.id)
            return existing
        }
        do {
            let doc = try MarkdownDocument(fileURL: url)
            let tab = WorkspaceTab(id: UUID(), title: url.lastPathComponent,
                                   content: .markdown(fileURL: url))
            markdownDocuments[tab.id] = doc
            selectedWorktree = worktree
            tabs[worktree.id, default: []].append(tab)
            activeTabId[worktree.id] = tab.id
            persistTabs(for: worktree.id)
            return tab
        } catch {
            lastError = "Apertura \(url.lastPathComponent) fallita: \(error.localizedDescription)"
            return nil
        }
    }

    /// Documento della tab; lo crea al volo per le tab ripristinate da sessione.
    func markdownDocument(for tab: WorkspaceTab) -> MarkdownDocument? {
        guard let url = tab.markdownFileURL else { return nil }
        if let doc = markdownDocuments[tab.id] { return doc }
        guard let doc = try? MarkdownDocument(fileURL: url) else { return nil }
        markdownDocuments[tab.id] = doc
        return doc
    }

    /// ⌘S: salva il documento della tab attiva, se è markdown. No-op altrimenti.
    func saveActiveMarkdownDocument() {
        guard let worktree = selectedWorktree,
              let tab = activeTab(for: worktree.id),
              let doc = markdownDocuments[tab.id] else { return }
        do { try doc.save() }
        catch { lastError = "Salvataggio \(doc.fileURL.lastPathComponent) fallito: \(error.localizedDescription)" }
    }

    /// Link attivato (cmd+click) in un pane del terminale: file markdown →
    /// tab editor nel worktree del pane; tutto il resto → apertura di sistema.
    func handleTerminalOpenURL(_ raw: String, in worktree: Worktree) {
        if let fileURL = MarkdownFileLink.resolve(raw, worktreePath: worktree.path) {
            openMarkdownTab(fileURL: fileURL, in: worktree)
        } else if let url = URL(string: raw) {
            NSWorkspace.shared.open(url)
        }
    }

    /// File > Apri file… (⌘O): NSOpenPanel filtrato su markdown, nel worktree selezionato.
    func openMarkdownFilePanel() {
        guard let worktree = selectedWorktree else { return }
        let panel = NSOpenPanel()
        panel.allowedContentTypes = MarkdownFileLink.extensions
            .compactMap { UTType(filenameExtension: $0) }
        panel.directoryURL = URL(fileURLWithPath: worktree.path)
        panel.canChooseDirectories = false
        panel.allowsMultipleSelection = false
        guard panel.runModal() == .OK, let url = panel.url else { return }
        openMarkdownTab(fileURL: url, in: worktree)
    }

    /// Alert modale per chiusura con modifiche non salvate.
    /// true = procedere con la chiusura.
    private func resolveDirtyClose(_ doc: MarkdownDocument) -> Bool {
        let alert = NSAlert()
        alert.messageText = "Salvare le modifiche a \(doc.fileURL.lastPathComponent)?"
        alert.informativeText = "Le modifiche andranno perse se non le salvi."
        alert.addButton(withTitle: "Salva")
        alert.addButton(withTitle: "Non salvare")
        alert.addButton(withTitle: "Annulla")
        switch alert.runModal() {
        case .alertFirstButtonReturn:
            do { try doc.save(); return true }
            catch {
                lastError = "Salvataggio fallito: \(error.localizedDescription)"
                return false
            }
        case .alertSecondButtonReturn:
            return true
        default:
            return false
        }
    }

    private func teardownMarkdownDocument(tabId: UUID) {
        markdownDocuments[tabId]?.stopWatching()
        markdownDocuments[tabId] = nil
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
    private func restoreAgentSessions(for worktree: Worktree, paneIds: Set<UUID>) async {
        guard let store else { return }
        let refs = (try? await store.agentSessionRefs(of: worktree.id)) ?? []
        for ref in refs {
            guard paneIds.contains(ref.paneId) else {
                try? await store.deleteAgentSessionRef(paneId: ref.paneId)
                continue
            }
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
                sessionRestoreLogger.warning("restore: stale session ref for pane \(ref.paneId.uuidString, privacy: .public) (\(agentId, privacy: .public)) — pruning")
                try? await store.deleteAgentSessionRef(paneId: ref.paneId)
                continue
            }
            let hc = tillerctlPath()
            do {
                try adapter.prepare(worktreePath: worktree.path, paneId: ref.paneId, tillerctlPath: hc)
            } catch {
                sessionRestoreLogger.warning("restore: prepare failed for pane \(ref.paneId.uuidString, privacy: .public): \(String(describing: error), privacy: .public) — falling back to fresh shell")
                continue
            }
            guard var command = adapter.resumeCommand(
                worktreePath: worktree.path, paneId: ref.paneId,
                tillerctlPath: hc, sessionRef: ref.sessionRef
            ) else { continue }
            if let override = configDirOverride(forAgentId: adapter.id) {
                command = "\(override.envKey)=\(Self.shellQuote(override.path)) \(command)"
            }
            paneCommands[ref.paneId] = command
            agentActivity.agentSpawned(paneId: ref.paneId, agentId: adapter.id, now: Date())
            watchExit(paneId: ref.paneId)
        }
    }

    func spawnAgent(_ adapter: any AgentAdapter, in worktree: Worktree) async {
        Task { await notifier.ensureAuthorization() }
        do {
            let hc = tillerctlPath()
            let paneId = UUID()
            try adapter.prepare(worktreePath: worktree.path, paneId: paneId, tillerctlPath: hc)
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
        if agentActivity.processOwnedPanes.contains(paneId) {
            // Re-confirm the process is still alive; clears the badge when
            // the agent exits back to the shell prompt.
            checkForegroundAgent(paneId: paneId)
        }
        guard let status = ScreenManifest.detect(tailText: tailText, agentId: agentId) else { return }
        guard let t = agentActivity.applyContentSignal(paneId: paneId, status: status, now: Date()) else { return }
        notifyTransition(paneId: paneId, from: t.old, to: t.new)
    }

    /// Layer D driver: resolves the pane's shell child processes off-main
    /// and registers (or clears) the pane's agent accordingly.
    private func checkForegroundAgent(paneId: UUID) {
        Task.detached {
            guard let pid = await PaneRegistry.shared.shellPid(paneId: paneId) else { return }
            let agentId = ForegroundProcessAgent.identify(shellPid: pid)
            await MainActor.run { [weak self] in
                guard let self else { return }
                if let agentId {
                    self.agentActivity.processIdentified(paneId: paneId, agentId: agentId, now: Date())
                } else {
                    self.agentActivity.processGone(paneId: paneId)
                }
            }
        }
    }

    private func notifyTransition(paneId: UUID, from old: AgentStatus?, to new: AgentStatus) {
        let visible = isSelectedWorktreeContaining(paneId: paneId)
        guard NotificationPolicy.shouldNotify(
            old: old, new: new, appActive: NSApp.isActive, paneVisible: visible
        ) else { return }
        guard let payload = buildPayload(paneId: paneId, status: new) else { return }
        notifier.post(payload)
    }

    private func isSelectedWorktreeContaining(paneId: UUID) -> Bool {
        guard let sel = selectedWorktree else { return false }
        return (tabs[sel.id] ?? []).contains { $0.leafIds.contains(paneId) }
    }

    private func worktreeContaining(paneId: UUID) -> Worktree? {
        worktrees.values.flatMap { $0 }.first { wt in
            (tabs[wt.id] ?? []).contains { $0.leafIds.contains(paneId) }
        }
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

    private func focusPane(paneId: UUID) {
        guard let worktree = worktreeContaining(paneId: paneId) else { return }
        selectedWorktree = worktree
        NSApp.activate(ignoringOtherApps: true)
        NSApp.windows.first?.makeKeyAndOrderFront(nil)
    }

    /// tillerctl ships next to the app binary in DEBUG dev loops; fall back to PATH.
    func tillerctlPath() -> String {
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
