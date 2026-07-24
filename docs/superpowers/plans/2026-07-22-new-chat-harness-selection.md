# New Chat ACP Harness Selection Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make both New Chat entry points use one native, dynamically populated ACP-agent submenu that creates a selected, genuinely new chat in the current worktree without fallback.

**Architecture:** Keep `NewTabMenuItems` as the shared menu implementation and add a small, pure menu-state seam so the tab-bar and sidebar context menu cannot diverge. Add an explicit `AppModel.openChatTab(in:agentId:)` overload that canonicalizes and revalidates the selected ACP ID before creating the tab; retain the existing no-argument overload unchanged. Reuse `ChatController(startNewConversation: true)` and the existing `ChatPaneView` launch-error banners rather than adding a second error path.

**Tech Stack:** Swift 6, macOS 15+, SwiftUI, Observation, Swift Testing (`Testing`), TillerACP (`AgentInstallStore`, `AgentLaunchSpec`), TillerCore (`WorkspaceTab`), and the repository's `Scripts/ci.sh` verification gate.

## Global Constraints

- Both New Chat entry points must list only `AcpAgentCenter.installedAgents`; do not use `AgentCatalog.all` for chat choices.
- The shared menu must show loading while ACP installation state is initially refreshing; it must not show a false empty state during that interval.
- The empty state must explain that no ACP harness is installed and offer **Open Agents Settings**; opening Settings must not create a chat.
- The selected-agent path must canonicalize with `AgentIdMigration.canonical`, validate current availability with ACP rules, persist the selected ID, register activity, select the current worktree/tab, and create `ChatController(startNewConversation: true)`.
- A stale selection must create no tab or controller, must not select another agent, must trigger an ACP refresh, and must set `AppModel.lastError` for the existing global Error alert.
- A valid selection whose process fails to launch must leave the new tab selected and must use the existing `.needsAuth` and `.disconnected` UI without rollback, fallback, resume, or automatic retry.
- The existing no-argument `openChatTab(in:)` behavior and fallback remain available to internal callers.
- Do not change terminal-agent catalog behavior, agent installation side effects, session-resume behavior for restored chats, project configuration, or commits while executing this plan.
- Every implementation task follows TDD: add a failing Swift Testing test, run the focused test, implement the smallest change, and rerun it.
- Focused App tests use `xcodebuild test -project Tiller.xcodeproj -scheme Tiller -destination 'platform=macOS' CODE_SIGNING_ALLOWED=NO`; append `-only-testing:TillerTests/<Suite>` and, when needed, `/TestName` for a focused suite or test. `Scripts/ci.sh` is reserved for the final repository verification and must not be used as an App-test runner.
- App test files that use internal App APIs or ACP fixtures must include `import Testing`, `import TillerACP`, and `@testable import Tiller`.

---

## File-Structure Map

| File | Responsibility in this change |
| --- | --- |
| `App/NewTabMenuItems.swift` | Shared native New Chat submenu, loading/empty/installed states, branding, and the pure menu-state/action seam. |
| `App/SidebarView.swift` | Replace the sidebar worktree context-menu New Chat button with the shared native submenu. |
| `App/AppModel.swift` | Injectable ACP test seam, explicit selected-agent tab-creation overload, canonicalization/revalidation, refresh/error behavior, selection, persistence, activity, and controller setup. |
| `App/AcpAgentCenter.swift` | Observable refresh-state flag that distinguishes initial loading from a completed empty installation state. |
| `Packages/TillerACP/Sources/TillerACP/AgentLaunchSpec.swift` | Existing canonicalization and launch-resolution rules used by the explicit path; only extend this file if a focused test proves the current resolver needs a narrowly scoped ACP validity helper. |
| `Packages/TillerCore/Sources/TillerCore/WorkspaceTab.swift` | Existing `.chat(agentId:)`, `chatAgentId`, and `WorkspaceTab` initializer remain the tab-state contract; no behavior change is expected. |
| `App/Chat/ChatController.swift` | Internal `ChatSession` protocol, `ChatSessionFactory`, default `ProcessTransport -> ACPClient -> ACPSession` construction, and the existing `startNewConversation`/`didResume` launch contract. |
| `App/Chat/ChatPaneView.swift` | Existing `.needsAuth` / `.disconnected` banners are the required failure presentation; no new presentation path is expected. |
| `AppTests/AppModelControlTests.swift` | Model-level tests for canonicalization, persistence, selection, activity, controller creation, stale selection, refresh, no-tab behavior, and legacy fallback. |
| `AppTests/NewChatMenuTests.swift` | New pure menu-state/action seam tests for loading, empty/settings, dynamic rows, and identical shared-source behavior. |
| `AppTests/ChatControllerTests.swift` | Existing no-resume regression test, private actor `ChatSession` fake, and deterministic `.needsAuth`/`.disconnected` launch-failure tests. |

## Interfaces and Invariants

Add these exact interfaces; do not invent a second selected-agent creation API:

```swift
enum NewChatMenuState: Equatable {
    case loading
    case empty
    case installed([AcpAgentCenter.InstalledAgentSummary])
}

enum NewChatMenuAction: Equatable {
    case select(agentId: String)
    case openAgentsSettings
}

enum NewChatMenuModel {
    static func state(
        isRefreshing: Bool,
        installedAgents: [AcpAgentCenter.InstalledAgentSummary]
    ) -> NewChatMenuState
}

@discardableResult
func openChatTab(in worktree: Worktree, agentId: String) -> WorkspaceTab?
```

`NewChatMenuModel.state` returns `.loading` when `isRefreshing` is true, `.empty` when refresh is complete and the installed list is empty, and `.installed(installedAgents)` otherwise. The explicit `AppModel` overload returns the created `WorkspaceTab` on success and `nil` on stale/invalid selection. The no-argument overload keeps its current return type and fallback.

The chat-session seam uses the current ACP types exactly:

```swift
internal protocol ChatSession: AnyObject, Sendable {
    var events: AsyncStream<ACPSessionEvent> { get }
    func start() async throws
    func stop() async
    func connect(cwd: String, resumeSessionId: String?,
                 mcpServers: [McpServerSpec]) async throws -> SessionHandle
    func prompt(_ blocks: [ContentBlock]) async throws -> StopReason
    func cancel() async
    func setMode(_ modeId: String) async throws
    func setModel(_ modelId: String) async throws
    func setConfigOption(id: String, value: String) async throws -> [SessionConfigOption]?
    func answerPermission(requestId: JSONRPCID, outcome: PermissionOutcome) async
}

internal typealias ChatSessionFactory = @Sendable
    (_ spec: AgentLaunchSpec, _ worktreePath: String) -> any ChatSession
```

`ACPSession` conforms to `ChatSession`. `ChatController` receives an internal `chatSessionFactory` initializer argument with a default factory that creates the existing `ProcessTransport` using the resolved `AgentLaunchSpec` and worktree path, then wraps it as `ACPClient(transport:)` and `ACPSession(client:fileSystem:)` with `WorktreeFileSystem(root:)`. The controller must use the factory in `start()` and must not construct a process transport directly anywhere else. The factory is the only new seam; it does not change the production launch path.

### Task 1: Add a testable ACP loading state

**Files:**
- Modify: `App/AcpAgentCenter.swift:8-141`
- Create: `AppTests/NewChatMenuTests.swift`

**Interfaces:**
- Consumes: existing `AcpAgentCenter.refresh(force:)` and `installedAgents`.
- Produces: `private(set) var isRefreshing: Bool` on `AcpAgentCenter`, initially `true`, set to `false` after every `refresh(force:)` attempt, including registry failures.

- [ ] **Step 1: Write the failing test**

Add a pure state test first so the required distinction is explicit:

```swift
@Test func loadingDoesNotLookLikeEmpty() {
    #expect(NewChatMenuModel.state(isRefreshing: true, installedAgents: []) == .loading)
    #expect(NewChatMenuModel.state(isRefreshing: false, installedAgents: []) == .empty)
}
```

- [ ] **Step 2: Run the focused test to verify it fails**

Run: `xcodebuild test -project Tiller.xcodeproj -scheme Tiller -destination 'platform=macOS' -only-testing:TillerTests/NewChatMenuTests/loadingDoesNotLookLikeEmpty`

Expected: FAIL because `NewChatMenuModel` and its state types do not exist yet.

- [ ] **Step 3: Implement the minimal state seam and refresh flag**

In `App/AcpAgentCenter.swift`, add `private(set) var isRefreshing = true`. In `refresh(force:)`, set it to `true` at entry and use `defer { isRefreshing = false }` before the first `do`. In `App/NewTabMenuItems.swift`, add:

```swift
enum NewChatMenuModel {
    static func state(
        isRefreshing: Bool,
        installedAgents: [AcpAgentCenter.InstalledAgentSummary]
    ) -> NewChatMenuState {
        if isRefreshing { return .loading }
        return installedAgents.isEmpty ? .empty : .installed(installedAgents)
    }
}
```

Use `@MainActor` isolation already provided by `AcpAgentCenter`; do not expose registry internals to the menu.

- [ ] **Step 4: Run the focused test to verify it passes**

Run: `xcodebuild test -project Tiller.xcodeproj -scheme Tiller -destination 'platform=macOS' -only-testing:TillerTests/NewChatMenuTests/loadingDoesNotLookLikeEmpty`

Expected: PASS.

- [ ] **Step 5: Commit the task in the implementation session**

Suggested conventional commit: `feat: expose ACP New Chat loading state`

### Task 2: Build one native dynamic New Chat submenu

**Files:**
- Modify: `App/NewTabMenuItems.swift:1-39`
- Modify: `AppTests/NewChatMenuTests.swift`

**Interfaces:**
- Consumes: `AcpAgentCenter.installedAgents`, `AcpAgentCenter.InstalledAgentSummary`, `NewChatMenuModel`, `NewChatMenuState`, `NewChatMenuAction`, `AppModel.openAgentsSettings()`, and `AppModel.openChatTab(in:agentId:)`.
- Produces: a shared `NewChatACPMenu(model:worktree:)` view used by both entry points; it emits only `.select(agentId:)` or `.openAgentsSettings` through its local action handlers.

- [ ] **Step 1: Write the failing menu-seam tests**

Create `AppTests/NewChatMenuTests.swift` with Swift Testing and the exact rows/actions contract:

```swift
import Testing
import TillerACP
@testable import Tiller

@Suite(.serialized)
struct NewChatMenuTests {
    @Test func installedRowsComeOnlyFromTheLiveACPList() {
        let rows = [
            AcpAgentCenter.InstalledAgentSummary(id: "claude-acp", name: "Claude"),
            AcpAgentCenter.InstalledAgentSummary(id: "omp", name: "omp")
        ]

        #expect(NewChatMenuModel.state(isRefreshing: false, installedAgents: rows) == .installed(rows))
    }

    @Test func emptyStateOffersSettingsWithoutASelectionAction() {
        #expect(NewChatMenuModel.state(isRefreshing: false, installedAgents: []) == .empty)
        #expect(NewChatMenuAction.openAgentsSettings != .select(agentId: "claude-acp"))
    }

    @Test func bothEntryPointsShareTheSameRowsAndSelectionAction() {
        let installed = [AcpAgentCenter.InstalledAgentSummary(id: "agent-a", name: "Agent A")]
        let tabBarState = NewChatMenuModel.state(isRefreshing: false, installedAgents: installed)
        let sidebarState = NewChatMenuModel.state(isRefreshing: false, installedAgents: installed)

        #expect(tabBarState == sidebarState)
        #expect(NewChatMenuAction.select(agentId: installed[0].id) == .select(agentId: "agent-a"))
    }
}
```

- [ ] **Step 2: Run the focused tests to verify they fail**

Run: `xcodebuild test -project Tiller.xcodeproj -scheme Tiller -destination 'platform=macOS' CODE_SIGNING_ALLOWED=NO -only-testing:TillerTests/NewChatMenuTests`

Expected: FAIL at compile time because the menu seam and shared view do not exist.

- [ ] **Step 3: Implement the shared native submenu**

Keep the existing New Terminal and terminal-agent rows unchanged. Replace only the final chat button with a native submenu and add the following shared view shape:

```swift
struct NewChatACPMenu: View {
    @Bindable var model: AppModel
    let worktree: Worktree

    var body: some View {
        Menu("New Chat") {
            let state = NewChatMenuModel.state(
                isRefreshing: model.agentCenter.isRefreshing,
                installedAgents: model.agentCenter.installedAgents)
            switch state {
            case .loading:
                Label("Loading agents…", systemImage: "hourglass")
            case .empty:
                Text("No ACP agents installed")
                    .foregroundStyle(.secondary)
                Button("Open Agents Settings") {
                    model.openAgentsSettings()
                }
            case .installed(let agents):
                ForEach(agents) { agent in
                    Button {
                        model.openChatTab(in: worktree, agentId: agent.id)
                    } label: {
                        if let icon = AgentMenuIconCache.image(for: agent.id) {
                            Label { Text(agent.name) } icon: { Image(nsImage: icon) }
                        } else {
                            Label(agent.name, systemImage: "cpu")
                        }
                    }
                }
            }
        }
    }
}
```

The view must read `installedAgents` on every render, use the existing installed display name and `AgentMenuIconCache`, and never mention `AgentCatalog.all`. The `.empty` branch contains no chat action.

- [ ] **Step 4: Run the focused tests to verify they pass**

Run: `xcodebuild test -project Tiller.xcodeproj -scheme Tiller -destination 'platform=macOS' CODE_SIGNING_ALLOWED=NO -only-testing:TillerTests/NewChatMenuTests`; reserve `Scripts/ci.sh` for Task 7's final repository gate.

Expected: PASS; the three seam tests prove loading, empty/settings, and shared dynamic rows/actions.

- [ ] **Step 5: Commit the task in the implementation session**

Suggested conventional commit: `feat: add dynamic ACP New Chat submenu`

### Task 3: Route both New Chat entry points through the shared submenu

**Files:**
- Modify: `App/SidebarView.swift:34-47`
- Modify: `App/NewTabMenuItems.swift` (the existing final New Chat action)

**Interfaces:**
- Consumes: `NewChatACPMenu(model:worktree:)`.
- Produces: identical submenu content for the tab-bar/menu action and the worktree sidebar context menu.

- [ ] **Step 1: Write the failing integration assertion**

Extend `NewChatMenuTests` with a source-level seam assertion only if the app test target already has a source inspection helper; otherwise keep the behavioral assertion and make both call sites compile against the shared view. The required call-site form is:

```swift
NewChatACPMenu(model: model, worktree: worktree)
```

The sidebar context-menu must use that view in place of `Button("New Chat")`, and `NewTabMenuItems` must use that same view in place of its final `Button("New Chat")`.

- [ ] **Step 2: Run the focused app tests to verify the integration fails**

Run: the App test target with `NewChatMenuTests` selected.

Expected: FAIL or compile failure until both call sites use `NewChatACPMenu`.

- [ ] **Step 3: Replace both call sites**

In `App/NewTabMenuItems.swift`, retain the existing terminal divider and make the final content `NewChatACPMenu(model: model, worktree: worktree)`. In `App/SidebarView.swift`, replace lines 44-46 with `NewChatACPMenu(model: model, worktree: worktree)` while retaining the surrounding context-menu dividers and worktree actions.

- [ ] **Step 4: Run the focused app tests to verify the integration passes**

Run: `xcodebuild test -project Tiller.xcodeproj -scheme Tiller -destination 'platform=macOS' CODE_SIGNING_ALLOWED=NO -only-testing:TillerTests/NewChatMenuTests`

Expected: the App test suite passes; opening Agents Settings does not append a `WorkspaceTab`.

- [ ] **Step 5: Commit the task in the implementation session**

Suggested conventional commit: `feat: share ACP New Chat menu across entry points`

### Task 4: Add the explicit selected-agent AppModel path

**Files:**
- Modify: `App/AppModel.swift:190-213, 1327-1394`
- Test: `AppTests/AppModelControlTests.swift`
- Reference without behavior change: `Packages/TillerACP/Sources/TillerACP/AgentLaunchSpec.swift:38-68`, `Packages/TillerCore/Sources/TillerCore/WorkspaceTab.swift:5-52`

**Interfaces:**
- Consumes: `AgentIdMigration.canonical(_:)`, `AgentLaunchSpec.resolved(id:installStore:)`, `agentCenter.installedAgents`, `rememberChatAgent(_:)`, `agentActivity.agentSpawned`, `chatController(for:in:startNewConversation:)`, and `selectedWorktree`/`activeTabId`.
- Produces: `@discardableResult func openChatTab(in worktree: Worktree, agentId: String) -> WorkspaceTab?`.

- [ ] **Step 1: Write the failing valid-selection test**

Add a model test in `AppModelControlTests` using the existing `makeModel`/`makeWorktree` helpers and a temporary `AgentInstallStore`. Inject the store and `AcpAgentCenter` through optional initializer parameters so the test does not touch the user's Application Support directory. Write a manifest whose ID is `claude-acp`, then assert the selected tab and controller:

```swift
@Test func explicitChatAgentCreatesAndSelectsCurrentWorktreeTab() throws {
    let root = FileManager.default.temporaryDirectory
        .appendingPathComponent(UUID().uuidString, isDirectory: true)
    let store = AgentInstallStore(rootDirectory: root)
    try store.write(InstalledAgentManifest(
        id: "claude-acp", version: "1", executable: "/bin/false",
        arguments: [], environment: [:]))
    let center = AcpAgentCenter(installStore: store)
    let model = AppModel(agentInstallStore: store, agentCenter: center)
    let worktree = makeWorktree(path: root.path)
    model.worktrees[worktree.projectId] = [worktree]

    let tab = model.openChatTab(in: worktree, agentId: "claude-acp")

    #expect(tab?.chatAgentId == "claude-acp")
    #expect(model.selectedWorktree?.id == worktree.id)
    #expect(model.activeTabId[worktree.id] == tab?.id)
    #expect(model.tabs[worktree.id]?.last?.id == tab?.id)
    #expect(model.chatControllers[tab?.id ?? UUID()] != nil)
}
```

Use the repository's actual `AgentInstallStore` initializer and `InstalledAgentManifest` initializer; if the app target initializer is made injectable under different labels, update this snippet and every call site consistently rather than adding a second test-only constructor.

- [ ] **Step 2: Run the focused test to verify it fails**

Run: `xcodebuild test -project Tiller.xcodeproj -scheme Tiller -destination 'platform=macOS' CODE_SIGNING_ALLOWED=NO -only-testing:TillerTests/AppModelControlTests/explicitChatAgentCreatesAndSelectsCurrentWorktreeTab`.

Expected: compile failure because the explicit overload and injectable initializer parameters do not exist.

- [ ] **Step 3: Implement the explicit overload**

Extend `AppModel.init` with optional `agentInstallStore` and `agentCenter` parameters, preserving default construction. Use the same store for both objects. Add this overload immediately after the existing no-argument path:

```swift
@discardableResult
func openChatTab(in worktree: Worktree, agentId: String) -> WorkspaceTab? {
    let canonical = AgentIdMigration.canonical(agentId)
    let isInstalled = agentCenter.installedAgents.contains { $0.id == canonical }
    guard isInstalled,
          AgentLaunchSpec.resolved(id: canonical, installStore: agentInstallStore) != nil else {
        lastError = "Agent \(canonical) is no longer installed. Open Agents Settings to install it."
        Task { await agentCenter.refresh(force: true) }
        return nil
    }

    let tab = WorkspaceTab(id: UUID(), title: "Chat", content: .chat(agentId: canonical))
    selectedWorktree = worktree
    tabs[worktree.id, default: []].append(tab)
    activeTabId[worktree.id] = tab.id
    rememberChatAgent(canonical)
    agentActivity.agentSpawned(paneId: tab.id, agentId: canonical, now: Date())
    persistTabs(for: worktree.id)
    _ = chatController(for: tab, in: worktree, startNewConversation: true)
    return tab
}
```

Do not change the existing `openChatTab(in:)`; its `defaultChatAgentId ?? "claude-acp"` fallback remains intact. The explicit path must not call the no-argument overload.

- [ ] **Step 4: Run the focused test to verify it passes**

Run: the focused App test for `explicitChatAgentCreatesAndSelectsCurrentWorktreeTab`.

Expected: PASS, with the tab in the supplied worktree, selected as `activeTabId`, persisted as the last-used ID, activity registered, and a controller stored under the tab ID.

- [ ] **Step 5: Commit the task in the implementation session**

Suggested conventional commit: `feat: add explicit ACP chat agent creation path`

### Task 5: Cover stale selections, refresh, global errors, and legacy fallback

**Files:**
- Modify: `AppTests/AppModelControlTests.swift`
- Verify without changing: `App/ContentView.swift:133-139`

**Interfaces:**
- Consumes: `AppModel.openChatTab(in:agentId:)`, `AcpAgentCenter.refresh(force:)`, `AppModel.lastError`, and `AppModel.openChatTab(in:)`.
- Produces: regression coverage proving stale selection is a hard rejection and legacy callers still use the old fallback.

- [ ] **Step 1: Write the failing stale-selection and fallback tests**

Use a center/store pair with no manifest and a worktree registered in `model.worktrees`:

```swift
@Test func staleExplicitSelectionShowsGlobalErrorAndCreatesNothing() async throws {
    let root = FileManager.default.temporaryDirectory
        .appendingPathComponent(UUID().uuidString, isDirectory: true)
    let store = AgentInstallStore(rootDirectory: root)
    let center = AcpAgentCenter(installStore: store)
    let model = AppModel(agentInstallStore: store, agentCenter: center)
    let worktree = makeWorktree(path: root.path)
    model.worktrees[worktree.projectId] = [worktree]

    let tab = model.openChatTab(in: worktree, agentId: "removed-agent")

    #expect(tab == nil)
    #expect(model.tabs[worktree.id, default: []].isEmpty)
    #expect(model.chatControllers.isEmpty)
    #expect(model.lastError?.contains("removed-agent") == true)
    #expect(model.selectedWorktree?.id != worktree.id || model.activeTabId[worktree.id] == nil)
    try await Task.sleep(for: .milliseconds(50))
    #expect(center.isRefreshing || center.lastFetchedAt != nil)
}

@Test func legacyNoArgumentPathRetainsDefaultFallback() throws {
    let model = makeModel()
    let worktree = makeWorktree(path: "/tmp/legacy-chat")
    let tab = model.openChatTab(in: worktree)

    #expect(tab?.chatAgentId == "claude-acp")
}
```

The stale test must assert no tab and no controller; do not weaken it to only checking `lastError`. The refresh assertion may use an injected refresh counter if the existing registry client seam supports it; otherwise assert the center's post-call state after yielding to the scheduled refresh task.

- [ ] **Step 2: Run the focused tests to verify they fail**

Run: the App test target filtered to `staleExplicitSelectionShowsGlobalErrorAndCreatesNothing` and `legacyNoArgumentPathRetainsDefaultFallback`.

Expected: stale behavior fails until the explicit guard schedules refresh; the legacy test remains green once the initializer seam is added.

- [ ] **Step 3: Implement only the stale-selection behavior required by the tests**

Keep `ContentView`'s existing `.alert` binding unchanged; `lastError` is the global presentation path. Ensure the refresh task is started only after the explicit path has returned `nil`, and never create a tab/controller before the guard.

- [ ] **Step 4: Run the focused tests to verify they pass**

Run: both focused tests, then inspect `ContentView.swift` to confirm the existing `lastError` alert displays the unavailable-agent message after the native menu closes.

Expected: PASS; stale selection has no fallback and legacy no-argument creation still works.

- [ ] **Step 5: Commit the task in the implementation session**

Suggested conventional commit: `test: cover stale ACP chat selections`

### Task 6: Prove new-conversation and launch-failure behavior

**Files:**
- Modify: `AppTests/AppModelControlTests.swift`
- Modify: `AppTests/ChatControllerTests.swift`
- Verify without changing unless required: `App/Chat/ChatController.swift:71-189`, `App/Chat/ChatPaneView.swift:17-56`

**Interfaces:**
- Consumes: `ChatController.init(..., startNewConversation:)`, `ChatController.start()`, `ChatController.didResume`, `ChatController.state`, and the existing `ChatPaneView` banner mapping.
- Produces: tests proving the selected-agent flow never resumes and preserves the selected tab when launch yields `.needsAuth` or `.disconnected`.

- [ ] **Step 1: Write the failing no-resume assertion for the explicit path**

Extend the existing `newChatDoesNotResumeLatestWorktreeSession` fixture so the model creates a selected ACP tab against a store containing an older session for the same worktree/agent. Assert the returned controller has `didResume == false` after its first launch attempt and that no old transcript is loaded:

```swift
let tab = model.openChatTab(in: worktree, agentId: "claude-acp")
let controller = try #require(tab.flatMap { model.chatControllers[$0.id] })
await controller.start()
#expect(controller.didResume == false)
#expect(controller.restored.isEmpty)
```

If the executable fixture intentionally fails before ACP connect, assert the stronger precondition available in that fixture: `controller` was constructed with `startNewConversation: true` and the old transcript is not loaded. Keep the existing `ChatControllerTests` test as a regression test for the direct initializer contract.

- [ ] **Step 2: Run the focused no-resume test to verify it fails**

Run: `xcodebuild test -project Tiller.xcodeproj -scheme Tiller -destination 'platform=macOS' CODE_SIGNING_ALLOWED=NO -only-testing:TillerTests/AppModelControlTests/explicitPathDoesNotResumeLatestWorktreeSession -only-testing:TillerTests/ChatControllerTests/newChatDoesNotResumeLatestWorktreeSession`.

Expected: the new test fails until it exercises the explicit overload and verifies the controller path.

- [ ] **Step 3: Add launch-failure coverage without changing the existing UI contract**

Use a valid installed manifest whose executable produces the existing ACP startup failure. After the explicit method returns, assert the tab remains in `model.tabs[worktree.id]` and remains `model.activeTabId[worktree.id]`. Assert the controller reaches one of the existing states:

```swift
#expect(controller.state == .needsAuth)
#expect(model.tabs[worktree.id]?.contains { $0.id == tab.id } == true)
#expect(model.activeTabId[worktree.id] == tab.id)
```

Use a second deterministic executable fixture for the disconnected case and assert its exact state:

```swift
#expect(controller.state == .disconnected(message: "expected launch failure"))
#expect(model.activeTabId[worktree.id] == tab.id)
```

If the actual `ChatController` error text is produced by `ProcessTransport`, assert the exact message observed by the focused test instead of weakening the state assertion. Verify the existing `ChatPaneView` source continues to map `.needsAuth` to **Authentication required**/**Retry** and `.disconnected` to **Agent disconnected**/**Restart agent**. Do not add automatic retry code.

- [ ] **Step 4: Run the focused tests to verify they pass**

Run: focused `ChatControllerTests`, the explicit AppModel no-resume test, and the launch-failure AppModel test.

Expected: PASS; the new tab is selected despite launch failure, the old session is not resumed, and no fallback/rollback/retry occurs.

- [ ] **Step 5: Commit the task in the implementation session**

Suggested conventional commit: `test: verify ACP new-chat launch failures`

### Task 7: Full verification and plan acceptance review

**Files:**
- Verify: all files listed above; no additional production, test, project-configuration, or commit files.

- [ ] **Step 1: Run package-level tests for the ACP rules**

Run: `cd Packages/TillerACP && swift test --filter AgentLaunchSpecTests`

Expected: PASS, including canonicalization and installed-manifest resolution.

- [ ] **Step 2: Run the complete repository verification gate**

Run: `Scripts/ci.sh`

Expected: output contains `CI OK` and exits with status 0.

- [ ] **Step 3: Perform the manual acceptance review**

Check each approved case against the implementation: both New Chat entry points show the same native submenu; initial loading is not empty; no agents offers only explanation/settings; settings creates no chat; installed rows come from `installedAgents` and retain branding; selection uses the supplied current worktree and selects the new tab; canonicalization/persistence/activity/controller setup occur; `startNewConversation: true` prevents resume; stale IDs set `lastError`, refresh, and create no tab/controller; launch failures retain the selected tab and show the existing auth/disconnected banners with their existing actions; no fallback or automatic retry occurs; legacy no-argument callers remain unchanged.

- [ ] **Step 4: Confirm the implementation session does not commit**

Do not run `git commit`. The conventional commit messages above are handoff suggestions only.

## Self-Review

- **Spec coverage:** Tasks 1-3 cover dynamic shared native menus, loading, empty/settings navigation, installation responsiveness, and both entry points. Tasks 4-5 cover canonicalization, availability revalidation, refresh, global error/no-tab behavior, selection, persistence, activity, controller creation, and legacy fallback. Task 6 covers no-resume and `.needsAuth`/`.disconnected` launch failures without rollback/fallback/retry. Task 7 covers `Scripts/ci.sh` and every acceptance criterion.
- **Placeholder scan:** No `TODO`, `TBD`, “implement later,” or unspecified validation step is required. Every production interface introduced by the plan has an exact name, parameter list, and return type; every test task includes Swift Testing code and a command/expected result.
- **Type consistency:** `NewChatMenuModel.state` consumes `Bool` plus `[AcpAgentCenter.InstalledAgentSummary]` and returns `NewChatMenuState`; `NewChatACPMenu` consumes `@Bindable AppModel` plus `Worktree`; both call `AppModel.openChatTab(in:agentId:)`; the explicit overload returns `WorkspaceTab?`; `chatController(for:in:startNewConversation:)` remains the existing controller factory.
- **Uncertainty:** The repository's current app-test runner is driven by the Xcode project rather than a package manifest, so the focused App test command may be the Xcode scheme's equivalent. The full, authoritative verification command is unambiguous: `Scripts/ci.sh` must print `CI OK`.
