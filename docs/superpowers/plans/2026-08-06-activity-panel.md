# Activity Panel Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the right panel's "Agents" section with "Activity" — a flat, collapsible list of every terminal and chat tab across all open worktrees, each row closable.

**Architecture:** A pure builder in TillerCore (`ActivityListBuilder`) flattens the tabs of every mounted worktree into `[ActivityRow]`, dropping the agent-identity guard that made the old panel agent-only. A thin `@MainActor` adapter in the App layer (`ActivityPanelModel`) feeds it live `AppModel` state, and `ActivitySectionView` renders it. The old `AgentTreeBuilder` and its two-level tree are deleted; `ProcessNode` is extracted first because Layer-D process detection still depends on it.

**Tech Stack:** Swift 6, SwiftUI, swift-testing (`@Test` / `#expect`), xcodegen, GRDB (indirectly, via test fixtures).

**Spec:** `docs/superpowers/specs/2026-08-06-activity-panel-design.md`

## Global Constraints

- Tests first: swift-testing (`@Test` / `#expect`), never XCTest.
- All app-facing strings in English.
- Value types for models; `public` on everything TillerCore exposes.
- `TillerCore` must not import SwiftUI, AppKit, `TillerTerminal`, `TillerControl` or `TillerAgents`. `Scripts/check-module-boundaries.sh` enforces this.
- Commit messages: Conventional Commits, lower-case imperative subject.
- File renames do not need `project.yml` edits — xcodegen globs directories — but `xcodegen generate` must be re-run after any add/delete.
- `Scripts/ci.sh` is run by the repository owner, manually, at the end. Do not run it from a task. For reference: do not add `CODE_SIGNING_ALLOWED=NO` or `-derivedDataPath` to its App test invocation; both make the test host hang before test discovery.
- Per-package iteration during a task: `cd Packages/TillerCore && swift test --filter <TestName>`.

---

### Task 1: ActivityStatus

The display state of a row. It exists because `AgentStatus` has no case for "nothing is running" — a terminal on a bare shell has no agent and therefore no status — and because the close-confirmation rule needs to be unit-testable without driving a SwiftUI alert.

**Files:**
- Create: `Packages/TillerCore/Sources/TillerCore/ActivityStatus.swift`
- Test: `Packages/TillerCore/Tests/TillerCoreTests/ActivityStatusTests.swift`

**Interfaces:**
- Consumes: `AgentStatus` (`Packages/TillerCore/Sources/TillerCore/Models.swift:56`), cases `.running`, `.needsInput`, `.done`, `.error`.
- Produces: `ActivityStatus` with cases `.running`, `.needsInput`, `.done`, `.error`, `.idle`; `static func from(_ status: AgentStatus?) -> ActivityStatus`; `var requiresCloseConfirmation: Bool`.

- [ ] **Step 1: Write the failing test**

Create `Packages/TillerCore/Tests/TillerCoreTests/ActivityStatusTests.swift`:

```swift
import Foundation
import Testing

@testable import TillerCore

@Suite struct ActivityStatusTests {
    @Test func everyAgentStatusMapsToItsOwnCase() {
        #expect(ActivityStatus.from(.running) == .running)
        #expect(ActivityStatus.from(.needsInput) == .needsInput)
        #expect(ActivityStatus.from(.done) == .done)
        #expect(ActivityStatus.from(.error) == .error)
    }

    /// A terminal running a bare shell has no agent and therefore no
    /// AgentStatus at all. It must still produce a row, so the absent status
    /// becomes `.idle` instead of disqualifying the tab.
    @Test func absentAgentStatusBecomesIdle() {
        #expect(ActivityStatus.from(nil) == .idle)
    }

    @Test func onlyLiveStatesAskBeforeClosing() {
        #expect(ActivityStatus.running.requiresCloseConfirmation)
        #expect(ActivityStatus.needsInput.requiresCloseConfirmation)
        #expect(ActivityStatus.error.requiresCloseConfirmation)
        #expect(!ActivityStatus.done.requiresCloseConfirmation)
        #expect(!ActivityStatus.idle.requiresCloseConfirmation)
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd Packages/TillerCore && swift test --filter ActivityStatusTests`
Expected: FAIL — `cannot find 'ActivityStatus' in scope`

- [ ] **Step 3: Write minimal implementation**

Create `Packages/TillerCore/Sources/TillerCore/ActivityStatus.swift`:

```swift
import Foundation

/// Display state of one Activity row. Distinct from `AgentStatus` because a
/// terminal running a bare shell has no agent, and `AgentStatus` has no case
/// for "nothing is running" — that gap is what `.idle` fills.
public enum ActivityStatus: String, Sendable, Equatable {
    case running
    case needsInput
    case done
    case error
    case idle

    public static func from(_ status: AgentStatus?) -> ActivityStatus {
        switch status {
        case .running: .running
        case .needsInput: .needsInput
        case .done: .done
        case .error: .error
        case nil: .idle
        }
    }

    /// Closing a row terminates its process, so states that imply live work ask
    /// first. `.error` is included: an agent that reported a failure may still
    /// be sitting at a prompt.
    public var requiresCloseConfirmation: Bool {
        switch self {
        case .running, .needsInput, .error: true
        case .done, .idle: false
        }
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd Packages/TillerCore && swift test --filter ActivityStatusTests`
Expected: PASS, 3 tests

- [ ] **Step 5: Commit**

```bash
git add Packages/TillerCore/Sources/TillerCore/ActivityStatus.swift \
        Packages/TillerCore/Tests/TillerCoreTests/ActivityStatusTests.swift
git commit -m "feat: add ActivityStatus with idle case and close-confirmation rule"
```

---

### Task 2: ActivityListBuilder

The flat list. This is where the behavioural change lives: the old builder dropped any tab without both an `agentStatus` and a `paneAgents` entry, which is why a plain shell was invisible.

Built alongside the old `AgentTreeBuilder`, which keeps compiling until Task 5 removes its consumer.

**Files:**
- Create: `Packages/TillerCore/Sources/TillerCore/ActivityList.swift`
- Test: `Packages/TillerCore/Tests/TillerCoreTests/ActivityListBuilderTests.swift`

**Interfaces:**
- Consumes: `ActivityStatus.from` (Task 1); `WorkspaceTab` (`Packages/TillerCore/Sources/TillerCore/Workspace/WorkspaceTab.swift:31`) with `id: WorkspaceTabID`, `title: String`, `content: WorkspaceContentRef`; `WorkspaceTabID.rawValue: UUID`; `WorkspaceContentRef` cases `.terminal(TerminalContentID)`, `.chat(ChatContentID)`, `.document(...)`.
- Produces:
  - `ActivityRow` — `id: String`, `worktreeId: UUID`, `tabId: UUID`, `kind: ActivityRow.Kind` (`.terminal` / `.chat`), `title: String`, `agentId: String?`, `worktreeLabel: String`, `status: ActivityStatus`; init `ActivityRow(worktreeId:tabId:kind:title:agentId:worktreeLabel:status:)`.
  - `ActivityListBuilder.WorktreeInput` — init `(worktreeId: UUID, label: String, tabs: [WorkspaceTab], livePaneIds: [WorkspaceTabID: UUID])`.
  - `ActivityListBuilder.build(worktrees: [WorktreeInput], agentStatus: [UUID: AgentStatus], paneAgents: [UUID: String]) -> [ActivityRow]`.

- [ ] **Step 1: Write the failing test**

Create `Packages/TillerCore/Tests/TillerCoreTests/ActivityListBuilderTests.swift`:

```swift
import Foundation
import Testing

@testable import TillerCore

@Suite struct ActivityListBuilderTests {
    // Fixed UUIDs so ids and ordering are assertable.
    static let worktreeA = UUID(uuidString: "AAAAAAAA-AAAA-AAAA-AAAA-AAAAAAAAAAAA")!
    static let worktreeB = UUID(uuidString: "BBBBBBBB-BBBB-BBBB-BBBB-BBBBBBBBBBBB")!
    static let chatTabId = UUID(uuidString: "11111111-1111-1111-1111-111111111111")!
    static let termTabId = UUID(uuidString: "22222222-2222-2222-2222-222222222222")!
    static let paneId = UUID(uuidString: "33333333-3333-3333-3333-333333333333")!
    static let docTabId = UUID(uuidString: "44444444-4444-4444-4444-444444444444")!

    static func chatTab(_ id: UUID = chatTabId, title: String = "Fix login bug") -> WorkspaceTab {
        WorkspaceTab(id: WorkspaceTabID(id), title: title, titleIsAutoNamed: false,
                     content: .chat(ChatContentID("claude-acp-chat")))
    }

    static func terminalTab(_ id: UUID = termTabId, title: String = "zsh") -> WorkspaceTab {
        WorkspaceTab(id: WorkspaceTabID(id), title: title, titleIsAutoNamed: false,
                     content: .terminal(TerminalContentID()))
    }

    static func input(
        worktreeId: UUID = worktreeA,
        label: String = "tiller/main",
        tabs: [WorkspaceTab],
        livePaneIds: [WorkspaceTabID: UUID] = [WorkspaceTabID(termTabId): paneId]
    ) -> ActivityListBuilder.WorktreeInput {
        ActivityListBuilder.WorktreeInput(
            worktreeId: worktreeId, label: label, tabs: tabs, livePaneIds: livePaneIds)
    }

    /// The whole point of the rewrite: the old AgentTreeBuilder dropped this
    /// tab because no agent was ever identified in it.
    @Test func terminalWithoutAnyAgentIsStillListed() {
        let rows = ActivityListBuilder.build(
            worktrees: [Self.input(tabs: [Self.terminalTab()])],
            agentStatus: [:], paneAgents: [:])

        #expect(rows.count == 1)
        #expect(rows[0].kind == .terminal)
        #expect(rows[0].title == "zsh")
        #expect(rows[0].agentId == nil)
        #expect(rows[0].status == .idle)
        #expect(rows[0].tabId == Self.termTabId)
    }

    @Test func terminalRowTakesStatusAndAgentFromItsLivePane() {
        let rows = ActivityListBuilder.build(
            worktrees: [Self.input(tabs: [Self.terminalTab()])],
            agentStatus: [Self.paneId: .running],
            paneAgents: [Self.paneId: "codex"])

        #expect(rows[0].agentId == "codex")
        #expect(rows[0].status == .running)
    }

    /// A chat's status and agent are keyed by the tab id, not by a pane id.
    @Test func chatRowIsKeyedByTabIdAndKeepsTheTabTitle() {
        let rows = ActivityListBuilder.build(
            worktrees: [Self.input(tabs: [Self.chatTab()], livePaneIds: [:])],
            agentStatus: [Self.chatTabId: .needsInput],
            paneAgents: [Self.chatTabId: "claude-acp"])

        #expect(rows.count == 1)
        #expect(rows[0].kind == .chat)
        #expect(rows[0].title == "Fix login bug")
        #expect(rows[0].agentId == "claude-acp")
        #expect(rows[0].status == .needsInput)
    }

    @Test func documentTabsAreExcluded() {
        let doc = WorkspaceTab(
            id: WorkspaceTabID(Self.docTabId), title: "README.md", titleIsAutoNamed: false,
            content: .document(
                DocumentID.make(worktreeID: Self.worktreeA,
                                fileURL: URL(fileURLWithPath: "/tmp/README.md")),
                editor: .markdown))
        let rows = ActivityListBuilder.build(
            worktrees: [Self.input(tabs: [doc, Self.chatTab()], livePaneIds: [:])],
            agentStatus: [:], paneAgents: [:])

        #expect(rows.count == 1)
        #expect(rows[0].kind == .chat)
    }

    /// No live pane means the PTY was never mounted: nothing is running, so
    /// there is nothing to list or to close.
    @Test func terminalWithoutLivePaneIsSkipped() {
        let rows = ActivityListBuilder.build(
            worktrees: [Self.input(tabs: [Self.terminalTab()], livePaneIds: [:])],
            agentStatus: [:], paneAgents: [:])

        #expect(rows.isEmpty)
    }

    @Test func rowsFollowWorktreeOrderThenTabOrder() {
        let rows = ActivityListBuilder.build(
            worktrees: [
                Self.input(worktreeId: Self.worktreeA, label: "tiller/main",
                           tabs: [Self.terminalTab(), Self.chatTab()]),
                Self.input(worktreeId: Self.worktreeB, label: "tiller/feat-x",
                           tabs: [Self.chatTab(Self.docTabId, title: "Other chat")],
                           livePaneIds: [:]),
            ],
            agentStatus: [:], paneAgents: [:])

        #expect(rows.count == 3)
        #expect(rows.map(\.worktreeLabel) == ["tiller/main", "tiller/main", "tiller/feat-x"])
        #expect(rows[0].kind == .terminal)
        #expect(rows[1].kind == .chat)
        #expect(rows[2].title == "Other chat")
    }

    /// Two worktrees can hold tabs with colliding ids only in theory, but the
    /// row id must still be unique per worktree for ForEach to be stable.
    @Test func rowIdCombinesWorktreeAndTab() {
        let rows = ActivityListBuilder.build(
            worktrees: [Self.input(tabs: [Self.chatTab()], livePaneIds: [:])],
            agentStatus: [:], paneAgents: [:])

        #expect(rows[0].id == "\(Self.worktreeA.uuidString):\(Self.chatTabId.uuidString)")
    }
}
```

The document case carries two associated values — `.document(DocumentID, editor: DocumentEditorKind)` (`Packages/TillerCore/Sources/TillerCore/Workspace/WorkspaceContentRef.swift:17`) — and `DocumentID` is built through `DocumentID.make(worktreeID:fileURL:)` (`WorkspaceIDs.swift:56`), not a plain initializer.

- [ ] **Step 2: Run test to verify it fails**

Run: `cd Packages/TillerCore && swift test --filter ActivityListBuilderTests`
Expected: FAIL — `cannot find 'ActivityListBuilder' in scope`

- [ ] **Step 3: Write minimal implementation**

Create `Packages/TillerCore/Sources/TillerCore/ActivityList.swift`:

```swift
import Foundation

/// One row of the Activity panel: a terminal or chat tab belonging to an open
/// worktree.
public struct ActivityRow: Identifiable, Equatable, Sendable {
    public enum Kind: Equatable, Sendable {
        case terminal
        case chat
    }

    public let id: String
    public let worktreeId: UUID
    public let tabId: UUID
    public let kind: Kind
    public let title: String
    /// nil when no agent was ever identified in this tab — a bare shell.
    public let agentId: String?
    public let worktreeLabel: String
    public let status: ActivityStatus

    public init(worktreeId: UUID, tabId: UUID, kind: Kind, title: String,
                agentId: String?, worktreeLabel: String, status: ActivityStatus) {
        self.id = "\(worktreeId.uuidString):\(tabId.uuidString)"
        self.worktreeId = worktreeId
        self.tabId = tabId
        self.kind = kind
        self.title = title
        self.agentId = agentId
        self.worktreeLabel = worktreeLabel
        self.status = status
    }
}

/// Flattens the tabs of every open worktree into one list. Unlike the agent
/// tree it replaces, a tab whose agent was never identified is kept: the panel
/// answers "what do I have open?", not "which agents are running?".
public enum ActivityListBuilder {
    public struct WorktreeInput: Sendable {
        public let worktreeId: UUID
        public let label: String
        public let tabs: [WorkspaceTab]
        public let livePaneIds: [WorkspaceTabID: UUID]

        public init(worktreeId: UUID, label: String, tabs: [WorkspaceTab],
                    livePaneIds: [WorkspaceTabID: UUID]) {
            self.worktreeId = worktreeId
            self.label = label
            self.tabs = tabs
            self.livePaneIds = livePaneIds
        }
    }

    public static func build(
        worktrees: [WorktreeInput],
        agentStatus: [UUID: AgentStatus],
        paneAgents: [UUID: String]
    ) -> [ActivityRow] {
        worktrees.flatMap { worktree in
            worktree.tabs.compactMap { tab in
                row(tab: tab, worktree: worktree,
                    agentStatus: agentStatus, paneAgents: paneAgents)
            }
        }
    }

    private static func row(
        tab: WorkspaceTab,
        worktree: WorktreeInput,
        agentStatus: [UUID: AgentStatus],
        paneAgents: [UUID: String]
    ) -> ActivityRow? {
        let kind: ActivityRow.Kind
        // Chats register their activity under the tab id, terminals under the
        // live pane id. Everything downstream reads one key.
        let activityKey: UUID
        switch tab.content {
        case .chat:
            kind = .chat
            activityKey = tab.id.rawValue
        case .terminal:
            guard let paneId = worktree.livePaneIds[tab.id] else { return nil }
            kind = .terminal
            activityKey = paneId
        case .document:
            return nil
        }
        return ActivityRow(
            worktreeId: worktree.worktreeId,
            tabId: tab.id.rawValue,
            kind: kind,
            title: tab.title,
            agentId: paneAgents[activityKey],
            worktreeLabel: worktree.label,
            status: .from(agentStatus[activityKey]))
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd Packages/TillerCore && swift test --filter ActivityListBuilderTests`
Expected: PASS, 7 tests

- [ ] **Step 5: Run the whole package to check nothing else broke**

Run: `cd Packages/TillerCore && swift test`
Expected: PASS. `AgentTreeBuilderTests` still passes — the old builder is untouched.

- [ ] **Step 6: Commit**

```bash
git add Packages/TillerCore/Sources/TillerCore/ActivityList.swift \
        Packages/TillerCore/Tests/TillerCoreTests/ActivityListBuilderTests.swift
git commit -m "feat: add ActivityListBuilder flattening tabs across worktrees"
```

---

### Task 3: Persisted expansion flag

**Files:**
- Modify: `Packages/TillerCore/Sources/TillerCore/AppSettings.swift:108-114` (add next to the other right-panel keys)
- Test: `Packages/TillerCore/Tests/TillerCoreTests/AppSettingsTests.swift`

**Interfaces:**
- Produces: `AppSettings.activitySectionExpandedKey: String` = `"activity.sectionExpanded"`; `AppSettings.defaultActivitySectionExpanded: Bool` = `true`.

- [ ] **Step 1: Write the failing test**

Add to the existing suite in `Packages/TillerCore/Tests/TillerCoreTests/AppSettingsTests.swift`, next to the `defaultRightPanelVisible` expectation around line 57:

```swift
    /// The Activity section starts open: a user who has never toggled it should
    /// see what is running, not an empty strip.
    @Test func activitySectionDefaultsToExpanded() {
        #expect(AppSettings.activitySectionExpandedKey == "activity.sectionExpanded")
        #expect(AppSettings.defaultActivitySectionExpanded == true)
    }
```

If the surrounding tests are free functions rather than methods in a suite, match whatever the file already does — do not restructure it.

- [ ] **Step 2: Run test to verify it fails**

Run: `cd Packages/TillerCore && swift test --filter activitySectionDefaultsToExpanded`
Expected: FAIL — `type 'AppSettings' has no member 'activitySectionExpandedKey'`

- [ ] **Step 3: Write minimal implementation**

In `Packages/TillerCore/Sources/TillerCore/AppSettings.swift`, immediately after the `rightPanelModeKey` line:

```swift
    /// Whether the Activity section at the bottom of the right panel is
    /// expanded. Collapsed, Files/Changes takes the full panel height.
    public static let activitySectionExpandedKey = "activity.sectionExpanded"
    public static let defaultActivitySectionExpanded = true
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd Packages/TillerCore && swift test --filter activitySectionDefaultsToExpanded`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add Packages/TillerCore/Sources/TillerCore/AppSettings.swift \
        Packages/TillerCore/Tests/TillerCoreTests/AppSettingsTests.swift
git commit -m "feat: add persisted activity section expansion setting"
```

---

### Task 4: ActivityPanelModel

The App-layer adapter: turns live `AppModel` state into builder inputs. Created alongside `AgentsPanelModel`, which Task 5 deletes.

**Files:**
- Create: `App/RightPanel/ActivityPanelModel.swift`

**Interfaces:**
- Consumes: `ActivityListBuilder.build` and `WorktreeInput` (Task 2); `AppModel.openWorktreeIds: [UUID]` (`App/AppModel.swift:76`); `AppModel.worktree(byId:) -> Worktree?` (`:82`); `AppModel.projects: [Project]` (`:21`); `AppModel.agentActivity` (`:111`) exposing `agentStatus: [UUID: AgentStatus]` and `paneAgents: [UUID: String]`; `WorkspaceCoordinator.layouts: [UUID: WorkspaceLayout]` (`App/Workspace/WorkspaceCoordinator.swift:24`) with `allTabs`; `WorkspaceCoordinator.liveControlPaneId(contentID:in:) -> UUID?` (`:154`).
- Produces: `ActivityPanelModel.rows(appModel:) -> [ActivityRow]` and `ActivityPanelModel.label(for:appModel:) -> String`, both `@MainActor`.

- [ ] **Step 1: Write the implementation**

There is no unit test at this step — the pure logic is already covered by Task 2, and the wiring is covered by the AppTests in Task 6, which need the view from Task 5 to be meaningful. Create `App/RightPanel/ActivityPanelModel.swift`:

```swift
import Foundation
import TillerCore

/// Assembles the Activity list from live AppModel state across every mounted
/// worktree.
@MainActor
enum ActivityPanelModel {
    /// Iterates `openWorktreeIds`, not the selected worktree: only mounted
    /// worktrees have live PTYs, and all of them are in scope.
    static func rows(appModel: AppModel) -> [ActivityRow] {
        let inputs = appModel.openWorktreeIds.compactMap {
            input(worktreeId: $0, appModel: appModel)
        }
        return ActivityListBuilder.build(
            worktrees: inputs,
            agentStatus: appModel.agentActivity.agentStatus,
            paneAgents: appModel.agentActivity.paneAgents)
    }

    /// A worktree id with no worktree or no layout is skipped rather than
    /// treated as an error: both are ordinary transient states while a
    /// worktree is being closed.
    private static func input(
        worktreeId: UUID, appModel: AppModel
    ) -> ActivityListBuilder.WorktreeInput? {
        guard let worktree = appModel.worktree(byId: worktreeId),
              let layout = appModel.workspaceCoordinator.layouts[worktreeId]
        else { return nil }
        let tabs = layout.allTabs
        var livePaneIds: [WorkspaceTabID: UUID] = [:]
        for tab in tabs {
            guard case .terminal(let contentID) = tab.content else { continue }
            // Assigning nil removes the key, which is exactly what a terminal
            // tab with no mounted pane should leave behind.
            livePaneIds[tab.id] = appModel.workspaceCoordinator.liveControlPaneId(
                contentID: contentID, in: worktreeId)
        }
        return ActivityListBuilder.WorktreeInput(
            worktreeId: worktreeId, label: label(for: worktree, appModel: appModel),
            tabs: tabs, livePaneIds: livePaneIds)
    }

    /// "project/branch", degrading to the bare branch when the project row is
    /// gone — a worktree briefly outlives its project during removal, and a
    /// half-labelled row beats a crash.
    static func label(for worktree: Worktree, appModel: AppModel) -> String {
        guard let project = appModel.projects.first(where: { $0.id == worktree.projectId })
        else { return worktree.branch }
        let name = project.displayName.flatMap { $0.isEmpty ? nil : $0 } ?? project.name
        return "\(name)/\(worktree.branch)"
    }
}
```

- [ ] **Step 2: Verify it compiles**

Run:
```bash
xcodegen generate
xcodebuild -project Tiller.xcodeproj -scheme Tiller -configuration Debug \
  -derivedDataPath DerivedData CODE_SIGNING_ALLOWED=NO \
  -skipPackagePluginValidation -skipMacroValidation -skipPackageUpdates build | tail -5
```
Expected: `** BUILD SUCCEEDED **`

- [ ] **Step 3: Commit**

```bash
git add App/RightPanel/ActivityPanelModel.swift
git commit -m "feat: add ActivityPanelModel reading every open worktree"
```

---

### Task 5: ActivitySectionView and the switchover

The atomic swap: the new view lands, the panel is rewired, and the four dead files go. Splitting this would leave the build broken between commits.

**Files:**
- Create: `App/RightPanel/ActivitySectionView.swift`
- Create: `Packages/TillerCore/Sources/TillerCore/ProcessNode.swift`
- Modify: `App/RightPanel/RightPanelView.swift:25-60` (body) and `:75-80` (Italian strings)
- Modify: `Packages/TillerCore/Tests/TillerCoreTests/AgentActivityModelTests.swift:88-90` (stale comment)
- Delete: `App/RightPanel/AgentsSectionView.swift`
- Delete: `App/RightPanel/AgentsPanelModel.swift`
- Delete: `Packages/TillerCore/Sources/TillerCore/AgentTree.swift`
- Delete: `Packages/TillerCore/Tests/TillerCoreTests/AgentTreeBuilderTests.swift`

**Interfaces:**
- Consumes: `ActivityPanelModel.rows(appModel:)` and `.label(for:appModel:)` (Task 4); `ActivityStatus.requiresCloseConfirmation` (Task 1); `AppSettings.activitySectionExpandedKey` / `.defaultActivitySectionExpanded` (Task 3); `AgentIcon(agentId:)` (`App/AgentIcon.swift:8`, `size` defaults to 14); `RunningDots(color:dotSize:)` (`App/RunningDots.swift:8`); `AppModel.focusTab(tabId:in:)` (`App/AppModel.swift:2012`); `AppModel.closeTab(_:in:)` (`:1356`).
- Produces: `ActivitySectionView(appModel:isExpanded:)` where `isExpanded` is a `Binding<Bool>`.

- [ ] **Step 1: Extract ProcessNode before deleting its file**

`ProcessNode` lives in `AgentTree.swift` but belongs to Layer-D process detection: `App/ForegroundProcessAgent.swift:35`, `App/ProcessScanCoordinator.swift:75`, `App/AppModel.swift:1995`, and two AppTests depend on it. Deleting it with its neighbours would break the build.

Create `Packages/TillerCore/Sources/TillerCore/ProcessNode.swift` with exactly the declaration currently at the top of `AgentTree.swift`:

```swift
import Foundation

/// One process in a pane's shell subtree (captured by the App-layer libproc
/// walk; modelled here so the scan logic stays testable in TillerCore).
public struct ProcessNode: Equatable, Sendable {
    public let pid: Int32
    public let name: String
    public let children: [ProcessNode]

    public init(pid: Int32, name: String, children: [ProcessNode]) {
        self.pid = pid
        self.name = name
        self.children = children
    }
}
```

- [ ] **Step 2: Write the new view**

Create `App/RightPanel/ActivitySectionView.swift`:

```swift
import SwiftUI
import TillerCore
import Inject

/// A row awaiting close confirmation, held out of the row itself so the alert
/// survives the list being rebuilt underneath it.
struct PendingActivityClose: Identifiable {
    let row: ActivityRow
    var id: String { row.id }
}

/// Bottom section of the right panel: every terminal and chat tab of every open
/// worktree. Click focuses the owning tab; the trailing button closes it.
struct ActivitySectionView: View {
    @ObserveInjection private var inject

    @Bindable var appModel: AppModel
    @Binding var isExpanded: Bool
    @State private var pendingClose: PendingActivityClose?

    private var rows: [ActivityRow] { ActivityPanelModel.rows(appModel: appModel) }
    private var runningCount: Int { rows.count { $0.status == .running } }

    var body: some View {
        VStack(spacing: 0) {
            header
            if isExpanded {
                Divider()
                content
            }
        }
        .alert(item: $pendingClose) { pending in
            Alert(
                title: Text("Close \(pending.row.title)?"),
                message: Text(
                    "The process running in \(pending.row.worktreeLabel) will be terminated."),
                primaryButton: .destructive(Text("Close")) { close(pending.row) },
                secondaryButton: .cancel(Text("Cancel")))
        }
    .enableInjection()
    }

    private var header: some View {
        Button {
            isExpanded.toggle()
        } label: {
            HStack(spacing: 6) {
                Image(systemName: isExpanded ? "chevron.down" : "chevron.right")
                    .font(.caption2)
                    .foregroundStyle(.secondary)
                Text("Activity")
                    .font(.subheadline.weight(.semibold))
                Spacer()
                if runningCount > 0 {
                    Text("\(runningCount) running")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            }
            .contentShape(Rectangle())
            .padding(.horizontal, 10)
            .padding(.vertical, 6)
        }
        .buttonStyle(.plain)
        .accessibilityLabel(isExpanded ? "Collapse Activity" : "Expand Activity")
    }

    @ViewBuilder
    private var content: some View {
        if rows.isEmpty {
            ContentUnavailableView(
                "No activity",
                systemImage: "bolt.slash",
                description: Text("Terminals and chats appear here as you open them."))
                .frame(maxWidth: .infinity, maxHeight: .infinity)
        } else {
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 2) {
                    ForEach(rows) { row in
                        rowView(row)
                    }
                }
                .padding(6)
            }
        }
    }

    private func rowView(_ row: ActivityRow) -> some View {
        HStack(spacing: 6) {
            icon(row)
                .frame(width: 14, height: 14)
            VStack(alignment: .leading, spacing: 1) {
                Text(row.title)
                    .font(.callout)
                    .lineLimit(1)
                    .truncationMode(.tail)
                Text(row.worktreeLabel)
                    .font(.caption2)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                    .truncationMode(.middle)
            }
            Spacer(minLength: 4)
            statusIndicator(row.status)
            // Always visible, not hover-only: across a list spanning every
            // worktree the pointer never passes over most rows.
            Button {
                requestClose(row)
            } label: {
                Image(systemName: "xmark")
                    .font(.caption2)
                    .foregroundStyle(.secondary)
            }
            .buttonStyle(.plain)
            .help("Close")
            .accessibilityLabel("Close \(row.title)")
        }
        .padding(.vertical, 3)
        .padding(.horizontal, 4)
        .contentShape(Rectangle())
        .onTapGesture { focus(row) }
    }

    @ViewBuilder
    private func icon(_ row: ActivityRow) -> some View {
        if let agentId = row.agentId {
            AgentIcon(agentId: agentId)
        } else {
            Image(systemName: row.kind == .chat
                  ? "bubble.left.and.text.bubble.right" : "terminal")
                .font(.caption)
                .foregroundStyle(.secondary)
        }
    }

    @ViewBuilder
    private func statusIndicator(_ status: ActivityStatus) -> some View {
        switch status {
        case .running:
            RunningDots(color: .green, dotSize: 3)
        case .needsInput:
            Circle().fill(.yellow).frame(width: 7, height: 7)
        case .done:
            Image(systemName: "checkmark.circle.fill")
                .font(.caption2).foregroundStyle(.secondary)
        case .error:
            Image(systemName: "xmark.circle.fill")
                .font(.caption2).foregroundStyle(.red)
        case .idle:
            Circle().strokeBorder(.secondary, lineWidth: 1).frame(width: 7, height: 7)
        }
    }

    private func focus(_ row: ActivityRow) {
        guard let worktree = appModel.worktree(byId: row.worktreeId) else { return }
        appModel.focusTab(tabId: row.tabId, in: worktree)
    }

    private func requestClose(_ row: ActivityRow) {
        if row.status.requiresCloseConfirmation {
            pendingClose = PendingActivityClose(row: row)
        } else {
            close(row)
        }
    }

    private func close(_ row: ActivityRow) {
        guard let worktree = appModel.worktree(byId: row.worktreeId) else { return }
        appModel.closeTab(row.tabId, in: worktree)
    }
}
```

- [ ] **Step 3: Rewire RightPanelView**

In `App/RightPanel/RightPanelView.swift`, add the storage property next to the other `@Bindable`/`@State` declarations near line 13:

```swift
    @AppStorage(AppSettings.activitySectionExpandedKey)
    private var activityExpanded = AppSettings.defaultActivitySectionExpanded
```

Replace the `GeometryReader` block (lines 26-42) with:

```swift
        GeometryReader { geo in
            VStack(spacing: 0) {
                // Capped at 75% while Activity is open; uncapped when it
                // collapses, which is what gives Files/Changes the full height.
                toolsRegion
                    .frame(maxHeight: activityExpanded ? geo.size.height * 0.75 : .infinity)
                Divider()
                ActivitySectionView(appModel: appModel, isExpanded: $activityExpanded)
                    .frame(maxHeight: activityExpanded ? .infinity : nil)
            }
            .animation(.easeInOut(duration: 0.2), value: activityExpanded)
        }
```

The old `if let worktree = panelModel.worktree` wrapper and its "No agents running" placeholder go: Activity is no longer scoped to the selected worktree, and its own empty state covers the case where nothing is open.

Then translate the two Italian strings at lines 79-80:

```swift
                .help("Hide right panel (⌃⌘I)")
                .accessibilityLabel("Hide right panel")
```

- [ ] **Step 4: Update the stale comment in AgentActivityModelTests**

At `Packages/TillerCore/Tests/TillerCoreTests/AgentActivityModelTests.swift:88-90`, the comment references a guard that no longer exists. The test itself is still valid and stays. Replace the comment with:

```swift
    // A restored chat tab that later receives a real notify() must end up with
    // both paneAgents and agentStatus set, or its Activity row shows as idle
    // while an agent is actually working in it.
```

- [ ] **Step 5: Delete the four dead files**

`AgentsSectionView.swift` already has uncommitted modifications, so it needs
`-f`:

```bash
git rm -f App/RightPanel/AgentsSectionView.swift \
          App/RightPanel/AgentsPanelModel.swift \
          Packages/TillerCore/Sources/TillerCore/AgentTree.swift \
          Packages/TillerCore/Tests/TillerCoreTests/AgentTreeBuilderTests.swift
```

This removes `AgentNode`, `AgentTreeBuilder`, and `ChatSubagentInput`, whose only consumer was `AgentsPanelModel`. `ProcessNode` survives in the file created in Step 1.

- [ ] **Step 6: Regenerate and build**

Run:
```bash
xcodegen generate
xcodebuild -project Tiller.xcodeproj -scheme Tiller -configuration Debug \
  -derivedDataPath DerivedData CODE_SIGNING_ALLOWED=NO \
  -skipPackagePluginValidation -skipMacroValidation -skipPackageUpdates build | tail -5
```
Expected: `** BUILD SUCCEEDED **`. If anything still references `AgentNode`, `AgentTreeBuilder` or `ChatSubagentInput`, the compiler names the file — fix those references rather than restoring the deleted types.

- [ ] **Step 7: Run the TillerCore package**

Run: `cd Packages/TillerCore && swift test`
Expected: PASS, with `AgentTreeBuilderTests` gone from the run.

- [ ] **Step 8: Commit**

The working tree carries ~70 unrelated modified files (an in-progress hot-reload
instrumentation pass). Stage by name, never `git add -A` or `git add .`, or that
work gets swept into this commit:

```bash
git add App/RightPanel/ActivitySectionView.swift \
        App/RightPanel/RightPanelView.swift \
        Packages/TillerCore/Sources/TillerCore/ProcessNode.swift \
        Packages/TillerCore/Tests/TillerCoreTests/AgentActivityModelTests.swift
git commit -m "feat: replace agents panel with activity list"
```

The four deletions from Step 5 are already staged by `git rm`. Note that
`AgentsSectionView.swift` and `RightPanelView.swift` were already modified before
this work started, so `git rm` on the former needs `-f`.

---

### Task 6: App-level wiring test and full gate

**Files:**
- Create: `AppTests/ActivityPanelTests.swift`

**Interfaces:**
- Consumes: `ActivityPanelModel.rows(appModel:)` (Task 4); `UniversalChatFixture.make(suiteName:)`, `.openChatTab(_:agentId:)`, `.tabs(_:)`, `.settle(_:until:)` (`AppTests/Workspace/UniversalChatFixture.swift`).

- [ ] **Step 1: Write the failing test**

The fixture builds an `AppModel` with a chat-capable coordinator and one worktree. The test deliberately leaves `selectedWorktree` nil: that is the assertion that the list is driven by `openWorktreeIds`, which is the whole point of the global scope.

Create `AppTests/ActivityPanelTests.swift`:

```swift
import Foundation
import Testing
import TillerCore

@testable import Tiller

@Suite("ActivityPanelTests", .serialized)
@MainActor
struct ActivityPanelTests {
    /// The list follows the mounted worktrees, not the sidebar selection: with
    /// nothing selected, an open worktree's chat still shows up.
    @Test func openWorktreesDriveTheListWithoutASelection() async throws {
        let fixture = try await UniversalChatFixture.make(
            suiteName: "dev.tiller.Tiller.ActivityPanelTests.openWorktrees")
        defer { fixture.cleanUp() }
        _ = try #require(await UniversalChatFixture.openChatTab(fixture))

        fixture.model.projects = [Project(
            id: fixture.worktree.projectId, name: "P", rootPath: fixture.worktree.path)]
        fixture.model.openWorktreeIds = [fixture.worktree.id]
        fixture.model.selectedWorktree = nil

        let rows = ActivityPanelModel.rows(appModel: fixture.model)

        #expect(rows.count == 1)
        #expect(rows[0].kind == .chat)
        #expect(rows[0].worktreeLabel == "P/main")
    }

    /// A worktree that is not mounted contributes nothing, even though its
    /// layout is still in the coordinator.
    @Test func unmountedWorktreesContributeNothing() async throws {
        let fixture = try await UniversalChatFixture.make(
            suiteName: "dev.tiller.Tiller.ActivityPanelTests.unmounted")
        defer { fixture.cleanUp() }
        _ = try #require(await UniversalChatFixture.openChatTab(fixture))

        fixture.model.openWorktreeIds = []

        #expect(ActivityPanelModel.rows(appModel: fixture.model).isEmpty)
    }

    /// The list is derived, never cached: closing the tab empties it.
    @Test func closingTheTabRemovesItsRow() async throws {
        let fixture = try await UniversalChatFixture.make(
            suiteName: "dev.tiller.Tiller.ActivityPanelTests.close")
        defer { fixture.cleanUp() }
        let tab = try #require(await UniversalChatFixture.openChatTab(fixture))
        fixture.model.openWorktreeIds = [fixture.worktree.id]

        fixture.model.closeTab(tab.id.rawValue, in: fixture.worktree)
        await UniversalChatFixture.settle(fixture) {
            UniversalChatFixture.tabs(fixture).isEmpty
        }

        #expect(ActivityPanelModel.rows(appModel: fixture.model).isEmpty)
    }
}
```

If `AppModel.selectedWorktree` is not settable from a test, drop that single line — the assertion that matters is that rows appear while `openWorktreeIds` is populated.

- [ ] **Step 2: Run the App test target to verify it fails**

Run:
```bash
xcodebuild test -project Tiller.xcodeproj -scheme Tiller -configuration Debug \
  -skipPackagePluginValidation -skipMacroValidation -skipPackageUpdates \
  -only-testing:TillerTests/ActivityPanelTests 2>&1 | tail -20
```
Expected: FAIL initially only if the wiring is wrong. If Tasks 4 and 5 were done correctly this may pass immediately — that is fine; the test's job is to pin the behaviour, and a passing run here still proves the seam. Do not weaken the assertions to force a red.

- [ ] **Step 3: Fix anything the test surfaces**

Likely culprits, in order: `openWorktreeIds` not being read (rows empty), the label falling back to the bare branch because `model.projects` was not populated, or a chat tab arriving with no `agentActivity` entry (expected — it should surface as `.idle`, not vanish).

- [ ] **Step 4: Hand the gate over**

`Scripts/ci.sh` is **not** run by the implementer or by Claude — the repository owner runs it manually. Stop here and report that the work is ready for the gate.

For reference when they do run it: if `TillerTerminal`'s `PtyProcessTests` or `DividerCursorStripTests` fail, they are known flaky under load. Diagnosing that needs two runs on the *same* tree; comparing against `HEAD` instead is what makes the diagnosis wrong.

- [ ] **Step 5: Commit**

```bash
git add AppTests/ActivityPanelTests.swift
git commit -m "test: cover activity panel wiring across open worktrees"
```

---

## Manual QA checklist

Automated tests cannot see the layout, and the collapse behaviour is exactly the sort of thing that compiles and looks wrong. After `CI OK`, build and run, then check:

- [ ] The section header reads "Activity", not "Agents"
- [ ] A terminal running only `zsh` appears in the list — this is the change; if it does not, nothing else matters
- [ ] Chats appear with their current (auto-renamed) title
- [ ] An open markdown or code document does **not** appear
- [ ] With two worktrees open, both contribute rows, each with its own `project/branch` label
- [ ] Clicking a row focuses its tab, including one in a non-selected worktree
- [ ] ✕ on an idle row closes tab and row immediately
- [ ] ✕ on a row with a working agent raises the confirmation; Cancel leaves it running
- [ ] Collapsing the section gives Files/Changes the full panel height, and the state survives a relaunch
- [ ] The running count in the header matches the green rows
