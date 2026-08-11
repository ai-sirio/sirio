# Context Ring Click Popover Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Clicking the context-window ring in the chat composer opens a popover with context usage, session cost, and a token breakdown, when the active agent driver reports them.

**Architecture:** `ContextUsage` (TillerACP) gains optional cost/breakdown fields. Only `ClaudeStreamJSONDriver` populates them (Codex/OpenCode/Pi have no such data on the wire). A new pure static function on `ComposerControlBar` turns a `ContextUsage` into display strings, following this file's existing convention of testing view logic through static functions rather than rendering. The ring becomes tappable and opens a `.popover` built from that function's output.

**Tech Stack:** Swift 6, SwiftUI, swift-testing (`@Test`/`#expect`), GRDB (unaffected — see Global Constraints).

## Global Constraints

- Cost/breakdown fields must **not** persist across app restarts — `ChatSessionStore.setContextUsage` already only writes `.used`/`.size` as explicit DB columns, so adding fields to `ContextUsage` requires no store/migration change. Do not touch `ChatSessionStore`.
- Only `ClaudeStreamJSONDriver` populates the new fields. Do not touch `CodexAppServerDriver.swift`, `OpenCodeHTTPDriver.swift`, `PiRPCDriver.swift`, or `PiWire.swift`.
- Do not touch the generic ACP wire decoder (`SessionUpdate.init(from:)` / `usage_update` case in `SessionUpdate.swift`) — it's the path the other three drivers go through and is correct as-is.
- Tests first (swift-testing), following each file's existing pattern exactly — this codebase tests SwiftUI view logic through pure static functions on the view struct, never by rendering/inspecting views.
- `Scripts/ci.sh` must print `CI OK` before this is considered done.

---

### Task 1: Add cost/breakdown fields to `ContextUsage`

**Files:**
- Modify: `Packages/TillerACP/Sources/TillerACP/SessionUpdate.swift:23-30`

**Interfaces:**
- Produces: `ContextUsage.costUsd: Double?`, `.inputTokens: Int?`, `.outputTokens: Int?`, `.cacheCreationTokens: Int?`, `.cacheReadTokens: Int?` — all default `nil`, consumed by Task 2 (driver) and Task 3 (formatting).

No dedicated test for this task — it's a plain struct field addition with no behavior of its own. It's exercised by Task 2's test (which constructs a `ContextUsage` with the new fields and asserts equality) and must not break any existing call site, verified by the full build in Task 2's test run.

- [ ] **Step 1: Edit the struct**

Replace lines 23-30 of `Packages/TillerACP/Sources/TillerACP/SessionUpdate.swift`:

```swift
public struct ContextUsage: Sendable, Equatable, Codable {
    public var used: Int
    public var size: Int
    public var costUsd: Double?
    public var inputTokens: Int?
    public var outputTokens: Int?
    public var cacheCreationTokens: Int?
    public var cacheReadTokens: Int?
    public init(used: Int, size: Int, costUsd: Double? = nil,
                inputTokens: Int? = nil, outputTokens: Int? = nil,
                cacheCreationTokens: Int? = nil, cacheReadTokens: Int? = nil) {
        self.used = used
        self.size = size
        self.costUsd = costUsd
        self.inputTokens = inputTokens
        self.outputTokens = outputTokens
        self.cacheCreationTokens = cacheCreationTokens
        self.cacheReadTokens = cacheReadTokens
    }
}
```

- [ ] **Step 2: Build the package to confirm existing call sites still compile**

Run: `cd Packages/TillerACP && swift build`
Expected: builds clean (all existing `ContextUsage(used:size:)` call sites still work because the new params default to `nil`).

- [ ] **Step 3: Commit**

```bash
git add Packages/TillerACP/Sources/TillerACP/SessionUpdate.swift
git commit -m "feat: add cost and token breakdown fields to ContextUsage"
```

---

### Task 2: Populate cost/breakdown from the Claude driver's result message

**Files:**
- Modify: `Packages/TillerACP/Sources/TillerACP/Drivers/ClaudeStreamJSONDriver.swift:518-528`
- Test: `Packages/TillerACP/Tests/TillerACPTests/ClaudeDriverTests.swift`

**Interfaces:**
- Consumes: `ContextUsage(used:size:costUsd:inputTokens:outputTokens:cacheCreationTokens:cacheReadTokens:)` from Task 1. `ClaudeResult.totalCostUsd: Double?` and `ClaudeResult.usage: ClaudeUsage?` (with `.inputTokens`, `.outputTokens`, `.cacheCreationInputTokens`, `.cacheReadInputTokens`, all `Int?`) already exist in `ClaudeWire.swift:299-337`.
- Produces: `contextUsage(from result: ClaudeResult) -> ContextUsage?` now carries cost/breakdown; consumed by the existing `.result` case at `ClaudeStreamJSONDriver.swift:302-304`, unchanged.

- [ ] **Step 1: Write the failing test**

Add to `Packages/TillerACP/Tests/TillerACPTests/ClaudeDriverTests.swift`, right after `contextUsageComesFromModelUsageWithoutProbing` (ends at line 407):

```swift
    @Test func contextUsageFromModelUsageIncludesCostAndBreakdown() async throws {
        let mock = MockTransport()
        let driver = ClaudeStreamJSONDriver(transport: mock, permissionMode: .ask,
                                            model: nil, resumeSessionId: nil)
        let collector = EventCollector()
        let eventTask = collect(driver, into: collector)
        try await driver.start()
        _ = try await connectWithInitializeResponse(driver, mock: mock)

        let promptTask = Task { try await driver.prompt([.text("hi")]) }
        _ = try await mock.waitForSent(count: 2)
        await mock.emit(#"{"type":"result","subtype":"success","is_error":false,"session_id":"s1","total_cost_usd":0.0421,"usage":{"input_tokens":4,"output_tokens":123,"cache_read_input_tokens":83967,"cache_creation_input_tokens":512},"modelUsage":{"claude-sonnet-5":{"contextWindow":1000000}}}"#)
        _ = try await promptTask.value

        let events = await waitForEvent(collector) { event in
            if case .update(.usageUpdate) = event { return true }
            return false
        }
        #expect(events.contains { event in
            guard case .update(.usageUpdate(let usage)) = event else { return false }
            return usage.costUsd == 0.0421
                && usage.inputTokens == 4
                && usage.outputTokens == 123
                && usage.cacheReadTokens == 83967
                && usage.cacheCreationTokens == 512
        })

        eventTask.cancel()
        await driver.stop()
    }

    @Test func contextUsageOmitsCostAndBreakdownWhenResultLacksThem() async throws {
        let mock = MockTransport()
        let driver = ClaudeStreamJSONDriver(transport: mock, permissionMode: .ask,
                                            model: nil, resumeSessionId: nil)
        let collector = EventCollector()
        let eventTask = collect(driver, into: collector)
        try await driver.start()
        _ = try await connectWithInitializeResponse(driver, mock: mock)

        let promptTask = Task { try await driver.prompt([.text("hi")]) }
        _ = try await mock.waitForSent(count: 2)
        await mock.emit(#"{"type":"result","subtype":"success","is_error":false,"session_id":"s1","usage":{"input_tokens":4,"output_tokens":123},"modelUsage":{"claude-sonnet-5":{"contextWindow":1000000}}}"#)
        _ = try await promptTask.value

        let events = await waitForEvent(collector) { event in
            if case .update(.usageUpdate) = event { return true }
            return false
        }
        #expect(events.contains { event in
            guard case .update(.usageUpdate(let usage)) = event else { return false }
            return usage.costUsd == nil
                && usage.inputTokens == 4
                && usage.outputTokens == 123
                && usage.cacheReadTokens == nil
                && usage.cacheCreationTokens == nil
        })

        eventTask.cancel()
        await driver.stop()
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd Packages/TillerACP && swift test --filter ClaudeDriverTests`
Expected: `contextUsageFromModelUsageIncludesCostAndBreakdown` and `contextUsageOmitsCostAndBreakdownWhenResultLacksThem` FAIL (usage.costUsd etc. don't exist yet, or are always nil since the driver doesn't set them) — compile error until Task 1 fields exist (they do, from Task 1) and logic mismatch until Step 3 below.

- [ ] **Step 3: Implement the minimal mapping**

Replace `contextUsage(from result:)` at `ClaudeStreamJSONDriver.swift:518-528`:

```swift
    /// `modelUsage` is keyed by model name; any entry carries the same
    /// context window, so the first one with the field wins.
    private func contextUsage(from result: ClaudeResult) -> ContextUsage? {
        guard case .object(let byModel)? = result.modelUsage,
              let size = byModel.values.compactMap({ $0["contextWindow"]?.intValue }).first,
              size > 0, let usage = result.usage else { return nil }
        let used = (usage.inputTokens ?? 0)
            + (usage.outputTokens ?? 0)
            + (usage.cacheReadInputTokens ?? 0)
            + (usage.cacheCreationInputTokens ?? 0)
        guard used >= 0 else { return nil }
        return ContextUsage(
            used: used, size: size, costUsd: result.totalCostUsd,
            inputTokens: usage.inputTokens, outputTokens: usage.outputTokens,
            cacheCreationTokens: usage.cacheCreationInputTokens,
            cacheReadTokens: usage.cacheReadInputTokens)
    }
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd Packages/TillerACP && swift test --filter ClaudeDriverTests`
Expected: PASS, including the two new tests and every pre-existing test in the file (`contextUsageComesFromModelUsageWithoutProbing` still passes since it doesn't assert on the new fields).

- [ ] **Step 5: Commit**

```bash
git add Packages/TillerACP/Sources/TillerACP/Drivers/ClaudeStreamJSONDriver.swift Packages/TillerACP/Tests/TillerACPTests/ClaudeDriverTests.swift
git commit -m "feat: map Claude result cost and token breakdown onto ContextUsage"
```

---

### Task 3: Pure formatting function for the popover content

**Files:**
- Modify: `App/Chat/ComposerControlBar.swift` (add near `contextRingColor`, `ComposerControlBar.swift:307-311`)
- Test: `AppTests/ComposerControlBarTests.swift`

**Interfaces:**
- Consumes: `ContextUsage` from Task 1 (`used`, `size`, `costUsd`, `inputTokens`, `outputTokens`, `cacheCreationTokens`, `cacheReadTokens`).
- Produces: `ComposerControlBar.ContextUsageDetail` struct with `percentLine: String`, `tokensLine: String`, `costLine: String?`, `breakdownLine: String?`, and `static func contextUsageDetail(_ usage: ContextUsage) -> ContextUsageDetail` — consumed by Task 4's popover view.

- [ ] **Step 1: Write the failing test**

Add to `AppTests/ComposerControlBarTests.swift`, right after `contextRingUsesAgentAccentNormallyAndRedForWarning` (ends at line 108):

```swift
    @Test func contextUsageDetailOmitsCostAndBreakdownWhenAbsent() {
        let usage = ContextUsage(used: 1000, size: 200_000)
        let detail = ComposerControlBar.contextUsageDetail(usage)

        #expect(detail.percentLine == "1% of context used")
        #expect(detail.tokensLine == "1,000 / 200,000 tokens")
        #expect(detail.costLine == nil)
        #expect(detail.breakdownLine == nil)
    }

    @Test func contextUsageDetailIncludesCostAndBreakdownWhenPresent() {
        let usage = ContextUsage(used: 620_602, size: 1_000_000, costUsd: 0.0421,
                                  inputTokens: 4, outputTokens: 123,
                                  cacheCreationTokens: 512, cacheReadTokens: 83_967)
        let detail = ComposerControlBar.contextUsageDetail(usage)

        #expect(detail.percentLine == "62% of context used")
        #expect(detail.tokensLine == "620,602 / 1,000,000 tokens")
        #expect(detail.costLine == "Cost: $0.04")
        #expect(detail.breakdownLine == "Input: 4 · Output: 123 · Cache write: 512 · Cache read: 83,967")
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `Scripts/ci.sh` will eventually run these, but for a fast local loop use Xcode's test navigator on `ComposerControlBarTests`, or:
Run: `xcodebuild test -project Tiller.xcodeproj -scheme Tiller -destination 'platform=macOS' -only-testing:AppTests/ComposerControlBarTests/contextUsageDetailOmitsCostAndBreakdownWhenAbsent -only-testing:AppTests/ComposerControlBarTests/contextUsageDetailIncludesCostAndBreakdownWhenPresent`
Expected: FAIL — `contextUsageDetail` doesn't exist yet (compile error).

- [ ] **Step 3: Implement the minimal formatting function**

Add to `ComposerControlBar` in `App/Chat/ComposerControlBar.swift`, right after `contextRingColor` (after line 311):

```swift
    struct ContextUsageDetail: Equatable {
        let percentLine: String
        let tokensLine: String
        let costLine: String?
        let breakdownLine: String?
    }

    /// Turns a driver's `ContextUsage` into the popover's display strings.
    /// Cost and the token breakdown are Claude-only on the wire today, so
    /// both lines are `nil` for every other agent.
    static func contextUsageDetail(_ usage: ContextUsage) -> ContextUsageDetail {
        let fraction = usage.size > 0 ? min(1, max(0, Double(usage.used) / Double(usage.size))) : 0
        let percent = Int((fraction * 100).rounded())
        let costLine = usage.costUsd.map { "Cost: " + $0.formatted(.currency(code: "USD")) }
        let breakdownLine: String? = {
            guard let input = usage.inputTokens, let output = usage.outputTokens else { return nil }
            var line = "Input: \(input.formatted()) · Output: \(output.formatted())"
            if let write = usage.cacheCreationTokens, let read = usage.cacheReadTokens {
                line += " · Cache write: \(write.formatted()) · Cache read: \(read.formatted())"
            }
            return line
        }()
        return ContextUsageDetail(
            percentLine: "\(percent)% of context used",
            tokensLine: "\(usage.used.formatted()) / \(usage.size.formatted()) tokens",
            costLine: costLine,
            breakdownLine: breakdownLine)
    }
```

- [ ] **Step 4: Run the tests to verify they pass**

Run the same `xcodebuild test -only-testing:...` command from Step 2.
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add App/Chat/ComposerControlBar.swift AppTests/ComposerControlBarTests.swift
git commit -m "feat: add ComposerControlBar.contextUsageDetail formatter"
```

---

### Task 4: Wire the tap gesture and popover onto the ring

**Files:**
- Modify: `App/Chat/ComposerControlBar.swift:21` (state) and `:269-294` (`contextUsageIndicator`)

**Interfaces:**
- Consumes: `ComposerControlBar.contextUsageDetail(_:)` and `.ContextUsageDetail` from Task 3.
- Produces: no new symbols for other tasks to consume — this is the plan's terminal, UI-only task.

No new automated test: this codebase has no SwiftUI view-rendering test harness (confirmed by every existing `ComposerControlBarTests` case testing a static function, never a rendered view), and `contextUsageDetail` — the only piece of real logic here — is already covered by Task 3. Verify this task by building and by the manual QA step below.

- [ ] **Step 1: Add popover state**

In `App/Chat/ComposerControlBar.swift`, change line 21 from:

```swift
    @State private var modelPickerShown = false
```

to:

```swift
    @State private var modelPickerShown = false
    @State private var contextPopoverShown = false
```

- [ ] **Step 2: Make the ring tappable and attach the popover**

Replace `contextUsageIndicator` (lines 269-294) with:

```swift
    /// Context-window meter; always shown so its control-bar position stays
    /// stable. Empty/dimmed until the agent reports usage. Turns red past the
    /// 80% warning threshold so it stays distinct from the agent's accent.
    /// Tapping opens a popover with the full breakdown when the driver
    /// reports usage; inert otherwise, same as the tooltip-only state today.
    private var contextUsageIndicator: some View {
        let usage = controller.contextUsage
        let fraction = usage.flatMap { $0.size > 0 ? min(1, max(0, Double($0.used) / Double($0.size))) : nil } ?? 0
        let warning = fraction > 0.8
        let ring = ZStack {
            Circle().stroke(.quaternary, lineWidth: 2)
            if usage != nil {
                Circle()
                    .trim(from: 0, to: fraction)
                    .stroke(Self.contextRingColor(
                        warning: warning, agentAccentColor: agentAccentColor),
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

        return Button {
            contextPopoverShown = true
        } label: {
            ring
        }
        .buttonStyle(.plain)
        .disabled(usage == nil)
        .popover(isPresented: $contextPopoverShown) {
            if let usage {
                let detail = Self.contextUsageDetail(usage)
                VStack(alignment: .leading, spacing: 4) {
                    Text(detail.percentLine).fontWeight(.semibold)
                    Text(detail.tokensLine).foregroundStyle(.secondary)
                    if let costLine = detail.costLine {
                        Text(costLine)
                    }
                    if let breakdownLine = detail.breakdownLine {
                        Text(breakdownLine).foregroundStyle(.secondary)
                    }
                }
                .font(AppFont.system(size: 12))
                .padding(12)
            }
        }
    }
```

- [ ] **Step 3: Build the app target**

Run: `xcodebuild build -project Tiller.xcodeproj -scheme Tiller -destination 'platform=macOS'`
Expected: builds clean.

- [ ] **Step 4: Manual QA**

Run the app (`open Tiller.xcodeproj`, ⌘R), open a Claude chat, send a prompt so the ring fills in:
- Hover the ring: tooltip still shows percent + tokens (unchanged).
- Click the ring: popover opens showing percent, tokens, cost (`Cost: $0.0X`), and the input/output/cache breakdown.
- Click outside the popover: it dismisses.
- Switch to a Codex or OpenCode chat, send a prompt: clicking the ring opens a popover with percent + tokens only, no cost/breakdown rows.
- Switch to a Pi/omp chat: the ring stays empty and inert (unchanged from today — click does nothing, matching the disabled-when-`usage == nil` state).

- [ ] **Step 5: Run the full gate**

Run: `Scripts/ci.sh`
Expected: prints `CI OK`.

- [ ] **Step 6: Commit**

```bash
git add App/Chat/ComposerControlBar.swift
git commit -m "feat: open a popover with context usage, cost, and token breakdown on ring click"
```
