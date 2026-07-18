# Chat-pane technical dossier

Repository: `/Users/enzopiopalmisano/Desktop/Progetti/tiller-feature/chat-interface`

## 1. Tab/pane model

Path: `Packages/TillerCore/Sources/TillerCore/WorkspaceTab.swift`

```swift
public enum TabContent: Equatable, Sendable {
    case terminal(SplitTree)
    case markdown(fileURL: URL)
}

public struct WorkspaceTab: Identifiable, Equatable, Sendable {
    public let id: UUID
    public var title: String
    public var content: TabContent

    public init(id: UUID, title: String, content: TabContent)
    public init(id: UUID, title: String, tree: SplitTree)

    public var leafIds: [UUID]
    public var terminalTree: SplitTree?
    public var markdownFileURL: URL?
}
```

A terminal `WorkspaceTab` stores `TabContent.terminal(SplitTree)`. A Markdown tab stores `TabContent.markdown(fileURL:)`. `leafIds` and `terminalTree` return terminal data only; for Markdown, `leafIds == []` and `terminalTree == nil`.

Path: `Packages/TillerCore/Sources/TillerCore/SplitTree.swift`

```swift
public enum SplitAxis: Equatable, Sendable {
    case horizontal
    case vertical
}

public indirect enum SplitTree: Equatable, Sendable {
    case leaf(id: UUID)
    case split(axis: SplitAxis, first: SplitTree, second: SplitTree)

    public func splitting(
        leaf target: UUID,
        axis: SplitAxis,
        newLeaf: UUID,
        placement: SplitPlacement = .after
    ) -> SplitTree

    public func removing(leaf target: UUID) -> SplitTree?
    public var leafIds: [UUID]
}
```

Each `SplitTree.leaf(id:)` UUID is a terminal pane ID. `AppModel` holds tabs by worktree:

```swift
// App/AppModel.swift
var tabs: [UUID: [WorkspaceTab]] = [:]
var activeTabId: [UUID: UUID] = [:]
```

Path: `Packages/TillerPersistence/Sources/TillerPersistence/Records.swift`

```swift
public struct TerminalTabRecord: Codable, FetchableRecord, PersistableRecord, Sendable, Equatable {
    public static let databaseTableName = "terminalTab"
    public var id: String
    public var worktreeId: String
    public var title: String
    public var orderIdx: Int
    public var isActive: Bool
    public var treeJSON: String
    public var updatedAt: Date
    public var kind: String
    public var filePath: String?

    public init(
        id: String, worktreeId: String, title: String, orderIdx: Int,
        isActive: Bool, treeJSON: String, updatedAt: Date,
        kind: String = "terminal", filePath: String? = nil
    )
}
```

Path: `Packages/TillerPersistence/Sources/TillerPersistence/AppDatabase.swift`

```swift
migrator.registerMigration("v3") { db in
    try db.create(table: "terminalTab") { t in
        t.primaryKey("id", .text)
        t.column("worktreeId", .text).notNull()
            .references("worktree", onDelete: .cascade)
        t.column("title", .text).notNull()
        t.column("orderIdx", .integer).notNull()
        t.column("isActive", .boolean).notNull()
        t.column("treeJSON", .text).notNull()
        t.column("updatedAt", .datetime).notNull()
    }
}

migrator.registerMigration("v7") { db in
    try db.alter(table: "terminalTab") { t in
        t.add(column: "kind", .text).notNull().defaults(to: "terminal")
        t.add(column: "filePath", .text)
    }
}
```

Current persisted `kind` values are `"terminal"` and `"markdown"`. There is no `TabKind` enum and no current `"chat"` value.

Path: `Packages/TillerCore/Sources/TillerCore/ProjectStore.swift`

```swift
switch tab.content {
case .terminal(let tree):
    let treeJSON = String(decoding: try encoder.encode(tree), as: UTF8.self)
    record = TerminalTabRecord(
        id: tab.id.uuidString, worktreeId: worktreeId.uuidString,
        title: tab.title, orderIdx: idx,
        isActive: tab.id == activeTabId,
        treeJSON: treeJSON, updatedAt: Date()
    )
case .markdown(let fileURL):
    record = TerminalTabRecord(
        id: tab.id.uuidString, worktreeId: worktreeId.uuidString,
        title: tab.title, orderIdx: idx,
        isActive: tab.id == activeTabId,
        treeJSON: "", updatedAt: Date(),
        kind: "markdown", filePath: fileURL.path
    )
}
```

```swift
switch record.kind {
case "markdown":
    guard let path = record.filePath else { continue }
    tabs.append(WorkspaceTab(
        id: id, title: record.title,
        content: .markdown(fileURL: URL(fileURLWithPath: path))))
default:
    guard let tree = try? decoder.decode(
        SplitTree.self, from: Data(record.treeJSON.utf8)) else { continue }
    tabs.append(WorkspaceTab(id: id, title: record.title, tree: tree))
}
```

Other switches are on the in-memory content:

```swift
// App/ContentView.swift
switch tab.content {
case .terminal(let tree):
    TerminalSplitHost(tree: tree, ...)
case .markdown:
    MarkdownEditorTabView(document: doc)
}
```

```swift
// App/AppModel.swift
tabs[tuple.worktree.id]?[tuple.index].content = .terminal(
    tree.splitting(
        leaf: paneId, axis: axis, newLeaf: newPaneId, placement: placement
    )
)
```

`chatSession` and `chatItem` are separate v8 tables; they are not current `terminalTab.kind` values.

## 2. Markdown editor precedent

Files:

- `App/MarkdownEditor/MarkdownEditorTabView.swift` — `MarkdownEditorTabView`, preview/code modes, banners, MarkdownUI.
- `App/MarkdownEditor/MarkdownToolbar.swift` — `MarkdownToolbar` and text transformations.
- `Packages/TillerCore/Sources/TillerCore/MarkdownDocument.swift` — document state and file watcher.

```swift
// Packages/TillerCore/Sources/TillerCore/MarkdownDocument.swift
@Observable @MainActor
public final class MarkdownDocument {
    public let fileURL: URL
    public var text: String
    public private(set) var savedText: String
    public private(set) var externalChangeConflict = false
    public private(set) var fileDeleted = false

    public var isDirty: Bool { text != savedText }
    public init(fileURL: URL) throws
    public func save() throws
    public func reloadFromDisk()
    public func keepLocalBuffer()
    public func stopWatching()
}
```

Creation and deduplication:

```swift
// App/AppModel.swift
var markdownDocuments: [UUID: MarkdownDocument] = [:]

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
```

Callers:

```swift
// App/ContentView.swift
.dropDestination(for: URL.self) { urls, _ in
    guard let worktree = model.selectedWorktree,
          let url = urls.first(where: { MarkdownFileLink.isMarkdown($0) }) else { return false }
    model.openMarkdownTab(fileURL: url, in: worktree)
    return true
}
```

```swift
// App/AppModel.swift
func handleTerminalOpenURL(_ raw: String, in worktree: Worktree) {
    if let fileURL = MarkdownFileLink.resolve(raw, worktreePath: worktree.path) {
        openMarkdownTab(fileURL: fileURL, in: worktree)
    } else if let url = URL(string: raw) {
        NSWorkspace.shared.open(url)
    }
}

func openMarkdownFilePanel() {
    ...
    openMarkdownTab(fileURL: url, in: worktree)
}
```

```swift
// App/RightPanel/FileExplorerView.swift
private func open(_ node: FileTreeNode) {
    guard let root = panelModel.rootURL else { return }
    let url = node.url(relativeTo: root)
    if MarkdownFileLink.isMarkdown(url) {
        appModel.openMarkdownTab(fileURL: url, in: worktree)
    } else {
        NSWorkspace.shared.open(url)
    }
}
```

Rendering in the host:

```swift
// App/ContentView.swift
ForEach(model.openWorktreeIds, id: \.self) { worktreeId in
    if let worktree = model.worktree(byId: worktreeId) {
        ForEach(model.tabs[worktreeId] ?? []) { tab in
            Group {
                switch tab.content {
                case .terminal(let tree):
                    TerminalSplitHost(tree: tree, ...)
                case .markdown:
                    if let doc = model.markdownDocument(for: tab) {
                        MarkdownEditorTabView(document: doc)
                    } else {
                        ContentUnavailableView("File non trovato", ...)
                    }
                }
            }
            .opacity(isVisible ? 1 : 0)
            .allowsHitTesting(isVisible)
        }
    }
}
```

```swift
// App/MarkdownEditor/MarkdownEditorTabView.swift
struct MarkdownEditorTabView: View {
    @Bindable var document: MarkdownDocument
    ...
    if mode == .code {
        MarkdownToolbar(document: document, selection: $selection)
        TextEditor(text: $document.text, selection: $selection)
    } else {
        ScrollView {
            Markdown(document.text)
                .markdownTheme(.gitHub)
                .textSelection(.enabled)
        }
    }
}
```

Restored tabs lazily create the document:

```swift
func markdownDocument(for tab: WorkspaceTab) -> MarkdownDocument? {
    guard let url = tab.markdownFileURL else { return nil }
    if let doc = markdownDocuments[tab.id] { return doc }
    guard let doc = try? MarkdownDocument(fileURL: url) else { return nil }
    markdownDocuments[tab.id] = doc
    return doc
}
```

Persistence signatures:

```swift
// Packages/TillerCore/Sources/TillerCore/ProjectStore.swift
public func saveTabs(
    worktreeId: UUID, tabs: [WorkspaceTab], activeTabId: UUID?
) throws

public func loadTabs(of worktreeId: UUID) throws
    -> (tabs: [WorkspaceTab], activeTabId: UUID?)
```

`saveTabs` deletes the worktree’s `terminalTab` rows and reinserts the ordered list. Markdown rows use `kind: "markdown"` and `filePath: fileURL.path`. Terminal rows encode `SplitTree` in `treeJSON`.

Close/teardown:

```swift
// App/AppModel.swift
func closeTab(_ tabId: UUID, in worktree: Worktree) {
    if let doc = markdownDocuments[tabId], doc.isDirty {
        guard resolveDirtyClose(doc) else { return }
    }
    ...
    list.removeAll { $0.id == tabId }
    teardownMarkdownDocument(tabId: tabId)
    tabs[worktree.id] = list
    ...
    persistTabs(for: worktree.id)
}

private func teardownMarkdownDocument(tabId: UUID) {
    markdownDocuments[tabId]?.stopWatching()
    markdownDocuments[tabId] = nil
}
```

The same teardown is used in project/worktree removal. Dirty tabs go through `resolveDirtyClose(_:)` before removal.

## 3. New tabs and splits

### New-tab views and actions

```swift
// App/TillerApp.swift
CommandGroup(after: .newItem) {
    Button("Nuova tab") { model.newShellTabInSelected() }
        .keyboardShortcut("t", modifiers: .command)
    Button("Chiudi tab") { model.closeActiveTab() }
        .keyboardShortcut("w", modifiers: .command)
}
```

```swift
// App/EmptyWorktreeView.swift
struct EmptyWorktreeView: View {
    let onNewTerminal: () -> Void
    ...
    Button("New Terminal", action: onNewTerminal)
}
```

```swift
// App/ContentView.swift
if model.openWorktreeIds.isEmpty {
    ContentUnavailableView(...)
} else if let worktree = model.selectedWorktree,
          (model.tabs[worktree.id] ?? []).isEmpty {
    EmptyWorktreeView(onNewTerminal: { model.newShellTabInSelected() })
}
```

```swift
// App/SidebarView.swift — WorktreeRow hover menu
Menu {
    Button {
        model.newShellTab(in: worktree)
    } label: {
        Label("Nuovo Terminale", systemImage: "terminal")
    }
    ...
} label: {
    Image(systemName: "plus")
}
```

The same worktree row has a context-menu item:

```swift
Button("Nuovo Terminale") {
    model.newShellTab(in: worktree)
}
```

Creation path:

```swift
// App/AppModel.swift
func newShellTabInSelected() {
    guard let worktree = selectedWorktree else { return }
    newShellTab(in: worktree)
}

func newShellTab(in worktree: Worktree) {
    let title = WorkspaceTab.nextShellTitle(existing: tabs[worktree.id] ?? [])
    openTab(paneId: UUID(), title: title, in: worktree)
}

@discardableResult
func openTab(
    paneId: UUID,
    title: String,
    in worktree: Worktree,
    activate: Bool = true,
    persist: Bool = true
) -> WorkspaceTab {
    let tab = WorkspaceTab(id: UUID(), title: title, tree: .leaf(id: paneId))
    tabs[worktree.id, default: []].append(tab)
    if activate { activeTabId[worktree.id] = tab.id }
    if persist { persistTabs(for: worktree.id) }
    return tab
}
```

Agent menu items end in the same `openTab`:

```swift
// App/AppModel.swift
func spawnAgent(_ adapter: any AgentAdapter, in worktree: Worktree) async {
    ...
    paneCommands[paneId] = command
    agentActivity.agentSpawned(paneId: paneId, agentId: adapter.id, now: Date())
    selectedWorktree = worktree
    openTab(paneId: paneId, title: adapter.displayName, in: worktree)
    watchExit(paneId: paneId)
}
```

### Split views and actions

```swift
// App/ContentView.swift — toolbar
Button {
    model.splitCurrent(.horizontal)
} label: {
    Image(systemName: "square.split.1x2")
}
```

```swift
// App/SidebarView.swift — TerminalPaneMenu
Button("Split orizzontale") {
    model.split(paneId: paneId, axis: .vertical)
}
Button("Split verticale") {
    model.split(paneId: paneId, axis: .horizontal)
}
```

```swift
// App/TerminalContextMenuProvider.swift
case .splitRight:
    model?.split(paneId: paneId, axis: .horizontal)
case .splitDown:
    model?.split(paneId: paneId, axis: .vertical)
```

The AppModel path is:

```swift
func splitCurrent(_ axis: SplitAxis) {
    guard let worktree = selectedWorktree,
          let tab = activeTab(for: worktree.id),
          let target = tab.leafIds.first else { return }
    split(paneId: target, axis: axis)
}

@discardableResult
func split(
    paneId: UUID,
    axis: SplitAxis,
    newPaneId: UUID = UUID(),
    placement: SplitPlacement = .after,
    persist: Bool = true
) -> Bool {
    guard let tuple = tabContaining(paneId: paneId),
          let tree = tuple.tab.terminalTree,
          tree.leafIds.contains(paneId) else { return false }
    tabs[tuple.worktree.id]?[tuple.index].content = .terminal(
        tree.splitting(
            leaf: paneId, axis: axis, newLeaf: newPaneId, placement: placement
        )
    )
    if persist { persistTabs(for: tuple.worktree.id) }
    return true
}
```

The split changes only the tab’s `SplitTree`. `TerminalSplitHost` owns/reuses one controller per leaf UUID.

## 4. Pane lifecycle

### Worktree hosts

```swift
// App/AppModel.swift
var selectedWorktree: Worktree? {
    didSet {
        ...
        guard let worktree = selectedWorktree else { return }
        if !openWorktreeIds.contains(worktree.id) {
            openWorktreeIds.append(worktree.id)
        }
        evictIdleWorktreesIfNeeded()
    }
}

var openWorktreeIds: [UUID] = [] {
    didSet {
        UserDefaults.standard.set(
            openWorktreeIds.map(\.uuidString),
            forKey: AppSettings.openWorktreeIdsKey)
    }
}
```

```swift
// App/ContentView.swift
ForEach(model.openWorktreeIds, id: \.self) { worktreeId in
    if let worktree = model.worktree(byId: worktreeId) {
        ...
        TerminalSplitHost(...)
    }
}
```

Removing an ID from `openWorktreeIds` removes that worktree’s host from the `ForEach`. The resulting view disappearance is the terminal teardown path. Bootstrap restores IDs from `UserDefaults`:

```swift
let storedOpenIds = (UserDefaults.standard
    .stringArray(forKey: AppSettings.openWorktreeIdsKey) ?? [])
    .compactMap(UUID.init)
openWorktreeIds = storedOpenIds.filter { id in
    worktree(byId: id) != nil
}
```

`AppModel.acquireControlMount(for:)` and `releaseControlMount(_:success:)` also add/remove `openWorktreeIds` around control-socket panel creation.

### Terminal pane teardown

Path: `Packages/TillerTerminal/Sources/TillerTerminal/PtyTerminalPane.swift`

```swift
public struct PtyTerminalPane: View {
    @State private var runtime: PtyRuntime?
    ...
    public var body: some View {
        TerminalSurfaceView(context: state)
            .onAppear { ... runtime = rt; ... rt.start() }
            .onDisappear {
                TerminalOpenURLRouter.unregister(state)
                PaneProxyRegistry.shared.unregister(paneId: paneId)
                runtime?.stop()
                if let onScrollback, let rt = runtime {
                    Task { await onScrollback(paneId, await rt.scrollback.snapshot()) }
                }
            }
    }
}
```

```swift
// Packages/TillerTerminal/Sources/TillerTerminal/PtyTerminalPane.swift
func stop() {
    stateLock.lock()
    stopped = true
    stateLock.unlock()
    debouncer.cancel()
    pty.terminate()
    Task { [self] in
        await PaneRegistry.shared.unregister(paneId: paneId)
    }
}
```

```swift
// Packages/TillerTerminal/Sources/TillerTerminal/PtyProcess.swift
public func terminate() {
    if processId > 0 { kill(processId, SIGTERM) }
    queue.async { [weak self] in
        self?.stopReadLoop()
        self?.reapChildWithRetry()
    }
}
```

`Packages/TillerTerminal/Sources/TillerTerminal/TerminalPaneCache.swift` removes stale controllers in `prune(keeping:)`; controller deallocation is documented as PTY teardown. `AppModel.closeTerminal(paneId:)` chooses whole-tab close or split-leaf close; `closeTab` removes the tab and `closePane` replaces the tree with `tree.removing(leaf:)`.

There is no current `TabContent`-level process lifecycle protocol or generic non-PTY kill callback. The existing process-specific hook is `PtyTerminalPane.body.onDisappear -> PtyRuntime.stop()`.

## 5. Agent activity/status

### Layer A path

```swift
// Packages/TillerControl/Sources/tillerctl/Tillerctl.swift
struct Notify: ParsableCommand {
    @Option var session: String?
    @Option var status: String?   // running | needs-input | done | error
    ...
    func run() throws {
        ...
        let response = try ControlClient.roundTrip(
            socketPath: socketOptions.socket,
            request: TillerctlRequestBuilder.notify(
                session: session!, status: status!, agentSession: ref)
        )
        guard response.ok else { ... }
    }
}
```

```swift
// Packages/TillerControl/Sources/TillerControl/TillerctlRequestBuilder.swift
public static func notify(
    session: String,
    status: String,
    agentSession: String? = nil
) -> ControlRequest {
    var params = ["session": session, "status": status]
    if let agentSession, !agentSession.isEmpty {
        params["agentSession"] = agentSession
    }
    return ControlRequest(id: UUID().uuidString, method: "notify", params: params)
}
```

```swift
// Packages/TillerControl/Sources/TillerControl/ControlServer.swift
public typealias Handler = @Sendable (ControlRequest) async -> ControlResponse
```

```swift
// App/AppModel.swift
func startControlServer() {
    ...
    let server = ControlServer(socketPath: path) { [weak self] request in
        return await Task { @MainActor [weak self] in
            guard let self else { ... }
            return await self.handleControl(request)
        }.value
    }
    ...
}

func handleControl(_ request: ControlRequest) async -> ControlResponse {
    switch request.method {
    case "notify":
        guard let sessionId = UUID(uuidString: request.params["session"] ?? ""),
              let status = AgentStatus(rawValue: request.params["status"] ?? "")
        else { ... }
        let transition = agentActivity.notify(
            paneId: sessionId, status: status, now: Date())
        notifyTransition(
            paneId: sessionId, from: transition.old, to: transition.new)
        ...
        return .success(id: request.id)
    ...
    }
}
```

### State model and consumers

Path: `Packages/TillerCore/Sources/TillerCore/AgentActivityModel.swift`

```swift
@Observable
public final class AgentActivityModel {
    public var agentStatus: [UUID: AgentStatus] = [:]
    public var paneAgents: [UUID: String] = [:]

    @discardableResult
    public func notify(
        paneId: UUID, status: AgentStatus, now: Date
    ) -> AgentTransition

    public func agentSpawned(
        paneId: UUID, agentId: String, now: Date
    )

    public func paneClosed(paneId: UUID)
    public func statusForWorktree(
        paneIds: some Collection<UUID>
    ) -> AgentStatus?
    public func agentIdForWorktree(
        paneIds: some Collection<UUID>
    ) -> String?
    public func runningAgentIds(
        paneIds: some Collection<UUID>, catalogIds: [String]
    ) -> [String]
}
```

```swift
public enum AgentStatus: String, Sendable {
    case running, needsInput = "needs-input", done, error
}

public struct AgentTransition: Sendable, Equatable {
    public let paneId: UUID
    public let old: AgentStatus?
    public let new: AgentStatus
}
```

The sidebar aggregation filters only terminal leaves:

```swift
// App/AppModel.swift
func statusForWorktree(_ worktree: Worktree) -> AgentStatus? {
    let paneIds = (tabs[worktree.id] ?? []).flatMap { $0.leafIds }
    return agentActivity.statusForWorktree(paneIds: paneIds)
}

func runningAgentIds(for worktree: Worktree) -> [String] {
    let paneIds = (tabs[worktree.id] ?? []).flatMap { $0.leafIds }
    return agentActivity.runningAgentIds(
        paneIds: paneIds, catalogIds: AgentCatalog.all.map(\.id))
}
```

The notification call is:

```swift
// App/AppModel.swift
private func notifyTransition(
    paneId: UUID, from old: AgentStatus?, to new: AgentStatus
) {
    let visible = isSelectedWorktreeContaining(paneId: paneId)
    guard NotificationPolicy.shouldNotify(
        old: old, new: new, appActive: NSApp.isActive, paneVisible: visible)
    else { return }
    guard let payload = buildPayload(paneId: paneId, status: new) else { return }
    notifier.post(payload)
}
```

`buildPayload` finds the worktree through `WorkspaceTab.leafIds`:

```swift
private func worktreeContaining(paneId: UUID) -> Worktree? {
    worktrees.values.flatMap { $0 }.first { wt in
        (tabs[wt.id] ?? []).contains { $0.leafIds.contains(paneId) }
    }
}
```

The minimal existing state-machine call is `agentActivity.notify(paneId:status:now:)`. The notification-producing AppModel call is `notifyTransition(paneId:from:to:)`. In the current code, a chat pane ID must be represented by `WorkspaceTab.leafIds` for the existing sidebar aggregation and worktree notification lookup to find it.

## 6. RightPanel file open/reveal API

There is no current `RightPanelModel` API for an external caller to open/reveal a file, and no current API accepting a line number.

Path: `App/RightPanel/RightPanelModel.swift`

```swift
@MainActor @Observable
final class RightPanelModel {
    private(set) var worktree: Worktree?
    private(set) var childrenByDirectory: [String: [FileTreeNode]] = [:]
    private(set) var expandedDirectories: Set<String> = []
    ...
    func activate(worktree: Worktree?, isGitRepository: Bool) async
    func deactivate()
    func toggleDirectory(_ path: String) async
    func refresh() async
}
```

There is no `openFile`, `revealFile`, `selectFile`, or `line` method/property in this type.

The instance is private to the root view:

```swift
// App/ContentView.swift
@State private var rightPanelModel = RightPanelModel()

RightPanelView(
    appModel: model,
    panelModel: rightPanelModel,
    modeRaw: $rightPanelModeRaw,
    isGitRepository: rightPanelContext.gitProject,
    onClose: { rightPanelVisible = false })
```

Existing file-tree operations are private to `FileExplorerView`:

```swift
// App/RightPanel/FileExplorerView.swift
private func open(_ node: FileTreeNode) {
    guard let root = panelModel.rootURL else { return }
    let url = node.url(relativeTo: root)
    if MarkdownFileLink.isMarkdown(url) {
        appModel.openMarkdownTab(fileURL: url, in: worktree)
    } else {
        NSWorkspace.shared.open(url)
    }
}

private func reveal(_ node: FileTreeNode) {
    guard let root = panelModel.rootURL else { return }
    NSWorkspace.shared.activateFileViewerSelecting([
        node.url(relativeTo: root)])
}
```

`open(_:)` opens a Markdown tab in the central worktree host or opens the file with `NSWorkspace.shared.open`. `reveal(_:)` opens Finder selection, not the in-app right panel. The right-panel view initializer is:

```swift
// App/RightPanel/RightPanelView.swift
struct RightPanelView: View {
    @Bindable var appModel: AppModel
    @Bindable var panelModel: RightPanelModel
    @Binding var modeRaw: String
    let isGitRepository: Bool
    let onClose: () -> Void
}
```

No line-number parameter exists in these APIs.

## 7. AppDatabase ownership and worktree removal

### Creation and access

```swift
// App/TillerApp.swift
@main
struct TillerApp: App {
    @State private var model = AppModel()
    ...
    WindowGroup(id: "main") {
        ContentView(model: model, updater: updater)
    }
}
```

```swift
// App/AppModel.swift
private var store: ProjectStore?
private var database: AppDatabase?
var agentAccounts: AgentAccountStore?

func bootstrap() async {
    let dir = FileManager.default.urls(
        for: .applicationSupportDirectory, in: .userDomainMask)[0]
        .appendingPathComponent("Tiller", isDirectory: true)
    ...
    let db = try AppDatabase(
        path: dir.appendingPathComponent("tiller.sqlite").path)
    let store = ProjectStore(database: db)
    self.store = store
    self.database = db
    self.agentAccounts = AgentAccountStore(database: db)
    ...
}
```

```swift
// Packages/TillerPersistence/Sources/TillerPersistence/AppDatabase.swift
public final class AppDatabase: Sendable {
    private let dbQueue: DatabaseQueue

    public init(path: String) throws {
        dbQueue = try DatabaseQueue(path: path)
        try Self.migrator.migrate(dbQueue)
    }

    public func read<T>(_ block: (Database) throws -> T) throws -> T
    @discardableResult
    public func write<T>(_ block: (Database) throws -> T) throws -> T
}
```

Views receive `AppModel`, not `AppDatabase`:

```swift
// App/ContentView.swift
struct ContentView: View {
    var model: AppModel
    ...
}

// App/SidebarView.swift
struct SidebarView: View {
    @Bindable var model: AppModel
}
```

`AgentAccountStore` is separately initialized with the same database:

```swift
// App/AgentAccountStore.swift
@MainActor @Observable
final class AgentAccountStore {
    private let database: AppDatabase
    init(database: AppDatabase) {
        self.database = database
        reload()
    }
}
```

### Worktree removal

Persistence-only methods:

```swift
// Packages/TillerCore/Sources/TillerCore/ProjectStore.swift
public func removeProject(_ id: UUID) throws {
    try database.write { db in
        _ = try ProjectRecord.deleteOne(db, key: id.uuidString)
    }
}

public func removeWorktree(_ id: UUID) throws {
    try database.write { db in
        _ = try WorktreeRecord.deleteOne(db, key: id.uuidString)
    }
}
```

App-level cleanup:

```swift
// App/AppModel.swift
func removeWorktree(_ worktree: Worktree) async {
    guard let store else { return }
    guard let project = projects.first(where: {
        $0.id == worktree.projectId
    }) else { return }
    do {
        try await store.removeWorktree(worktree.id)
        worktrees[worktree.projectId]?.removeAll { $0.id == worktree.id }
        let tabsBeingRemoved = tabs[worktree.id] ?? []
        tabs[worktree.id] = nil
        paneCaches[worktree.id] = nil
        for tab in tabsBeingRemoved {
            teardownMarkdownDocument(tabId: tab.id)
        }
        activeTabId[worktree.id] = nil
        openWorktreeIds.removeAll { $0 == worktree.id }
        ...
        if worktree.path != project.rootPath {
            try await GitWorktrees.remove(
                repoPath: project.rootPath, path: worktree.path)
        }
    } catch { ... }
}
```

Project removal performs the same tab/document/open-host cleanup for all project worktrees and invokes `GitWorktrees.remove` for non-root worktrees:

```swift
func removeProject(_ project: Project) async {
    ...
    try await store.removeProject(project.id)
    projects.removeAll { $0.id == project.id }
    worktrees[project.id] = nil
    for worktree in projectWorktrees {
        let tabsBeingRemoved = tabs[worktree.id] ?? []
        tabs[worktree.id] = nil
        for tab in tabsBeingRemoved { teardownMarkdownDocument(tabId: tab.id) }
        activeTabId[worktree.id] = nil
    }
    openWorktreeIds.removeAll {
        id in projectWorktrees.contains { $0.id == id }
    }
    ...
    try await GitWorktrees.remove(
        repoPath: project.rootPath, path: worktree.path)
}
```

UI entry points are `App/SidebarView.swift` (worktree context menu) and `App/ProjectSettingsSheet.swift` (project delete alert). `openWorktreeIds.removeAll` is the unmount cleanup hook; host disappearance terminates mounted PTYs.

## 8. AgentIcon and MarkdownUI

### AgentIcon

Path: `App/AgentIcon.swift`

```swift
struct AgentIcon: View {
    let agentId: String
    var size: CGFloat = 14

    var body: some View {
        Group {
            switch agentId {
            case "claude":
                Image("agent-claude")
                    .renderingMode(.template)
                    .resizable()
                    .scaledToFit()
                    .foregroundStyle(Self.claudeOrange)
            case "codex":
                Image("agent-codex")
                    .renderingMode(.template)
                    .resizable()
                    .scaledToFit()
                    .foregroundStyle(.primary)
            case "opencode": OpenCodeLogo()
            case "pi": PiLogo()
            case "omp": OmpLogo()
            default:
                ZStack {
                    Circle().fill(Self.color(for: agentId))
                    Text(String(agentId.prefix(1)).uppercased())
                }
            }
        }
        .frame(width: size, height: size)
    }
}
```

Render with `AgentIcon(agentId: agentId, size: 12)`. The type is App-target file-internal (`struct`, not `public`). Known IDs are `claude`, `codex`, `opencode`, `pi`, and `omp`; unknown IDs use a colored monogram circle.

### MarkdownUI

Path: `project.yml`

```yaml
packages:
  MarkdownUI:
    url: https://github.com/gonzalezreal/swift-markdown-ui
    from: 2.4.0
...
targets:
  Tiller:
    dependencies:
      ...
      - package: MarkdownUI
```

Path: `App/MarkdownEditor/MarkdownEditorTabView.swift`

```swift
import MarkdownUI
...
Markdown(document.text)
    .markdownTheme(.gitHub)
    .textSelection(.enabled)
```

MarkdownUI is imported and used in the App target; TillerCore does not import it.

## 9. project.yml and local TillerACP

Current local-package declaration pattern:

```yaml
# project.yml
packages:
  TillerCore:
    path: Packages/TillerCore
  TillerTerminal:
    path: Packages/TillerTerminal
  TillerPersistence:
    path: Packages/TillerPersistence
  TillerControl:
    path: Packages/TillerControl
  TillerGit:
    path: Packages/TillerGit
  TillerAgents:
    path: Packages/TillerAgents
```

Current App target dependencies:

```yaml
targets:
  Tiller:
    type: application
    platform: macOS
    ...
    dependencies:
      - package: TillerCore
      - package: TillerTerminal
      - package: TillerPersistence
      - package: TillerControl
      - package: TillerAgents
      - package: MarkdownUI
      - package: TillerGit
      - package: Sparkle
```

`TillerACP` is not currently declared in `project.yml` and is not in the `Tiller` target dependency list.

Path: `Packages/TillerACP/Package.swift`

```swift
let package = Package(
    name: "TillerACP",
    platforms: [.macOS(.v15)],
    products: [.library(name: "TillerACP", targets: ["TillerACP"])],
    dependencies: [
        .package(path: "../TillerPersistence")
    ],
    targets: [
        .target(name: "TillerACP", dependencies: ["TillerPersistence"]),
        ...
    ]
)
```

The local package declaration and target link use this existing syntax:

```yaml
packages:
  TillerACP:
    path: Packages/TillerACP
...
targets:
  Tiller:
    dependencies:
      ...
      - package: TillerACP
```

Repository instructions identify `project.yml` as the source for package/target changes; the generated Xcode project is not hand-edited.

## 10. AppSettings pattern

Path: `Packages/TillerCore/Sources/TillerCore/AppSettings.swift`

Existing bounded numeric setting:

```swift
public static let terminalFontSizeKey = "appearance.terminalFontSize"
public static let defaultTerminalFontSize = 13
public static let terminalFontSizeRange: ClosedRange<Int> = 9...24

public static func clampTerminalFontSize(_ size: Int) -> Int {
    min(max(size, terminalFontSizeRange.lowerBound), terminalFontSizeRange.upperBound)
}
```

App binding:

```swift
// App/AppearanceSettingsView.swift
@AppStorage(AppSettings.terminalFontSizeKey)
private var terminalFontSize = AppSettings.defaultTerminalFontSize

Stepper(value: $terminalFontSize, in: AppSettings.terminalFontSizeRange) {
    Text("Font size")
    Text("\(terminalFontSize) pt")
}
```

The corresponding range/clamp tests are in `Packages/TillerCore/Tests/TillerCoreTests/AppSettingsTests.swift`:

```swift
@Test func rightPanelWidthClampsToSupportedRange() {
    #expect(AppSettings.clampRightPanelWidth(120) == 280)
    #expect(AppSettings.clampRightPanelWidth(420) == 420)
    #expect(AppSettings.clampRightPanelWidth(900) == 600)
}
```

`AppSettings` contains Foundation-only keys, defaults, ranges, and clamp functions; SwiftUI views apply them through `@AppStorage`.
