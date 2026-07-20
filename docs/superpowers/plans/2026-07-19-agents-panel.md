# Agents Panel Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add an "Agents" section (bottom 1/3 of the right sidebar) listing agents of the selected worktree with a subagent tree, for both ACP chat sessions and terminal panes.

**Architecture:** Pure tree-building logic in `TillerCore` (`AgentTreeBuilder`), chat-subagent extraction in `TillerACP` (from transcript tool calls carrying `subagent_type` in `rawInput`), recursive libproc process walk in `App` (extends Layer D), thin assembly + SwiftUI views in `App/RightPanel`. Top-level statuses come from the existing `AgentActivityModel` — chat tabs are already registered there with `paneId == tab.id`.

**Tech Stack:** Swift 6, SwiftUI (macOS 15+), swift-testing (`@Test`/`#expect`), libproc.

**Spec:** `docs/superpowers/specs/2026-07-19-agents-panel-design.md`

## Global Constraints

- All work happens in worktree `/Users/enzopiopalmisano/Desktop/Progetti/tiller-worktrees/agent_panel`, branch `agent_panel`. Run all commands from that directory.
- Tests first (swift-testing, `@Test` / `#expect`), never XCTest.
- Domain types are value types (structs/enums). No new classes except `@MainActor @Observable` UI models.
- Package boundaries: `TillerCore` must not import TillerACP/TillerTerminal/App code. libproc calls stay in `App/`.
- All UI strings in English.
- Conventional Commits, lower-case imperative subject.
- Final gate: `Scripts/ci.sh` prints `CI OK`. Known flake: TillerTerminal `spawnCapturesOutput` may need up to 5-6 retries — rerun until CI OK.
- After adding new files, run `xcodegen generate` so the Xcode project picks them up (packages don't need it; `App/` files do).

---

### Task 1: `AgentTree` model + `AgentTreeBuilder` (TillerCore)

**Files:**
- Create: `Packages/TillerCore/Sources/TillerCore/AgentTree.swift`
- Test: `Packages/TillerCore/Tests/TillerCoreTests/AgentTreeBuilderTests.swift`

**Interfaces:**
- Consumes: existing `WorkspaceTab`, `TabContent`, `AgentStatus` (TillerCore `Models.swift:56`: `case running, needsInput = "needs-input", done, error`).
- Produces (used by Tasks 3 and 4):
  - `struct ProcessNode: Equatable, Sendable { let pid: Int32; let name: String; let children: [ProcessNode] }`
  - `struct ChatSubagentInput: Equatable, Sendable, Identifiable { let id: String; let title: String; let status: AgentStatus }`
  - `struct AgentNode: Identifiable, Equatable, Sendable` (fields below)
  - `enum AgentTreeBuilder { static func build(tabs:agentStatus:paneAgents:chatSubagents:processTrees:catalogIds:displayNames:) -> [AgentNode] }`

- [ ] **Step 1: Write the failing tests**

Create `Packages/TillerCore/Tests/TillerCoreTests/AgentTreeBuilderTests.swift`:

```swift
import Foundation
import Testing
@testable import TillerCore

@Suite struct AgentTreeBuilderTests {
    // Fixed UUIDs so ids are assertable.
    static let chatTabId = UUID(uuidString: "11111111-1111-1111-1111-111111111111")!
    static let termTabId = UUID(uuidString: "22222222-2222-2222-2222-222222222222")!
    static let paneId = UUID(uuidString: "33333333-3333-3333-3333-333333333333")!

    func makeTabs() -> [WorkspaceTab] {
        let chatTab = WorkspaceTab(id: Self.chatTabId, title: "Claude Code",
                                   content: .chat(agentId: "claude"))
        let tree = SplitTree(paneId: Self.paneId)
        let termTab = WorkspaceTab(id: Self.termTabId, title: "Terminale 1", tree: tree)
        // Terminal tab listed first: builder must still put chat first.
        return [termTab, chatTab]
    }

    @Test func chatTabsComeFirstThenTerminals() {
        let nodes = AgentTreeBuilder.build(
            tabs: makeTabs(),
            agentStatus: [Self.chatTabId: .running, Self.paneId: .running],
            paneAgents: [Self.chatTabId: "claude", Self.paneId: "codex"],
            chatSubagents: [:], processTrees: [:],
            catalogIds: ["claude", "codex"],
            displayNames: ["claude": "Claude Code", "codex": "Codex"])
        #expect(nodes.count == 2)
        #expect(nodes[0].id == "chat:\(Self.chatTabId.uuidString)")
        #expect(nodes[1].id == "term:\(Self.paneId.uuidString)")
        #expect(nodes[0].title == "Claude Code")
        #expect(nodes[1].agentId == "codex")
    }

    @Test func panesWithoutAgentAreOmitted() {
        let nodes = AgentTreeBuilder.build(
            tabs: makeTabs(),
            agentStatus: [:], paneAgents: [:],
            chatSubagents: [:], processTrees: [:],
            catalogIds: ["claude"], displayNames: [:])
        #expect(nodes.isEmpty)
    }

    @Test func chatSubagentsBecomeChildren() {
        let sub = ChatSubagentInput(id: "tool-1", title: "Explore repo", status: .running)
        let nodes = AgentTreeBuilder.build(
            tabs: makeTabs(),
            agentStatus: [Self.chatTabId: .running],
            paneAgents: [Self.chatTabId: "claude"],
            chatSubagents: [Self.chatTabId: [sub]],
            processTrees: [:],
            catalogIds: ["claude"], displayNames: ["claude": "Claude Code"])
        #expect(nodes.count == 1)
        #expect(nodes[0].children.count == 1)
        let child = nodes[0].children[0]
        #expect(child.id == "chat:\(Self.chatTabId.uuidString):tool:tool-1")
        #expect(child.title == "Explore repo")
        #expect(child.status == .running)
        #expect(child.kind == .subagent)
    }

    @Test func terminalSubagentsFromProcessTree() {
        // shell -> claude -> claude (subagent). Only catalog names below the
        // first matched agent become subagent nodes.
        let inner = ProcessNode(pid: 300, name: "claude", children: [])
        let agent = ProcessNode(pid: 200, name: "claude", children: [
            ProcessNode(pid: 250, name: "rg", children: [inner])
        ])
        let nodes = AgentTreeBuilder.build(
            tabs: makeTabs(),
            agentStatus: [Self.paneId: .running],
            paneAgents: [Self.paneId: "claude"],
            chatSubagents: [:],
            processTrees: [Self.paneId: [agent]],
            catalogIds: ["claude"], displayNames: ["claude": "Claude Code"])
        #expect(nodes.count == 1)
        #expect(nodes[0].children.count == 1)
        #expect(nodes[0].children[0].id == "term:\(Self.paneId.uuidString):pid:300")
        #expect(nodes[0].children[0].agentId == "claude")
        #expect(nodes[0].children[0].status == .running)
    }

    @Test func nonCatalogProcessesNeverAppear() {
        let agent = ProcessNode(pid: 200, name: "claude", children: [
            ProcessNode(pid: 250, name: "rg", children: []),
            ProcessNode(pid: 260, name: "node", children: []),
        ])
        let nodes = AgentTreeBuilder.build(
            tabs: makeTabs(),
            agentStatus: [Self.paneId: .running],
            paneAgents: [Self.paneId: "claude"],
            chatSubagents: [:],
            processTrees: [Self.paneId: [agent]],
            catalogIds: ["claude"], displayNames: [:])
        #expect(nodes[0].children.isEmpty)
    }

    @Test func doneAndErrorStatusesPassThrough() {
        let nodes = AgentTreeBuilder.build(
            tabs: makeTabs(),
            agentStatus: [Self.chatTabId: .done, Self.paneId: .error],
            paneAgents: [Self.chatTabId: "claude", Self.paneId: "codex"],
            chatSubagents: [:], processTrees: [:],
            catalogIds: ["claude", "codex"], displayNames: [:])
        #expect(nodes[0].status == .done)
        #expect(nodes[1].status == .error)
    }
}
```

Note: check `SplitTree`'s single-pane initializer before running — if `SplitTree(paneId:)` does not exist, use the actual leaf initializer found in `Packages/TillerCore/Sources/TillerCore/SplitTree.swift` (there is one; `WorkspaceTabTests.swift` shows the working pattern).

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd Packages/TillerCore && swift test --filter AgentTreeBuilderTests`
Expected: compile FAILURE — `AgentTreeBuilder`, `AgentNode`, `ProcessNode`, `ChatSubagentInput` not found.

- [ ] **Step 3: Implement `AgentTree.swift`**

Create `Packages/TillerCore/Sources/TillerCore/AgentTree.swift`:

```swift
import Foundation

/// One process in a pane's shell subtree (captured by the App-layer libproc
/// walk; assembled here so the tree logic stays testable in TillerCore).
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

/// A subagent task extracted from a chat transcript (Task-type tool call).
public struct ChatSubagentInput: Equatable, Sendable, Identifiable {
    public let id: String       // toolCallId
    public let title: String
    public let status: AgentStatus

    public init(id: String, title: String, status: AgentStatus) {
        self.id = id
        self.title = title
        self.status = status
    }
}

/// A node of the Agents panel tree: a top-level agent (chat tab or terminal
/// pane) or a subagent spawned by it.
public struct AgentNode: Identifiable, Equatable, Sendable {
    public enum Kind: Equatable, Sendable {
        case chat(tabId: UUID)
        case terminal(paneId: UUID)
        case subagent
    }

    public let id: String
    public let agentId: String
    public let title: String
    public let kind: Kind
    public let status: AgentStatus
    public let children: [AgentNode]

    public init(id: String, agentId: String, title: String, kind: Kind,
                status: AgentStatus, children: [AgentNode] = []) {
        self.id = id
        self.agentId = agentId
        self.title = title
        self.kind = kind
        self.status = status
        self.children = children
    }
}

/// Pure assembly of the Agents panel tree from snapshots of existing state.
/// Chat tabs first, then terminal panes, both in `tabs` order.
public enum AgentTreeBuilder {
    public static func build(
        tabs: [WorkspaceTab],
        agentStatus: [UUID: AgentStatus],
        paneAgents: [UUID: String],
        chatSubagents: [UUID: [ChatSubagentInput]],
        processTrees: [UUID: [ProcessNode]],
        catalogIds: [String],
        displayNames: [String: String]
    ) -> [AgentNode] {
        var chatNodes: [AgentNode] = []
        var terminalNodes: [AgentNode] = []

        for tab in tabs {
            switch tab.content {
            case .chat:
                guard let status = agentStatus[tab.id],
                      let agentId = paneAgents[tab.id] else { continue }
                let children = (chatSubagents[tab.id] ?? []).map { sub in
                    AgentNode(
                        id: "chat:\(tab.id.uuidString):tool:\(sub.id)",
                        agentId: agentId,
                        title: sub.title,
                        kind: .subagent,
                        status: sub.status)
                }
                chatNodes.append(AgentNode(
                    id: "chat:\(tab.id.uuidString)",
                    agentId: agentId,
                    title: displayNames[agentId] ?? agentId,
                    kind: .chat(tabId: tab.id),
                    status: status,
                    children: children))
            case .terminal:
                for paneId in tab.leafIds {
                    guard let status = agentStatus[paneId],
                          let agentId = paneAgents[paneId] else { continue }
                    let children = subagentNodes(
                        paneId: paneId,
                        forest: processTrees[paneId] ?? [],
                        catalogIds: catalogIds,
                        displayNames: displayNames,
                        paneStatus: status)
                    terminalNodes.append(AgentNode(
                        id: "term:\(paneId.uuidString)",
                        agentId: agentId,
                        title: displayNames[agentId] ?? agentId,
                        kind: .terminal(paneId: paneId),
                        status: status,
                        children: children))
                }
            case .markdown:
                continue
            }
        }
        return chatNodes + terminalNodes
    }

    /// Descendants of the first catalog-matched process that themselves match
    /// the catalog become subagent nodes. Intermediate non-catalog processes
    /// (shim shells, ripgrep, node) are skipped but their subtrees are still
    /// searched.
    private static func subagentNodes(
        paneId: UUID,
        forest: [ProcessNode],
        catalogIds: [String],
        displayNames: [String: String],
        paneStatus: AgentStatus
    ) -> [AgentNode] {
        let ids = Set(catalogIds)
        guard let root = firstMatch(in: forest, ids: ids) else { return [] }
        return collectMatches(in: root.children, ids: ids).map { proc in
            AgentNode(
                id: "term:\(paneId.uuidString):pid:\(proc.pid)",
                agentId: proc.name,
                title: displayNames[proc.name] ?? proc.name,
                kind: .subagent,
                status: paneStatus,
                children: [])
        }
    }

    private static func firstMatch(in forest: [ProcessNode], ids: Set<String>) -> ProcessNode? {
        for node in forest {
            if ids.contains(node.name) { return node }
            if let found = firstMatch(in: node.children, ids: ids) { return found }
        }
        return nil
    }

    private static func collectMatches(in forest: [ProcessNode], ids: Set<String>) -> [ProcessNode] {
        var result: [ProcessNode] = []
        for node in forest {
            if ids.contains(node.name) {
                result.append(node)
            }
            result.append(contentsOf: collectMatches(in: node.children, ids: ids))
        }
        return result
    }
}
```

Design note (deviation from spec, justified): subagent nodes are flattened one level under their agent (no nested subagent-of-subagent display) and terminal subagents inherit the pane's status — a child process has no independent status signal. The spec's per-node `Status` enum is replaced by reusing `AgentStatus`, since chat statuses already arrive through `AgentActivityModel` in that type.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd Packages/TillerCore && swift test --filter AgentTreeBuilderTests`
Expected: all 6 tests PASS.

- [ ] **Step 5: Commit**

```bash
git add Packages/TillerCore/Sources/TillerCore/AgentTree.swift Packages/TillerCore/Tests/TillerCoreTests/AgentTreeBuilderTests.swift
git commit -m "feat: add AgentTreeBuilder for agents panel tree"
```

---

### Task 2: Chat subagent extraction (TillerACP + ChatController)

**Files:**
- Modify: `Packages/TillerACP/Sources/TillerACP/TranscriptItem.swift` (add `rawInput` to `ToolCallItem`)
- Create: `Packages/TillerACP/Sources/TillerACP/SubagentTasks.swift`
- Modify: `App/Chat/ChatController.swift` (computed `activeSubagentTasks`)
- Test: `Packages/TillerACP/Tests/TillerACPTests/SubagentTasksTests.swift`

**Interfaces:**
- Consumes: `TranscriptItem`, `ToolCallItem`, `ToolCall`, `ToolCallUpdate`, `ToolCallStatus`, `JSONValue` (all in TillerACP).
- Produces (used by Task 4):
  - `struct SubagentTaskInfo: Equatable, Sendable { let toolCallId: String; let title: String; let status: ToolCallStatus }`
  - `enum SubagentTasks { static func extract(from items: [TranscriptItem]) -> [SubagentTaskInfo] }`
  - `ChatController.activeSubagentTasks: [SubagentTaskInfo]` (computed, `@MainActor`)

Background: Claude Code's ACP adapter reports subagent spawns as tool calls whose `rawInput` object contains a `subagent_type` key (the `Task` tool input schema: `subagent_type`, `description`, `prompt`). `ToolCall` already decodes `rawInput`; `ToolCallItem` currently drops it — this task threads it through.

- [ ] **Step 1: Write the failing tests**

Create `Packages/TillerACP/Tests/TillerACPTests/SubagentTasksTests.swift`:

```swift
import Foundation
import Testing
@testable import TillerACP

@Suite struct SubagentTasksTests {
    func taskToolCall(id: String, status: ToolCallStatus,
                      description: String? = nil) -> TranscriptItem {
        var input: [String: JSONValue] = [
            "subagent_type": .string("Explore"),
            "prompt": .string("look around"),
        ]
        if let description { input["description"] = .string(description) }
        var item = ToolCallItem(toolCallId: id, title: "Task", kind: .other,
                                status: status)
        item.rawInput = .object(input)
        return .toolCall(item)
    }

    @Test func extractsTaskToolCallsWithSubagentType() {
        let items: [TranscriptItem] = [
            .userMessage(id: "u1", blocks: []),
            taskToolCall(id: "t1", status: .inProgress, description: "Explore repo"),
            .agentMessage(id: "a1", text: "hi", isComplete: true),
        ]
        let tasks = SubagentTasks.extract(from: items)
        #expect(tasks.count == 1)
        #expect(tasks[0].toolCallId == "t1")
        #expect(tasks[0].title == "Explore repo")
        #expect(tasks[0].status == .inProgress)
    }

    @Test func ignoresOrdinaryToolCalls() {
        let read = ToolCallItem(toolCallId: "r1", title: "Read file", kind: .read,
                                status: .completed)
        let tasks = SubagentTasks.extract(from: [.toolCall(read)])
        #expect(tasks.isEmpty)
    }

    @Test func fallsBackToToolTitleThenSubagentWhenNoDescription() {
        let tasks = SubagentTasks.extract(from: [
            taskToolCall(id: "t1", status: .completed)
        ])
        #expect(tasks[0].title == "Task")

        var untitled = ToolCallItem(toolCallId: "t2", title: "", kind: .other,
                                    status: .pending)
        untitled.rawInput = .object(["subagent_type": .string("general")])
        let fallback = SubagentTasks.extract(from: [.toolCall(untitled)])
        #expect(fallback[0].title == "Subagent")
    }

    @Test func rawInputSurvivesToolCallItemMergeAndCodableRoundTrip() throws {
        let call = ToolCall(toolCallId: "t1", title: "Task", kind: .other,
                            status: .pending,
                            rawInput: .object(["subagent_type": .string("x")]))
        var item = ToolCallItem(call)
        #expect(item.rawInput == call.rawInput)

        // A later update without rawInput must not erase it.
        item.merge(ToolCallUpdate(toolCallId: "t1", status: .completed))
        #expect(item.rawInput == call.rawInput)
        #expect(item.status == .completed)

        let data = try JSONEncoder().encode(item)
        let decoded = try JSONDecoder().decode(ToolCallItem.self, from: data)
        #expect(decoded.rawInput == call.rawInput)
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd Packages/TillerACP && swift test --filter SubagentTasksTests`
Expected: compile FAILURE — `SubagentTasks` not found, `ToolCallItem` has no `rawInput`.

- [ ] **Step 3: Add `rawInput` to `ToolCallItem`**

In `Packages/TillerACP/Sources/TillerACP/TranscriptItem.swift`:

1. Add the stored property after `terminalExit` (line ~36):

```swift
    /// Raw tool input as reported by the agent; carries the Task tool's
    /// `subagent_type` used to detect subagent spawns.
    public var rawInput: JSONValue?
```

2. Extend the memberwise `init` with a trailing parameter `rawInput: JSONValue? = nil` and assign it (`self.rawInput = rawInput`).

3. In `init(_ call: ToolCall)` pass it through:

```swift
    init(_ call: ToolCall) {
        self.init(toolCallId: call.toolCallId, title: call.title, kind: call.kind,
                  status: call.status, content: call.content, locations: call.locations,
                  rawInput: call.rawInput)
    }
```

4. In `mutating func merge(_ update: ToolCallUpdate)` add (updates rarely carry it, but keep the latest when they do):

```swift
        if let rawInput = update.rawInput { self.rawInput = rawInput }
```

`ToolCallItem` is `Codable` with synthesized conformance, so an optional new field decodes as `nil` from previously persisted transcripts — backward compatible.

- [ ] **Step 4: Implement `SubagentTasks`**

Create `Packages/TillerACP/Sources/TillerACP/SubagentTasks.swift`:

```swift
import Foundation

/// A subagent spawn extracted from the transcript (a tool call whose input
/// carries `subagent_type` — the Claude Code Task tool schema).
public struct SubagentTaskInfo: Equatable, Sendable {
    public let toolCallId: String
    public let title: String
    public let status: ToolCallStatus

    public init(toolCallId: String, title: String, status: ToolCallStatus) {
        self.toolCallId = toolCallId
        self.title = title
        self.status = status
    }
}

public enum SubagentTasks {
    /// Tool calls that spawned a subagent, in transcript order.
    public static func extract(from items: [TranscriptItem]) -> [SubagentTaskInfo] {
        items.compactMap { item in
            guard case .toolCall(let call) = item,
                  case .object(let input)? = call.rawInput,
                  input["subagent_type"] != nil else { return nil }
            let description: String? =
                if case .string(let text)? = input["description"] { text } else { nil }
            let title = description ?? (call.title.isEmpty ? nil : call.title) ?? "Subagent"
            return SubagentTaskInfo(toolCallId: call.toolCallId,
                                    title: title, status: call.status)
        }
    }
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cd Packages/TillerACP && swift test --filter SubagentTasksTests`
Expected: all 4 tests PASS.
Then run the full package suite to catch `ToolCallItem` call-site breaks:
Run: `cd Packages/TillerACP && swift test`
Expected: PASS. If a call site of the memberwise `init` breaks, the new parameter has a default — only pattern-matching exhaustiveness or `Equatable` fixture mismatches would fail; fix fixtures, never weaken the tests.

- [ ] **Step 6: Expose `activeSubagentTasks` on ChatController**

In `App/Chat/ChatController.swift`, next to the other computed transcript accessors (`var items: [TranscriptItem]`, line ~48), add:

```swift
    /// Subagent spawns visible in the transcript (Task-type tool calls),
    /// consumed by the Agents panel.
    var activeSubagentTasks: [SubagentTaskInfo] { SubagentTasks.extract(from: items) }
```

- [ ] **Step 7: Build the app target to verify**

Run: `Scripts/ci.sh` (or at minimum `cd Packages/TillerACP && swift test` plus an app build via ci.sh)
Expected: `CI OK` (retry PTY flake if needed).

- [ ] **Step 8: Commit**

```bash
git add Packages/TillerACP App/Chat/ChatController.swift
git commit -m "feat: extract subagent tasks from chat transcript"
```

---

### Task 3: Recursive process tree (Layer D extension)

**Files:**
- Modify: `App/ForegroundProcessAgent.swift` (recursive walk with injectable process access)
- Modify: `App/AppModel.swift` (store `paneProcessTrees`, update in `checkForegroundAgent`)
- Test: `AppTests/ForegroundProcessTreeTests.swift`

**Interfaces:**
- Consumes: `ProcessNode` (TillerCore, Task 1), `PaneRegistry.shared.shellPid(paneId:)` (existing), `proc_listchildpids`/`proc_name` (existing pattern in the same file).
- Produces (used by Task 4):
  - `ForegroundProcessAgent.processTree(shellPid:) -> [ProcessNode]`
  - `ForegroundProcessAgent.buildTree(roots:childrenOf:nameOf:maxDepth:maxCount:) -> [ProcessNode]` (internal, generic core for tests)
  - `AppModel.paneProcessTrees: [UUID: [ProcessNode]]`

- [ ] **Step 1: Write the failing tests**

Create `AppTests/ForegroundProcessTreeTests.swift` (follow the existing AppTests target conventions — see `AppTests/AppModelControlTests.swift` for the import pattern; the app module is imported `@testable`):

```swift
import Foundation
import Testing
import TillerCore
@testable import Tiller

@Suite struct ForegroundProcessTreeTests {
    // Fake process table: parent pid -> children, pid -> name.
    static let children: [pid_t: [pid_t]] = [
        100: [200],          // shell -> claude
        200: [250, 260],     // claude -> rg, claude(sub)
        260: [270],          // claude(sub) -> node
    ]
    static let names: [pid_t: String] = [
        200: "claude", 250: "rg", 260: "claude", 270: "node",
    ]

    func build(maxDepth: Int = 5, maxCount: Int = 50) -> [ProcessNode] {
        ForegroundProcessAgent.buildTree(
            roots: Self.children[100] ?? [],
            childrenOf: { Self.children[$0] ?? [] },
            nameOf: { Self.names[$0] },
            maxDepth: maxDepth, maxCount: maxCount)
    }

    @Test func buildsNestedTreeWithNames() {
        let tree = build()
        #expect(tree.count == 1)
        #expect(tree[0].pid == 200)
        #expect(tree[0].name == "claude")
        #expect(tree[0].children.map(\.pid) == [250, 260])
        #expect(tree[0].children[1].children.map(\.name) == ["node"])
    }

    @Test func depthCapTruncatesGrandchildren() {
        let tree = build(maxDepth: 1)
        #expect(tree.count == 1)
        #expect(tree[0].children.isEmpty)
    }

    @Test func countCapStopsWalk() {
        let tree = build(maxCount: 2)
        var total = 0
        func count(_ nodes: [ProcessNode]) {
            for n in nodes { total += 1; count(n.children) }
        }
        count(tree)
        #expect(total <= 2)
    }

    @Test func unnamedProcessesAreSkipped() {
        let tree = ForegroundProcessAgent.buildTree(
            roots: [999],
            childrenOf: { _ in [] },
            nameOf: { _ in nil },
            maxDepth: 5, maxCount: 50)
        #expect(tree.isEmpty)
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `Scripts/ci.sh` runs AppTests via xcodebuild; for iteration use the project's app-test invocation. If there is no faster path, build check: the new symbols don't exist yet, so compilation of the test target FAILS — that is the expected failure signal.

- [ ] **Step 3: Implement the recursive walk**

In `App/ForegroundProcessAgent.swift`, add to the existing enum:

```swift
    // MARK: - Recursive tree (Agents panel)

    /// Recursive snapshot of the shell's process subtree. Caps guard against
    /// runaway trees; excess is silently truncated.
    static func processTree(shellPid: pid_t,
                            maxDepth: Int = 5, maxCount: Int = 50) -> [ProcessNode] {
        buildTree(roots: childPids(of: shellPid),
                  childrenOf: childPids(of:),
                  nameOf: processName(of:),
                  maxDepth: maxDepth, maxCount: maxCount)
    }

    /// Generic core, injectable for tests. Depth-first; `maxCount` counts
    /// emitted nodes across the whole forest.
    static func buildTree(roots: [pid_t],
                          childrenOf: (pid_t) -> [pid_t],
                          nameOf: (pid_t) -> String?,
                          maxDepth: Int, maxCount: Int) -> [ProcessNode] {
        var budget = maxCount

        func walk(_ pids: [pid_t], depth: Int) -> [ProcessNode] {
            guard depth <= maxDepth else { return [] }
            var nodes: [ProcessNode] = []
            for pid in pids where pid > 0 {
                guard budget > 0 else { break }
                guard let name = nameOf(pid) else { continue }
                budget -= 1
                let children = walk(childrenOf(pid), depth: depth + 1)
                nodes.append(ProcessNode(pid: pid, name: name, children: children))
            }
            return nodes
        }
        return walk(roots, depth: 1)
    }

    private static func childPids(of pid: pid_t) -> [pid_t] {
        var pids = [pid_t](repeating: 0, count: 64)
        let byteCount = proc_listchildpids(pid, &pids, Int32(pids.count * MemoryLayout<pid_t>.size))
        guard byteCount > 0 else { return [] }
        let count = min(Int(byteCount) / MemoryLayout<pid_t>.size, pids.count)
        return Array(pids.prefix(count)).filter { $0 > 0 }
    }

    private static func processName(of pid: pid_t) -> String? {
        var buffer = [CChar](repeating: 0, count: Int(MAXCOMLEN) * 2 + 1)
        guard proc_name(pid, &buffer, UInt32(buffer.count)) > 0 else { return nil }
        return String(cString: buffer)
    }
```

Also add `import TillerCore` at the top of the file (for `ProcessNode`).

Refactor note: `childProcessNames(of:)` can now delegate to `childPids(of:)` + `processName(of:)` to avoid duplicating the libproc calls — do it, it stays behavior-identical:

```swift
    private static func childProcessNames(of pid: pid_t) -> Set<String> {
        Set(childPids(of: pid).compactMap(processName(of:)))
    }
```

- [ ] **Step 4: Store the snapshot in AppModel**

In `App/AppModel.swift`:

1. Near `var paneTitles: [UUID: String]` (line ~1284) add:

```swift
    /// Layer-D recursive process snapshot per pane, feeding the Agents
    /// panel's terminal subagent tree. In-memory only.
    var paneProcessTrees: [UUID: [ProcessNode]] = [:]
```

2. In `checkForegroundAgent(paneId:)` (line ~1596), capture the tree in the same detached task and clear it when the agent is gone:

```swift
    private func checkForegroundAgent(paneId: UUID) {
        Task.detached {
            guard let pid = await PaneRegistry.shared.shellPid(paneId: paneId) else { return }
            let agentId = ForegroundProcessAgent.identify(shellPid: pid)
            let tree = agentId != nil ? ForegroundProcessAgent.processTree(shellPid: pid) : []
            await MainActor.run { [weak self] in
                guard let self else { return }
                if let agentId {
                    self.agentActivity.processIdentified(paneId: paneId, agentId: agentId, now: Date())
                    self.paneProcessTrees[paneId] = tree
                } else {
                    self.agentActivity.processGone(paneId: paneId)
                    self.paneProcessTrees[paneId] = nil
                }
            }
        }
    }
```

Caveat: `checkForegroundAgent` currently runs only for unregistered or process-owned panes (see `handleContentSignal`, line ~1577). Spawn-owned panes (agents Tiller launched) would never get a tree. Extend `handleContentSignal` so registered panes also refresh their tree on content signals — add after the `processOwnedPanes` branch:

```swift
        // Refresh the subagent process snapshot for any registered pane
        // (spawn-, title- and process-owned alike) — content output means
        // the agent is active and its child tree may have changed.
        checkForegroundAgent(paneId: paneId)
```

Careful: `checkForegroundAgent` on a spawn-owned pane whose agent emits no matching process (e.g. pi/omp under node) returns `agentId == nil` → it calls `processGone(paneId:)`. `processGone` is guarded by `processOwnedPanes.contains(paneId)`, so spawn-owned panes are untouched — safe. But it would also clear `paneProcessTrees` for them; that is correct (no catalog process → no subagents to show).

3. Clean up on pane close — in the pane-close path that already calls `agentActivity.paneClosed(paneId:)` (search for `paneClosed(paneId:` call sites in `App/`), add alongside each:

```swift
        paneProcessTrees[paneId] = nil
```

- [ ] **Step 5: Run tests and CI**

Run: `Scripts/ci.sh`
Expected: `CI OK` (AppTests include the 4 new tests; retry PTY flake if needed).

- [ ] **Step 6: Commit**

```bash
git add App/ForegroundProcessAgent.swift App/AppModel.swift AppTests/ForegroundProcessTreeTests.swift
git commit -m "feat: capture recursive pane process tree for agents panel"
```

---

### Task 4: Agents section UI + right panel split

**Files:**
- Create: `App/RightPanel/AgentsSectionView.swift`
- Create: `App/RightPanel/AgentsPanelModel.swift`
- Modify: `App/RightPanel/RightPanelView.swift` (vertical split)
- Modify: `Packages/TillerCore/Sources/TillerCore/AppSettings.swift` (fraction key)
- Modify: `App/AppModel.swift` (add `focusTab`)

**Interfaces:**
- Consumes: `AgentTreeBuilder` (Task 1), `SubagentTaskInfo`/`activeSubagentTasks` (Task 2), `AppModel.paneProcessTrees` (Task 3), existing `AgentIcon`, `AgentCatalog`, `AppTheme`, `tabs`, `activeTabId`, `agentActivity`, `chatControllers`.
- Produces: the visible Agents section; `AppModel.focusTab(tabId:in:)`.

- [ ] **Step 1: Add the settings key**

In `Packages/TillerCore/Sources/TillerCore/AppSettings.swift`, next to `rightPanelWidthKey` (line ~73):

```swift
    public static let rightPanelAgentsFractionKey = "rightPanel.agentsFraction"
    public static let defaultRightPanelAgentsFraction: Double = 1.0 / 3.0
```

- [ ] **Step 2: Add `focusTab` to AppModel**

In `App/AppModel.swift`, next to `openChatTab` (line ~1300):

```swift
    /// Select a worktree tab from the Agents panel. No-op when the tab is gone.
    func focusTab(tabId: UUID, in worktree: Worktree) {
        guard (tabs[worktree.id] ?? []).contains(where: { $0.id == tabId }) else { return }
        selectedWorktree = worktree
        activeTabId[worktree.id] = tabId
        persistTabs(for: worktree.id)
    }
```

(`persistTabs` is private to AppModel; `focusTab` lives in the same type, so access is fine.)

- [ ] **Step 3: Implement `AgentsPanelModel`**

Create `App/RightPanel/AgentsPanelModel.swift` — a namespace that assembles builder inputs from `AppModel` (kept out of the view so the wiring is readable and the view stays declarative):

```swift
import Foundation
import TillerACP
import TillerAgents
import TillerCore

/// Assembles the Agents panel tree for a worktree from live AppModel state.
@MainActor
enum AgentsPanelModel {
    static func nodes(appModel: AppModel, worktree: Worktree) -> [AgentNode] {
        let tabs = appModel.tabs[worktree.id] ?? []
        var chatSubagents: [UUID: [ChatSubagentInput]] = [:]
        for tab in tabs where tab.chatAgentId != nil {
            guard let controller = appModel.chatControllers[tab.id] else { continue }
            chatSubagents[tab.id] = controller.activeSubagentTasks.map {
                ChatSubagentInput(id: $0.toolCallId, title: $0.title,
                                  status: agentStatus(for: $0.status))
            }
        }
        return AgentTreeBuilder.build(
            tabs: tabs,
            agentStatus: appModel.agentActivity.agentStatus,
            paneAgents: appModel.agentActivity.paneAgents,
            chatSubagents: chatSubagents,
            processTrees: appModel.paneProcessTrees,
            catalogIds: AgentCatalog.all.map(\.id),
            displayNames: Dictionary(uniqueKeysWithValues:
                AgentCatalog.all.map { ($0.id, $0.displayName) }))
    }

    static func agentStatus(for status: ToolCallStatus) -> AgentStatus {
        switch status {
        case .pending, .inProgress: .running
        case .completed: .done
        case .failed: .error
        }
    }
}
```

Note: `chatController(for:in:)` creates controllers lazily; here we read `chatControllers` directly and skip tabs whose controller was never built — a chat tab that was restored but never opened has no live transcript, so there is nothing to show. Do NOT call `chatController(for:in:)` here: building controllers as a side effect of rendering the panel would spawn ACP processes.

If `agentActivity` is `private` in AppModel, expose the two snapshots instead with a tiny accessor next to it (keep it read-only):

```swift
    var agentActivitySnapshot: (status: [UUID: AgentStatus], agents: [UUID: String]) {
        (agentActivity.agentStatus, agentActivity.paneAgents)
    }
```

and use that from `AgentsPanelModel`. Check visibility first; `agentActivity` is referenced from `ContentView` in some flows, so it is likely already internal.

- [ ] **Step 4: Implement `AgentsSectionView`**

Create `App/RightPanel/AgentsSectionView.swift`:

```swift
import SwiftUI
import TillerCore

/// Bottom section of the right panel: agents of the selected worktree with
/// their subagent tree. Click focuses the owning tab.
struct AgentsSectionView: View {
    @Bindable var appModel: AppModel
    let worktree: Worktree
    @State private var collapsedIds: Set<String> = []

    private var nodes: [AgentNode] {
        AgentsPanelModel.nodes(appModel: appModel, worktree: worktree)
    }

    private var runningCount: Int {
        nodes.filter { $0.status == .running }.count
    }

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                Text("Agents")
                    .font(.subheadline.weight(.semibold))
                Spacer()
                if runningCount > 0 {
                    Text("\(runningCount) running")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            }
            .padding(.horizontal, 10)
            .padding(.vertical, 6)
            Divider()
            if nodes.isEmpty {
                ContentUnavailableView(
                    "No agents running",
                    systemImage: "person.2.slash",
                    description: Text("Agents launched in this worktree appear here."))
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else {
                ScrollView {
                    LazyVStack(alignment: .leading, spacing: 2) {
                        ForEach(nodes) { node in
                            row(node, depth: 0)
                            if !collapsedIds.contains(node.id) {
                                ForEach(node.children) { child in
                                    row(child, depth: 1)
                                }
                            }
                        }
                    }
                    .padding(6)
                }
            }
        }
    }

    @ViewBuilder
    private func row(_ node: AgentNode, depth: Int) -> some View {
        HStack(spacing: 6) {
            if depth == 0, !node.children.isEmpty {
                Button {
                    if collapsedIds.contains(node.id) {
                        collapsedIds.remove(node.id)
                    } else {
                        collapsedIds.insert(node.id)
                    }
                } label: {
                    Image(systemName: collapsedIds.contains(node.id)
                          ? "chevron.right" : "chevron.down")
                        .font(.caption2)
                }
                .buttonStyle(.plain)
            }
            AgentIcon(agentId: node.agentId)
                .frame(width: 14, height: 14)
            Text(node.title)
                .font(.callout)
                .lineLimit(1)
                .truncationMode(.tail)
            Spacer(minLength: 4)
            statusDot(node.status)
        }
        .padding(.vertical, 3)
        .padding(.leading, CGFloat(depth) * 18 + 4)
        .padding(.trailing, 6)
        .contentShape(Rectangle())
        .onTapGesture { focus(node) }
        .accessibilityElement(children: .combine)
        .accessibilityLabel("\(node.title), \(node.status.humanLabel)")
    }

    @ViewBuilder
    private func statusDot(_ status: AgentStatus) -> some View {
        switch status {
        case .running:
            RunningDots()
        case .needsInput:
            Circle().fill(.yellow).frame(width: 7, height: 7)
        case .done:
            Image(systemName: "checkmark.circle.fill")
                .font(.caption2).foregroundStyle(.secondary)
        case .error:
            Image(systemName: "xmark.circle.fill")
                .font(.caption2).foregroundStyle(.red)
        }
    }

    private func focus(_ node: AgentNode) {
        switch node.kind {
        case .chat(let tabId):
            appModel.focusTab(tabId: tabId, in: worktree)
        case .terminal(let paneId):
            guard let tab = (appModel.tabs[worktree.id] ?? [])
                .first(where: { $0.leafIds.contains(paneId) }) else { return }
            appModel.focusTab(tabId: tab.id, in: worktree)
        case .subagent:
            // Subagents focus their parent: find the top-level node owning it.
            guard let parent = nodes.first(where: { p in
                p.children.contains(where: { $0.id == node.id })
            }) else { return }
            focus(parent)
        }
    }
}
```

Adaptation notes for the implementer:
- `AgentIcon` — check its actual initializer in `App/AgentIcon.swift` (it may take an adapter or an id; adapt the call, keep 14pt sizing).
- `RunningDots` — check `App/RunningDots.swift`; if its default size is too large for a row, wrap in `.scaleEffect` or use a plain green `Circle()` (7pt) instead. A static green dot is acceptable; do not build a new animation.
- Respect reduced motion if `RunningDots` animates (follow whatever the sidebar already does with it).

- [ ] **Step 5: Split `RightPanelView`**

In `App/RightPanel/RightPanelView.swift`, wrap the existing content: everything currently in `body`'s `VStack` below the header stays as the top region; append the Agents section with a draggable horizontal divider. Replace `body` with:

```swift
    @AppStorage(AppSettings.rightPanelAgentsFractionKey)
    private var agentsFraction = AppSettings.defaultRightPanelAgentsFraction

    var body: some View {
        GeometryReader { geo in
            VStack(spacing: 0) {
                toolsRegion
                    .frame(height: max(120, geo.size.height * (1 - agentsFraction)))
                splitDivider(totalHeight: geo.size.height)
                Group {
                    if let worktree = panelModel.worktree {
                        AgentsSectionView(appModel: appModel, worktree: worktree)
                    } else {
                        ContentUnavailableView(
                            "No agents running",
                            systemImage: "person.2.slash")
                    }
                }
                .frame(maxHeight: .infinity)
            }
        }
        .task(id: effectiveMode) {
            if effectiveMode == .diff { await panelModel.ensureDiffLoaded() }
        }
        .alert(item: $pendingDiscard) { pending in
            // ... existing alert body unchanged ...
        }
    }

    /// Existing panel content (header picker + Files/Diff/Status), unchanged.
    private var toolsRegion: some View {
        VStack(spacing: 0) {
            // move the existing header HStack, monitorError label and mode
            // Group here verbatim
        }
    }

    private func splitDivider(totalHeight: CGFloat) -> some View {
        Divider()
            .frame(maxWidth: .infinity)
            .frame(height: 7)
            .contentShape(Rectangle())
            .onHover { inside in
                if inside { NSCursor.resizeUpDown.push() } else { NSCursor.pop() }
            }
            .gesture(
                DragGesture(coordinateSpace: .named("rightPanelSplit"))
                    .onChanged { value in
                        guard totalHeight > 0 else { return }
                        let newFraction = agentsFraction - value.translation.height / totalHeight
                        agentsFraction = min(0.6, max(0.15, newFraction))
                    })
    }
```

and add `.coordinateSpace(name: "rightPanelSplit")` on the `GeometryReader` content. Move the existing header/monitorError/mode-switch content into `toolsRegion` verbatim — no changes to `FileExplorerView`/`GitDiffView`/`GitStatusView` call sites. The "No worktree selected" `ContentUnavailableView` stays inside `toolsRegion` as today.

Ponytail note: fraction-based drag with clamps (0.15–0.6) instead of `VSplitView`, because `VSplitView` exposes no divider-position binding for persistence.

- [ ] **Step 6: Regenerate the Xcode project and build**

Run:
```bash
xcodegen generate
Scripts/ci.sh
```
Expected: `CI OK` (retry PTY flake if needed).

- [ ] **Step 7: Commit**

```bash
git add App/RightPanel Packages/TillerCore/Sources/TillerCore/AppSettings.swift App/AppModel.swift project.yml
git commit -m "feat: add agents section to right panel"
```

(Only add `project.yml` if it was actually modified; `xcodegen generate` output `Tiller.xcodeproj` is gitignored.)

---

### Task 5: Manual smoke checklist (no code)

- [ ] Open a worktree, launch a Claude Code chat tab → Agents section shows "Claude Code" with pulsing/running indicator.
- [ ] Ask the chat agent to spawn a subagent (e.g. a Task/Explore request) → child row appears under the chat agent with the task description; completes → check icon.
- [ ] Open a terminal tab, run `claude` manually → terminal agent row appears (Layer B/D); spawn nothing → no children.
- [ ] In the terminal Claude, trigger a subagent → child row appears (process tree); exit Claude → row disappears (process-owned) or shows done (spawn-owned exit).
- [ ] Click a row → the owning tab activates. Click a subagent row → parent tab activates.
- [ ] Drag the divider → proportions persist across app restart (`rightPanel.agentsFraction`).
- [ ] Worktree with no agents → "No agents running" empty state; panel still usable.
- [ ] Non-git project → Files-only mode on top, Agents section still present below.

---

## Self-review notes

- Spec coverage: layout split (T4), worktree scope (T4 via `panelModel.worktree`), chat+terminal top-level (T1+existing AgentActivityModel), chat subagents via ACP (T2), terminal subagents via libproc (T3), tree UI + focus + empty state (T4), lifecycle rule B (inherited from AgentActivityModel exit mapping), caps (T3), English strings (T4). Deviations from spec, both justified inline: `AgentStatus` reused instead of a parallel `Status` enum; subagent nodes flattened one level.
- Types consistent across tasks: `ProcessNode(pid:name:children:)` (T1↔T3), `ChatSubagentInput(id:title:status:)` (T1↔T4), `SubagentTaskInfo(toolCallId:title:status:)` (T2↔T4), `focusTab(tabId:in:)` (T4).
- `SplitTree` single-pane init and `AgentIcon`/`RunningDots` initializers are flagged as check-before-use with fallback instructions.
