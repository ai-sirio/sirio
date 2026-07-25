# Chat Cards (Task, Question, Transcript Pass) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give Tiller's chat a shared card language, a Task card that contains a subagent's work, and a Question card that lives in the transcript — plus the transcript fixes listed in the spec.

**Architecture:** Parent/child nesting is a persisted `parentToolCallId` on the tool-call model plus one pure grouping function (`ToolCallTree.group`) called once per items change. Views consume roots + a children map. All logic lands in `TillerACP` (covered by `Scripts/ci.sh`); `App/Chat` holds only SwiftUI.

**Tech Stack:** Swift 6, SwiftUI (macOS 15+), swift-testing, GRDB (untouched here), xcodegen.

**Spec:** `docs/superpowers/specs/2026-07-25-chat-cards-design.md`

## Global Constraints

- Swift 6, macOS 15 minimum — `ScrollPosition` and `scrollPosition(_:)` are available and allowed.
- Tests use swift-testing (`@Test` / `#expect`) inside a `@Suite struct`. Never XCTest.
- All user-facing strings are English, even when the surrounding conversation is not.
- Domain types are structs; classes only for real identity.
- Never hand-edit `Tiller.xcodeproj`. New files under `App/` are picked up by `xcodegen generate`, which `Scripts/ci.sh` runs first.
- `Scripts/ci.sh` is the gate and must print `CI OK`. It **builds** the `App/` target (so compile errors are caught) but runs **no** App tests — anything with verification value belongs in `TillerACP`.
- `Packages/TillerTerminal` has a known flaky PTY test. A single red run there is not a regression: rerun.
- No `GeometryReader` inside the transcript `ScrollView`. That is what caused the layout storms behind the still-disabled timeline rows.
- Chat cards read colours from `AppTheme`, never `.quaternary` or literal colours.
- **No transcript item may ever be dropped** because its parent is missing.
- Commits follow Conventional Commits, lower-case imperative subject.

---

### Task 1: Probe how `AskUserQuestion` reaches Tiller

The spec's one open question. It decides which adapter Task 6 wires up; it does not change any type.

**Files:**
- Create: `Packages/TillerACP/Tests/TillerACPTests/Fixtures/askuserquestion-wire.jsonl`
- Modify: `docs/superpowers/specs/2026-07-25-chat-cards-design.md` (replace the "Open question" section with the finding)

- [ ] **Step 1: Capture the native stream-json wire**

Tiller launches Claude as `claude -p --input-format stream-json --output-format stream-json --verbose --permission-mode <mode>` (`ClaudeStreamJSONDriver.swift:74`). For the probe, drive it with a plain prompt:

```bash
cd /tmp && mkdir -p askq-probe && cd askq-probe
claude -p "Use the AskUserQuestion tool to ask me which of three storage backends I want: SQLite, in-memory, or a flat file. Ask, do not decide." \
  --output-format stream-json --verbose --permission-mode default \
  2>&1 | tee /tmp/askuserquestion-wire.jsonl
```

- [ ] **Step 2: Classify what arrived**

Look for exactly one of these in `/tmp/askuserquestion-wire.jsonl`:

```bash
grep -c '"can_use_tool"' /tmp/askuserquestion-wire.jsonl        # control-request path
grep -c '"name":"AskUserQuestion"' /tmp/askuserquestion-wire.jsonl  # tool_use path
```

Record which one is non-zero, and copy the full JSON object carrying the question payload (the one containing `"questions"`).

- [ ] **Step 3: Save the fixture**

Write the captured lines (question payload plus the surrounding `assistant` / `control_request` envelope) to `Packages/TillerACP/Tests/TillerACPTests/Fixtures/askuserquestion-wire.jsonl`. Strip any absolute paths or repository content from the prompt echo.

If **nothing** matched in Step 2 — the tool is unavailable in non-interactive mode — save the file with a single line:

```
{"probe":"AskUserQuestion not emitted in stream-json mode"}
```

- [ ] **Step 4: Record the decision in the spec**

Replace the "Open question (resolve first, do not block on)" section of the spec with a "Question source" section stating, in one paragraph: which path carries the question (`can_use_tool`, `tool_use`, or neither), and therefore whether `ChatQuestion.from(toolCall:)` (Task 6) reads `rawInput["questions"]` in practice or only ever sees permission options. Keep both code paths regardless — the ACP transport may differ from the native one.

- [ ] **Step 5: Commit**

```bash
git add Packages/TillerACP/Tests/TillerACPTests/Fixtures/askuserquestion-wire.jsonl \
        docs/superpowers/specs/2026-07-25-chat-cards-design.md
git commit -m "docs: record AskUserQuestion wire probe result"
```

---

### Task 2: Carry `parentToolCallId` through the tool-call model

**Files:**
- Modify: `Packages/TillerACP/Sources/TillerACP/ToolCall.swift:149-219`
- Modify: `Packages/TillerACP/Sources/TillerACP/TranscriptItem.swift:25-84`
- Test: `Packages/TillerACP/Tests/TillerACPTests/ToolCallParentTests.swift`

**Interfaces:**
- Produces: `ToolCall.parentToolCallId: String?`, `ToolCallUpdate.parentToolCallId: String?`, `ToolCallItem.parentToolCallId: String?`. All optional, all `decodeIfPresent`.

- [ ] **Step 1: Write the failing test**

```swift
import Foundation
import Testing
@testable import TillerACP

@Suite struct ToolCallParentTests {
    @Test func legacyToolCallDecodesWithoutParent() throws {
        let json = Data(#"{"toolCallId":"t1","title":"Read","kind":"read","status":"completed"}"#.utf8)
        let call = try JSONDecoder().decode(ToolCall.self, from: json)
        #expect(call.parentToolCallId == nil)
    }

    @Test func parentSurvivesEncodeDecode() throws {
        let call = ToolCall(toolCallId: "child", title: "Read", kind: .read,
                            status: .completed, parentToolCallId: "task-1")
        let data = try JSONEncoder().encode(call)
        let decoded = try JSONDecoder().decode(ToolCall.self, from: data)
        #expect(decoded.parentToolCallId == "task-1")
    }

    @Test func itemInheritsParentFromCall() {
        let call = ToolCall(toolCallId: "child", title: "Read", kind: .read,
                            status: .completed, parentToolCallId: "task-1")
        let item = ToolCallItem(call)
        #expect(item.parentToolCallId == "task-1")
    }

    @Test func mergeAdoptsParentWhenUpdateCarriesIt() {
        var item = ToolCallItem(toolCallId: "child", title: "Read", kind: .read,
                                status: .pending)
        item.merge(ToolCallUpdate(toolCallId: "child", status: .completed,
                                  parentToolCallId: "task-1"))
        #expect(item.parentToolCallId == "task-1")
        #expect(item.status == .completed)
    }

    @Test func mergeKeepsExistingParentWhenUpdateOmitsIt() {
        var item = ToolCallItem(toolCallId: "child", title: "Read", kind: .read,
                                status: .pending)
        item.parentToolCallId = "task-1"
        item.merge(ToolCallUpdate(toolCallId: "child", status: .completed))
        #expect(item.parentToolCallId == "task-1")
    }
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cd Packages/TillerACP && swift test --filter ToolCallParentTests`
Expected: compile failure — `ToolCall` has no `parentToolCallId` parameter.

- [ ] **Step 3: Add the field to the three types**

In `ToolCall.swift`, inside `struct ToolCall`: add the stored property, the coding key, the init parameter (last, defaulted), and the decode line.

```swift
    public var parentToolCallId: String?
    private enum CodingKeys: String, CodingKey {
        case toolCallId, title, kind, status, content, locations, rawInput
        case parentToolCallId
        case terminalMeta = "_meta"
    }

    public init(toolCallId: String, title: String, kind: ToolKind,
                status: ToolCallStatus, content: [ToolCallContent] = [],
                locations: [ToolCallLocation] = [], rawInput: JSONValue? = nil,
                terminalMeta: TerminalMeta? = nil,
                parentToolCallId: String? = nil) {
        // …existing assignments…
        self.parentToolCallId = parentToolCallId
    }

    // in init(from:)
        parentToolCallId = try container.decodeIfPresent(String.self,
                                                         forKey: .parentToolCallId)
```

Apply the same three additions to `ToolCallUpdate` (stored property, coding key, defaulted init parameter, `decodeIfPresent`).

In `TranscriptItem.swift`, add to `ToolCallItem`: the stored property, a defaulted init parameter, propagation in `init(_ call: ToolCall)`, and one line in `merge`:

```swift
    public var parentToolCallId: String?

    // in init(_ call:)
        self.parentToolCallId = call.parentToolCallId

    // in merge(_:)
        if let parent = update.parentToolCallId { parentToolCallId = parent }
```

`ToolCallItem.init(_ call:)` currently calls `self.init(...)`; add `parentToolCallId` as a defaulted parameter on the memberwise init and pass `call.parentToolCallId` through it.

- [ ] **Step 4: Run the test to verify it passes**

Run: `cd Packages/TillerACP && swift test --filter ToolCallParentTests`
Expected: 5 tests pass.

- [ ] **Step 5: Commit**

```bash
git add Packages/TillerACP/Sources/TillerACP/ToolCall.swift \
        Packages/TillerACP/Sources/TillerACP/TranscriptItem.swift \
        Packages/TillerACP/Tests/TillerACPTests/ToolCallParentTests.swift
git commit -m "feat: carry parentToolCallId on tool calls"
```

---

### Task 3: `ToolCallTree.group` — one pass, roots plus children

**Files:**
- Create: `Packages/TillerACP/Sources/TillerACP/ToolCallTree.swift`
- Test: `Packages/TillerACP/Tests/TillerACPTests/ToolCallTreeTests.swift`

**Interfaces:**
- Consumes: `ToolCallItem.parentToolCallId` (Task 2).
- Produces:
  - `ToolCallTree.Grouped` with `roots: [TranscriptItem]`, `children: [String: [ToolCallItem]]`, `pendingPlanApproval: PermissionState?`, and `func children(of id: String) -> [ToolCallItem]`.
  - `ToolCallTree.group(items: [TranscriptItem]) -> Grouped`.

- [ ] **Step 1: Write the failing test**

```swift
import Foundation
import Testing
@testable import TillerACP

@Suite struct ToolCallTreeTests {
    func call(_ id: String, parent: String? = nil,
              kind: ToolKind = .read) -> TranscriptItem {
        var item = ToolCallItem(toolCallId: id, title: id, kind: kind,
                                status: .completed)
        item.parentToolCallId = parent
        return .toolCall(item)
    }

    @Test func childrenAreLiftedOutOfRoots() {
        let items = [call("task"), call("c1", parent: "task"), call("c2", parent: "task")]
        let grouped = ToolCallTree.group(items: items)
        #expect(grouped.roots.count == 1)
        #expect(grouped.roots.first?.id == "task")
        #expect(grouped.children(of: "task").map(\.toolCallId) == ["c1", "c2"])
    }

    @Test func orphanChildStaysARoot() {
        let grouped = ToolCallTree.group(items: [call("c1", parent: "missing")])
        #expect(grouped.roots.count == 1)
        #expect(grouped.roots.first?.id == "c1")
        #expect(grouped.children(of: "missing").isEmpty)
    }

    @Test func childArrivingBeforeParentIsStillNested() {
        let items = [call("c1", parent: "task"), call("task")]
        let grouped = ToolCallTree.group(items: items)
        #expect(grouped.roots.map(\.id) == ["task"])
        #expect(grouped.children(of: "task").map(\.toolCallId) == ["c1"])
    }

    @Test func noItemIsEverLost() {
        let items = [
            .userMessage(id: "u1", blocks: []),
            call("task"), call("c1", parent: "task"), call("c2", parent: "nope"),
            .agentMessage(id: "a1", text: "hi", isComplete: true),
        ] as [TranscriptItem]
        let grouped = ToolCallTree.group(items: items)
        let nested = grouped.children.values.reduce(0) { $0 + $1.count }
        #expect(grouped.roots.count + nested == items.count)
    }

    @Test func nonToolItemsKeepTheirOrder() {
        let items: [TranscriptItem] = [
            .userMessage(id: "u1", blocks: []),
            call("task"),
            .agentMessage(id: "a1", text: "hi", isComplete: true),
        ]
        #expect(ToolCallTree.group(items: items).roots.map(\.id) == ["u1", "task", "a1"])
    }

    @Test func pendingPlanApprovalIsFoundInTheSamePass() {
        var switchMode = ToolCallItem(toolCallId: "sm", title: "exit plan",
                                      kind: .switchMode, status: .pending)
        switchMode.permission = PermissionState(
            requestId: .string("r1"),
            options: [PermissionOption(optionId: "allow_once", name: "Approve",
                                       kind: .allowOnce)])
        let grouped = ToolCallTree.group(items: [.toolCall(switchMode)])
        #expect(grouped.pendingPlanApproval?.requestId == .string("r1"))
    }

    @Test func resolvedPlanApprovalIsNotReported() {
        var switchMode = ToolCallItem(toolCallId: "sm", title: "exit plan",
                                      kind: .switchMode, status: .completed)
        switchMode.permission = PermissionState(
            requestId: .string("r1"), options: [],
            resolution: .selected(optionId: "allow_once"))
        #expect(ToolCallTree.group(items: [.toolCall(switchMode)]).pendingPlanApproval == nil)
    }
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cd Packages/TillerACP && swift test --filter ToolCallTreeTests`
Expected: compile failure — no such type `ToolCallTree`.

- [ ] **Step 3: Implement the grouping**

Create `Packages/TillerACP/Sources/TillerACP/ToolCallTree.swift`:

```swift
import Foundation

/// Splits a flat transcript into top-level items plus each task's children,
/// in one pass. Called once per items change — never per rendered card.
public enum ToolCallTree {
    public struct Grouped: Sendable, Equatable {
        public var roots: [TranscriptItem]
        public var children: [String: [ToolCallItem]]
        /// Pending plan-mode approval (kind .switchMode), collected here so
        /// views never rescan the transcript to find it.
        public var pendingPlanApproval: PermissionState?

        public func children(of toolCallId: String) -> [ToolCallItem] {
            children[toolCallId] ?? []
        }
    }

    public static func group(items: [TranscriptItem]) -> Grouped {
        var known = Set<String>()
        for item in items {
            if case .toolCall(let call) = item { known.insert(call.toolCallId) }
        }

        var roots: [TranscriptItem] = []
        var children: [String: [ToolCallItem]] = [:]
        var pendingPlan: PermissionState?

        for item in items {
            guard case .toolCall(let call) = item else {
                roots.append(item)
                continue
            }
            if call.kind == .switchMode, let permission = call.permission,
               permission.isPending {
                pendingPlan = permission
            }
            // An unknown parent means the child keeps its place in the flow:
            // information is never hidden because an item is missing.
            if let parent = call.parentToolCallId, known.contains(parent) {
                children[parent, default: []].append(call)
            } else {
                roots.append(item)
            }
        }

        return Grouped(roots: roots, children: children,
                       pendingPlanApproval: pendingPlan)
    }
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `cd Packages/TillerACP && swift test --filter ToolCallTreeTests`
Expected: 7 tests pass.

- [ ] **Step 5: Commit**

```bash
git add Packages/TillerACP/Sources/TillerACP/ToolCallTree.swift \
        Packages/TillerACP/Tests/TillerACPTests/ToolCallTreeTests.swift
git commit -m "feat: group tool calls by parent in one pass"
```

---

### Task 4: Driver propagates the parent and stops misattributing subagent text

Fixes spec item #1.

**Files:**
- Modify: `Packages/TillerACP/Sources/TillerACP/Drivers/ClaudeStreamJSONDriver.swift:233-236, 300-316`
- Test: `Packages/TillerACP/Tests/TillerACPTests/ClaudeSubagentNestingTests.swift`

**Interfaces:**
- Consumes: `ClaudeAssistantMessage.parentToolUseId` (`ClaudeWire.swift:70`), `ToolCall.parentToolCallId` (Task 2).
- Produces: emitted `SessionUpdate.toolCall` values carry `parentToolCallId`; assistant text and thinking blocks that belong to a subagent produce **no** event.

**Deliberate reduction:** a subagent's intermediate prose is dropped rather than re-homed. Its conclusion still arrives as the Task tool's result (the `user` / `tool_result` line, already merged onto the card). Re-homing would need an append-content channel on `ToolCallUpdate`, whose `merge` replaces content wholesale — not worth a protocol field for intermediate chatter. If it later matters, add `ToolCallUpdate.appendContent`.

- [ ] **Step 1: Write the failing test**

```swift
import Foundation
import Testing
@testable import TillerACP

@Suite struct ClaudeSubagentNestingTests {
    /// Feeds raw wire lines through the driver and collects everything it emits
    /// until the transport closes. `MockTransport` already exists in this target
    /// (`Tests/TillerACPTests/MockTransport.swift`) — do not add another double.
    func events(from lines: [String]) async throws -> [ACPSessionEvent] {
        let transport = MockTransport()
        let driver = ClaudeStreamJSONDriver(transport: transport,
                                            permissionMode: .ask,
                                            model: nil, resumeSessionId: nil)
        let stream = driver.events
        try await driver.start()
        for line in lines { await transport.emit(line) }
        await transport.close()
        var collected: [ACPSessionEvent] = []
        for await event in stream { collected.append(event) }
        return collected
    }

    @Test func childToolCallCarriesItsParent() async throws {
        let line = """
        {"type":"assistant","session_id":"s","parent_tool_use_id":"task-1",\
        "message":{"role":"assistant","content":[{"type":"tool_use","id":"c1",\
        "name":"Read","input":{"file_path":"/tmp/a.swift"}}]}}
        """
        let events = try await events(from: [line])
        let calls: [ToolCall] = events.compactMap {
            if case .update(.toolCall(let call)) = $0 { return call } else { return nil }
        }
        #expect(calls.count == 1)
        #expect(calls[0].parentToolCallId == "task-1")
    }

    @Test func topLevelToolCallHasNoParent() async throws {
        let line = """
        {"type":"assistant","session_id":"s",\
        "message":{"role":"assistant","content":[{"type":"tool_use","id":"t1",\
        "name":"Read","input":{}}]}}
        """
        let events = try await events(from: [line])
        let calls: [ToolCall] = events.compactMap {
            if case .update(.toolCall(let call)) = $0 { return call } else { return nil }
        }
        #expect(calls[0].parentToolCallId == nil)
    }

    @Test func subagentTextIsNotEmittedAsAgentMessage() async throws {
        let line = """
        {"type":"assistant","session_id":"s","parent_tool_use_id":"task-1",\
        "message":{"role":"assistant","content":[{"type":"text",\
        "text":"looking around the repo"}]}}
        """
        let events = try await events(from: [line])
        let chunks = events.filter {
            if case .update(.agentMessageChunk) = $0 { return true } else { return false }
        }
        #expect(chunks.isEmpty)
    }

    @Test func mainAgentTextStillBecomesAnAgentMessage() async throws {
        let line = """
        {"type":"assistant","session_id":"s",\
        "message":{"role":"assistant","content":[{"type":"text","text":"done"}]}}
        """
        let events = try await events(from: [line])
        let chunks = events.filter {
            if case .update(.agentMessageChunk) = $0 { return true } else { return false }
        }
        #expect(chunks.count == 1)
    }
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cd Packages/TillerACP && swift test --filter ClaudeSubagentNestingTests`
Expected: `childToolCallCarriesItsParent` and `subagentTextIsNotEmittedAsAgentMessage` fail (parent is nil; a chunk is emitted).

- [ ] **Step 3: Thread the parent through**

In `ClaudeStreamJSONDriver.swift`, pass the parent down:

```swift
        case .assistant(let assistant):
            for block in assistant.message.content {
                handleAssistantBlock(block,
                                     parentToolUseId: assistant.parentToolUseId)
            }
```

and rewrite `handleAssistantBlock`:

```swift
    /// `parentToolUseId` is non-nil for blocks produced *inside* a subagent.
    /// Its tool calls nest under the spawning Task; its prose is intermediate
    /// chatter and is dropped — the final report arrives as the tool result.
    private func handleAssistantBlock(_ block: ClaudeContentBlock,
                                      parentToolUseId: String?) {
        switch block {
        case .text(let text):
            guard parentToolUseId == nil else { return }
            eventContinuation.yield(.update(.agentMessageChunk(.text(text))))
        case .thinking(let thinking):
            guard parentToolUseId == nil else { return }
            eventContinuation.yield(.update(.agentThoughtChunk(.text(thinking))))
        case .toolUse(let toolUse):
            eventContinuation.yield(.update(.toolCall(ToolCall(
                toolCallId: toolUse.id,
                title: toolTitle(name: toolUse.name, input: toolUse.input),
                kind: toolKind(for: toolUse.name),
                status: .inProgress,
                rawInput: toolUse.input,
                parentToolCallId: parentToolUseId))))
        case .toolResult, .unknown:
            break
        }
    }
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `cd Packages/TillerACP && swift test --filter ClaudeSubagentNestingTests`
Expected: 4 tests pass.

- [ ] **Step 5: Commit**

```bash
git add Packages/TillerACP/Sources/TillerACP/Drivers/ClaudeStreamJSONDriver.swift \
        Packages/TillerACP/Tests/TillerACPTests/ClaudeSubagentNestingTests.swift
git commit -m "fix: nest subagent tool calls and stop misattributing their text"
```

---

### Task 5: Expose the subagent type once, for both the panel and the card

**Files:**
- Modify: `Packages/TillerACP/Sources/TillerACP/SubagentTasks.swift`
- Test: `Packages/TillerACP/Tests/TillerACPTests/SubagentTasksTests.swift`

**Interfaces:**
- Produces: `SubagentTaskInfo.subagentType: String`, and `SubagentTasks.info(for item: ToolCallItem) -> SubagentTaskInfo?` used by both `extract(from:)` and `TaskCardView` (Task 11).

- [ ] **Step 1: Write the failing test**

Append to the existing `SubagentTasksTests` suite:

```swift
    @Test func infoExposesSubagentTypeForASingleItem() {
        var item = ToolCallItem(toolCallId: "t1", title: "Task", kind: .other,
                                status: .inProgress)
        item.rawInput = .object(["subagent_type": .string("Explore"),
                                 "description": .string("Explore repo")])
        let info = SubagentTasks.info(for: item)
        #expect(info?.subagentType == "Explore")
        #expect(info?.title == "Explore repo")
    }

    @Test func infoIsNilForAPlainToolCall() {
        let item = ToolCallItem(toolCallId: "t1", title: "Read", kind: .read,
                                status: .completed)
        #expect(SubagentTasks.info(for: item) == nil)
    }
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cd Packages/TillerACP && swift test --filter SubagentTasksTests`
Expected: compile failure — no `info(for:)`, no `subagentType`.

- [ ] **Step 3: Refactor `SubagentTasks` around the single-item function**

```swift
public struct SubagentTaskInfo: Equatable, Sendable {
    public let toolCallId: String
    public let title: String
    public let status: ToolCallStatus
    public let subagentType: String

    public init(toolCallId: String, title: String, status: ToolCallStatus,
                subagentType: String) {
        self.toolCallId = toolCallId
        self.title = title
        self.status = status
        self.subagentType = subagentType
    }
}

public enum SubagentTasks {
    /// The subagent spawn a tool call represents, or nil if it is a plain call.
    public static func info(for call: ToolCallItem) -> SubagentTaskInfo? {
        guard case .object(let input)? = call.rawInput,
              case .string(let subagentType)? = input["subagent_type"] else { return nil }
        let description: String? =
            if case .string(let text)? = input["description"] { text } else { nil }
        let title = description ?? (call.title.isEmpty ? nil : call.title) ?? "Subagent"
        return SubagentTaskInfo(toolCallId: call.toolCallId, title: title,
                                status: call.status, subagentType: subagentType)
    }

    /// Tool calls that spawned a subagent, in transcript order.
    public static func extract(from items: [TranscriptItem]) -> [SubagentTaskInfo] {
        items.compactMap { item in
            guard case .toolCall(let call) = item else { return nil }
            return info(for: call)
        }
    }
}
```

Note the behaviour change: `subagent_type` must now be a **string** (it always is on the wire). Existing tests that pass `.object(["subagent_type": .string("x")])` still hold.

- [ ] **Step 4: Run the test to verify it passes**

Run: `cd Packages/TillerACP && swift test --filter SubagentTasksTests`
Expected: all tests in the suite pass, old and new.

- [ ] **Step 5: Commit**

```bash
git add Packages/TillerACP/Sources/TillerACP/SubagentTasks.swift \
        Packages/TillerACP/Tests/TillerACPTests/SubagentTasksTests.swift
git commit -m "refactor: expose subagent type per tool call"
```

---

### Task 6: `ChatQuestion` — one shape for both question sources

**Files:**
- Create: `Packages/TillerACP/Sources/TillerACP/ChatQuestion.swift`
- Test: `Packages/TillerACP/Tests/TillerACPTests/ChatQuestionTests.swift`

**Interfaces:**
- Produces: `ChatQuestion` (with nested `Option`) and `ChatQuestion.from(_ call: ToolCallItem) -> ChatQuestion?`, consumed by `QuestionCardView` (Task 12).

- [ ] **Step 1: Write the failing test**

```swift
import Foundation
import Testing
@testable import TillerACP

@Suite struct ChatQuestionTests {
    @Test func buildsFromAPermissionRequest() {
        var call = ToolCallItem(toolCallId: "t1", title: "Write config.toml",
                                kind: .edit, status: .pending)
        call.permission = PermissionState(
            requestId: .string("r1"),
            options: [PermissionOption(optionId: "allow_once", name: "Allow once",
                                       kind: .allowOnce),
                      PermissionOption(optionId: "reject_once", name: "Reject",
                                       kind: .rejectOnce)])
        let question = ChatQuestion.from(call)
        #expect(question?.header == "Write config.toml")
        #expect(question?.options.map(\.id) == ["allow_once", "reject_once"])
        #expect(question?.options.last?.isRejection == true)
        #expect(question?.chosenOptionId == nil)
    }

    @Test func buildsFromAnAskUserQuestionInput() {
        var call = ToolCallItem(toolCallId: "t1", title: "AskUserQuestion",
                                kind: .other, status: .pending)
        call.rawInput = .object([
            "questions": .array([.object([
                "header": .string("Storage"),
                "question": .string("Which backend?"),
                "options": .array([
                    .object(["label": .string("SQLite"),
                             "description": .string("durable")]),
                    .object(["label": .string("In-memory")]),
                ]),
            ])]),
        ])
        call.permission = PermissionState(requestId: .string("r1"), options: [])
        let question = ChatQuestion.from(call)
        #expect(question?.header == "Storage")
        #expect(question?.prompt == "Which backend?")
        #expect(question?.options.map(\.label) == ["SQLite", "In-memory"])
        #expect(question?.options.first?.detail == "durable")
        #expect(question?.options.allSatisfy { !$0.isRejection } == true)
    }

    @Test func reportsTheChosenOption() {
        var call = ToolCallItem(toolCallId: "t1", title: "Write", kind: .edit,
                                status: .completed)
        call.permission = PermissionState(
            requestId: .string("r1"),
            options: [PermissionOption(optionId: "allow_once", name: "Allow once",
                                       kind: .allowOnce)],
            resolution: .selected(optionId: "allow_once"))
        #expect(ChatQuestion.from(call)?.chosenOptionId == "allow_once")
    }

    @Test func aCancelledRequestIsResolvedButUnanswered() {
        var call = ToolCallItem(toolCallId: "t1", title: "Write", kind: .edit,
                                status: .failed)
        call.permission = PermissionState(
            requestId: .string("r1"),
            options: [PermissionOption(optionId: "allow_once", name: "Allow once",
                                       kind: .allowOnce)],
            resolution: .cancelled)
        let question = ChatQuestion.from(call)
        #expect(question?.isExpired == true)
        #expect(question?.isAnswered == false)
        #expect(question?.isResolved == true)
    }

    @Test func isNilWithoutAPermission() {
        let call = ToolCallItem(toolCallId: "t1", title: "Read", kind: .read,
                                status: .completed)
        #expect(ChatQuestion.from(call) == nil)
    }
}
```

The cancelled case matters: `TranscriptReducer.turnEnded` resolves every still-pending permission as `.cancelled` (`TranscriptReducer.swift:63-70`), so a session that dies mid-question lands here. Without `isExpired` the card would show its buttons again for a request no one can answer.

- [ ] **Step 2: Run the test to verify it fails**

Run: `cd Packages/TillerACP && swift test --filter ChatQuestionTests`
Expected: compile failure — no such type `ChatQuestion`.

- [ ] **Step 3: Implement**

```swift
import Foundation

/// A question put to the user, normalised from either source: an
/// `AskUserQuestion`-shaped tool input, or a plain permission request.
/// Views read this and never touch the raw payloads.
public struct ChatQuestion: Sendable, Equatable {
    public struct Option: Sendable, Equatable, Identifiable {
        public var id: String
        public var label: String
        public var detail: String?
        public var isRejection: Bool

        public init(id: String, label: String, detail: String? = nil,
                    isRejection: Bool = false) {
            self.id = id
            self.label = label
            self.detail = detail
            self.isRejection = isRejection
        }
    }

    public var requestId: JSONRPCID
    public var toolCallId: String
    public var header: String
    public var prompt: String
    public var options: [Option]
    public var chosenOptionId: String?
    /// The turn ended before anyone answered — the reducer cancels pending
    /// permissions on `turnEnded`. Offering the buttons again would be a lie.
    public var isExpired: Bool

    public var isAnswered: Bool { chosenOptionId != nil }
    public var isResolved: Bool { isAnswered || isExpired }

    public static func from(_ call: ToolCallItem) -> ChatQuestion? {
        guard let permission = call.permission else { return nil }
        let chosen: String? = if case .selected(let optionId)? = permission.resolution {
            optionId
        } else {
            nil
        }
        let expired = permission.resolution == .cancelled
        if let structured = structuredQuestion(from: call.rawInput) {
            return ChatQuestion(requestId: permission.requestId,
                                toolCallId: call.toolCallId,
                                header: structured.header,
                                prompt: structured.prompt,
                                options: structured.options,
                                chosenOptionId: chosen,
                                isExpired: expired)
        }
        return ChatQuestion(
            requestId: permission.requestId,
            toolCallId: call.toolCallId,
            header: call.title,
            prompt: "",
            options: permission.options.map {
                Option(id: $0.optionId, label: $0.name,
                       isRejection: $0.kind == .rejectOnce || $0.kind == .rejectAlways)
            },
            chosenOptionId: chosen,
            isExpired: expired)
    }

    /// `AskUserQuestion` input: `{questions: [{header, question, options: [{label, description}]}]}`.
    /// Only the first question is surfaced; multi-question payloads are rare and
    /// the extra ones would need a second card.
    private static func structuredQuestion(from rawInput: JSONValue?)
        -> (header: String, prompt: String, options: [Option])? {
        guard case .object(let input)? = rawInput,
              case .array(let questions)? = input["questions"],
              case .object(let first)? = questions.first,
              case .string(let prompt)? = first["question"] else { return nil }
        let header: String = if case .string(let value)? = first["header"] {
            value
        } else {
            prompt
        }
        guard case .array(let rawOptions)? = first["options"] else { return nil }
        let options: [Option] = rawOptions.enumerated().compactMap { index, value in
            guard case .object(let option) = value,
                  case .string(let label)? = option["label"] else { return nil }
            let detail: String? = if case .string(let text)? = option["description"] {
                text
            } else {
                nil
            }
            return Option(id: label.isEmpty ? "option-\(index)" : label,
                          label: label, detail: detail)
        }
        guard !options.isEmpty else { return nil }
        return (header, prompt, options)
    }
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `cd Packages/TillerACP && swift test --filter ChatQuestionTests`
Expected: 4 tests pass.

- [ ] **Step 5: Commit**

```bash
git add Packages/TillerACP/Sources/TillerACP/ChatQuestion.swift \
        Packages/TillerACP/Tests/TillerACPTests/ChatQuestionTests.swift
git commit -m "feat: normalise agent questions into ChatQuestion"
```

---

### Task 7: Answering with a chosen option (`updatedInput`)

**Files:**
- Modify: `Packages/TillerACP/Sources/TillerACP/Permission.swift:30-58`
- Modify: `Packages/TillerACP/Sources/TillerACP/AgentDriver.swift`
- Modify: `Packages/TillerACP/Sources/TillerACP/Drivers/ClaudeStreamJSONDriver.swift:185-203`
- Test: `Packages/TillerACP/Tests/TillerACPTests/PermissionAnswerTests.swift`

**Interfaces:**
- Produces: `PermissionOutcome.answered(optionId: String, updatedInput: JSONValue)`; `AgentDriver.supportsStructuredAnswers: Bool` (protocol extension default `false`, `ClaudeStreamJSONDriver` returns `true`); the driver's control response gains `updatedInput`.
- Consumed by `ChatController.answerQuestion` (Task 12).

- [ ] **Step 1: Write the failing test**

```swift
import Foundation
import Testing
@testable import TillerACP

@Suite struct PermissionAnswerTests {
    @Test func answeredEncodesAsSelectedForTheACPWire() throws {
        let outcome = PermissionOutcome.answered(
            optionId: "SQLite", updatedInput: .object(["choice": .string("SQLite")]))
        let data = try JSONEncoder().encode(outcome)
        let decoded = try JSONDecoder().decode(PermissionOutcome.self, from: data)
        #expect(decoded == .selected(optionId: "SQLite"))
    }

    /// Uses the target's existing `MockTransport`, whose `sent` records every
    /// line the driver wrote.
    func sentLines(answering outcome: PermissionOutcome) async -> [String] {
        let transport = MockTransport()
        let driver = ClaudeStreamJSONDriver(transport: transport,
                                            permissionMode: .ask,
                                            model: nil, resumeSessionId: nil)
        await driver.answerPermission(requestId: .string("r1"), outcome: outcome)
        return await transport.sent.map { String(decoding: $0, as: UTF8.self) }
    }

    @Test func controlResponseCarriesUpdatedInput() async {
        let sent = await sentLines(answering: .answered(
            optionId: "SQLite", updatedInput: .object(["choice": .string("SQLite")])))
        #expect(sent.contains { $0.contains("\"behavior\":\"allow\"") })
        #expect(sent.contains { $0.contains("updatedInput") })
        #expect(sent.contains { $0.contains("SQLite") })
    }

    @Test func plainSelectionStillSendsNoUpdatedInput() async {
        let sent = await sentLines(answering: .selected(optionId: "allow_once"))
        #expect(sent.contains { $0.contains("\"behavior\":\"allow\"") })
        #expect(sent.allSatisfy { !$0.contains("updatedInput") })
    }
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cd Packages/TillerACP && swift test --filter PermissionAnswerTests`
Expected: compile failure — no `.answered` case.

- [ ] **Step 3: Add the case, the capability, and the encoding**

In `Permission.swift`:

```swift
public enum PermissionOutcome: Sendable, Equatable {
    case selected(optionId: String)
    /// A chosen answer to a question, carried back to the agent as the tool's
    /// updated input. Encodes as `selected` on the ACP wire, which has no
    /// equivalent field.
    case answered(optionId: String, updatedInput: JSONValue)
    case cancelled
}
```

In `encode(to:)` add:

```swift
        case .answered(let optionId, _):
            try container.encode("selected", forKey: .outcome)
            try container.encode(optionId, forKey: .optionId)
```

In `AgentDriver.swift`:

```swift
public protocol AgentDriver: Actor {
    // …existing requirements…
    /// Whether the driver can carry a chosen option back to the agent
    /// (`updatedInput`). Drivers that cannot make question cards fall back to
    /// plain allow/reject.
    nonisolated var supportsStructuredAnswers: Bool { get }
}

public extension AgentDriver {
    nonisolated var supportsStructuredAnswers: Bool { false }
}
```

In `ClaudeStreamJSONDriver`, add `public nonisolated var supportsStructuredAnswers: Bool { true }` and rewrite `answerPermission`:

```swift
    public func answerPermission(requestId: JSONRPCID,
                                 outcome: PermissionOutcome) async {
        let id = requestIdString(requestId)
        var payload: [String: JSONValue]
        switch outcome {
        case .selected(let optionId):
            payload = ["behavior": .string(optionId.hasPrefix("allow") ? "allow" : "deny")]
        case .answered(_, let updatedInput):
            payload = ["behavior": .string("allow"), "updatedInput": updatedInput]
        case .cancelled:
            payload = ["behavior": .string("deny")]
        }
        let response = JSONValue.object([
            "type": .string("control_response"),
            "response": .object([
                "request_id": .string(id),
                "subtype": .string("success"),
                "response": .object(payload)
            ])
        ])
        try? await transport.send(line: makeLine(response))
    }
```

If Task 1 found that questions never reach the native driver, keep this code anyway — it is the mechanism the ACP path will need — and set `supportsStructuredAnswers` accordingly for the driver that does carry them.

- [ ] **Step 4: Run the test to verify it passes**

Run: `cd Packages/TillerACP && swift test --filter PermissionAnswerTests`
Expected: 3 tests pass. Then run the whole package: `swift test` — the new enum case must not break `ACPSession` or the other drivers (add the `case .answered` branch wherever the compiler flags a non-exhaustive switch, mapping it to the same behaviour as `.selected`).

- [ ] **Step 5: Commit**

```bash
git add Packages/TillerACP/Sources/TillerACP/Permission.swift \
        Packages/TillerACP/Sources/TillerACP/AgentDriver.swift \
        Packages/TillerACP/Sources/TillerACP/Drivers/ \
        Packages/TillerACP/Tests/TillerACPTests/PermissionAnswerTests.swift
git commit -m "feat: answer permission requests with a chosen option"
```

---

### Task 8: `DiffStats` — line counts for the diff header

Backs spec item #3, and is the testable half of it.

**Files:**
- Create: `Packages/TillerACP/Sources/TillerACP/DiffStats.swift`
- Test: `Packages/TillerACP/Tests/TillerACPTests/DiffStatsTests.swift`

**Interfaces:**
- Produces: `DiffStats.counts(oldText: String?, newText: String) -> (added: Int, removed: Int)`, consumed by `ToolCallCardView` (Task 15).

- [ ] **Step 1: Write the failing test**

```swift
import Testing
@testable import TillerACP

@Suite struct DiffStatsTests {
    @Test func countsBothSides() {
        let stats = DiffStats.counts(oldText: "a\nb", newText: "a\nb\nc")
        #expect(stats.removed == 2)
        #expect(stats.added == 3)
    }

    @Test func treatsMissingOldTextAsAPureAddition() {
        let stats = DiffStats.counts(oldText: nil, newText: "a\nb")
        #expect(stats.removed == 0)
        #expect(stats.added == 2)
    }

    @Test func emptyTextCountsAsZeroLines() {
        let stats = DiffStats.counts(oldText: "", newText: "")
        #expect(stats.removed == 0)
        #expect(stats.added == 0)
    }

    @Test func trailingNewlineDoesNotAddAPhantomLine() {
        #expect(DiffStats.counts(oldText: nil, newText: "a\n").added == 1)
    }
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cd Packages/TillerACP && swift test --filter DiffStatsTests`
Expected: compile failure — no such type `DiffStats`.

- [ ] **Step 3: Implement**

```swift
import Foundation

/// Line counts for a tool call's diff payload. The card shows these so a
/// truncated preview still tells the truth about the change's size.
public enum DiffStats {
    public static func counts(oldText: String?, newText: String)
        -> (added: Int, removed: Int) {
        (added: lineCount(newText), removed: lineCount(oldText))
    }

    /// Blank lines in the middle count; a trailing newline does not invent a
    /// final empty line.
    private static func lineCount(_ text: String?) -> Int {
        guard let text, !text.isEmpty else { return 0 }
        var lines = text.split(separator: "\n", omittingEmptySubsequences: false)
        if lines.last?.isEmpty == true { lines.removeLast() }
        return lines.count
    }
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `cd Packages/TillerACP && swift test --filter DiffStatsTests`
Expected: 4 tests pass.

- [ ] **Step 5: Commit**

```bash
git add Packages/TillerACP/Sources/TillerACP/DiffStats.swift \
        Packages/TillerACP/Tests/TillerACPTests/DiffStatsTests.swift
git commit -m "feat: count diff lines for tool call cards"
```

---

### Task 9: `ChatCard` and its tokens

Spec items #6 and #7. Views from here on are gated by the app build in `Scripts/ci.sh` plus a named visual check — `App/` has no test target in CI.

**Files:**
- Create: `App/Chat/ChatCard.swift`
- Modify: `App/AppTheme.swift` (after the `diffHunkBackground` token, before `bottomBarHeight`)
- Modify: `App/Chat/ToolCallCardView.swift:17-29`
- Modify: `App/Chat/EditSummaryCardView.swift:23-41`

**Interfaces:**
- Produces: `ChatCardKind` (`.task`, `.question`, `.edit`, `.tool`, `.plan`) and `ChatCard<Content>` with `init(kind:isHighlighted:@ViewBuilder content:)`. Consumed by Tasks 10, 11, 12, 15.

- [ ] **Step 1: Add the tokens**

In `App/AppTheme.swift`:

```swift
    /// Chat card surface — one step off the chat background, no border. Every
    /// card in the transcript uses this instead of `.quaternary`, which
    /// bypasses the tuned light/dark pairs above.
    static let cardFill = dynamic(
        light: NSColor(srgbRed: 0.91, green: 0.915, blue: 0.94, alpha: 1),
        dark: NSColor(srgbRed: 0.157, green: 0.165, blue: 0.208, alpha: 1))
    /// Left accent rails: the card's kind, readable while scrolling.
    static let railTask = dynamic(
        light: NSColor(srgbRed: 0.36, green: 0.30, blue: 0.68, alpha: 1),
        dark: NSColor(srgbRed: 0.49, green: 0.42, blue: 0.84, alpha: 1))
    static let railQuestion = gitModified
    static let railEdit = gitStaged
    static let railTool = dynamic(
        light: NSColor(srgbRed: 0.55, green: 0.57, blue: 0.65, alpha: 1),
        dark: NSColor(srgbRed: 0.40, green: 0.42, blue: 0.50, alpha: 1))
```

- [ ] **Step 2: Write the card container**

Create `App/Chat/ChatCard.swift`:

```swift
import SwiftUI

/// What a chat card represents. Drives the accent rail only — every card
/// shares one fill, one radius, one padding.
enum ChatCardKind {
    case task, question, edit, tool, plan

    var railColor: Color {
        switch self {
        case .task: AppTheme.railTask
        case .question: AppTheme.railQuestion
        case .edit: AppTheme.railEdit
        case .tool, .plan: AppTheme.railTool
        }
    }
}

/// The one place a chat card's appearance is defined. Before this existed the
/// same background modifier was copied into five views and had already drifted
/// to two different corner radii.
struct ChatCard<Content: View>: View {
    let kind: ChatCardKind
    var isHighlighted = false
    @ViewBuilder let content: Content

    private static var radius: CGFloat { 10 }

    var body: some View {
        content
            .padding(.vertical, 10)
            .padding(.leading, 12)
            .padding(.trailing, 12)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(AppTheme.cardFill)
            .overlay(alignment: .leading) {
                Rectangle().fill(kind.railColor).frame(width: 3)
            }
            .clipShape(RoundedRectangle(cornerRadius: Self.radius))
            .overlay {
                RoundedRectangle(cornerRadius: Self.radius)
                    .strokeBorder(isHighlighted ? kind.railColor : .clear)
            }
    }
}
```

- [ ] **Step 3: Adopt it in the two existing cards**

In `ToolCallCardView.body`, replace the `VStack` modifiers `.padding(8) / .frame / .background / .overlay` with:

```swift
        ChatCard(kind: .tool, isHighlighted: isPermissionPending) {
            VStack(alignment: .leading, spacing: 6) {
                header
                if expanded || isPermissionPending {
                    contentBody
                }
            }
        }
```

In `EditSummaryCardView.body`, wrap the existing `VStack` in `ChatCard(kind: .edit) { … }` and delete its `.padding(8)`, `.frame(maxWidth:alignment:)` and `.background(...)` modifiers. Keep `.confirmationDialog` attached to the `ChatCard`.

- [ ] **Step 4: Build and look at it**

Run: `Scripts/ci.sh`
Expected: `CI OK` (rerun if `TillerTerminal`'s flaky PTY test is the only failure).

Then launch the app and open a chat with at least one tool call and one edit summary. Check: cards have a visible rail, one consistent radius, and readable text in **both** light and dark (System Settings → Appearance, or Settings → Appearance in Tiller).

- [ ] **Step 5: Commit**

```bash
git add App/Chat/ChatCard.swift App/AppTheme.swift \
        App/Chat/ToolCallCardView.swift App/Chat/EditSummaryCardView.swift
git commit -m "feat: add shared chat card container"
```

---

### Task 10: Extract the plan card and stop rescanning the transcript

Spec items #5 and #8.

**Files:**
- Create: `App/Chat/PlanCardView.swift`
- Modify: `App/Chat/TranscriptView.swift:45-53, 93-94, 211-250`
- Modify: `App/Chat/ChatController.swift:63-95`

**Interfaces:**
- Consumes: `ToolCallTree.group(items:)` and `Grouped.pendingPlanApproval` (Task 3), `ChatCard` (Task 9).
- Produces: `ChatController.grouped: ToolCallTree.Grouped` (cached per items change), consumed by Tasks 11 and 12; `PlanCardView(entries:approval:controller:)`.

- [ ] **Step 1: Cache the grouping on the controller**

In `ChatController.swift`, next to `items`:

```swift
    /// Roots + children + pending plan approval, computed once per items
    /// change. Views must read this instead of scanning `items` themselves.
    var grouped: ToolCallTree.Grouped { ToolCallTree.group(items: items) }
```

Replace the body of `hasPlanAwaitingApproval` — it currently rebuilds the whole timeline on every access, even though timeline rendering is disabled:

```swift
    var hasPlanAwaitingApproval: Bool {
        grouped.pendingPlanApproval?.isPending == true
    }
```

- [ ] **Step 2: Move the plan card into its own file**

Create `App/Chat/PlanCardView.swift` with the markup currently inlined in `TranscriptView.planCard`, plus the progress count and the completed-state collapse:

```swift
import SwiftUI
import TillerACP

/// The agent's proposed plan. Shows progress at a glance and collapses once
/// every entry is done, so a finished plan stops competing with live output.
struct PlanCardView: View {
    let entries: [PlanEntry]
    let approval: PermissionState?
    let controller: ChatController

    @State private var isExpanded: Bool?

    private var completedCount: Int {
        entries.filter { $0.status == "completed" }.count
    }
    private var isComplete: Bool {
        !entries.isEmpty && completedCount == entries.count
    }
    private var showsEntries: Bool { isExpanded ?? !isComplete }

    var body: some View {
        ChatCard(kind: .plan) {
            VStack(alignment: .leading, spacing: 4) {
                Button {
                    withAnimation(.easeOut(duration: 0.12)) {
                        isExpanded = !showsEntries
                    }
                } label: {
                    HStack(spacing: 6) {
                        Image(systemName: "checklist")
                        Text("Plan")
                        Text("\(completedCount)/\(entries.count)")
                            .foregroundStyle(.tertiary)
                        Spacer()
                        Image(systemName: "chevron.right")
                            .font(.caption2.weight(.semibold))
                            .rotationEffect(.degrees(showsEntries ? 90 : 0))
                    }
                    .font(.caption.weight(.semibold))
                    .foregroundStyle(.secondary)
                }
                .buttonStyle(.plain)

                if showsEntries {
                    ForEach(Array(entries.enumerated()), id: \.offset) { _, entry in
                        HStack(alignment: .firstTextBaseline, spacing: 6) {
                            Image(systemName: symbol(for: entry.status))
                                .foregroundStyle(entry.status == "completed"
                                                 ? AppTheme.railEdit : .secondary)
                                .font(.caption)
                            Text(entry.content).font(.callout)
                        }
                    }
                }

                if let approval, approval.isPending {
                    HStack(spacing: 8) {
                        ForEach(approval.options, id: \.optionId) { option in
                            Button(option.name) {
                                Task {
                                    await controller.answerPermission(
                                        requestId: approval.requestId,
                                        optionId: option.optionId)
                                }
                            }
                            .buttonStyle(.bordered)
                            .tint(option.kind == .allowOnce || option.kind == .allowAlways
                                  ? AppTheme.railEdit : AppTheme.gitConflict)
                            .controlSize(.small)
                        }
                    }
                    .padding(.top, 4)
                }
            }
        }
    }

    private func symbol(for status: String) -> String {
        switch status {
        case "completed": "checkmark.circle.fill"
        case "in_progress": "circle.dotted"
        default: "circle"
        }
    }
}
```

- [ ] **Step 3: Rewire `TranscriptView`**

- Delete the `pendingPlanApproval` computed property (lines 45-53) and the `planCard(_:approval:)` method (lines 211-250).
- In `itemView`, replace the `.plan` branch with:

```swift
        case .plan(_, let entries):
            PlanCardView(entries: entries,
                         approval: controller.grouped.pendingPlanApproval,
                         controller: controller)
```

- In `rowView`, replace `planCard(entries, approval: approval)` with `PlanCardView(entries: entries, approval: approval, controller: controller)` so the still-compiled timeline path keeps building.

- [ ] **Step 4: Build and check**

Run: `Scripts/ci.sh`
Expected: `CI OK`.

Manual: ask an agent in plan mode for a plan. Check the header reads `Plan 0/N`, entries tick over as they complete, the card collapses itself once complete, and the approve/reject buttons still answer.

- [ ] **Step 5: Commit**

```bash
git add App/Chat/PlanCardView.swift App/Chat/TranscriptView.swift App/Chat/ChatController.swift
git commit -m "refactor: extract plan card and drop per-redraw transcript scans"
```

---

### Task 11: Task card with nested children

Spec item #9.

**Files:**
- Create: `App/Chat/TaskCardView.swift`
- Modify: `App/Chat/TranscriptView.swift` (`body`, `itemView`)

**Interfaces:**
- Consumes: `SubagentTasks.info(for:)` (Task 5), `ToolCallTree.Grouped` (Task 3), `ChatCard` (Task 9).
- Produces: `TaskCardView(info:item:children:controller:worktree:appModel:)`.

- [ ] **Step 1: Render roots from the grouping**

In `TranscriptView.body`, iterate the grouped roots instead of raw items, so nested children are not also drawn at top level:

```swift
                LazyVStack(alignment: .leading, spacing: 14) {
                    ForEach(controller.grouped.roots) { item in
                        itemView(item, meta: nil)
                            .id(item.id)
                    }
```

In `itemView`, route subagent spawns to the new card:

```swift
        case .toolCall(let toolCall):
            if let info = SubagentTasks.info(for: toolCall) {
                TaskCardView(info: info, item: toolCall,
                             children: controller.grouped.children(of: toolCall.toolCallId),
                             controller: controller, worktree: worktree,
                             appModel: appModel)
            } else {
                ToolCallCardView(item: toolCall, controller: controller,
                                 worktree: worktree, appModel: appModel)
            }
```

- [ ] **Step 2: Write the card**

Create `App/Chat/TaskCardView.swift`:

```swift
import SwiftUI
import TillerACP
import TillerCore

/// A subagent spawn and the work it did. Children are collapsed by default:
/// a running task shows only its current action, so a forty-call subagent
/// stays one card tall while it streams.
struct TaskCardView: View {
    let info: SubagentTaskInfo
    let item: ToolCallItem
    let children: [ToolCallItem]
    let controller: ChatController
    let worktree: Worktree
    let appModel: AppModel

    @State private var isExpanded = false

    private var isRunning: Bool {
        item.status == .pending || item.status == .inProgress
    }
    private var currentChild: ToolCallItem? {
        children.last { $0.status == .pending || $0.status == .inProgress }
            ?? children.last
    }

    var body: some View {
        ChatCard(kind: .task) {
            VStack(alignment: .leading, spacing: 6) {
                header
                if isExpanded {
                    ForEach(children) { child in
                        ToolCallCardView(item: child, controller: controller,
                                         worktree: worktree, appModel: appModel)
                    }
                } else if isRunning, let currentChild {
                    HStack(spacing: 6) {
                        RunningDots(color: AppTheme.railQuestion)
                        Text(currentChild.title)
                            .font(.caption)
                            .foregroundStyle(.secondary)
                            .lineLimit(1)
                    }
                }
            }
        }
    }

    private var header: some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack(spacing: 6) {
                Text(info.title)
                    .font(.callout.weight(.medium))
                    .lineLimit(isExpanded ? nil : 1)
                Text(info.subagentType)
                    .font(.caption2)
                    .padding(.horizontal, 6)
                    .padding(.vertical, 1)
                    .background(AppTheme.primaryPillBg,
                                in: RoundedRectangle(cornerRadius: 4))
                    .foregroundStyle(.secondary)
                Spacer()
                statusGlyph
            }
            Button {
                withAnimation(.easeOut(duration: 0.12)) { isExpanded.toggle() }
            } label: {
                HStack(spacing: 5) {
                    Image(systemName: "chevron.right")
                        .font(.caption2.weight(.semibold))
                        .rotationEffect(.degrees(isExpanded ? 90 : 0))
                    Text(children.count == 1 ? "1 tool call"
                                             : "\(children.count) tool calls")
                        .font(.caption2)
                }
                .foregroundStyle(.tertiary)
            }
            .buttonStyle(.plain)
            .opacity(children.isEmpty ? 0 : 1)
            .disabled(children.isEmpty)
        }
    }

    @ViewBuilder
    private var statusGlyph: some View {
        switch item.status {
        case .pending, .inProgress:
            ProgressView().controlSize(.small)
        case .completed:
            Image(systemName: "checkmark.circle.fill")
                .foregroundStyle(AppTheme.railEdit).font(.caption)
        case .failed:
            Image(systemName: "xmark.circle.fill")
                .foregroundStyle(AppTheme.gitConflict).font(.caption)
        }
    }
}
```

- [ ] **Step 3: Build and check**

Run: `Scripts/ci.sh`
Expected: `CI OK`.

Manual, with Claude as the agent: ask it to "use a subagent to explore this repository". Check that a single Task card appears with the subagent type as a pill, that the running card shows one changing action line, that the child calls appear only after expanding, and that **no** child call is also drawn at top level.

- [ ] **Step 4: Check the degraded path**

Open a Codex or OpenCode chat and run any tool call. Those drivers never set a parent, so every call must render exactly as before, at top level.

- [ ] **Step 5: Commit**

```bash
git add App/Chat/TaskCardView.swift App/Chat/TranscriptView.swift
git commit -m "feat: render subagent tasks as a card with nested children"
```

---

### Task 12: Question card and pending bar

Spec item #10.

**Files:**
- Create: `App/Chat/QuestionCardView.swift`
- Create: `App/Chat/PendingQuestionBar.swift`
- Delete: `App/Chat/ComposerApprovalPanel.swift`
- Modify: `App/Chat/ChatPaneView.swift:83-89`
- Modify: `App/Chat/TranscriptView.swift` (`itemView`, scroll target)
- Modify: `App/Chat/ChatController.swift`

**Interfaces:**
- Consumes: `ChatQuestion.from(_:)` (Task 6), `PermissionOutcome.answered` (Task 7), `ComposerPermissions.extract` (existing), `ChatCard` (Task 9).
- Produces: `ChatController.scrollTarget: String?` and `ChatController.answerQuestion(_ question: ChatQuestion, optionId: String) async`.

- [ ] **Step 1: Add the controller plumbing**

In `ChatController.swift`:

```swift
    /// Transcript id the view should scroll to; cleared by the transcript once
    /// it has scrolled. Set by the pending-question bar.
    var scrollTarget: String?

    /// Answers a question card. Uses the structured channel when the driver
    /// supports it, so the agent learns *which* option was chosen; otherwise
    /// falls back to the plain allow/reject the permission gate already uses.
    func answerQuestion(_ question: ChatQuestion, optionId: String) async {
        guard let driver else { return }
        let option = question.options.first { $0.id == optionId }
        if driver.supportsStructuredAnswers, option?.isRejection != true,
           !question.prompt.isEmpty {
            reducer.permissionResolved(requestId: question.requestId,
                                       resolution: .selected(optionId: optionId))
            await driver.answerPermission(
                requestId: question.requestId,
                outcome: .answered(optionId: optionId,
                                   updatedInput: .object(["choice": .string(optionId)])))
            onStatusChange?(.running)
            persist()
        } else {
            await answerPermission(requestId: question.requestId, optionId: optionId)
        }
    }
```

- [ ] **Step 2: Write the question card**

Create `App/Chat/QuestionCardView.swift`:

```swift
import SwiftUI
import TillerACP

/// A question the agent put to the user, answered in place. Unlike the old
/// composer panel this stays in the transcript, so a reopened chat still shows
/// what was asked and what was answered.
struct QuestionCardView: View {
    let question: ChatQuestion
    let controller: ChatController

    var body: some View {
        ChatCard(kind: .question, isHighlighted: !question.isAnswered) {
            VStack(alignment: .leading, spacing: 8) {
                Text(question.header)
                    .font(.callout.weight(.medium))
                if !question.prompt.isEmpty {
                    Text(question.prompt)
                        .font(.system(size: 13))
                        .textSelection(.enabled)
                }
                if let chosen = question.chosenOptionId {
                    answered(chosen)
                } else if question.isExpired {
                    Text("No answer — the turn ended")
                        .font(.caption)
                        .foregroundStyle(.tertiary)
                } else {
                    options
                }
            }
        }
    }

    private var options: some View {
        VStack(alignment: .leading, spacing: 6) {
            ForEach(question.options) { option in
                Button {
                    Task { await controller.answerQuestion(question, optionId: option.id) }
                } label: {
                    VStack(alignment: .leading, spacing: 1) {
                        Text(option.label)
                        if let detail = option.detail {
                            Text(detail).font(.caption2).foregroundStyle(.secondary)
                        }
                    }
                }
                .buttonStyle(.bordered)
                .tint(option.isRejection ? AppTheme.gitConflict : AppTheme.railQuestion)
                .controlSize(.small)
            }
        }
    }

    private func answered(_ optionId: String) -> some View {
        let label = question.options.first { $0.id == optionId }?.label ?? optionId
        return HStack(spacing: 5) {
            Image(systemName: "checkmark.circle.fill")
                .font(.caption2)
                .foregroundStyle(AppTheme.railEdit)
            Text(label).font(.caption)
        }
        .foregroundStyle(.secondary)
    }
}
```

- [ ] **Step 3: Write the pending bar and swap it in**

Create `App/Chat/PendingQuestionBar.swift`:

```swift
import SwiftUI
import TillerACP

/// One line above the composer while a question is unanswered. It never
/// measures the transcript: it exists exactly while a pending request does.
struct PendingQuestionBar: View {
    let permissions: [ComposerPermission]
    let controller: ChatController

    var body: some View {
        if let current = permissions.first {
            Button {
                controller.scrollTarget = current.toolCallId
            } label: {
                HStack(spacing: 8) {
                    Image(systemName: "questionmark.circle.fill")
                        .font(.caption)
                        .foregroundStyle(AppTheme.railQuestion)
                    Text(permissions.count > 1
                         ? "\(permissions.count) questions waiting · \(current.title)"
                         : "Question waiting · \(current.title)")
                        .font(.caption)
                        .lineLimit(1)
                    Spacer()
                    Text("Show").font(.caption2).foregroundStyle(.secondary)
                }
                .padding(.vertical, 6)
                .padding(.horizontal, 10)
                .background(AppTheme.cardFill, in: RoundedRectangle(cornerRadius: 8))
                .overlay(RoundedRectangle(cornerRadius: 8)
                    .strokeBorder(AppTheme.railQuestion.opacity(0.6), lineWidth: 1))
            }
            .buttonStyle(.plain)
        }
    }
}
```

In `ChatPaneView.swift`, replace the `ComposerApprovalPanel(...)` call inside `captureLayout(.approvalPanel)` with:

```swift
                PendingQuestionBar(permissions: controller.composerPermissions,
                                   controller: controller)
                    .padding(.horizontal, 12)
                    .padding(.vertical, 6)
                    .layoutPriority(1)
```

Then delete `App/Chat/ComposerApprovalPanel.swift`. Confirm nothing else references it:

```bash
TOKENSAVE_DISABLE_GREP_HOOK=1 grep -rn "ComposerApprovalPanel" --include="*.swift" App Packages
```

Expected: no matches (the `ChatPaneLayoutRole.approvalPanel` case keeps its name — it identifies the slot, not the deleted view).

- [ ] **Step 4: Route questions in the transcript and honour the scroll target**

In `TranscriptView.itemView`, replace the whole `.toolCall` branch written in Task 11 with the three-way routing below.

The ordering rule, stated once: **an unanswered question always wins**, because the card is the only place with buttons — a Task spawn sitting behind a permission gate must stay answerable. Once resolved, a subagent spawn reverts to its Task card; any other resolved request keeps the question card so the answer stays visible in history.

```swift
        case .toolCall(let toolCall):
            let question = ChatQuestion.from(toolCall)
            let subagent = SubagentTasks.info(for: toolCall)
            if let question, !question.options.isEmpty,
               !(question.isResolved && subagent != nil) {
                QuestionCardView(question: question, controller: controller)
            } else if let subagent {
                TaskCardView(info: subagent, item: toolCall,
                             children: controller.grouped.children(of: toolCall.toolCallId),
                             controller: controller, worktree: worktree,
                             appModel: appModel)
            } else {
                ToolCallCardView(item: toolCall, controller: controller,
                                 worktree: worktree, appModel: appModel)
            }
```

In `TranscriptView.body`, inside `ScrollViewReader`, add next to the existing `onChange`:

```swift
            .onChange(of: controller.scrollTarget) {
                guard let target = controller.scrollTarget else { return }
                withAnimation(.easeOut(duration: 0.15)) {
                    proxy.scrollTo(target, anchor: .center)
                }
                controller.scrollTarget = nil
            }
```

- [ ] **Step 5: Build and check**

Run: `Scripts/ci.sh`
Expected: `CI OK`.

Manual, all four in one session:
1. Trigger a permission gate (ask the agent to write a file with permissions on). The card appears inline with a rail and a highlighted border; the bar appears above the composer.
2. Answer from the card → both the buttons and the bar are replaced by the chosen option.
3. Trigger another one, scroll up, click the bar → the transcript scrolls to the card.
4. Close and reopen the chat → the answered card is still there, showing the answer.

- [ ] **Step 6: Commit**

```bash
git add App/Chat/QuestionCardView.swift App/Chat/PendingQuestionBar.swift \
        App/Chat/ChatPaneView.swift App/Chat/TranscriptView.swift App/Chat/ChatController.swift
git rm App/Chat/ComposerApprovalPanel.swift
git commit -m "feat: answer agent questions from the transcript"
```

---

### Task 13: Autoscroll that follows streaming, and a working row that says what is happening

Spec items #2 and #11.

**Files:**
- Modify: `App/Chat/ChatController.swift` (event flush)
- Modify: `App/Chat/TranscriptView.swift:21-43, 128-138`

**Interfaces:**
- Produces: `ChatController.streamTick: Int` — incremented once per flushed event batch (at most every 40 ms, per the existing coalescing), and `ChatController.currentActivity: String?`.

- [ ] **Step 1: Publish a coalesced tick and the current activity**

In `ChatController.swift`, add the properties and bump the tick wherever the buffered events are applied (the flush path described at lines 446-454):

```swift
    /// Bumped once per flushed event batch. The transcript scrolls on this
    /// rather than on `items.count`, which never changes while a message grows.
    private(set) var streamTick = 0

    /// Title of the tool call currently in flight, for the working row.
    var currentActivity: String? {
        for item in items.reversed() {
            guard case .toolCall(let call) = item else { continue }
            if call.status == .pending || call.status == .inProgress { return call.title }
        }
        return nil
    }
```

At the end of the flush function, after the buffered events have been applied: `streamTick &+= 1`.

- [ ] **Step 2: Rewrite the transcript's scrolling**

In `TranscriptView.body`:

```swift
    @State private var scrollPosition = ScrollPosition(idType: String.self)

    var body: some View {
        ScrollViewReader { proxy in
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 14) {
                    ForEach(controller.grouped.roots) { item in
                        itemView(item, meta: nil)
                            .id(item.id)
                    }
                    if controller.state == .prompting {
                        thinkingRow
                    }
                    Color.clear.frame(height: 1).id("bottom")
                }
                .padding(.horizontal, 16)
                .padding(.vertical, 14)
            }
            .scrollPosition($scrollPosition)
            // Streaming growth: follow only while the user has not taken over
            // the scroll. No geometry reads — `isPositionedByUser` is the
            // scroll view's own state.
            .onChange(of: controller.streamTick) {
                guard !scrollPosition.isPositionedByUser else { return }
                scrollPosition.scrollTo(edge: .bottom)
            }
            // A new item re-pins only when the user just sent something —
            // an agent's new tool call must not yank the view while reading.
            .onChange(of: controller.items.count) {
                guard let last = controller.items.last,
                      case .userMessage = last else { return }
                withAnimation(.easeOut(duration: 0.15)) {
                    scrollPosition.scrollTo(edge: .bottom)
                }
            }
            .onChange(of: controller.scrollTarget) {
                guard let target = controller.scrollTarget else { return }
                withAnimation(.easeOut(duration: 0.15)) {
                    proxy.scrollTo(target, anchor: .center)
                }
                controller.scrollTarget = nil
            }
        }
    }
```

- [ ] **Step 3: Make the working row name the action**

Replace `thinkingRow`:

```swift
    /// Live status while a turn is in flight. Naming the current tool call
    /// turns a mute spinner into an answer to "what is it doing?".
    private var thinkingRow: some View {
        HStack(spacing: 6) {
            RunningDots(color: AppTheme.railQuestion)
            Text(controller.currentActivity ?? "Thinking")
                .font(.caption)
                .foregroundStyle(.secondary)
                .lineLimit(1)
        }
    }
```

- [ ] **Step 4: Build and check**

Run: `Scripts/ci.sh`
Expected: `CI OK`.

Manual: ask for a long answer. While it streams, the view stays at the bottom; scroll up mid-stream and it must **stop** following; send a new message and it re-pins to the bottom. The working row names the running tool and falls back to "Thinking" between calls.

- [ ] **Step 5: Commit**

```bash
git add App/Chat/ChatController.swift App/Chat/TranscriptView.swift
git commit -m "fix: follow streaming output and name the current activity"
```

---

### Task 14: Spacing hierarchy and the stray Italian string

Spec items #10 and #4.

**Files:**
- Modify: `App/Chat/TranscriptView.swift:24, 142-168`

- [ ] **Step 1: Give the column a hierarchy**

Uniform 14pt spacing makes a user message weigh the same as a tool call. Replace the `LazyVStack`'s fixed spacing with per-item padding:

```swift
                LazyVStack(alignment: .leading, spacing: 0) {
                    ForEach(controller.grouped.roots) { item in
                        itemView(item, meta: nil)
                            .padding(.top, Self.topSpacing(for: item))
                            .id(item.id)
                    }
```

and add:

```swift
    /// Vertical rhythm: a new user turn gets room, cards in a run stay tight,
    /// dividers keep their own breathing space.
    private static func topSpacing(for item: TranscriptItem) -> CGFloat {
        switch item {
        case .userMessage: 20
        case .turnDivider: 14
        case .agentMessage, .thought: 12
        case .toolCall, .plan, .editSummary, .systemNotice: 6
        }
    }
```

- [ ] **Step 2: Fix the string**

In `userBlockView`, `Label("Immagine", systemImage: "photo")` → `Label("Image", systemImage: "photo")`.

- [ ] **Step 3: Build and check**

Run: `Scripts/ci.sh`
Expected: `CI OK`.

Manual: a transcript with a user message followed by several tool calls should read as grouped runs, not an evenly spaced list.

Confirm no Italian remains in chat-facing strings:

```bash
TOKENSAVE_DISABLE_GREP_HOOK=1 grep -rnE "\"(Immagine|Ripristina|Chiedi|Domanda|Attendi)" --include="*.swift" App/Chat
```

Expected: no matches.

- [ ] **Step 4: Commit**

```bash
git add App/Chat/TranscriptView.swift
git commit -m "fix: give the transcript a spacing hierarchy and an English image label"
```

---

### Task 15: Diffs that admit what they are hiding

Spec item #3.

**Files:**
- Modify: `App/Chat/ToolCallCardView.swift:112-137`

**Interfaces:**
- Consumes: `DiffStats.counts(oldText:newText:)` (Task 8).

- [ ] **Step 1: Rewrite `diffView`**

```swift
    private static let previewLineLimit = 40

    private func diffView(path: String, oldText: String?, newText: String) -> some View {
        let stats = DiffStats.counts(oldText: oldText, newText: newText)
        let oldLines = lines(of: oldText).prefix(Self.previewLineLimit)
        let newLines = lines(of: newText).prefix(Self.previewLineLimit)
        let hidden = (stats.removed + stats.added) - (oldLines.count + newLines.count)
        return VStack(alignment: .leading, spacing: 1) {
            HStack(spacing: 6) {
                Text((path as NSString).lastPathComponent)
                    .font(.caption2.weight(.semibold))
                    .foregroundStyle(.secondary)
                Text("+\(stats.added)")
                    .font(.caption2).foregroundStyle(AppTheme.diffAddition)
                Text("−\(stats.removed)")
                    .font(.caption2).foregroundStyle(AppTheme.diffDeletion)
            }
            ForEach(Array(oldLines.enumerated()), id: \.offset) { _, line in
                diffLine("- " + line, color: AppTheme.diffDeletion,
                         background: AppTheme.diffDeletionBackground)
            }
            ForEach(Array(newLines.enumerated()), id: \.offset) { _, line in
                diffLine("+ " + line, color: AppTheme.diffAddition,
                         background: AppTheme.diffAdditionBackground)
            }
            if hidden > 0 {
                Button {
                    appModel.handleTerminalOpenURL(path, in: worktree)
                } label: {
                    Text("\(hidden) more lines — open file")
                        .font(.caption2)
                }
                .buttonStyle(.plain)
                .foregroundStyle(.tint)
                .padding(.top, 2)
            }
        }
        .textSelection(.enabled)
    }

    private func lines(of text: String?) -> [String] {
        guard let text, !text.isEmpty else { return [] }
        var split = text.split(separator: "\n", omittingEmptySubsequences: false)
            .map(String.init)
        if split.last?.isEmpty == true { split.removeLast() }
        return split
    }

    private func diffLine(_ text: String, color: Color,
                          background: Color) -> some View {
        Text(text)
            .font(.system(.caption, design: .monospaced))
            .foregroundStyle(color)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(background)
    }
```

This also moves the diff colours onto `AppTheme`'s tuned pairs instead of `.red`/`.green` opacity literals.

- [ ] **Step 2: Build and check**

Run: `Scripts/ci.sh`
Expected: `CI OK`.

Manual: have the agent edit a file with more than 40 changed lines. The card shows `+N −M`, a truncated preview, and a line saying how many lines are hidden, which opens the file in the right panel.

- [ ] **Step 3: Commit**

```bash
git add App/Chat/ToolCallCardView.swift
git commit -m "fix: show diff counts and stop truncating silently"
```

---

### Task 16: Final gate

**Files:** none — verification only.

- [ ] **Step 1: Full gate**

Run: `Scripts/ci.sh`
Expected: `CI OK`. If only `TillerTerminal`'s known-flaky PTY test failed, rerun until it is green; anything else is a real failure.

- [ ] **Step 2: Walk the spec's smoke checklist**

From the spec, in one running build:

- [ ] Card appearance in light and dark; rails distinguishable, text readable.
- [ ] Task card expands during execution; the live line tracks the current child.
- [ ] A subagent with many children stays compact while collapsed and does not stutter while streaming.
- [ ] A question answered from the card, and another answered after jumping from the pending bar.
- [ ] Reopening a saved chat shows the question and the chosen answer.
- [ ] A question left unanswered when the turn ends shows "No answer — the turn ended" instead of dead buttons.
- [ ] Autoscroll follows at the bottom; scrolling up suspends it; a new user message re-pins.
- [ ] A Codex or OpenCode session renders exactly as before.

- [ ] **Step 3: Commit anything the walkthrough fixed**

```bash
git add -A
git commit -m "fix: address chat card smoke findings"
```

(Skip if the walkthrough found nothing.)

---

## Notes for the implementer

- Tasks 2-8 are pure `TillerACP` and independently verifiable; 9-15 are SwiftUI and rely on the app build plus the named manual check.
- Tasks 11, 12 and 13 all edit `TranscriptView.itemView` / `body`. Do them in order; if you take them out of order, reconcile by hand rather than reverting a neighbour's edit.
- `RunningDots` already exists (`App/RunningDots.swift`) and is used by the current thinking row.
- `TimelineRow` / `TimelineBuilder` stay compiled and unused. Do not delete them and do not re-enable timeline rendering here.
