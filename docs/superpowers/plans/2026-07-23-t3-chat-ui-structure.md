# T3-style Chat UI Structure Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Adopt t3code's chat area structure — typed timeline row model with work grouping and turn folding, composer-anchored approvals, permission mode dropdown, model/effort picker popover, upgraded context meter — while keeping Tiller's visual design tokens and the existing T3-style text rendering.

**Architecture:** A pure `TimelineBuilder` in TillerACP derives `[TimelineRow]` from the canonical `[TranscriptItem]` plus a small UI-owned `TimelineState` (expansion sets, streaming flag, durations). Views in `App/Chat/` render rows without re-deriving logic. Permission requests move out of tool-call cards into a composer-anchored approval panel driven by a pure `ComposerPermissions.extract`. The composer gains a native permission-mode dropdown (Spec 1's `PermissionMode`) and a model/effort picker popover.

**Tech Stack:** Swift 6, SwiftUI (macOS 15+), swift-testing (`@Test`/`#expect`), existing TillerACP types (`TranscriptItem`, `TranscriptReducer`, `PermissionMode`, `ModelInfo`, `SessionConfigOption`).

## Global Constraints

- Tests use swift-testing (`@Test` / `#expect`), never XCTest.
- `Scripts/ci.sh` must print `CI OK` before the plan is considered done. Known flaky: TillerTerminal PTY spawn tests — retry up to 5-6 times.
- After adding files to `App/`, run `xcodegen generate`. Never hand-edit `Tiller.xcodeproj`.
- All UI strings in English.
- Keep Tiller's design tokens (system materials, `.quaternary`, `AppTheme`); no new color constants beyond system styles.
- Content column: max width 700pt, centered.
- Minimap (t3code left gutter rail) is explicitly out of scope.
- Commit messages: Conventional Commits, lower-case imperative.
- Never add `AppTests` code that touches `UserDefaults.standard` (wipes user preferences).
- Do not stage or commit `docs/superpowers/plans/2026-07-22-new-chat-harness-selection.md` (unrelated untracked file).
- Deviation from spec wording, agreed: t3code's separate `work` + `work-toggle` rows are merged into a single `.work` row (SwiftUI LazyVStack needs no virtualization split). The toggle renders inside the work row's view.
- Durations: `TranscriptItem` is Codable-persisted and must not change shape. Turn durations are transient reducer state (`turnDurations`), never persisted; restored history shows no duration. This is accepted.
- `ContextUsage` carries only `used`/`size` (no input/output/cache split); tooltip shows exact used/size tokens.

---

### Task 1: TimelineRow types + TimelineBuilder segmentation

**Files:**
- Create: `Packages/TillerACP/Sources/TillerACP/Timeline/TimelineRow.swift`
- Create: `Packages/TillerACP/Sources/TillerACP/Timeline/TimelineBuilder.swift`
- Test: `Packages/TillerACP/Tests/TillerACPTests/TimelineBuilderTests.swift`

**Interfaces:**
- Consumes: `TranscriptItem`, `ToolCallItem`, `PlanEntry`, `PermissionState`, `ContentBlock` (all existing in TillerACP).
- Produces: `TimelineRow` (enum, cases `.message(item:meta:)`, `.work(groupId:entries:isExpanded:)`, `.turnFold(turnId:label:at:)`, `.turnDivider(id:at:)`, `.proposedPlan(id:entries:approval:)`, `.working`), `TimelineRow.MessageMeta` (`duration: TimeInterval?`, `showsCopyButton: Bool`), `TimelineRow.WorkEntry` (`item: ToolCallItem`, `label: String`), `TimelineState` (`expandedWorkGroups: Set<String>`, `unfoldedTurns: Set<String>`, `isStreaming: Bool`, `turnDurations: [String: TimeInterval]`), `TimelineBuilder.rows(items:state:) -> [TimelineRow]`, `TimelineBuilder.maxVisibleWorkEntries == 1`, `TimelineBuilder.openTurnCount == 2`.

- [ ] **Step 1: Write the failing tests**

```swift
// Packages/TillerACP/Tests/TillerACPTests/TimelineBuilderTests.swift
import Testing
import Foundation
@testable import TillerACP

@Suite("TimelineBuilder")
struct TimelineBuilderTests {
    func user(_ id: String, _ text: String) -> TranscriptItem {
        .userMessage(id: id, blocks: [.text(text)])
    }
    func agent(_ id: String, _ text: String, complete: Bool = true) -> TranscriptItem {
        .agentMessage(id: id, text: text, isComplete: complete)
    }
    func tool(_ id: String, title: String = "Read file",
              kind: ToolKind = .read,
              status: ToolCallStatus = .completed,
              permission: PermissionState? = nil) -> TranscriptItem {
        .toolCall(ToolCallItem(toolCallId: id, title: title, kind: kind,
                               status: status, permission: permission))
    }
    func divider(_ id: String, at: Date = Date(timeIntervalSince1970: 100)) -> TranscriptItem {
        .turnDivider(id: id, at: at)
    }

    @Test func mapsPlainMessagesToMessageRows() {
        let rows = TimelineBuilder.rows(
            items: [user("u1", "hi"), agent("a1", "hello")],
            state: TimelineState())
        #expect(rows.count == 2)
        guard case .message(let first, _) = rows[0] else { Issue.record("expected message"); return }
        #expect(first.id == "u1")
        guard case .message(let second, _) = rows[1] else { Issue.record("expected message"); return }
        #expect(second.id == "a1")
    }

    @Test func closedTurnKeepsItsDividerRow() {
        let rows = TimelineBuilder.rows(
            items: [user("u1", "hi"), agent("a1", "done"), divider("d1")],
            state: TimelineState())
        guard case .turnDivider(let id, _) = rows.last else {
            Issue.record("expected trailing divider"); return
        }
        #expect(id == "d1")
    }

    @Test func streamingAppendsWorkingRow() {
        let rows = TimelineBuilder.rows(
            items: [user("u1", "hi")],
            state: TimelineState(isStreaming: true))
        #expect(rows.last == .working)
    }

    @Test func notStreamingHasNoWorkingRow() {
        let rows = TimelineBuilder.rows(items: [user("u1", "hi")], state: TimelineState())
        #expect(!rows.contains(.working))
    }

    @Test func rowIdsAreStableAndUnique() {
        let rows = TimelineBuilder.rows(
            items: [user("u1", "hi"), agent("a1", "hello"), divider("d1")],
            state: TimelineState())
        #expect(Set(rows.map(\.id)).count == rows.count)
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd Packages/TillerACP && swift test --filter TimelineBuilderTests`
Expected: compile FAILURE — `TimelineBuilder`, `TimelineRow`, `TimelineState` not defined.

- [ ] **Step 3: Write the implementation**

```swift
// Packages/TillerACP/Sources/TillerACP/Timeline/TimelineRow.swift
import Foundation

/// One renderable row of the chat timeline. Pure value derived from the
/// canonical transcript by TimelineBuilder; the view layer renders rows
/// without re-deriving grouping or folding.
public enum TimelineRow: Sendable, Equatable, Identifiable {
    /// Meta attached to assistant message rows (turn duration, copy action).
    public struct MessageMeta: Sendable, Equatable {
        public var duration: TimeInterval?
        public var showsCopyButton: Bool

        public init(duration: TimeInterval? = nil, showsCopyButton: Bool = false) {
            self.duration = duration
            self.showsCopyButton = showsCopyButton
        }
    }

    /// One tool call inside a work group, with its compact one-line label.
    public struct WorkEntry: Sendable, Equatable, Identifiable {
        public var item: ToolCallItem
        public var label: String

        public var id: String { item.id }

        public init(item: ToolCallItem) {
            self.item = item
            self.label = Self.compactLabel(item.title)
        }

        /// First line of the title, trailing "completed" stripped
        /// ("Read AppModel.swift completed" -> "Read AppModel.swift").
        static func compactLabel(_ title: String) -> String {
            var text = title.split(separator: "\n").first.map(String.init) ?? title
            text = text.trimmingCharacters(in: .whitespaces)
            if text.lowercased().hasSuffix("completed") {
                text = String(text.dropLast("completed".count))
                    .trimmingCharacters(in: .whitespaces)
            }
            return text
        }
    }

    /// Non-tool transcript item rendered as-is (user/agent message, thought,
    /// edit summary, system notice). `meta` is non-nil only for the turn's
    /// final assistant message.
    case message(item: TranscriptItem, meta: MessageMeta?)
    /// Consecutive tool calls of one turn, grouped. Collapsed shows only the
    /// last `TimelineBuilder.maxVisibleWorkEntries`; expanded shows all.
    case work(groupId: String, entries: [WorkEntry], isExpanded: Bool)
    /// A collapsed older turn (single expandable row, no divider follows).
    case turnFold(turnId: String, label: String, at: Date)
    case turnDivider(id: String, at: Date)
    case proposedPlan(id: String, entries: [PlanEntry], approval: PermissionState?)
    /// Trailing indicator while a turn is streaming.
    case working

    public var id: String {
        switch self {
        case .message(let item, _): "msg-\(item.id)"
        case .work(let groupId, _, _): "work-\(groupId)"
        case .turnFold(let turnId, _, _): "fold-\(turnId)"
        case .turnDivider(let id, _): "divider-\(id)"
        case .proposedPlan(let id, _, _): "plan-\(id)"
        case .working: "working"
        }
    }
}
```

```swift
// Packages/TillerACP/Sources/TillerACP/Timeline/TimelineBuilder.swift
import Foundation

/// UI-owned inputs to row derivation: expansion toggles, streaming flag, and
/// per-turn durations from the reducer.
public struct TimelineState: Sendable, Equatable {
    public var expandedWorkGroups: Set<String>
    public var unfoldedTurns: Set<String>
    public var isStreaming: Bool
    /// turnDivider id -> seconds; from `TranscriptReducer.turnDurations`.
    public var turnDurations: [String: TimeInterval]

    public init(expandedWorkGroups: Set<String> = [],
                unfoldedTurns: Set<String> = [],
                isStreaming: Bool = false,
                turnDurations: [String: TimeInterval] = [:]) {
        self.expandedWorkGroups = expandedWorkGroups
        self.unfoldedTurns = unfoldedTurns
        self.isStreaming = isStreaming
        self.turnDurations = turnDurations
    }
}

/// Pure derivation of timeline rows from the canonical transcript, mirroring
/// t3code's MessagesTimeline.logic.ts. No I/O, no view types.
public enum TimelineBuilder {
    /// Entries shown by a collapsed work group (t3code's MAX_VISIBLE).
    public static let maxVisibleWorkEntries = 1
    /// Most-recent turns that stay open by default (current + previous).
    public static let openTurnCount = 2

    public static func rows(items: [TranscriptItem],
                            state: TimelineState) -> [TimelineRow] {
        let turns = segment(items)
        var rows: [TimelineRow] = []
        for (index, turn) in turns.enumerated() {
            let turnId = turn.divider?.id ?? "open"
            let isFoldable = index < turns.count - openTurnCount
                && turn.divider != nil && !turn.items.isEmpty
            if isFoldable, !state.unfoldedTurns.contains(turnId),
               let divider = turn.divider {
                rows.append(.turnFold(turnId: turnId, label: turn.label,
                                      at: divider.at))
                continue
            }
            rows.append(contentsOf: turnRows(turn, turnId: turnId, state: state))
            if let divider = turn.divider {
                rows.append(.turnDivider(id: divider.id, at: divider.at))
            }
        }
        if state.isStreaming { rows.append(.working) }
        return rows
    }

    // MARK: - Turn segmentation

    private struct Turn {
        var items: [TranscriptItem] = []
        var divider: (id: String, at: Date)?

        /// First line of the first user message, for the fold row.
        var label: String {
            for item in items {
                guard case .userMessage(_, let blocks) = item else { continue }
                for block in blocks {
                    if case .text(let text) = block {
                        let line = text.split(separator: "\n").first
                            .map(String.init) ?? text
                        return String(line.prefix(60))
                    }
                }
            }
            return "Turn"
        }
    }

    private static func segment(_ items: [TranscriptItem]) -> [Turn] {
        var turns: [Turn] = []
        var current = Turn()
        for item in items {
            if case .turnDivider(let id, let at) = item {
                current.divider = (id, at)
                turns.append(current)
                current = Turn()
            } else {
                current.items.append(item)
            }
        }
        if !current.items.isEmpty { turns.append(current) }
        return turns
    }

    // MARK: - Rows within one open turn

    private static func turnRows(_ turn: Turn, turnId: String,
                                 state: TimelineState) -> [TimelineRow] {
        var rows: [TimelineRow] = []
        var pendingWork: [TimelineRow.WorkEntry] = []
        let lastAgentMessageId = turn.items.last {
            if case .agentMessage = $0 { return true }
            return false
        }?.id
        let planApproval = turn.items.compactMap { item -> PermissionState? in
            guard case .toolCall(let call) = item, call.kind == .switchMode,
                  call.permission?.isPending == true else { return nil }
            return call.permission
        }.first

        func flushWork() {
            guard let first = pendingWork.first else { return }
            let groupId = "wg-\(first.id)"
            rows.append(.work(groupId: groupId, entries: pendingWork,
                              isExpanded: state.expandedWorkGroups.contains(groupId)))
            pendingWork = []
        }

        for item in turn.items {
            switch item {
            case .toolCall(let call):
                pendingWork.append(TimelineRow.WorkEntry(item: call))
            case .plan(let id, let entries):
                flushWork()
                rows.append(.proposedPlan(id: id, entries: entries,
                                          approval: planApproval))
            case .agentMessage(_, _, let isComplete):
                flushWork()
                var meta: TimelineRow.MessageMeta?
                if item.id == lastAgentMessageId {
                    meta = TimelineRow.MessageMeta(
                        duration: turn.divider.flatMap { state.turnDurations[$0.id] },
                        showsCopyButton: isComplete
                            && (turn.divider != nil || !state.isStreaming))
                }
                rows.append(.message(item: item, meta: meta))
            case .turnDivider:
                break // never inside a segmented turn
            default:
                flushWork()
                rows.append(.message(item: item, meta: nil))
            }
        }
        flushWork()
        return rows
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd Packages/TillerACP && swift test --filter TimelineBuilderTests`
Expected: PASS (5 tests).

- [ ] **Step 5: Commit**

```bash
git add Packages/TillerACP/Sources/TillerACP/Timeline Packages/TillerACP/Tests/TillerACPTests/TimelineBuilderTests.swift
git commit -m "feat: timeline row model with turn segmentation"
```

---

### Task 2: Work grouping behaviour

**Files:**
- Modify: `Packages/TillerACP/Sources/TillerACP/Timeline/TimelineBuilder.swift` (no change expected — behaviour already implemented in Task 1; this task locks it with tests and fixes any gap)
- Test: `Packages/TillerACP/Tests/TillerACPTests/TimelineBuilderTests.swift`

**Interfaces:**
- Consumes: Task 1's `TimelineBuilder.rows`, `TimelineRow.WorkEntry`.
- Produces: verified contract — consecutive `.toolCall` items in one turn form one `.work` row with `groupId == "wg-<firstToolCallId>"`; interleaved messages split groups; `isExpanded` mirrors `TimelineState.expandedWorkGroups`; `WorkEntry.compactLabel` strips a trailing "completed".

- [ ] **Step 1: Add the failing tests** (append to the `TimelineBuilderTests` suite)

```swift
    @Test func consecutiveToolCallsFormOneWorkGroup() {
        let rows = TimelineBuilder.rows(
            items: [user("u1", "go"), tool("t1"), tool("t2"), tool("t3")],
            state: TimelineState())
        let workRows = rows.compactMap { row -> [TimelineRow.WorkEntry]? in
            if case .work(_, let entries, _) = row { return entries }
            return nil
        }
        #expect(workRows.count == 1)
        #expect(workRows[0].count == 3)
        #expect(workRows[0].map(\.id) == ["t1", "t2", "t3"])
    }

    @Test func workGroupIdIsFirstToolCallId() {
        let rows = TimelineBuilder.rows(
            items: [tool("t1"), tool("t2")], state: TimelineState())
        guard case .work(let groupId, _, let isExpanded) = rows[0] else {
            Issue.record("expected work row"); return
        }
        #expect(groupId == "wg-t1")
        #expect(isExpanded == false)
    }

    @Test func expandedStateFollowsTimelineState() {
        let rows = TimelineBuilder.rows(
            items: [tool("t1"), tool("t2")],
            state: TimelineState(expandedWorkGroups: ["wg-t1"]))
        guard case .work(_, _, let isExpanded) = rows[0] else {
            Issue.record("expected work row"); return
        }
        #expect(isExpanded == true)
    }

    @Test func interleavedMessageSplitsWorkGroups() {
        let rows = TimelineBuilder.rows(
            items: [tool("t1"), agent("a1", "half"), tool("t2")],
            state: TimelineState())
        let groupIds = rows.compactMap { row -> String? in
            if case .work(let id, _, _) = row { return id }
            return nil
        }
        #expect(groupIds == ["wg-t1", "wg-t2"])
    }

    @Test func compactLabelStripsTrailingCompleted() {
        #expect(TimelineRow.WorkEntry.compactLabel("Read AppModel.swift completed")
                == "Read AppModel.swift")
        #expect(TimelineRow.WorkEntry.compactLabel("Read AppModel.swift")
                == "Read AppModel.swift")
        #expect(TimelineRow.WorkEntry.compactLabel("multi\nline title") == "multi")
    }
```

- [ ] **Step 2: Run tests**

Run: `cd Packages/TillerACP && swift test --filter TimelineBuilderTests`
Expected: PASS if Task 1's implementation is complete; otherwise fix `TimelineBuilder` until green. Do not change test expectations.

- [ ] **Step 3: Commit**

```bash
git add Packages/TillerACP
git commit -m "test: lock work grouping contract"
```

---

### Task 3: Turn folding

**Files:**
- Modify: `Packages/TillerACP/Sources/TillerACP/Timeline/TimelineBuilder.swift` (behaviour largely in place from Task 1)
- Test: `Packages/TillerACP/Tests/TillerACPTests/TimelineBuilderTests.swift`

**Interfaces:**
- Consumes: Task 1's builder.
- Produces: verified contract — with N closed turns plus an open one, all but the last `openTurnCount` (2) turns render as a single `.turnFold(turnId: <dividerId>, label:, at:)` row (no divider row after a fold); `unfoldedTurns` re-opens a folded turn; fold label is the first line of the turn's first user message (60-char cap), `"Turn"` fallback.

- [ ] **Step 1: Add the failing tests** (append to the suite)

```swift
    @Test func olderTurnsFoldKeepingLastTwoOpen() {
        // three closed turns + one open turn = 4 turns; first two fold
        let items: [TranscriptItem] = [
            user("u1", "first question"), agent("a1", "r1"), divider("d1"),
            user("u2", "second question"), agent("a2", "r2"), divider("d2"),
            user("u3", "third question"), agent("a3", "r3"), divider("d3"),
            user("u4", "fourth question"),
        ]
        let rows = TimelineBuilder.rows(items: items, state: TimelineState())
        let foldIds = rows.compactMap { row -> String? in
            if case .turnFold(let turnId, _, _) = row { return turnId }
            return nil
        }
        #expect(foldIds == ["d1", "d2"])
        // folded turns emit no divider row
        let dividerIds = rows.compactMap { row -> String? in
            if case .turnDivider(let id, _) = row { return id }
            return nil
        }
        #expect(dividerIds == ["d3"])
        // open turns keep their message rows
        #expect(rows.contains { $0.id == "msg-u3" })
        #expect(rows.contains { $0.id == "msg-u4" })
        #expect(!rows.contains { $0.id == "msg-u1" })
    }

    @Test func unfoldedTurnRendersItsRows() {
        let items: [TranscriptItem] = [
            user("u1", "first"), agent("a1", "r1"), divider("d1"),
            user("u2", "second"), divider("d2"),
            user("u3", "third"), divider("d3"),
        ]
        let rows = TimelineBuilder.rows(
            items: items, state: TimelineState(unfoldedTurns: ["d1"]))
        #expect(rows.contains { $0.id == "msg-u1" })
        #expect(!rows.contains { row in
            if case .turnFold(let id, _, _) = row { return id == "d1" }
            return false
        })
    }

    @Test func foldLabelComesFromFirstUserMessage() {
        let long = String(repeating: "x", count: 100)
        let items: [TranscriptItem] = [
            user("u1", long), divider("d1"),
            user("u2", "b"), divider("d2"),
            user("u3", "c"), divider("d3"),
        ]
        let rows = TimelineBuilder.rows(items: items, state: TimelineState())
        guard case .turnFold(_, let label, _) = rows[0] else {
            Issue.record("expected fold"); return
        }
        #expect(label.count == 60)
    }
```

- [ ] **Step 2: Run tests**

Run: `cd Packages/TillerACP && swift test --filter TimelineBuilderTests`
Expected: PASS with Task 1's implementation; fix builder if not. Do not change test expectations.

- [ ] **Step 3: Commit**

```bash
git add Packages/TillerACP
git commit -m "test: lock turn folding contract"
```

---

### Task 4: Turn durations + copy-button visibility

**Files:**
- Modify: `Packages/TillerACP/Sources/TillerACP/TranscriptReducer.swift`
- Test: `Packages/TillerACP/Tests/TillerACPTests/TranscriptReducerTests.swift` (existing file — append a new test)
- Test: `Packages/TillerACP/Tests/TillerACPTests/TimelineBuilderTests.swift`

**Interfaces:**
- Consumes: existing `TranscriptReducer.userPrompted(_:)` / `turnEnded(_:at:)`.
- Produces: `TranscriptReducer.turnDurations: [String: TimeInterval]` (public read-only; key = turnDivider id; transient, never persisted), `userPrompted(_ blocks:at:)` gains an `at: Date = Date()` parameter. Builder contract: `MessageMeta.duration` on the closed turn's final assistant message; `showsCopyButton` true on final complete assistant message when turn closed or not streaming.

- [ ] **Step 1: Write the failing tests**

Append to `TranscriptReducerTests.swift` (match the file's existing suite style):

```swift
    @Test func recordsTurnDurationKeyedByDividerId() {
        var reducer = TranscriptReducer()
        let start = Date(timeIntervalSince1970: 1_000)
        reducer.userPrompted([.text("hi")], at: start)
        reducer.apply(.agentMessageChunk(.text("hello")))
        reducer.turnEnded(.endTurn, at: start.addingTimeInterval(42))
        guard case .turnDivider(let dividerId, _) = reducer.items.last else {
            Issue.record("expected divider"); return
        }
        #expect(reducer.turnDurations[dividerId] == 42)
    }
```

Append to `TimelineBuilderTests`:

```swift
    @Test func finalAssistantMessageCarriesDurationAndCopy() {
        let items: [TranscriptItem] = [
            user("u1", "go"), agent("a1", "partial"), tool("t1"),
            agent("a2", "final"), divider("d1"),
        ]
        let rows = TimelineBuilder.rows(
            items: items,
            state: TimelineState(turnDurations: ["d1": 42]))
        func meta(_ rowId: String) -> TimelineRow.MessageMeta?? {
            for row in rows {
                if case .message(let item, let meta) = row, "msg-\(item.id)" == rowId {
                    return meta
                }
            }
            return nil
        }
        #expect(meta("msg-a1") == .some(nil))
        let final = meta("msg-a2")
        #expect(final??.duration == 42)
        #expect(final??.showsCopyButton == true)
    }

    @Test func streamingOpenTurnHidesCopyButton() {
        let items: [TranscriptItem] = [
            user("u1", "go"), agent("a1", "typing", complete: false),
        ]
        let rows = TimelineBuilder.rows(
            items: items, state: TimelineState(isStreaming: true))
        guard case .message(_, let meta) = rows[1] else {
            Issue.record("expected message"); return
        }
        #expect(meta?.showsCopyButton == false)
        #expect(meta?.duration == nil)
    }
```

- [ ] **Step 2: Run tests to verify the reducer test fails**

Run: `cd Packages/TillerACP && swift test --filter recordsTurnDuration`
Expected: compile FAILURE — `userPrompted` has no `at:` parameter, `turnDurations` undefined.

- [ ] **Step 3: Implement in TranscriptReducer**

In `TranscriptReducer.swift`:

1. Add stored properties after `contextUsage`:

```swift
    /// Seconds each closed turn took, keyed by its turnDivider id. Transient
    /// UI state (message meta); never persisted with the transcript.
    public private(set) var turnDurations: [String: TimeInterval] = [:]
    private var turnStartedAt: Date?
```

2. Change `userPrompted` signature and record the start:

```swift
    /// Records the user's prompt (called by the session when a turn starts).
    public mutating func userPrompted(_ blocks: [ContentBlock], at date: Date = Date()) {
        turnStartedAt = date
        turnEditPaths = []
        closeOpenStreams()
        items.append(.userMessage(id: makeId("user"), blocks: blocks))
    }
```

3. In `turnEnded(_:at:)`, replace the final divider append with:

```swift
        if !items.isEmpty {
            let dividerId = makeId("divider")
            if let start = turnStartedAt {
                turnDurations[dividerId] = date.timeIntervalSince(start)
                turnStartedAt = nil
            }
            items.append(.turnDivider(id: dividerId, at: date))
        }
```

- [ ] **Step 4: Run the full package tests**

Run: `cd Packages/TillerACP && swift test`
Expected: PASS (all suites; the two new builder tests pass against Task 1's implementation — fix builder if not).

- [ ] **Step 5: Commit**

```bash
git add Packages/TillerACP
git commit -m "feat: transient turn durations in transcript reducer"
```

---

### Task 5: Composer permission extraction + proposed-plan approval

**Files:**
- Create: `Packages/TillerACP/Sources/TillerACP/Timeline/ComposerPermissions.swift`
- Test: `Packages/TillerACP/Tests/TillerACPTests/ComposerPermissionsTests.swift`
- Test: `Packages/TillerACP/Tests/TillerACPTests/TimelineBuilderTests.swift`

**Interfaces:**
- Consumes: `TranscriptItem`, `PermissionState`, `PermissionOption`, `ToolKind`, `JSONRPCID`.
- Produces: `ComposerPermission` (struct: `requestId: JSONRPCID`, `toolCallId: String`, `title: String`, `kind: ToolKind`, `options: [PermissionOption]`; `Identifiable` by `toolCallId`), `ComposerPermissions.extract(from: [TranscriptItem]) -> [ComposerPermission]` — pending permissions in transcript order, excluding `kind == .switchMode` (those render in the proposed-plan card). Builder contract: `.proposedPlan.approval` carries the pending switchMode permission of the same turn.

- [ ] **Step 1: Write the failing tests**

```swift
// Packages/TillerACP/Tests/TillerACPTests/ComposerPermissionsTests.swift
import Testing
@testable import TillerACP

@Suite("ComposerPermissions")
struct ComposerPermissionsTests {
    func pending(_ id: String, kind: ToolKind = .execute) -> TranscriptItem {
        .toolCall(ToolCallItem(
            toolCallId: id, title: "Run \(id)", kind: kind, status: .pending,
            permission: PermissionState(
                requestId: .string(id),
                options: [PermissionOption(optionId: "allow", name: "Allow",
                                           kind: .allowOnce)])))
    }

    @Test func extractsPendingInTranscriptOrder() {
        let resolved = TranscriptItem.toolCall(ToolCallItem(
            toolCallId: "done", title: "Done", kind: .execute, status: .completed,
            permission: PermissionState(requestId: .string("done"), options: [],
                                        resolution: .cancelled)))
        let items: [TranscriptItem] = [
            pending("p1"), resolved,
            .agentMessage(id: "a1", text: "x", isComplete: true),
            pending("p2"),
        ]
        let result = ComposerPermissions.extract(from: items)
        #expect(result.map(\.toolCallId) == ["p1", "p2"])
        #expect(result[0].requestId == .string("p1"))
        #expect(result[0].options.count == 1)
    }

    @Test func excludesSwitchModePermissions() {
        let items: [TranscriptItem] = [pending("exec"), pending("plan-exit", kind: .switchMode)]
        let result = ComposerPermissions.extract(from: items)
        #expect(result.map(\.toolCallId) == ["exec"])
    }

    @Test func emptyWhenNothingPending() {
        #expect(ComposerPermissions.extract(from: [
            .agentMessage(id: "a1", text: "x", isComplete: true),
        ]).isEmpty)
    }
}
```

Append to `TimelineBuilderTests`:

```swift
    @Test func proposedPlanCarriesPendingSwitchModeApproval() {
        let approval = PermissionState(
            requestId: .string("exit"),
            options: [
                PermissionOption(optionId: "yes", name: "Approve", kind: .allowOnce),
                PermissionOption(optionId: "no", name: "Reject", kind: .rejectOnce),
            ])
        let items: [TranscriptItem] = [
            user("u1", "plan it"),
            .plan(id: "pl1", entries: [PlanEntry(content: "step 1", priority: "medium",
                                                 status: "pending")]),
            tool("exit-tool", title: "Exit plan mode", kind: .switchMode,
                 status: .pending, permission: approval),
        ]
        let rows = TimelineBuilder.rows(items: items, state: TimelineState())
        guard let planRow = rows.first(where: { row in
            if case .proposedPlan = row { return true }
            return false
        }), case .proposedPlan(_, let entries, let rowApproval) = planRow else {
            Issue.record("expected proposedPlan row"); return
        }
        #expect(entries.count == 1)
        #expect(rowApproval?.requestId == .string("exit"))
    }
```

(`PlanEntry(content:priority:status:)` verified against `SessionUpdate.swift` — all three parameters are non-optional Strings.)

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd Packages/TillerACP && swift test --filter ComposerPermissionsTests`
Expected: compile FAILURE — `ComposerPermissions` not defined.

- [ ] **Step 3: Write the implementation**

```swift
// Packages/TillerACP/Sources/TillerACP/Timeline/ComposerPermissions.swift
import Foundation

/// A pending permission request surfaced in the composer's approval panel.
public struct ComposerPermission: Sendable, Equatable, Identifiable {
    public var requestId: JSONRPCID
    public var toolCallId: String
    public var title: String
    public var kind: ToolKind
    public var options: [PermissionOption]

    public var id: String { toolCallId }

    public init(requestId: JSONRPCID, toolCallId: String, title: String,
                kind: ToolKind, options: [PermissionOption]) {
        self.requestId = requestId
        self.toolCallId = toolCallId
        self.title = title
        self.kind = kind
        self.options = options
    }
}

public enum ComposerPermissions {
    /// Pending permissions in transcript order. Plan-mode exits
    /// (kind .switchMode) are excluded: they render inside the
    /// proposed-plan card instead of the composer panel.
    public static func extract(from items: [TranscriptItem]) -> [ComposerPermission] {
        items.compactMap { item in
            guard case .toolCall(let call) = item,
                  call.kind != .switchMode,
                  let permission = call.permission, permission.isPending else {
                return nil
            }
            return ComposerPermission(
                requestId: permission.requestId, toolCallId: call.toolCallId,
                title: call.title, kind: call.kind, options: permission.options)
        }
    }
}
```

- [ ] **Step 4: Run the package tests**

Run: `cd Packages/TillerACP && swift test`
Expected: PASS (the proposedPlan builder test passes against Task 1's `planApproval` logic — fix builder if not).

- [ ] **Step 5: Commit**

```bash
git add Packages/TillerACP
git commit -m "feat: composer permission extraction and plan approval wiring"
```

---

### Task 6: Timeline rendering in the app

**Files:**
- Modify: `App/Chat/ChatController.swift`
- Modify: `App/Chat/TranscriptView.swift`
- Create: `App/Chat/TimelineRowViews.swift`
- After creating the file: run `xcodegen generate`

**Interfaces:**
- Consumes: `TimelineBuilder.rows`, `TimelineRow`, `TimelineState`, `reducer.turnDurations` (Task 4), existing views `ToolCallCardView`, `AgentMarkdownTextView`, `InsightCardView`, `EditSummaryCardView`, `AgentMessageSegmenter`, `RunningDots`.
- Produces: `ChatController.timelineRows: [TimelineRow]`, `ChatController.expandedWorkGroups: Set<String>`, `ChatController.unfoldedTurns: Set<String>` (both `var`, observable); views `WorkGroupView`, `TurnFoldRow` in `TimelineRowViews.swift`. Task 7 relies on `timelineRows` for the plan banner.

- [ ] **Step 1: Extend ChatController**

In `ChatController.swift`, after the `isFollowing`/`onFollowLocation` block, add:

```swift
    /// Timeline expansion state (work groups and folded turns the user opened).
    var expandedWorkGroups: Set<String> = []
    var unfoldedTurns: Set<String> = []
```

After the `items` computed property, add:

```swift
    var timelineRows: [TimelineRow] {
        TimelineBuilder.rows(items: items, state: TimelineState(
            expandedWorkGroups: expandedWorkGroups,
            unfoldedTurns: unfoldedTurns,
            isStreaming: state == .prompting,
            turnDurations: reducer.turnDurations))
    }
```

- [ ] **Step 2: Create the row views**

```swift
// App/Chat/TimelineRowViews.swift
import SwiftUI
import TillerACP
import TillerCore

/// Grouped tool work: "▸ N steps" toggle plus either the latest entry
/// (collapsed) or every entry as a full card (expanded).
struct WorkGroupView: View {
    let groupId: String
    let entries: [TimelineRow.WorkEntry]
    let isExpanded: Bool
    let controller: ChatController
    let worktree: Worktree
    let appModel: AppModel

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            if entries.count > 1 {
                Button {
                    withAnimation(.easeOut(duration: 0.12)) {
                        if isExpanded {
                            controller.expandedWorkGroups.remove(groupId)
                        } else {
                            controller.expandedWorkGroups.insert(groupId)
                        }
                    }
                } label: {
                    HStack(spacing: 5) {
                        Image(systemName: "chevron.right")
                            .font(.caption2.weight(.semibold))
                            .rotationEffect(.degrees(isExpanded ? 90 : 0))
                        Text("\(entries.count) steps").font(.caption)
                    }
                    .foregroundStyle(.secondary)
                }
                .buttonStyle(.plain)
            }
            if isExpanded {
                ForEach(entries) { entry in
                    ToolCallCardView(item: entry.item, controller: controller,
                                     worktree: worktree, appModel: appModel)
                }
            } else {
                ForEach(entries.suffix(TimelineBuilder.maxVisibleWorkEntries)) { entry in
                    compactRow(entry)
                }
            }
        }
    }

    private func compactRow(_ entry: TimelineRow.WorkEntry) -> some View {
        HStack(spacing: 6) {
            statusGlyph(entry.item.status)
            Text(entry.label)
                .font(.callout)
                .foregroundStyle(.secondary)
                .lineLimit(1)
            Spacer(minLength: 0)
        }
        .padding(.vertical, 2)
        .contentShape(Rectangle())
        .onTapGesture { controller.expandedWorkGroups.insert(groupId) }
    }

    @ViewBuilder
    private func statusGlyph(_ status: ToolCallStatus) -> some View {
        switch status {
        case .pending, .inProgress:
            ProgressView().controlSize(.mini)
        case .completed:
            Image(systemName: "checkmark.circle.fill")
                .foregroundStyle(.green).font(.caption)
        case .failed:
            Image(systemName: "xmark.circle.fill")
                .foregroundStyle(.red).font(.caption)
        }
    }
}

/// A collapsed older turn: tapping re-opens it in place.
struct TurnFoldRow: View {
    let turnId: String
    let label: String
    let at: Date
    let controller: ChatController

    var body: some View {
        Button {
            withAnimation(.easeOut(duration: 0.15)) {
                controller.unfoldedTurns.insert(turnId)
            }
        } label: {
            HStack(spacing: 6) {
                Image(systemName: "chevron.right")
                    .font(.caption2.weight(.semibold))
                Text("Turn: \(label)")
                    .font(.caption)
                    .lineLimit(1)
                Spacer()
                Text(at, format: .dateTime.hour().minute())
                    .font(.caption2)
                    .foregroundStyle(.tertiary)
            }
            .foregroundStyle(.secondary)
            .padding(.vertical, 6).padding(.horizontal, 8)
            .background(.quaternary.opacity(0.3),
                        in: RoundedRectangle(cornerRadius: 8))
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }
}
```

- [ ] **Step 3: Rewrite TranscriptView over rows**

Replace the `ScrollView` body and `itemView` in `TranscriptView.swift`:

```swift
    var body: some View {
        ScrollViewReader { proxy in
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 14) {
                    ForEach(controller.timelineRows) { row in
                        rowView(row)
                            .id(row.id)
                    }
                    Color.clear.frame(height: 1).id("bottom")
                }
                .frame(maxWidth: 700)
                .frame(maxWidth: .infinity, alignment: .center)
                .padding(.horizontal, 16)
                .padding(.vertical, 14)
            }
            .onChange(of: controller.items.count) {
                withAnimation(.easeOut(duration: 0.15)) {
                    proxy.scrollTo("bottom", anchor: .bottom)
                }
            }
        }
    }

    @ViewBuilder
    private func rowView(_ row: TimelineRow) -> some View {
        switch row {
        case .message(let item, let meta):
            itemView(item, meta: meta)
        case .work(let groupId, let entries, let isExpanded):
            WorkGroupView(groupId: groupId, entries: entries,
                          isExpanded: isExpanded, controller: controller,
                          worktree: worktree, appModel: appModel)
        case .turnFold(let turnId, let label, let at):
            TurnFoldRow(turnId: turnId, label: label, at: at,
                        controller: controller)
        case .turnDivider(_, let at):
            turnDivider(at)
        case .proposedPlan(_, let entries, let approval):
            planCard(entries, approval: approval)
        case .working:
            thinkingRow
        }
    }
```

Update `itemView` to take the meta and drop the cases rows now cover (`toolCall`, `plan`, `turnDivider` no longer arrive as `.message` items, but keep the switch exhaustive — render them via existing views as a fallback):

```swift
    @ViewBuilder
    private func itemView(_ item: TranscriptItem, meta: TimelineRow.MessageMeta?) -> some View {
        switch item {
        case .userMessage(_, let blocks):
            userBubble(blocks)
        case .agentMessage(_, let text, _):
            VStack(alignment: .leading, spacing: 4) {
                agentMessage(text)
                if let meta, meta.showsCopyButton || meta.duration != nil {
                    messageMetaRow(meta, text: text)
                }
            }
        case .thought(_, let text):
            ThoughtRow(text: text)
        case .toolCall(let toolCall):
            ToolCallCardView(item: toolCall, controller: controller,
                             worktree: worktree, appModel: appModel)
        case .plan(_, let entries):
            planCard(entries, approval: nil)
        case .turnDivider(_, let date):
            turnDivider(date)
        case .editSummary(_, let paths):
            EditSummaryCardView(paths: paths, worktree: worktree,
                                appModel: appModel)
        case .systemNotice(_, let text):
            Text(text)
                .font(.caption)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
                .frame(maxWidth: .infinity, alignment: .center)
                .padding(.vertical, 6)
        }
    }

    private func messageMetaRow(_ meta: TimelineRow.MessageMeta, text: String) -> some View {
        HStack(spacing: 8) {
            if let duration = meta.duration {
                Text(Self.formatDuration(duration))
                    .font(.caption2)
                    .foregroundStyle(.tertiary)
            }
            if meta.showsCopyButton {
                Button {
                    NSPasteboard.general.clearContents()
                    NSPasteboard.general.setString(text, forType: .string)
                } label: {
                    Image(systemName: "doc.on.doc").font(.caption2)
                }
                .buttonStyle(.plain)
                .foregroundStyle(.secondary)
                .help("Copy message")
            }
        }
    }

    static func formatDuration(_ seconds: TimeInterval) -> String {
        let total = Int(seconds.rounded())
        if total < 60 { return "\(total)s" }
        return "\(total / 60)m \(String(format: "%02d", total % 60))s"
    }
```

Add `import AppKit` at the top of `TranscriptView.swift` (for `NSPasteboard`). Change `planCard` to accept and render the approval:

```swift
    private func planCard(_ entries: [PlanEntry], approval: PermissionState?) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            Label("Plan", systemImage: "checklist")
                .font(.caption.weight(.semibold))
                .foregroundStyle(.secondary)
            ForEach(Array(entries.enumerated()), id: \.offset) { _, entry in
                HStack(alignment: .firstTextBaseline, spacing: 6) {
                    Image(systemName: entry.status == "completed"
                          ? "checkmark.circle.fill"
                          : entry.status == "in_progress" ? "circle.dotted" : "circle")
                        .foregroundStyle(entry.status == "completed" ? .green : .secondary)
                        .font(.caption)
                    Text(entry.content).font(.callout)
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
                              ? .green : .red)
                        .controlSize(.small)
                    }
                }
                .padding(.top, 4)
            }
        }
        .padding(8)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(.quaternary.opacity(0.4), in: RoundedRectangle(cornerRadius: 8))
    }
```

Note the existing "Piano" label becomes "Plan" (English-strings rule). Remove the old `if controller.state == .prompting { thinkingRow }` block from `body` — the `.working` row covers it.

- [ ] **Step 4: Regenerate the project and build**

Run: `xcodegen generate && Scripts/ci.sh`
Expected: `CI OK` (retry PTY flakes up to 5-6 times).

- [ ] **Step 5: Commit**

```bash
git add App/Chat/ChatController.swift App/Chat/TranscriptView.swift App/Chat/TimelineRowViews.swift project.yml
git commit -m "feat: timeline row rendering with work groups and turn folds"
```

(`project.yml` only if it changed; the file list is glob-based, usually unchanged.)

---

### Task 7: Composer-anchored approvals + banner stack

**Files:**
- Modify: `App/Chat/ChatController.swift`
- Modify: `App/Chat/ChatComposerView.swift`
- Modify: `App/Chat/ChatPaneView.swift`
- Modify: `App/Chat/ToolCallCardView.swift`
- Create: `App/Chat/ComposerApprovalPanel.swift`
- After creating the file: run `xcodegen generate`

**Interfaces:**
- Consumes: Task 5's `ComposerPermissions.extract`, `ComposerPermission`; Task 6's `timelineRows`.
- Produces: `ChatController.composerPermissions: [ComposerPermission]`, `ChatController.hasPlanAwaitingApproval: Bool`; `ComposerApprovalPanel` view. Permission buttons removed from `ToolCallCardView` (pending border stays).

- [ ] **Step 1: Extend ChatController**

Add next to `hasPendingPermission`:

```swift
    /// Pending permissions shown in the composer approval panel (plan-mode
    /// exits excluded — they render in the proposed-plan card).
    var composerPermissions: [ComposerPermission] {
        ComposerPermissions.extract(from: items)
    }
    var hasPlanAwaitingApproval: Bool {
        timelineRows.contains { row in
            if case .proposedPlan(_, _, .some(let approval)) = row {
                return approval.isPending
            }
            return false
        }
    }
```

- [ ] **Step 2: Create the approval panel**

```swift
// App/Chat/ComposerApprovalPanel.swift
import SwiftUI
import TillerACP

/// Pending permission requests anchored above the composer (t3code's
/// ComposerPendingApprovalPanel): the first request shows its options;
/// further queued requests appear as a count.
struct ComposerApprovalPanel: View {
    let permissions: [ComposerPermission]
    let controller: ChatController

    var body: some View {
        if let current = permissions.first {
            VStack(alignment: .leading, spacing: 6) {
                HStack(spacing: 6) {
                    Image(systemName: "lock.shield")
                        .font(.caption)
                        .foregroundStyle(.orange)
                    Text(current.title)
                        .font(.callout.weight(.medium))
                        .lineLimit(2)
                    Spacer()
                    if permissions.count > 1 {
                        Text("+\(permissions.count - 1) more")
                            .font(.caption2)
                            .foregroundStyle(.secondary)
                    }
                }
                HStack(spacing: 8) {
                    ForEach(current.options, id: \.optionId) { option in
                        Button(option.name) {
                            Task {
                                await controller.answerPermission(
                                    requestId: current.requestId,
                                    optionId: option.optionId)
                            }
                        }
                        .buttonStyle(.bordered)
                        .tint(option.kind == .allowOnce || option.kind == .allowAlways
                              ? .green : .red)
                        .controlSize(.small)
                    }
                }
            }
            .padding(10)
            .background(.quaternary.opacity(0.4),
                        in: RoundedRectangle(cornerRadius: 10))
            .overlay(RoundedRectangle(cornerRadius: 10)
                .strokeBorder(Color.orange.opacity(0.6), lineWidth: 1))
        }
    }
}
```

- [ ] **Step 3: Mount the panel in the composer**

In `ChatComposerView.swift` `body`, insert the panel just before `card`:

```swift
        VStack(alignment: .leading, spacing: 6) {
            if slashPopupVisible {
                slashPopup
            }
            if let query = mentionQuery, !mentionCandidates.isEmpty {
                mentionPopup(query: query)
            }
            queuedList
            ComposerApprovalPanel(permissions: controller.composerPermissions,
                                  controller: controller)
            card
        }
```

- [ ] **Step 4: Move the banner stack above the composer**

In `ChatPaneView.swift` `body`, move the `promptError` and `mcpWarning` banner blocks from above `TranscriptView` to between `TranscriptView` and the composer's `Divider()`, and add the plan banner. The `needsAuth`/`disconnected` state banners stay at the top. Resulting body order:

```swift
        VStack(spacing: 0) {
            header
            Divider()
            switch controller.state {
            case .needsAuth:
                banner(
                    "Authentication required",
                    detail: "Log in from the CLI (e.g. `claude /login`) in a terminal, then restart the agent.",
                    actionTitle: "Retry") {
                    Task { await controller.start() }
                }
            case .disconnected(let message):
                banner(
                    "Agent disconnected",
                    detail: message ?? "The process has terminated.",
                    actionTitle: "Restart agent") {
                    Task { await controller.start() }
                }
            default:
                EmptyView()
            }
            TranscriptView(controller: controller, worktree: worktree,
                           appModel: appModel)
            if let promptError = controller.promptError {
                banner(
                    "Turn error",
                    detail: promptError,
                    actionTitle: "OK") {
                    controller.promptError = nil
                }
            }
            if let mcpWarning = controller.mcpWarning {
                banner(
                    "MCP configuration",
                    detail: mcpWarning,
                    actionTitle: "OK") {
                    controller.mcpWarning = nil
                }
            }
            if controller.hasPlanAwaitingApproval {
                banner(
                    "Plan awaiting approval",
                    detail: "Review the proposed plan in the transcript, then approve or reject it.",
                    actionTitle: "OK") {}
            }
            Divider()
            ChatComposerView(controller: controller, worktreePath: worktree.path)
        }
```

- [ ] **Step 5: Strip inline permission buttons from ToolCallCardView**

In `ToolCallCardView.swift`, delete the `permissionButtons(_:)` function, the `tint(for:)` helper, and the call site:

```swift
            if isPermissionPending, let permission = item.permission {
                permissionButtons(permission)
            }
```

Keep `isPermissionPending` and the orange border/`contentBody` expansion — the card still highlights while its request waits in the composer panel.

- [ ] **Step 6: Regenerate, build, verify**

Run: `xcodegen generate && Scripts/ci.sh`
Expected: `CI OK`.

- [ ] **Step 7: Commit**

```bash
git add App/Chat project.yml
git commit -m "feat: composer-anchored permission approvals and banner stack"
```

---

### Task 8: Permission mode dropdown

**Files:**
- Modify: `Packages/TillerACP/Sources/TillerACP/PermissionMode.swift`
- Modify: `App/Chat/ChatComposerView.swift`
- Test: `Packages/TillerACP/Tests/TillerACPTests/PermissionModeTests.swift` (existing — append)

**Interfaces:**
- Consumes: Spec 1's `PermissionMode` (cases `ask`, `acceptEdits`, `plan`, `fullAuto`), `PermissionMode.supported(byDriverFor:)`, `ChatController.permissionMode` / `setPermissionMode(_:)`.
- Produces: `PermissionMode.displayName: String` ("Ask", "Accept edits", "Plan", "Full auto").

- [ ] **Step 1: Write the failing test** (append to the existing `PermissionModeTests` suite)

```swift
    @Test func displayNamesAreHumanReadable() {
        #expect(PermissionMode.ask.displayName == "Ask")
        #expect(PermissionMode.acceptEdits.displayName == "Accept edits")
        #expect(PermissionMode.plan.displayName == "Plan")
        #expect(PermissionMode.fullAuto.displayName == "Full auto")
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd Packages/TillerACP && swift test --filter displayNamesAreHumanReadable`
Expected: compile FAILURE — `displayName` not defined.

- [ ] **Step 3: Implement displayName**

In `PermissionMode.swift`, add to the enum:

```swift
    /// Human-readable name for the composer dropdown.
    public var displayName: String {
        switch self {
        case .ask: "Ask"
        case .acceptEdits: "Accept edits"
        case .plan: "Plan"
        case .fullAuto: "Full auto"
        }
    }
```

Run: `cd Packages/TillerACP && swift test --filter PermissionModeTests` — expected PASS.

- [ ] **Step 4: Replace the composer mode pill**

In `ChatComposerView.swift`, replace the whole `modePill` property with:

```swift
    @ViewBuilder
    private var modePill: some View {
        if let mode = controller.permissionMode {
            Menu {
                ForEach(PermissionMode.supported(byDriverFor: controller.agentId),
                        id: \.self) { candidate in
                    Button {
                        Task { await controller.setPermissionMode(candidate) }
                    } label: {
                        if candidate == mode {
                            Label(candidate.displayName, systemImage: "checkmark")
                        } else {
                            Text(candidate.displayName)
                        }
                    }
                }
            } label: {
                HStack(spacing: 5) {
                    Circle().fill(statusDotColor).frame(width: 6, height: 6)
                    Text(mode.displayName).font(.caption)
                    Image(systemName: "chevron.down").font(.system(size: 7, weight: .bold))
                }
            }
            .menuStyle(.borderlessButton)
            .menuIndicator(.hidden)
            .fixedSize()
            .modifier(PillBackground())
            .help("Permission mode")
        } else if let modes = controller.modes, !modes.availableModes.isEmpty {
            Menu {
                ForEach(modes.availableModes, id: \.id) { mode in
                    Button(mode.name) {
                        Task { await controller.setMode(mode.id) }
                    }
                }
            } label: {
                HStack(spacing: 5) {
                    Circle().fill(statusDotColor).frame(width: 6, height: 6)
                    Text(currentModeName).font(.caption)
                    Image(systemName: "chevron.down").font(.system(size: 7, weight: .bold))
                }
            }
            .menuStyle(.borderlessButton)
            .menuIndicator(.hidden)
            .fixedSize()
            .modifier(PillBackground())
        } else {
            HStack(spacing: 5) {
                Circle().fill(statusDotColor).frame(width: 6, height: 6)
                Text(stateLabel).font(.caption)
            }
            .modifier(PillBackground())
        }
    }
```

- [ ] **Step 5: Build and verify**

Run: `Scripts/ci.sh`
Expected: `CI OK`.

- [ ] **Step 6: Commit**

```bash
git add Packages/TillerACP App/Chat/ChatComposerView.swift
git commit -m "feat: permission mode dropdown for native drivers"
```

---

### Task 9: Model + effort picker popover

**Files:**
- Create: `Packages/TillerACP/Sources/TillerACP/Timeline/ModelPickerFilter.swift`
- Create: `App/Chat/ModelPickerPopover.swift`
- Modify: `App/Chat/ChatComposerView.swift`
- Test: `Packages/TillerACP/Tests/TillerACPTests/ModelPickerFilterTests.swift`
- After creating the App file: run `xcodegen generate`

**Interfaces:**
- Consumes: `ModelInfo` (`modelId`, `name`, `description`), `SessionModelState`, `SessionConfigOption`, `ChatController.models` / `effortOption` / `setModel(_:)` / `setEffort(_:)`.
- Produces: `ModelPickerFilter.filter(_ models: [ModelInfo], query: String) -> [ModelInfo]` (case-insensitive substring on name, modelId and description; empty query returns all, order preserved). View `ModelPickerPopover`. The recommended model is the driver's first list entry (driver order is authoritative), badged "Recommended".

- [ ] **Step 1: Write the failing tests**

```swift
// Packages/TillerACP/Tests/TillerACPTests/ModelPickerFilterTests.swift
import Testing
@testable import TillerACP

@Suite("ModelPickerFilter")
struct ModelPickerFilterTests {
    let models = [
        ModelInfo(modelId: "claude-opus-4-8", name: "Opus 4.8",
                  description: "Deepest reasoning"),
        ModelInfo(modelId: "claude-sonnet-5", name: "Sonnet 5",
                  description: "Best coding model"),
        ModelInfo(modelId: "claude-haiku-4-5", name: "Haiku 4.5", description: nil),
    ]

    @Test func emptyQueryReturnsAllInOrder() {
        #expect(ModelPickerFilter.filter(models, query: "").map(\.modelId)
                == models.map(\.modelId))
        #expect(ModelPickerFilter.filter(models, query: "   ").map(\.modelId)
                == models.map(\.modelId))
    }

    @Test func matchesNameCaseInsensitively() {
        #expect(ModelPickerFilter.filter(models, query: "opus").map(\.modelId)
                == ["claude-opus-4-8"])
    }

    @Test func matchesModelIdAndDescription() {
        #expect(ModelPickerFilter.filter(models, query: "haiku-4").map(\.modelId)
                == ["claude-haiku-4-5"])
        #expect(ModelPickerFilter.filter(models, query: "coding").map(\.modelId)
                == ["claude-sonnet-5"])
    }

    @Test func noMatchReturnsEmpty() {
        #expect(ModelPickerFilter.filter(models, query: "gemini").isEmpty)
    }
}
```

(`ModelInfo(modelId:name:description:)` verified against `ACPTypes.swift`.)

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd Packages/TillerACP && swift test --filter ModelPickerFilterTests`
Expected: compile FAILURE — `ModelPickerFilter` not defined.

- [ ] **Step 3: Implement the filter**

```swift
// Packages/TillerACP/Sources/TillerACP/Timeline/ModelPickerFilter.swift
import Foundation

/// Search filter for the model picker popover. Pure, order-preserving.
public enum ModelPickerFilter {
    public static func filter(_ models: [ModelInfo], query: String) -> [ModelInfo] {
        let trimmed = query.trimmingCharacters(in: .whitespaces).lowercased()
        guard !trimmed.isEmpty else { return models }
        return models.filter { model in
            model.name.lowercased().contains(trimmed)
                || model.modelId.lowercased().contains(trimmed)
                || (model.description?.lowercased().contains(trimmed) ?? false)
        }
    }
}
```

Run: `cd Packages/TillerACP && swift test --filter ModelPickerFilterTests` — expected PASS.

- [ ] **Step 4: Create the popover view**

```swift
// App/Chat/ModelPickerPopover.swift
import SwiftUI
import TillerACP

/// Model + effort picker: search field, driver-supplied model list with the
/// driver's first entry badged as recommended, effort chips at the bottom.
struct ModelPickerPopover: View {
    let controller: ChatController
    @Binding var isPresented: Bool
    @State private var query = ""

    private var models: [ModelInfo] {
        controller.models?.availableModels ?? []
    }
    private var filtered: [ModelInfo] {
        ModelPickerFilter.filter(models, query: query)
    }
    private var recommendedId: String? { models.first?.modelId }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            TextField("Search models…", text: $query)
                .textFieldStyle(.roundedBorder)
                .controlSize(.small)
            ScrollView {
                VStack(alignment: .leading, spacing: 2) {
                    ForEach(filtered, id: \.modelId) { model in
                        modelRow(model)
                    }
                    if filtered.isEmpty {
                        Text("No models match")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                            .padding(6)
                    }
                }
            }
            .frame(maxHeight: 260)
            effortSection
        }
        .padding(10)
        .frame(width: 300)
    }

    private func modelRow(_ model: ModelInfo) -> some View {
        Button {
            isPresented = false
            Task { await controller.setModel(model.modelId) }
        } label: {
            HStack(spacing: 6) {
                Image(systemName: "checkmark")
                    .font(.caption2.weight(.semibold))
                    .opacity(model.modelId == controller.models?.currentModelId ? 1 : 0)
                VStack(alignment: .leading, spacing: 1) {
                    Text(model.name).font(.callout)
                    if let description = model.description, !description.isEmpty {
                        Text(description)
                            .font(.caption2)
                            .foregroundStyle(.secondary)
                            .lineLimit(1)
                    }
                }
                Spacer(minLength: 0)
                if model.modelId == recommendedId {
                    Text("Recommended")
                        .font(.caption2)
                        .padding(.horizontal, 5).padding(.vertical, 1)
                        .background(Color.accentColor.opacity(0.15), in: Capsule())
                        .foregroundStyle(Color.accentColor)
                }
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .padding(.vertical, 3).padding(.horizontal, 4)
        .background(model.modelId == controller.models?.currentModelId
                        ? AnyShapeStyle(.quaternary.opacity(0.5))
                        : AnyShapeStyle(Color.clear),
                    in: RoundedRectangle(cornerRadius: 6))
    }

    @ViewBuilder
    private var effortSection: some View {
        if let effort = controller.effortOption,
           let choices = effort.options, !choices.isEmpty {
            Divider()
            HStack(spacing: 6) {
                Text(effort.name ?? "Effort")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                Spacer()
                ForEach(choices, id: \.value) { choice in
                    Button(choice.name) {
                        Task { await controller.setEffort(choice.value) }
                    }
                    .buttonStyle(.bordered)
                    .controlSize(.mini)
                    .tint(choice.value == effort.currentValue
                              ? Color.accentColor : Color.secondary)
                }
            }
        }
    }
}
```

- [ ] **Step 5: Replace agentPill with the popover trigger**

In `ChatComposerView.swift`, add a state var near the other `@State`s:

```swift
    @State private var modelPickerShown = false
```

Replace the `agentPill` property with:

```swift
    /// Compact model button opening the picker popover; plain agent badge
    /// when the agent advertises no models.
    @ViewBuilder
    private var agentPill: some View {
        if let models = controller.models, !models.availableModels.isEmpty {
            Button {
                modelPickerShown.toggle()
            } label: {
                HStack(spacing: 5) {
                    Text(currentModelName).font(.caption).lineLimit(1)
                    if let effort = controller.effortOption,
                       effort.currentValue != nil {
                        Text(effortLabel(effort))
                            .font(.caption2.weight(.semibold))
                            .foregroundStyle(.secondary)
                    }
                    Image(systemName: "chevron.down").font(.system(size: 7, weight: .bold))
                }
            }
            .buttonStyle(.plain)
            .modifier(PillBackground())
            .popover(isPresented: $modelPickerShown, arrowEdge: .top) {
                ModelPickerPopover(controller: controller,
                                   isPresented: $modelPickerShown)
            }
        } else {
            Text(agentDisplayName).font(.caption)
                .modifier(PillBackground())
        }
    }
```

Delete the now-unused `modelMenuTitle(_:)` helper and the standalone `effortPill` (the popover's effort section replaces it) — remove `effortPill` from `controlBar` too. Keep `effortLabel(_:)` (reused in the button badge) and `currentModelName`.

- [ ] **Step 6: Regenerate, build, verify**

Run: `xcodegen generate && Scripts/ci.sh`
Expected: `CI OK`.

- [ ] **Step 7: Commit**

```bash
git add Packages/TillerACP App/Chat project.yml
git commit -m "feat: model and effort picker popover with search"
```

---

### Task 10: Context meter upgrade

**Files:**
- Modify: `App/Chat/ChatComposerView.swift`

**Interfaces:**
- Consumes: `controller.contextUsage` (`ContextUsage`: `used`, `size`).
- Produces: warning color at >80% fill; tooltip with exact used/size tokens and percentage. (No input/output/cache split — `ContextUsage` doesn't carry it; Global Constraints.)

- [ ] **Step 1: Update the indicator**

Replace `contextUsageIndicator` in `ChatComposerView.swift`:

```swift
    /// Context-window meter; always shown so its control-bar position stays
    /// stable. Empty/dimmed until the agent reports usage. Turns orange past
    /// the 80% warning threshold.
    private var contextUsageIndicator: some View {
        let usage = controller.contextUsage
        let fraction = usage.flatMap { $0.size > 0 ? min(1, max(0, Double($0.used) / Double($0.size))) : nil } ?? 0
        let warning = fraction > 0.8
        return ZStack {
            Circle().stroke(.quaternary, lineWidth: 2)
            if usage != nil {
                Circle()
                    .trim(from: 0, to: fraction)
                    .stroke(warning ? Color.orange : Color.accentColor,
                            style: StrokeStyle(lineWidth: 2, lineCap: .round))
                    .rotationEffect(.degrees(-90))
            }
        }
        .frame(width: 16, height: 16)
        .contentShape(Circle())
        .animation(reduceMotion ? nil : .easeInOut(duration: 0.3), value: fraction)
        .help(usage.map { usage in
            let percent = Int((fraction * 100).rounded())
            return "\(percent)% of context used\n\(usage.used.formatted()) / \(usage.size.formatted()) tokens"
        } ?? "Context usage unavailable")
    }
```

- [ ] **Step 2: Build and verify**

Run: `Scripts/ci.sh`
Expected: `CI OK`.

- [ ] **Step 3: Commit**

```bash
git add App/Chat/ChatComposerView.swift
git commit -m "feat: context meter warning threshold and token tooltip"
```

---

### Task 11: Final gate + manual visual checklist

**Files:** none new.

- [ ] **Step 1: Full verification**

Run: `xcodegen generate && Scripts/ci.sh`
Expected: `CI OK` (retry PTY flakes). All package suites green, including every test added by Tasks 1-9.

- [ ] **Step 2: Review the diff against the spec**

Run: `git log --oneline` (this plan's commits) and `git diff <base>..HEAD --stat`. Confirm every spec section maps to a commit: row model (1-4), plan/approvals (5, 7), rendering (6), mode dropdown (8), picker (9), meter (10).

- [ ] **Step 3: Manual visual checklist** (human, in the running app — record results, don't skip)

- Work grouping: a turn with 3+ tool calls shows "N steps" + latest entry only; toggle expands to full cards.
- Turn fold: after 3+ turns, older turns collapse to "Turn: <label>"; tap re-opens.
- Duration + copy button appear on the final assistant message after the turn ends; copy puts markdown on the pasteboard.
- Proposed plan (Claude, Plan mode): plan card shows approve/reject; approving unlocks the execution turn.
- Composer approvals: run a tool needing permission in Ask mode — request appears above the input, not in the card; queue two, answer in order.
- Permission mode dropdown on the three native drivers (Claude/Codex/OpenCode); hidden for an ACP agent (e.g. omp); Plan absent for Codex/OpenCode.
- Model picker popover: search filters, Recommended badge on first entry, selection persists across app restart (v13 columns).
- Effort chips (Codex/OpenCode) switch effort; Claude effort applies at next session start (known MINOR).
- Context meter fills during a long turn, turns orange past 80%, tooltip shows exact tokens.
- One ACP agent (omp) end-to-end: transcript, modes pill, no regressions.
- Content column centered at ~700pt on a wide window.

- [ ] **Step 4: Commit any leftovers, do not push without the user**

```bash
git status  # must be clean except known untracked files
```
