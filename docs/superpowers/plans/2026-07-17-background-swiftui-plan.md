# Background Activity and SwiftUI Optimization — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (- [ ]) syntax for tracking.

## Goal

Reduce background CPU and rendering overhead by: (1) marking worktrees dirty instead of refreshing when the right panel is hidden, (2) stopping the `UsageStore` timer when all providers are disabled, (3) removing collection-wide sidebar animations, and (4) computing agent status once per render pass using a scoped/local derived snapshot rather than a fragile global cache with manual invalidation.

## Architecture

### Dirty Marker (C1)

`RightPanelModel.scheduleRefresh` currently checks `token == generation` and silently drops events for inactive worktrees. The change adds a `dirtyWorktrees: Set<UUID>` to `AppModel`. When an FS event arrives for an inactive worktrees, the handler inserts the worktree ID into the set instead of dropping the event. On activation (`RightPanelModel.activate`), the dirty flag is atomically consumed and triggers a full refresh.

The FSEvent monitor continues to run (kernel pushes events are cheap), but the handler only marks dirty instead of performing I/O. Unclassifiable FS events (bulk events with no path list) always mark dirty.

### UsageStore Timer (C5)

`UsageStore.start()` creates a timer that runs forever. The change adds `updatePolling()` which checks if any provider is enabled and stops/starts the timer accordingly. `.onChange(of:)` in `AIProvidersSettingsView` calls `updatePolling()` when any `showInBar` pref changes.

### Sidebar Animations (C3)

Remove the three collection-wide `.animation(.easeInOut(duration: 0.18), value:)` modifiers from `SidebarView`. Individual row animations (hover highlight, `RunningDots` pulse) are unaffected.

### Agent Status Computation (C4)

Instead of a fragile global cache with manual invalidation at many mutation sites, compute the sorted worktree list once per render pass using a scoped computed property on `SidebarView`. The key insight: `AttentionSort.sorted(...)` is called inside the `ForEach` in `SidebarView.body`, which re-evaluates on every `model.worktrees` or `model.agentActivity` change. Lifting it into a computed property on the view struct lets SwiftUI's dependency tracking narrow the re-evaluation scope.

## Tech Stack

- Swift 6, macOS 15+
- `swift-testing` (`@Test` / `#expect`)
- `Scripts/ci.sh` as verification gate
- No new dependencies

## Global Constraints

- No invisible file tree or Git refresh. One coalesced refresh on activation, at most one follow-up for events during refresh.
- Zero-pane worktrees follow the same dirty/refresh lifecycle as non-empty worktrees.
- `UsageStore` timer is absent when all providers are disabled. The timer/task is owned/cancelled explicitly.
- Remove only collection-wide sidebar animations. Per-row transitions (`.transition(.opacity)`) may be kept.
- Compute status once per render pass using a scoped computed property, not a global cache with manual invalidation.
- `Scripts/ci.sh` must print `CI OK`.

---

## Interfaces

### Consumed

```swift
// RightPanelModel (App/RightPanel/RightPanelModel.swift)
func activate(worktree: Worktree?, isGitRepository: Bool) async  // line 57
func deactivate()                                                 // line 71
private func scheduleRefresh(paths: [String], token: Int)        // line 186
private func refresh(changedPaths: [String], token: Int, forceAllLoadedDirectories: Bool) async  // line 199

// AppModel (App/AppModel.swift)
var dirtyWorktrees: Set<UUID>  // new
var selectedWorktree: Worktree?  // line 26
var worktrees: [UUID: [Worktree]]  // line 18
var tabs: [UUID: [WorkspaceTab]]  // line 152
var agentActivity: AgentActivityModel  // line 97
func statusForWorktree(_ worktree: Worktree) -> AgentStatus?  // line 101

// UsageStore (App/UsageStore.swift)
func start()  // line 41
func stop()   // new
func updatePolling()  // new
private nonisolated(unsafe) var timer: Task<Void, Never>?  // line 31

// AIProvidersSettingsView (App/AIProvidersSettingsView.swift)
.onChange(of: showClaudeInBar)  // line 214

// SidebarView (App/SidebarView.swift)
var body: some View  // line 14
.animation(.easeInOut(duration: 0.18), value: model.expandedProjectIds)  // line 73
.animation(.easeInOut(duration: 0.18), value: model.worktrees.mapValues { $0.map(\.id) })  // line 74
.animation(.easeInOut(duration: 0.18), value: model.tabs.mapValues { $0.map(\.id) })  // line 75

// ContentView (App/ContentView.swift)
.task(id: rightPanelContext)  // line 62
```

### Produced

```swift
// AppModel additions:
var dirtyWorktrees: Set<UUID>

// RightPanelModel additions:
private weak var appModel: AppModel?
func setAppModel(_ model: AppModel)
@ObservationIgnored private var isRefreshing = false

// UsageStore additions:
func updatePolling()
func stop()

// SidebarView changes:
// Remove three .animation modifiers
// Add computed property for sorted worktrees
```

---

## Tasks

### Task 1: Add `dirtyWorktrees` set to AppModel

**File:** `App/AppModel.swift`

Add after line 97 (`var agentActivity`):

```
oldString:     var agentActivity = AgentActivityModel()
newString:     var agentActivity = AgentActivityModel()

    /// Per-worktree dirty state for the right panel. Set by
    /// RightPanelModel when a filesystem event arrives for an inactive
    /// worktree. Consumed atomically on activation.
    var dirtyWorktrees: Set<UUID> = []
```

**Verification:**
```bash
Scripts/ci.sh
```
Expected: `CI OK`.

---

### Task 2: Wire dirty marker into RightPanelModel

**File:** `App/RightPanel/RightPanelModel.swift`

**Step 2.1 — Add `appModel` reference and `isRefreshing` flag:**

```
oldString:     @ObservationIgnored private var generation = 0
newString:     @ObservationIgnored private var generation = 0
    @ObservationIgnored private weak var appModel: AppModel?
    @ObservationIgnored private var isRefreshing = false
```

**Step 2.2 — Add `setAppModel` method:**

Add after line 38 (`@ObservationIgnored private var pendingPaths: Set<String> = []`):

```
oldString:     var rootURL: URL? {
newString:     func setAppModel(_ model: AppModel) {
        appModel = model
    }

    var rootURL: URL? {
```

**Step 2.3 — Modify `activate` to consume dirty state (lines 57-69):**

```
oldString:     func activate(worktree: Worktree?, isGitRepository: Bool) async {
        guard self.worktree?.id != worktree?.id || self.isGitRepository != isGitRepository else {
            return
        }
        deactivate()
        guard let worktree else { return }
        self.worktree = worktree
        self.isGitRepository = isGitRepository
        let token = generation
        await loadInitial(token: token)
        guard token == generation else { return }
        await startMonitor(token: token)
    }
newString:     func activate(worktree: Worktree?, isGitRepository: Bool) async {
        guard self.worktree?.id != worktree?.id || self.isGitRepository != isGitRepository else {
            // Same worktree: if dirty, consume dirty flag and refresh.
            if let wt = worktree, appModel?.dirtyWorktrees.remove(wt.id) != nil {
                await refresh(changedPaths: [], token: generation, forceAllLoadedDirectories: true)
            }
            return
        }
        deactivate()
        guard let worktree else { return }
        self.worktree = worktree
        self.isGitRepository = isGitRepository
        let token = generation
        let wasDirty = appModel?.dirtyWorktrees.remove(worktree.id) != nil
        await loadInitial(token: token)
        guard token == generation else { return }
        if wasDirty {
            await refresh(changedPaths: [], token: token, forceAllLoadedDirectories: true)
        }
        await startMonitor(token: token)
    }
```

**Step 2.4 — Modify `scheduleRefresh` to set dirty flag for inactive worktrees (lines 186-197):**

> **Note:** The `if let wt = worktree` guard in the stale-token branch is intentional — `worktree` may already be `nil` after `deactivate()` cleared it, and we must not force-unwrap or insert a nil ID into the dirty set.

```
oldString:     private func scheduleRefresh(paths: [String], token: Int) {
        guard token == generation else { return }
        pendingPaths.formUnion(paths)
        debounceTask?.cancel()
        debounceTask = Task { [weak self] in
            try? await Task.sleep(for: .milliseconds(250))
            guard !Task.isCancelled, let self, token == self.generation else { return }
            let paths = Array(self.pendingPaths)
            self.pendingPaths.removeAll()
            await self.refresh(changedPaths: paths, token: token, forceAllLoadedDirectories: false)
        }
    }
newString:     private func scheduleRefresh(paths: [String], token: Int) {
        guard token == generation else {
            // This worktree is no longer active. Mark it dirty so the next
            // activation picks up the changes.
            if let wt = worktree { appModel?.dirtyWorktrees.insert(wt.id) }
            return
        }
        pendingPaths.formUnion(paths)
        debounceTask?.cancel()
        debounceTask = Task { [weak self] in
            try? await Task.sleep(for: .milliseconds(250))
            guard !Task.isCancelled, let self, token == self.generation else { return }
            let paths = Array(self.pendingPaths)
            self.pendingPaths.removeAll()
            await self.refresh(changedPaths: paths, token: token, forceAllLoadedDirectories: false)
        }
    }
```

**Step 2.5 — Modify `refresh` to guard against concurrent refreshes (lines 199-223):**

> **Note:** Double-dirty marking during an in-progress refresh is safe. If a second FS event arrives while `isRefreshing` is true, the guard inserts the worktree ID into `dirtyWorktrees` again (a no-op for the same worktree, or a new entry for a different one). This may cause at most one follow-up refresh when the current refresh completes and the worktree is re-activated.

```
oldString:     private func refresh(
        changedPaths: [String], token: Int, forceAllLoadedDirectories: Bool
    ) async {
        guard token == generation, let rootURL, let worktree else { return }
newString:     private func refresh(
        changedPaths: [String], token: Int, forceAllLoadedDirectories: Bool
    ) async {
        guard !isRefreshing else {
            if let wt = worktree { appModel?.dirtyWorktrees.insert(wt.id) }
            return
        }
        isRefreshing = true
        defer { isRefreshing = false }
        guard token == generation, let rootURL, let worktree else { return }
```

**Step 2.6 — Wire `setAppModel` in ContentView:**

**File:** `App/ContentView.swift`, line 62

```
oldString:         .task(id: rightPanelContext) {
            guard rightPanelVisible else {
                rightPanelModel.deactivate()
                return
            }
            await rightPanelModel.activate(
                worktree: model.selectedWorktree,
                isGitRepository: rightPanelContext.gitProject)
        }
newString:         .task(id: rightPanelContext) {
            rightPanelModel.setAppModel(model)
            guard rightPanelVisible else {
                rightPanelModel.deactivate()
                return
            }
            await rightPanelModel.activate(
                worktree: model.selectedWorktree,
                isGitRepository: rightPanelContext.gitProject)
        }
```

**Verification:**
```bash
Scripts/ci.sh
```
Expected: `CI OK`.

---

### Task 3: Add `updatePolling` and `stop` to UsageStore

**File:** `App/UsageStore.swift`

**Step 3.1 — Make `timer` internal for testing:**

```
oldString:     private nonisolated(unsafe) var timer: Task<Void, Never>?
newString:     nonisolated(unsafe) var timer: Task<Void, Never>?
```

**Step 3.2 — Add `stop()` and `updatePolling()` methods:**

> **Note on key alignment:** The `@AppStorage` properties in `ContentView.swift` already declare explicit UserDefaults keys (`"usage.claude.showInBar"`, `"usage.codex.showInBar"`, `"usage.opencodeGo.showInBar"`, `"usage.ollamaCloud.showInBar"`), so the `updatePolling()` method below reads the same keys. No change to `ContentView` is needed — the keys are already explicit and match. This is option (a) from the review: explicit keys in `ContentView`, `usage.*` keys in `updatePolling`.

Add after line 58 (`restartTimer`):

```
oldString:     func refresh() async {
newString:     /// Idempotent: stops the timer if all providers are disabled, starts it
    /// (or keeps it running) if at least one is enabled. Call whenever a
    /// showInBar pref changes.
    func updatePolling() {
        let anyEnabled = UserDefaults.standard.bool(forKey: "usage.claude.showInBar")
            || UserDefaults.standard.bool(forKey: "usage.codex.showInBar")
            || UserDefaults.standard.bool(forKey: "usage.opencodeGo.showInBar")
            || UserDefaults.standard.bool(forKey: "usage.ollamaCloud.showInBar")
        if anyEnabled {
            if timer == nil { start() }
        } else {
            stop()
        }
    }

    func stop() {
        timer?.cancel()
        timer = nil
    }

    func refresh() async {
```

**Verification:**
```bash
Scripts/ci.sh
```
Expected: `CI OK`.

---

### Task 4: Wire `updatePolling` to `showInBar` changes

**File:** `App/AIProvidersSettingsView.swift`, lines 214-219

**Edit:**
```
oldString:         .onChange(of: showClaudeInBar) { _, isOn in
            if isOn { Task { await store.refresh() } }
        }
        .onChange(of: workspaceIdOverride) { _, _ in
            Task { await store.refreshOpencodeGo() }
        }
newString:         .onChange(of: showClaudeInBar) { _, isOn in
            store.updatePolling()
            if isOn { Task { await store.refresh() } }
        }
        .onChange(of: showCodexInBar) { _, isOn in
            store.updatePolling()
            if isOn { Task { await store.refreshCodex() } }
        }
        .onChange(of: showOpencodeGoInBar) { _, isOn in
            store.updatePolling()
            if isOn { Task { await store.refreshOpencodeGo() } }
        }
        .onChange(of: showOllamaCloudInBar) { _, isOn in
            store.updatePolling()
            if isOn { Task { await store.refreshOllamaCloud() } }
        }
        .onChange(of: workspaceIdOverride) { _, _ in
            Task { await store.refreshOpencodeGo() }
        }
```

**Verification:**
```bash
Scripts/ci.sh
```
Expected: `CI OK`.

---

### Task 5: Remove collection-wide sidebar animations

**File:** `App/SidebarView.swift`, lines 73-75

**Edit:**
```
oldString:                     .animation(.easeInOut(duration: 0.18), value: model.expandedProjectIds)
                    .animation(.easeInOut(duration: 0.18), value: model.worktrees.mapValues { $0.map(\.id) })
                    .animation(.easeInOut(duration: 0.18), value: model.tabs.mapValues { $0.map(\.id) })
newString:                     // Collection-wide animations removed. Individual row transitions
                    // (hover highlight, RunningDots pulse) are unaffected.
```

**Verification:**
```bash
Scripts/ci.sh
```
Expected: `CI OK`.

---

### Task 6: Compute sorted worktrees once per render pass

**File:** `App/SidebarView.swift`, line 28

**Current:**
```swift
ForEach(AttentionSort.sorted(model.worktrees[project.id] ?? [], statusOf: model.statusForWorktree)) { worktree in
```

**Change:** Lift the sorted list into a computed property on `SidebarView`. This is already a computed property on the view struct, so SwiftUI's dependency tracking applies. The key is to ensure `statusForWorktree` reads `agentActivity.agentStatus` through a single access point so SwiftUI can narrow the dependency.

Add a computed property to `SidebarView`:

```
oldString:     @State private var showAddProjectSheet = false
newString:     @State private var showAddProjectSheet = false

    /// Worktrees sorted by agent-status urgency, computed once per render
    /// pass. SwiftUI's dependency tracking narrows re-evaluation to the
    /// specific worktree rows whose status actually changed.
    private func sortedWorktrees(for project: Project) -> [Worktree] {
        AttentionSort.sorted(
            model.worktrees[project.id] ?? [],
            statusOf: model.statusForWorktree
        )
    }
```

Then replace the inline `AttentionSort.sorted(...)` call at line 28:

```
oldString:                             ForEach(AttentionSort.sorted(model.worktrees[project.id] ?? [], statusOf: model.statusForWorktree)) { worktree in
newString:                             ForEach(sortedWorktrees(for: project)) { worktree in
```

**Verification:**
```bash
Scripts/ci.sh
```
Expected: `CI OK`.

---

### Task 7: Manual verification

**Step 7.1 — Dirty marker:**
1. Open two git projects, each with a worktree.
2. Open the right panel (Files mode) for worktree A.
3. Switch to worktree B in the sidebar.
4. Using Terminal, `touch` a new file in worktree A's directory.
5. Switch back to worktree A — the new file should appear in the file tree (dirty state consumed on activation).

**Step 7.2 — UsageStore timer:**
1. Open Settings > AI Providers, disable all four `showInBar` toggles.
2. Verify the usage bar disappears and the timer stops (check with Activity Monitor that no network requests are made).
3. Re-enable one toggle — the usage bar reappears and polling resumes.

**Step 7.3 — Sidebar animations:**
1. Expand/collapse projects in the sidebar — verify no fade animation (instant transition).
2. Open/close tabs — verify no 180 ms hitch.
3. Hover highlight on rows still animates.

**Step 7.4 — Status computation:**
1. Spawn an agent in a worktree — verify the status glyph updates immediately.
2. Run existing `AgentActivityModelTests` to confirm the underlying model works.

---

## Acceptance Criteria

| AC | Verification |
|----|-------------|
| AC3 | No main-thread stalls > 100 ms during worktree switch (measured via Instruments HID template) |
| AC4 | No file tree or Git refresh for invisible worktrees (`os_signpost` `panelRefresh` — zero intervals while panel hidden) |
| AC8 | Correct agent state and notifications for hidden panes (sidebar badge updates on activation) |
| AC12 | `Scripts/ci.sh` prints `CI OK` |

## Commit Messages

```bash
# After Task 1:
git add App/AppModel.swift
git commit -m "feat: add dirtyWorktrees set to AppModel for right-panel dirty state"

# After Task 2:
git add App/RightPanel/RightPanelModel.swift App/ContentView.swift
git commit -m "feat: mark worktrees dirty instead of refreshing when right panel is hidden"

# After Tasks 3-4:
git add App/UsageStore.swift App/AIProvidersSettingsView.swift
git commit -m "feat: stop usage polling timer when all providers are disabled"

# After Task 5:
git add App/SidebarView.swift
git commit -m "perf: remove collection-wide sidebar animations"

# After Task 6:
git add App/SidebarView.swift
git commit -m "perf: compute sorted worktrees once per render pass"
```
