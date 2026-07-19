# Chat Context-Window Usage Ring Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a small circular progress ring in the chat composer showing how full the agent's context window is, sourced from the ACP `usage_update` session/update kind, hidden entirely for agents that never send it.

**Architecture:** Decode the existing-but-currently-dropped `usage_update` ACP notification into a new `SessionUpdate.usageUpdate(ContextUsage)` case, fold it into `TranscriptReducer`'s pure state, expose it through `ChatController`, and render it as a small `Circle().trim(...)` ring in `ChatComposerView`'s control bar with a native two-line `.help()` tooltip.

**Tech Stack:** Swift 6, SwiftUI, swift-testing (`@Test`/`#expect`), no new dependencies.

## Global Constraints

- All new/edited UI-facing strings must be in English (Tiller UI convention).
- Tests first (swift-testing, not XCTest), per repo convention.
- `Scripts/ci.sh` must print `CI OK` before this work is considered done.
- No settings toggle, no `_meta` parsing, no client-side token estimation — this plan only decodes the native `usage_update` protocol kind (see `docs/superpowers/specs/2026-07-19-chat-context-usage-ring-design.md`).

---

### Task 1: Decode `usage_update` into `SessionUpdate`

**Files:**
- Modify: `Packages/TillerACP/Sources/TillerACP/SessionUpdate.swift`
- Test: `Packages/TillerACP/Tests/TillerACPTests/SessionUpdateTests.swift`

**Interfaces:**
- Produces: `public struct ContextUsage: Sendable, Equatable, Codable { public var used: Int; public var size: Int }` and `SessionUpdate.usageUpdate(ContextUsage)`, both in module `TillerACP`. Later tasks (2, 3) consume `SessionUpdate.usageUpdate(ContextUsage)` by pattern-matching on it, and construct `ContextUsage(used:size:)` directly in tests.

- [ ] **Step 1: Write the failing test**

Add to `Packages/TillerACP/Tests/TillerACPTests/SessionUpdateTests.swift`, right after `decodesPlanCommandsAndMode` (after its closing `}` at line 69):

```swift
    @Test func decodesUsageUpdate() throws {
        let note = try decode("""
        {"sessionId":"s1","update":{"sessionUpdate":"usage_update","used":146000,"size":200000}}
        """)
        #expect(note.update == .usageUpdate(ContextUsage(used: 146000, size: 200000)))
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd Packages/TillerACP && swift test --filter decodesUsageUpdate`
Expected: FAIL to build — `type 'SessionUpdate' has no member 'usageUpdate'` (and `ContextUsage` undefined).

- [ ] **Step 3: Write minimal implementation**

In `Packages/TillerACP/Sources/TillerACP/SessionUpdate.swift`, add the `ContextUsage` struct right after the existing `AvailableCommand` struct (after its closing `}` at line 21):

```swift
public struct ContextUsage: Sendable, Equatable, Codable {
    public var used: Int
    public var size: Int
    public init(used: Int, size: Int) {
        self.used = used
        self.size = size
    }
}
```

Add a new case to the `SessionUpdate` enum (currently lines 24-34):

```swift
public enum SessionUpdate: Sendable, Equatable {
    case userMessageChunk(ContentBlock)
    case agentMessageChunk(ContentBlock)
    case agentThoughtChunk(ContentBlock)
    case toolCall(ToolCall)
    case toolCallUpdate(ToolCallUpdate)
    case plan([PlanEntry])
    case availableCommandsUpdate([AvailableCommand])
    case currentModeUpdate(String)
    case usageUpdate(ContextUsage)
    case unknown(String)
}
```

Update the `CodingKeys` enum (currently lines 37-39) to add `used, size`:

```swift
    private enum CodingKeys: String, CodingKey {
        case sessionUpdate, content, entries, availableCommands, currentModeId, used, size
    }
```

Add a decoding branch in the `switch discriminator` (currently lines 44-64), right before `default:`:

```swift
        case "usage_update":
            self = .usageUpdate(ContextUsage(
                used: try container.decode(Int.self, forKey: .used),
                size: try container.decode(Int.self, forKey: .size)))
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd Packages/TillerACP && swift test --filter decodesUsageUpdate`
Expected: PASS

- [ ] **Step 5: Run the full package test suite to check for regressions**

Run: `cd Packages/TillerACP && swift test`
Expected: all tests PASS (existing `unknownUpdateAndKindAreTolerated` still passes since `"usage_update"` is now a known case, not a regression — it only asserted `"totally_new_thing"` decodes to `.unknown`, which is untouched).

- [ ] **Step 6: Commit**

```bash
git add Packages/TillerACP/Sources/TillerACP/SessionUpdate.swift Packages/TillerACP/Tests/TillerACPTests/SessionUpdateTests.swift
git commit -m "feat: decode ACP usage_update session update"
```

---

### Task 2: Fold `usageUpdate` into `TranscriptReducer`

**Files:**
- Modify: `Packages/TillerACP/Sources/TillerACP/TranscriptReducer.swift`
- Test: `Packages/TillerACP/Tests/TillerACPTests/TranscriptReducerTests.swift`

**Interfaces:**
- Consumes: `SessionUpdate.usageUpdate(ContextUsage)` and `ContextUsage` from Task 1.
- Produces: `public private(set) var contextUsage: ContextUsage?` on `TranscriptReducer`. Task 3 consumes this via `reducer.contextUsage`.

- [ ] **Step 1: Write the failing test**

Add to `Packages/TillerACP/Tests/TillerACPTests/TranscriptReducerTests.swift`, right after `modeAndCommandsUpdateState` (after its closing `}` before the suite's final `}`):

```swift
    @Test func usageUpdateSetsContextUsage() {
        var reducer = TranscriptReducer()
        #expect(reducer.contextUsage == nil)
        reducer.apply(.usageUpdate(ContextUsage(used: 1000, size: 200000)))
        #expect(reducer.contextUsage == ContextUsage(used: 1000, size: 200000))
        reducer.apply(.usageUpdate(ContextUsage(used: 2500, size: 200000)))
        #expect(reducer.contextUsage == ContextUsage(used: 2500, size: 200000))
        #expect(reducer.items.isEmpty)
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd Packages/TillerACP && swift test --filter usageUpdateSetsContextUsage`
Expected: FAIL to build — `value of type 'TranscriptReducer' has no member 'contextUsage'`.

- [ ] **Step 3: Write minimal implementation**

In `Packages/TillerACP/Sources/TillerACP/TranscriptReducer.swift`, add a new stored property next to the existing published state (currently lines 6-9):

```swift
public struct TranscriptReducer: Sendable, Equatable {
    public private(set) var items: [TranscriptItem] = []
    public private(set) var currentModeId: String?
    public private(set) var availableCommands: [AvailableCommand] = []
    public private(set) var contextUsage: ContextUsage?
```

Add a new case to the `switch update` in `apply(_:)` (currently lines 130-134), right before `case .unknown:`:

```swift
        case .currentModeUpdate(let modeId):
            currentModeId = modeId

        case .usageUpdate(let usage):
            contextUsage = usage

        case .unknown:
            break
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd Packages/TillerACP && swift test --filter usageUpdateSetsContextUsage`
Expected: PASS

- [ ] **Step 5: Run the full package test suite to check for regressions**

Run: `cd Packages/TillerACP && swift test`
Expected: all tests PASS

- [ ] **Step 6: Commit**

```bash
git add Packages/TillerACP/Sources/TillerACP/TranscriptReducer.swift Packages/TillerACP/Tests/TillerACPTests/TranscriptReducerTests.swift
git commit -m "feat: fold usage_update into TranscriptReducer state"
```

---

### Task 3: Expose and render the context usage ring in the composer

**Files:**
- Modify: `App/Chat/ChatController.swift`
- Modify: `App/Chat/ChatComposerView.swift`

**Interfaces:**
- Consumes: `TranscriptReducer.contextUsage: ContextUsage?` from Task 2, and `ContextUsage.used`/`.size: Int` from Task 1.
- Produces: `ChatController.contextUsage: ContextUsage?` (read-only computed property), and a `contextUsageIndicator` view in `ChatComposerView` — neither is consumed by later tasks (this is the last task in the plan).

There is no App-layer test target for `ChatController`/`ChatComposerView` in this repo (verified: no existing test file references either type). Verification for this task is build success plus a manual smoke check, consistent with how the rest of `ChatComposerView`'s pills (`modePill`, `agentPill`, `effortPill`) are untested — the logic that can be wrong (decode, state fold) is already covered by Tasks 1-2's tests; this task is a thin, stateless render of already-tested data.

- [ ] **Step 1: Add the `contextUsage` passthrough property to `ChatController`**

In `App/Chat/ChatController.swift`, add a new computed property right after `currentModeId` (currently line 49):

```swift
    var items: [TranscriptItem] { restored + reducer.items }
    var currentModeId: String? { reducer.currentModeId ?? modes?.currentModeId }
    var contextUsage: ContextUsage? { reducer.contextUsage }
    var availableCommands: [AvailableCommand] { reducer.availableCommands }
```

- [ ] **Step 2: Verify the package builds with the new property**

Run: `cd Packages/TillerACP && swift build`
Expected: build succeeds (this confirms `ContextUsage`/`contextUsage` are correctly exported from `TillerACP` before touching App/ code, which can't be build-checked standalone via `swift build`).

- [ ] **Step 3: Add the `reduceMotion` environment property to `ChatComposerView`**

In `App/Chat/ChatComposerView.swift`, add the environment property to the existing `@State` block (currently lines 14-18):

```swift
    @State private var text = ""
    @State private var mentionPaths: [String] = []
    @State private var images: [ImageAttachment] = []
    @State private var mentionQuery: String?
    @State private var mentionCandidates: [String] = []
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
```

- [ ] **Step 4: Add the `contextUsageIndicator` view**

In `App/Chat/ChatComposerView.swift`, add this new private view after `effortPill` and before `effortLabel(_:)` (currently right after line 225, i.e. after the `effortPill`'s closing `}`):

```swift
    /// Context-window usage ring; hidden entirely when the agent never sent
    /// a `usage_update` (e.g. it doesn't implement that ACP extension) —
    /// there's nothing actionable the user can do about a missing signal,
    /// so no "unavailable" placeholder state (unlike UsageBarView).
    @ViewBuilder
    private var contextUsageIndicator: some View {
        if let usage = controller.contextUsage, usage.size > 0 {
            let fraction = min(1, max(0, Double(usage.used) / Double(usage.size)))
            let remaining = Int(((1 - fraction) * 100).rounded())
            ZStack {
                Circle().stroke(.quaternary, lineWidth: 2)
                Circle()
                    .trim(from: 0, to: fraction)
                    .stroke(Color.accentColor, style: StrokeStyle(lineWidth: 2, lineCap: .round))
                    .rotationEffect(.degrees(-90))
            }
            .frame(width: 16, height: 16)
            .animation(reduceMotion ? nil : .easeInOut(duration: 0.3), value: fraction)
            .help("\(remaining)% remaining\n\(usage.used.formatted()) / \(usage.size.formatted()) tokens")
        }
    }
```

- [ ] **Step 5: Wire it into `controlBar`**

In `App/Chat/ChatComposerView.swift`, modify `controlBar` (currently lines 71-92) to insert the indicator between `Spacer()` and the paperclip button:

```swift
    private var controlBar: some View {
        HStack(spacing: 8) {
            modePill
            agentPill
            effortPill
            Spacer()
            contextUsageIndicator
            Button {
                attachImage()
            } label: {
                Image(systemName: "paperclip")
                    .foregroundStyle(.secondary)
            }
            .buttonStyle(.plain)
            .help("Attach image (clipboard or file)")
            .disabled(!canInteract)
            if isPrompting {
                stopButton
            } else {
                sendButton
            }
        }
    }
```

- [ ] **Step 6: Regenerate the Xcode project and build the app**

Run: `xcodegen generate`
Run: `xcodebuild -project Tiller.xcodeproj -scheme Tiller -configuration Debug build`
Expected: `** BUILD SUCCEEDED **`

- [ ] **Step 7: Manual smoke check**

Open the app (`open Tiller.xcodeproj`, ⌘R), open a chat tab against Claude Code or OpenCode, send a message, and confirm:
- A small ring appears near the paperclip button after the first response.
- Hovering it shows a two-line tooltip: `"N% remaining"` then `"used / max tokens"`.
- The ring visually fills as more turns are sent.
- Opening a chat tab against an agent that never sends `usage_update` (if any available) shows no ring at all — not a broken/empty one.

- [ ] **Step 8: Run the full repo CI gate**

Run: `Scripts/ci.sh`
Expected: prints `CI OK`

- [ ] **Step 9: Commit**

```bash
git add App/Chat/ChatController.swift App/Chat/ChatComposerView.swift
git commit -m "feat: render context-window usage ring in chat composer"
```

---

## Self-Review Notes

- **Spec coverage:** data source/decode (Task 1), hidden-by-default fallback (Task 2's `nil` default + Task 3's `if let` guard — no code path renders an "unavailable" state), placement/color/fill-direction/tooltip format (Task 3 Step 4-5), reset-on-new-session (free, from `ChatController.swift:159`'s existing `reducer = TranscriptReducer()` — no task needed since Task 2's `contextUsage` defaults to `nil` on a fresh reducer). All spec sections have a task.
- **Placeholder scan:** none — every step has complete code.
- **Type consistency:** `ContextUsage(used:size:)` (Task 1) used identically in Task 2's test and Task 3's `usage.used`/`usage.size` field access; `SessionUpdate.usageUpdate(ContextUsage)` matches across Tasks 1-2; `ChatController.contextUsage: ContextUsage?` (Task 3 Step 1) matches what `contextUsageIndicator` (Task 3 Step 4) reads.
