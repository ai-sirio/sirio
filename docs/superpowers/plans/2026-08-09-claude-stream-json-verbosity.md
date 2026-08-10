# Claude stream-json verbosity Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Consume the parts of Claude Code's `stream-json` protocol that
`ClaudeStreamJSONDriver` currently drops — unanswered control requests,
unknown message types, discarded fields — and replace the driver's simulated
effort control with the CLI's real one.

**Architecture:** All work lands in `Packages/TillerACP`, which has no SwiftUI
dependency, so every new type is data. New wire types go in `ClaudeWire.swift`;
the driver translates them into existing canonical `SessionUpdate` cases where
possible. Exactly one new `SessionUpdate` case (`.notice`) is added; it maps to
the existing `TranscriptItem.systemNotice`, which is already rendered
(`App/Chat/TranscriptView.swift:190`) and already persisted
(`ChatSessionStore.swift:257`), so no new UI or persistence work is required.

**Tech Stack:** Swift 6, `swift-testing` (`@Test`/`#expect`), SwiftPM.

## Global Constraints

- Package: `Packages/TillerACP` only. Do not import SwiftUI or AppKit.
- Tests use `swift-testing` (`@Test`/`#expect`), never XCTest.
- Domain types are `struct`; the driver stays an `actor`.
- Iterate with `cd Packages/TillerACP && swift test`. Full gate is
  `Scripts/ci.sh`, which must print `CI OK`.
- **Baseline is red.** As of 2026-08-06 several suites fail before any change.
  Exonerate a diff by running the same tree twice, never by comparing to
  `HEAD`.
- Commit messages: Conventional Commits, lower-case imperative subject.
- Protocol facts are pinned to **Claude Code 2.1.226**. Unknown subtypes must
  degrade safely, never hang.
- **Test fixtures are inline JSON string literals with synthetic values**,
  following the existing style at `ClaudeWireTests.swift:60`. Do not commit
  captured live output: it contains account email, organization, internal MCP
  server names, and absolute paths with the username. Preserve the *shapes*
  observed on the wire (notably: empty `thinking` strings, and
  `supportsEffort` absent for haiku) with invented values.

---

### Task 1: Decode the dropped message types

**Files:**
- Modify: `Packages/TillerACP/Sources/TillerACP/Drivers/ClaudeWire.swift:4-42`
- Test: `Packages/TillerACP/Tests/TillerACPTests/ClaudeWireTests.swift`

**Interfaces:**
- Consumes: nothing (first task).
- Produces: new `ClaudeWireMessage` cases `.systemEvent(ClaudeSystemEvent)`,
  `.rateLimit(ClaudeRateLimit)`, `.promptSuggestion(String)`,
  `.streamEvent(JSONValue)`; and types `ClaudeSystemEvent` (fields:
  `subtype: String`, `payload: JSONValue`), `ClaudeRateLimit` (fields:
  `status: String`, `resetsAt: Int?`, `rateLimitType: String?`).
  Later tasks switch on these.

- [x] **Step 1: Write the failing test**

Add to `ClaudeWireTests.swift`, inside `@Suite struct ClaudeWireTests`:

```swift
    @Test func decodesSystemEventSubtypes() throws {
        let compact = Data(#"{"type":"system","subtype":"compact_boundary","session_id":"s1"}"#.utf8)
        guard case .systemEvent(let event) = try JSONDecoder()
            .decode(ClaudeWireMessage.self, from: compact) else {
            Issue.record("expected system event"); return
        }
        #expect(event.subtype == "compact_boundary")

        let thinking = Data(#"{"type":"system","subtype":"thinking_tokens","estimated_tokens":50,"estimated_tokens_delta":50}"#.utf8)
        guard case .systemEvent(let tokens) = try JSONDecoder()
            .decode(ClaudeWireMessage.self, from: thinking) else {
            Issue.record("expected system event"); return
        }
        #expect(tokens.subtype == "thinking_tokens")
        #expect(tokens.payload["estimated_tokens"]?.intValue == 50)
    }

    @Test func systemInitStillDecodesAsInit() throws {
        let data = Data(#"{"type":"system","subtype":"init","session_id":"s1","model":"m","tools":[],"slash_commands":[]}"#.utf8)
        guard case .systemInit = try JSONDecoder()
            .decode(ClaudeWireMessage.self, from: data) else {
            Issue.record("init must not be swallowed by systemEvent"); return
        }
    }

    @Test func decodesRateLimitAndPromptSuggestion() throws {
        let limit = Data(#"{"type":"rate_limit_event","rate_limit_info":{"status":"allowed","resetsAt":1786298400,"rateLimitType":"five_hour"}}"#.utf8)
        guard case .rateLimit(let info) = try JSONDecoder()
            .decode(ClaudeWireMessage.self, from: limit) else {
            Issue.record("expected rate limit"); return
        }
        #expect(info.status == "allowed")
        #expect(info.rateLimitType == "five_hour")

        let suggestion = Data(#"{"type":"prompt_suggestion","suggestion":"delete f.txt"}"#.utf8)
        guard case .promptSuggestion(let text) = try JSONDecoder()
            .decode(ClaudeWireMessage.self, from: suggestion) else {
            Issue.record("expected prompt suggestion"); return
        }
        #expect(text == "delete f.txt")
    }
```

- [x] **Step 2: Run test to verify it fails**

Run: `cd Packages/TillerACP && swift test --filter ClaudeWireTests`
Expected: FAIL — `.systemEvent`, `.rateLimit`, `.promptSuggestion` are not
members of `ClaudeWireMessage`.

- [x] **Step 3: Write minimal implementation**

In `ClaudeWire.swift`, add the two new types after `ClaudeInit`:

```swift
/// Any `type: "system"` line that is not the `init` handshake. The subtype
/// set is open and version-dependent, so the payload stays generic.
public struct ClaudeSystemEvent: Sendable, Equatable {
    public let subtype: String
    public let payload: JSONValue

    public init(subtype: String, payload: JSONValue) {
        self.subtype = subtype
        self.payload = payload
    }
}

public struct ClaudeRateLimit: Decodable, Sendable, Equatable {
    public let status: String
    public let resetsAt: Int?
    public let rateLimitType: String?

    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        status = try container.decodeIfPresent(String.self, forKey: .status) ?? ""
        resetsAt = try container.decodeIfPresent(Int.self, forKey: .resetsAt)
        rateLimitType = try container.decodeIfPresent(String.self, forKey: .rateLimitType)
    }

    enum CodingKeys: String, CodingKey {
        case status, resetsAt, rateLimitType
    }
}
```

Add the cases to `ClaudeWireMessage`:

```swift
    case systemEvent(ClaudeSystemEvent)
    case rateLimit(ClaudeRateLimit)
    case promptSuggestion(String)
    case streamEvent(JSONValue)
```

In `init(from:)`, the `system` case must stay ahead of the new catch-all so
`init` is not swallowed. Replace the `case "system" where ...` arm and add the
rest:

```swift
        case "system" where value["subtype"]?.stringValue == "init":
            self = .systemInit(try value.decoded(ClaudeInit.self))
        case "system":
            self = .systemEvent(ClaudeSystemEvent(
                subtype: value["subtype"]?.stringValue ?? "",
                payload: value))
        case "rate_limit_event":
            let info = value["rate_limit_info"] ?? value
            self = .rateLimit(try info.decoded(ClaudeRateLimit.self))
        case "prompt_suggestion":
            self = .promptSuggestion(value["suggestion"]?.stringValue ?? "")
        case "stream_event":
            self = .streamEvent(value["event"] ?? value)
```

- [x] **Step 4: Run test to verify it passes**

Run: `cd Packages/TillerACP && swift test --filter ClaudeWireTests`
Expected: PASS, including the pre-existing `unknownTypesDecodeAsUnknown`.

The driver's `handle(_:)` switch will now fail to compile because it is not
exhaustive. Add a temporary no-op arm so the package builds:

```swift
        case .systemEvent, .rateLimit, .promptSuggestion, .streamEvent:
            break
```

- [x] **Step 5: Commit**

```bash
git add Packages/TillerACP/Sources/TillerACP/Drivers/ClaudeWire.swift \
        Packages/TillerACP/Tests/TillerACPTests/ClaudeWireTests.swift
git commit -m "feat: decode claude system, rate limit and suggestion messages"
```

---

### Task 2: Answer every control request

**Files:**
- Modify: `Packages/TillerACP/Sources/TillerACP/Drivers/ClaudeStreamJSONDriver.swift:284-313`
- Test: `Packages/TillerACP/Tests/TillerACPTests/ClaudeDriverTests.swift`

**Interfaces:**
- Consumes: nothing from Task 1.
- Produces: private `sendControlResponse(id:payload:)` and
  `declineControlRequest(id:reason:)` on the driver; Task 6 does not use them,
  nothing else depends on them.

This is the correctness fix. The protocol correlates every `control_request`
to a `control_response` by `request_id`. Today anything that is not
`can_use_tool` hits `guard ... else { return }` and the CLI waits forever.

- [x] **Step 1: Write the failing test**

Add to `ClaudeDriverTests.swift`:

```swift
    @Test func unhandledControlRequestsStillGetAResponse() async throws {
        let mock = MockTransport()
        let driver = ClaudeStreamJSONDriver(transport: mock, permissionMode: .ask,
                                            model: nil, resumeSessionId: nil)
        let collector = EventCollector()
        let eventTask = collect(driver, into: collector)
        try await driver.start()
        _ = try await connectWithInitializeResponse(driver, mock: mock)

        await mock.emit(#"{"type":"control_request","request_id":"rq-9","request":{"subtype":"request_user_dialog"}}"#)

        let sent = try await mock.waitForSent(count: 2)
        let reply = try jsonValue(sent[1])
        #expect(reply["type"]?.stringValue == "control_response")
        #expect(reply["response"]?["request_id"]?.stringValue == "rq-9")
        #expect(reply["response"]?["subtype"]?.stringValue == "error")

        eventTask.cancel()
        await driver.stop()
    }
```

- [x] **Step 2: Run test to verify it fails**

Run: `cd Packages/TillerACP && swift test --filter unhandledControlRequestsStillGetAResponse`
Expected: FAIL — `waitForSent(count: 2)` times out and returns 1 line, so the
subscript traps or the expectations fail. Nothing is ever sent back.

- [x] **Step 3: Write minimal implementation**

In `ClaudeStreamJSONDriver.swift`, add the two helpers near
`answerPermission`:

```swift
    private func sendControlResponse(id: String, payload: JSONValue) async {
        let response = JSONValue.object([
            "type": .string("control_response"),
            "response": payload
        ])
        try? await transport.send(line: makeLine(response))
    }

    /// Every control request must be answered, including ones this driver
    /// does not implement: the CLI correlates by `request_id` and blocks
    /// until it hears back. An explicit refusal is an answer; silence is not.
    private func declineControlRequest(id: String, reason: String) {
        Task { [weak self] in
            await self?.sendControlResponse(id: id, payload: .object([
                "request_id": .string(id),
                "subtype": .string("error"),
                "error": .string(reason)
            ]))
        }
    }
```

Replace the `guard` at the head of the `.controlRequest` arm in `handle(_:)`:

```swift
        case .controlRequest(let id, let request):
            guard request.subtype == "can_use_tool" else {
                declineControlRequest(
                    id: id,
                    reason: "Tiller does not implement \(request.subtype)")
                return
            }
```

- [x] **Step 4: Run test to verify it passes**

Run: `cd Packages/TillerACP && swift test --filter ClaudeDriverTests`
Expected: PASS, and no pre-existing driver test regresses.

- [x] **Step 5: Commit**

```bash
git add Packages/TillerACP/Sources/TillerACP/Drivers/ClaudeStreamJSONDriver.swift \
        Packages/TillerACP/Tests/TillerACPTests/ClaudeDriverTests.swift
git commit -m "fix: answer every claude control request instead of dropping unknown subtypes"
```

---

### Task 3: Stop latching the slash-command list

**Files:**
- Modify: `Packages/TillerACP/Sources/TillerACP/Drivers/ClaudeStreamJSONDriver.swift:49,244-254,405-414`
- Test: `Packages/TillerACP/Tests/TillerACPTests/ClaudeDriverTests.swift`

**Interfaces:**
- Consumes: `ClaudeWireMessage.systemEvent` from Task 1 is *not* used here —
  `commands_changed` is a top-level type, handled via the existing
  `.unknown` path being replaced.
- Produces: nothing later tasks depend on.

`didEmitCommands` (`:49`) fires once per session. After a `reload_plugins` or
a skill install the command list is stale for the rest of the session.

**Correction, 2026-08-09, after a live probe of CLI 2.1.226.** The latch was
doing two jobs, and this task originally described only one of them. The
command list arrives twice, from sources of unequal quality:

| Source | Order | Payload |
|---|---|---|
| `initialize` control response | first | 266 commands as objects, all with a non-empty `description` |
| `system` / `init` stream message | second | 263 commands as bare strings → `description: ""` |

Removing the latch alone lets the second, impoverished source overwrite the
first two lines after it arrives, losing all 266 descriptions and 3 commands
every session. So this task also **stops `.systemInit` from emitting
`availableCommandsUpdate` at all**; refreshes come from `commands_changed`,
which carries the rich payload. Do not re-add a latch of any kind — suppressing
the poor source is not the same as freezing the good one.

- [x] **Step 1: Write the failing test**

```swift
    @Test func commandsChangedRefreshesTheCommandList() async throws {
        let mock = MockTransport()
        let driver = ClaudeStreamJSONDriver(transport: mock, permissionMode: .ask,
                                            model: nil, resumeSessionId: nil)
        let collector = EventCollector()
        let eventTask = collect(driver, into: collector)
        try await driver.start()
        _ = try await connectWithInitializeResponse(driver, mock: mock)

        await mock.emit(#"{"type":"commands_changed","commands":[{"name":"newcmd","description":"Added later"}]}"#)

        let events = await waitForEvent(collector) { event in
            guard case .update(.availableCommandsUpdate(let commands)) = event else { return false }
            return commands.contains { $0.name == "newcmd" }
        }
        #expect(events.contains { event in
            guard case .update(.availableCommandsUpdate(let commands)) = event else { return false }
            return commands.contains { $0.name == "newcmd" }
        })

        eventTask.cancel()
        await driver.stop()
    }
```

- [x] **Step 2: Run test to verify it fails**

Run: `cd Packages/TillerACP && swift test --filter commandsChangedRefreshesTheCommandList`
Expected: FAIL — `commands_changed` decodes as `.unknown` and is ignored, so
no second `availableCommandsUpdate` is ever emitted.

- [x] **Step 3: Write minimal implementation**

In `ClaudeWire.swift` `init(from:)`, add:

```swift
        case "commands_changed":
            self = .commandsChanged(value["commands"]?.arrayValue ?? [])
```

and the case:

```swift
    case commandsChanged([JSONValue])
```

In `ClaudeStreamJSONDriver.swift`, delete the `didEmitCommands` property at
`:49` and both of its guards. In `handle(_:)`'s `.systemInit` arm, emit
unconditionally:

```swift
        case .systemInit(let initMessage):
            if !initMessage.sessionId.isEmpty {
                sessionId = initMessage.sessionId
            }
            eventContinuation.yield(.update(.availableCommandsUpdate(
                initMessage.slashCommands.map {
                    AvailableCommand(name: $0, description: "")
                })))
```

Add the new arm:

```swift
        case .commandsChanged(let values):
            eventContinuation.yield(.update(.availableCommandsUpdate(
                commands(from: values))))
```

Change `emitAvailableCommands(from:)` to drop its `didEmitCommands` guard and
extract the mapping so both call sites share it:

```swift
    private func commands(from values: [JSONValue]) -> [AvailableCommand] {
        values.compactMap { value in
            guard let name = value["name"]?.stringValue else { return nil }
            return AvailableCommand(name: name,
                                    description: value["description"]?.stringValue ?? "")
        }
    }

    private func emitAvailableCommands(from payload: JSONValue?) {
        guard let values = payload?["commands"]?.arrayValue else { return }
        eventContinuation.yield(.update(.availableCommandsUpdate(commands(from: values))))
    }
```

- [x] **Step 4: Run test to verify it passes**

Run: `cd Packages/TillerACP && swift test --filter ClaudeDriverTests`
Expected: PASS. `fixtureTurnMapsToCanonicalUpdates` asserts
`availableCommandsUpdate` *contains* the doctor command, so a second emission
does not break it.

- [x] **Step 5: Commit**

```bash
git add Packages/TillerACP/Sources/TillerACP
git commit -m "fix: refresh claude slash commands instead of latching the first list"
```

---

### Task 4: Read the real context window from the result message

**Files:**
- Modify: `Packages/TillerACP/Sources/TillerACP/Drivers/ClaudeWire.swift:292-309`
- Modify: `Packages/TillerACP/Sources/TillerACP/Drivers/ClaudeStreamJSONDriver.swift:273-282,376-386`
- Test: `Packages/TillerACP/Tests/TillerACPTests/ClaudeDriverTests.swift`

**Interfaces:**
- Consumes: nothing.
- Produces: `ClaudeResult.modelUsage: JSONValue?` and
  `ClaudeResult.totalCostUsd/durationMs/ttftMs/numTurns/subtype/permissionDenials`,
  all read by Task 8.

The `result` message carries `modelUsage[model].contextWindow`. Today the
driver instead fires a `get_context_usage` control request after every turn
(`:376`). Keep that as a fallback for older CLIs; stop treating it as primary.

- [x] **Step 1: Write the failing test**

```swift
    @Test func contextUsageComesFromModelUsageWithoutProbing() async throws {
        let mock = MockTransport()
        let driver = ClaudeStreamJSONDriver(transport: mock, permissionMode: .ask,
                                            model: nil, resumeSessionId: nil)
        let collector = EventCollector()
        let eventTask = collect(driver, into: collector)
        try await driver.start()
        _ = try await connectWithInitializeResponse(driver, mock: mock)

        let promptTask = Task { try await driver.prompt([.text("hi")]) }
        _ = try await mock.waitForSent(count: 2)
        await mock.emit(#"{"type":"result","subtype":"success","is_error":false,"session_id":"s1","usage":{"input_tokens":4,"output_tokens":123,"cache_read_input_tokens":83967},"modelUsage":{"claude-sonnet-5":{"contextWindow":1000000}}}"#)
        _ = try await promptTask.value

        let events = await waitForEvent(collector) { event in
            if case .update(.usageUpdate) = event { return true }
            return false
        }
        #expect(events.contains { event in
            guard case .update(.usageUpdate(let usage)) = event else { return false }
            return usage.size == 1_000_000 && usage.used == 4 + 123 + 83967
        })

        eventTask.cancel()
        await driver.stop()
    }
```

- [x] **Step 2: Run test to verify it fails**

Run: `cd Packages/TillerACP && swift test --filter contextUsageComesFromModelUsageWithoutProbing`
Expected: FAIL — the only `usageUpdate` path today is the async
`get_context_usage` probe, which the mock never answers.

- [x] **Step 3: Write minimal implementation**

In `ClaudeWire.swift`, extend `ClaudeResult` with the fields Task 8 also
needs:

```swift
public struct ClaudeResult: Decodable, Sendable, Equatable {
    public let sessionId: String
    public let usage: ClaudeUsage?
    public let isError: Bool
    public let subtype: String?
    public let totalCostUsd: Double?
    public let durationMs: Int?
    public let ttftMs: Int?
    public let numTurns: Int?
    public let modelUsage: JSONValue?
    public let permissionDenials: [JSONValue]?

    enum CodingKeys: String, CodingKey {
        case sessionId = "session_id"
        case usage
        case isError = "is_error"
        case subtype
        case totalCostUsd = "total_cost_usd"
        case durationMs = "duration_ms"
        case ttftMs = "ttft_ms"
        case numTurns = "num_turns"
        case modelUsage
        case permissionDenials = "permission_denials"
    }

    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        sessionId = try container.decodeIfPresent(String.self, forKey: .sessionId) ?? ""
        usage = try container.decodeIfPresent(ClaudeUsage.self, forKey: .usage)
        isError = try container.decodeIfPresent(Bool.self, forKey: .isError) ?? false
        subtype = try container.decodeIfPresent(String.self, forKey: .subtype)
        totalCostUsd = try container.decodeIfPresent(Double.self, forKey: .totalCostUsd)
        durationMs = try container.decodeIfPresent(Int.self, forKey: .durationMs)
        ttftMs = try container.decodeIfPresent(Int.self, forKey: .ttftMs)
        numTurns = try container.decodeIfPresent(Int.self, forKey: .numTurns)
        modelUsage = try container.decodeIfPresent(JSONValue.self, forKey: .modelUsage)
        permissionDenials = try container.decodeIfPresent([JSONValue].self,
                                                          forKey: .permissionDenials)
    }
}
```

In `ClaudeStreamJSONDriver.swift`, add the extraction and use it before
falling back to the probe:

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
        return ContextUsage(used: used, size: size)
    }
```

Replace the `.result` arm's probe call:

```swift
        case .result(let result):
            if let usage = contextUsage(from: result) {
                eventContinuation.yield(.update(.usageUpdate(usage)))
            } else {
                Task { [weak self] in await self?.probeContextUsage() }
            }
```

- [x] **Step 4: Run test to verify it passes**

Run: `cd Packages/TillerACP && swift test --filter ClaudeDriverTests`
Expected: PASS. `fixtureTurnMapsToCanonicalUpdates` asserts *no*
`usageUpdate`; its fixture result has no `modelUsage`, so it still takes the
probe path and stays green.

- [x] **Step 5: Commit**

```bash
git add Packages/TillerACP/Sources/TillerACP
git commit -m "feat: read claude context window from result modelUsage"
```

---

### Task 5: Decode per-model effort capabilities

**Files:**
- Modify: `Packages/TillerACP/Sources/TillerACP/ACPTypes.swift:127-136`
- Modify: `Packages/TillerACP/Sources/TillerACP/Drivers/ClaudeStreamJSONDriver.swift:416-431`
- Test: `Packages/TillerACP/Tests/TillerACPTests/ClaudeDriverTests.swift`

**Interfaces:**
- Consumes: nothing.
- Produces: `ModelInfo.supportedEffortLevels: [String]?` — Task 6 reads it to
  build the effort option list. `nil` means the model reports no effort
  support and no picker should be offered.

The `initialize` response reports capabilities per model. Verified shape:
`default`, `opus[1m]`, `claude-fable-5[1m]` and `sonnet` report five levels;
**`haiku` reports no effort support at all.**

- [x] **Step 1: Write the failing test**

```swift
    @Test func modelsCarryTheirEffortLevels() async throws {
        let mock = MockTransport()
        let driver = ClaudeStreamJSONDriver(transport: mock, permissionMode: .ask,
                                            model: "sonnet", resumeSessionId: nil)
        try await driver.start()

        let connectTask = Task {
            try await driver.connect(cwd: "/tmp/w", resumeSessionId: nil, mcpServers: [])
        }
        let sent = try await mock.waitForSent(count: 1)
        let requestId = try jsonValue(sent[0])["request_id"]?.stringValue ?? ""
        await mock.emit("""
        {"type":"control_response","response":{"subtype":"success","request_id":"\(requestId)","response":{"models":[\
        {"value":"sonnet","displayName":"Sonnet","supportsEffort":true,"supportedEffortLevels":["low","medium","high","xhigh","max"]},\
        {"value":"haiku","displayName":"Haiku"}]}}}
        """)
        let handle = try await connectTask.value

        let models = handle.models?.availableModels ?? []
        #expect(models.first { $0.modelId == "sonnet" }?.supportedEffortLevels
            == ["low", "medium", "high", "xhigh", "max"])
        #expect(models.first { $0.modelId == "haiku" }?.supportedEffortLevels == nil)

        await driver.stop()
    }
```

- [x] **Step 2: Run test to verify it fails**

Run: `cd Packages/TillerACP && swift test --filter modelsCarryTheirEffortLevels`
Expected: FAIL — `ModelInfo` has no `supportedEffortLevels` member.

- [x] **Step 3: Write minimal implementation**

In `ACPTypes.swift`, extend `ModelInfo`. Keep the existing initialiser
signature working by defaulting the new parameter:

```swift
public struct ModelInfo: Sendable, Equatable, Codable {
    public var modelId: String
    public var name: String
    public var description: String?
    /// Effort levels this model accepts, as reported by the agent. `nil`
    /// means the model reports no effort support and no picker is offered.
    public var supportedEffortLevels: [String]?

    public init(modelId: String, name: String, description: String? = nil,
                supportedEffortLevels: [String]? = nil) {
        self.modelId = modelId
        self.name = name
        self.description = description
        self.supportedEffortLevels = supportedEffortLevels
    }
}
```

In `ClaudeStreamJSONDriver.modelState(from:)`, populate it:

```swift
            let levels: [String]? = value["supportsEffort"]?.boolValue == true
                ? value["supportedEffortLevels"]?.arrayValue?.compactMap(\.stringValue)
                : nil
            return ModelInfo(modelId: modelId, name: name,
                             description: value["description"]?.stringValue,
                             supportedEffortLevels: levels)
```

- [x] **Step 4: Run test to verify it passes**

Run: `cd Packages/TillerACP && swift test --filter ClaudeDriverTests`
Expected: PASS.

- [x] **Step 5: Commit**

```bash
git add Packages/TillerACP/Sources/TillerACP
git commit -m "feat: decode per-model effort capabilities from claude initialize"
```

---

### Task 6: Replace the simulated effort with the real one

**Files:**
- Modify: `Packages/TillerACP/Sources/TillerACP/Drivers/ClaudeStreamJSONDriver.swift:70-91,182-197,482-496,524-526`
- Test: `Packages/TillerACP/Tests/TillerACPTests/ClaudeDriverTests.swift`

**Interfaces:**
- Consumes: `ModelInfo.supportedEffortLevels` (Task 5).
- Produces: nothing later tasks depend on.

Today `setEffort` stores a string and `promptBlocks` prepends *"Reasoning
effort for this and following turns: high."* to the user's prompt. That is a
request to the model, not a setting.

**Critical constraint:** `apply_flag_settings` performs **no validation** — a
request carrying `{"effort":"bogus"}` returns `subtype: "success"` exactly
like a valid one. `success` means *received*, not *applied*. Tiller is the
only place a bad value can be caught, so validation happens before sending.

**Live implementation correction:** `sendControlRequest` waits for the
correlated control response, while the test fixture intentionally does not
send one. `setEffort` therefore schedules the already-validated request and
returns after the line is queued; the driver still records the selected value
locally and does not claim that Claude applied it.

The launch call site has also moved from the plan's `App/` location to
`Packages/TillerACP/Sources/TillerACP/AgentDriverFactory.swift`; the package
caller now passes the stored effort to `launchTransport`.

- [x] **Step 1: Write the failing test**

```swift
    @Test func effortIsSentAsAFlagSettingAndValidated() async throws {
        let mock = MockTransport()
        let driver = ClaudeStreamJSONDriver(transport: mock, permissionMode: .ask,
                                            model: "sonnet", resumeSessionId: nil)
        try await driver.start()

        let connectTask = Task {
            try await driver.connect(cwd: "/tmp/w", resumeSessionId: nil, mcpServers: [])
        }
        let sent = try await mock.waitForSent(count: 1)
        let requestId = try jsonValue(sent[0])["request_id"]?.stringValue ?? ""
        await mock.emit("""
        {"type":"control_response","response":{"subtype":"success","request_id":"\(requestId)","response":{"models":[\
        {"value":"sonnet","displayName":"Sonnet","supportsEffort":true,"supportedEffortLevels":["low","medium","high","xhigh","max"]}]}}}
        """)
        _ = try await connectTask.value

        await driver.setEffort("xhigh")
        let afterValid = try await mock.waitForSent(count: 2)
        let request = try jsonValue(afterValid[1])
        #expect(request["request"]?["subtype"]?.stringValue == "apply_flag_settings")
        #expect(request["request"]?["settings"]?["effort"]?.stringValue == "xhigh")

        // Rejected before it reaches the wire: the CLI would answer "success".
        await driver.setEffort("bogus_level")
        #expect(await mock.sent.count == 2)

        let options = await driver.staticEffortOptions()
        #expect(options?.options?.map(\.value)
            == ["low", "medium", "high", "xhigh", "max"])
        #expect(options?.currentValue == "xhigh")

        await driver.stop()
    }

    @Test func modelWithoutEffortSupportOffersNoOptions() async throws {
        let mock = MockTransport()
        let driver = ClaudeStreamJSONDriver(transport: mock, permissionMode: .ask,
                                            model: "haiku", resumeSessionId: nil)
        try await driver.start()

        let connectTask = Task {
            try await driver.connect(cwd: "/tmp/w", resumeSessionId: nil, mcpServers: [])
        }
        let sent = try await mock.waitForSent(count: 1)
        let requestId = try jsonValue(sent[0])["request_id"]?.stringValue ?? ""
        await mock.emit("""
        {"type":"control_response","response":{"subtype":"success","request_id":"\(requestId)","response":{"models":[\
        {"value":"haiku","displayName":"Haiku"}]}}}
        """)
        _ = try await connectTask.value

        #expect(await driver.staticEffortOptions() == nil)
        await driver.stop()
    }

    @Test func promptCarriesNoEffortPrefix() async throws {
        let mock = MockTransport()
        let driver = ClaudeStreamJSONDriver(transport: mock, permissionMode: .ask,
                                            model: nil, resumeSessionId: nil,
                                            effort: "high")
        try await driver.start()
        _ = try await connectWithInitializeResponse(driver, mock: mock)

        let promptTask = Task { try await driver.prompt([.text("do the thing")]) }
        let sent = try await mock.waitForSent(count: 2)
        let text = try jsonValue(sent[1])["message"]?["content"]?
            .arrayValue?.first?["text"]?.stringValue
        #expect(text == "do the thing")

        await mock.emit(#"{"type":"result","subtype":"success","is_error":false,"session_id":"s1"}"#)
        _ = try await promptTask.value
        await driver.stop()
    }
```

- [x] **Step 2: Run test to verify it fails**

Run: `cd Packages/TillerACP && swift test --filter ClaudeDriverTests`
Expected: FAIL on all three — `setEffort` sends nothing,
`staticEffortOptions` returns a hardcoded three-level list for every model
including haiku, and the prompt text is prefixed.

- [x] **Step 3: Write minimal implementation**

Add stored capability state to the driver, next to `effort`:

```swift
    private var effortLevels: [String] = []
```

In `makeHandle(from:sessionId:didResume:)`, after computing `models`, record
the selected model's levels:

```swift
        let models = modelState(from: payload)
        let selected = requestedModel ?? models?.currentModelId
        effortLevels = models?.availableModels
            .first { $0.modelId == selected }?.supportedEffortLevels ?? []
```

and pass `models` into the `SessionHandle` as before.

Replace the hardcoded list at `:188` and `staticEffortOptions()`:

```swift
    /// Effort levels are reported per model by the agent, not fixed: on
    /// 2.1.226 most models accept five, and haiku reports none at all.
    public func staticEffortOptions() async -> SessionConfigOption? {
        guard !effortLevels.isEmpty else { return nil }
        return SessionConfigOption(
            id: "effort", name: "Effort", currentValue: effort,
            options: effortLevels.map {
                SessionConfigOption.Choice(value: $0, name: $0.capitalized)
            })
    }
```

Replace `setEffort`:

```swift
    /// `apply_flag_settings` answers `success` for any value, valid or not,
    /// so an unsupported level must be rejected here — this is the last
    /// point in the chain where a bad value is still visible.
    public func setEffort(_ newEffort: String?) async {
        guard let newEffort else {
            effort = nil
            return
        }
        guard effortLevels.contains(newEffort) else { return }
        effort = newEffort
        _ = try? await sendControlRequest(.object([
            "subtype": .string("apply_flag_settings"),
            "settings": .object(["effort": .string(newEffort)])
        ]))
    }
```

Delete `effortPrefix(_:)` at `:524` and reduce `promptBlocks` to the identity
it now is — remove the function and call `wireBlocks(blocks)` directly in
`makeUserPromptLine`:

```swift
    private func makeUserPromptLine(_ blocks: [ContentBlock]) throws -> Data {
        let content = JSONValue.array(wireBlocks(blocks))
        return makeLine(.object([
            "type": .string("user"),
            "message": .object([
                "role": .string("user"),
                "content": content
            ])
        ]))
    }
```

Add the launch flag in `launchTransport`, after the `--model` block:

```swift
        if let effort {
            command += " --effort \(shellArgument(effort))"
        }
```

and add `effort: String? = nil` to `launchTransport`'s parameter list, placed
after `model`. Update its call site in `App/` to pass the stored preference;
find it with:

```bash
rg -n "launchTransport" App/
```

- [x] **Step 4: Run test to verify it passes**

Run: `cd Packages/TillerACP && swift test --filter ClaudeDriverTests`
Expected: PASS. Then build the app target to catch the `launchTransport`
call-site change.

- [x] **Step 5: Commit**

```bash
git add Packages/TillerACP App
git commit -m "feat: use claude native effort flag instead of a prompt prefix"
```

---

### Task 7: Add the `.notice` session update

**Files:**
- Modify: `Packages/TillerACP/Sources/TillerACP/SessionUpdate.swift:31-41,55-73`
- Modify: `Packages/TillerACP/Sources/TillerACP/TranscriptReducer.swift:94`
- Test: `Packages/TillerACP/Tests/TillerACPTests/TranscriptReducerTests.swift`

**Interfaces:**
- Consumes: nothing.
- Produces: `SessionUpdate.notice(String)`. Tasks 8 and 9 emit it. The reducer
  turns it into `TranscriptItem.systemNotice`, which is already rendered at
  `App/Chat/TranscriptView.swift:190` and already persisted at
  `ChatSessionStore.swift:257` — so this one case covers all inline output in
  this plan with no new UI.

- [x] **Step 1: Write the failing test**

Add to `TranscriptReducerTests.swift` (create the file with this content if it
does not exist, using the same `@Suite struct` style as the other suites):

```swift
    @Test func noticeBecomesASystemNoticeItem() {
        var reducer = TranscriptReducer()
        reducer.apply(.notice("Context compacted"))
        #expect(reducer.items.contains { item in
            guard case .systemNotice(_, let text) = item else { return false }
            return text == "Context compacted"
        })
    }
```

- [x] **Step 2: Run test to verify it fails**

Run: `cd Packages/TillerACP && swift test --filter noticeBecomesASystemNoticeItem`
Expected: FAIL — `.notice` is not a member of `SessionUpdate`.

- [x] **Step 3: Write minimal implementation**

In `SessionUpdate.swift`, add the case to the enum:

```swift
    /// Driver-synthesised inline notice (compaction, model fallback, turn
    /// stats). Never sent by the ACP wire; carried here so every driver can
    /// surface one through the same path.
    case notice(String)
```

The `Decodable` conformance switches on a wire discriminator that never
carries this case, so no decoding arm is needed — but the compiler does not
require one either, since `default` already maps to `.unknown`.

In `TranscriptReducer.apply(_:)`, add an arm:

```swift
        case .notice(let text):
            closeOpenStreams()
            items.append(.systemNotice(id: makeId("notice"), text: text))
```

- [x] **Step 4: Run test to verify it passes**

Run: `cd Packages/TillerACP && swift test`
Expected: PASS. Other `switch`es over `SessionUpdate` may now be non-exhaustive;
build errors will name them. Handle `.notice` explicitly where a case is
required rather than adding a `default`.

- [x] **Step 5: Commit**

```bash
git add Packages/TillerACP
git commit -m "feat: add notice session update mapped to system notice items"
```

---

### Task 8: Emit turn stats and inline notices

**Files:**
- Modify: `Packages/TillerACP/Sources/TillerACP/Drivers/ClaudeStreamJSONDriver.swift` (`.result`, `.systemEvent`, `.rateLimit` arms)
- Test: `Packages/TillerACP/Tests/TillerACPTests/ClaudeDriverTests.swift`

**Interfaces:**
- Consumes: `SessionUpdate.notice` (Task 7); `ClaudeResult.totalCostUsd`,
  `durationMs`, `ttftMs`, `numTurns`, `permissionDenials` (Task 4);
  `ClaudeSystemEvent`, `ClaudeRateLimit` (Task 1).
- Produces: nothing later tasks depend on.

- [x] **Step 1: Write the failing test**

```swift
    @Test func resultEmitsTurnStats() async throws {
        let mock = MockTransport()
        let driver = ClaudeStreamJSONDriver(transport: mock, permissionMode: .ask,
                                            model: nil, resumeSessionId: nil)
        let collector = EventCollector()
        let eventTask = collect(driver, into: collector)
        try await driver.start()
        _ = try await connectWithInitializeResponse(driver, mock: mock)

        let promptTask = Task { try await driver.prompt([.text("hi")]) }
        _ = try await mock.waitForSent(count: 2)
        await mock.emit(#"{"type":"result","subtype":"success","is_error":false,"session_id":"s1","total_cost_usd":0.408,"duration_ms":11672,"ttft_ms":3794,"num_turns":2,"usage":{"input_tokens":4,"output_tokens":123}}"#)
        _ = try await promptTask.value

        let events = await waitForEvent(collector) { event in
            if case .update(.notice) = event { return true }
            return false
        }
        let notice = events.compactMap { event -> String? in
            guard case .update(.notice(let text)) = event else { return nil }
            return text
        }.first
        #expect(notice?.contains("$0.41") == true)
        #expect(notice?.contains("11.7s") == true)
        #expect(notice?.contains("123") == true)

        eventTask.cancel()
        await driver.stop()
    }

    @Test func compactionAndRateLimitBecomeNotices() async throws {
        let mock = MockTransport()
        let driver = ClaudeStreamJSONDriver(transport: mock, permissionMode: .ask,
                                            model: nil, resumeSessionId: nil)
        let collector = EventCollector()
        let eventTask = collect(driver, into: collector)
        try await driver.start()
        _ = try await connectWithInitializeResponse(driver, mock: mock)

        await mock.emit(#"{"type":"system","subtype":"compact_boundary","session_id":"s1"}"#)
        await mock.emit(#"{"type":"rate_limit_event","rate_limit_info":{"status":"allowed","rateLimitType":"five_hour"}}"#)
        await mock.emit(#"{"type":"rate_limit_event","rate_limit_info":{"status":"rejected","rateLimitType":"five_hour"}}"#)

        _ = await waitForEvent(collector) { event in
            guard case .update(.notice(let text)) = event else { return false }
            return text.lowercased().contains("rate limit")
        }
        let notices = await collector.snapshot().compactMap { event -> String? in
            guard case .update(.notice(let text)) = event else { return nil }
            return text
        }
        #expect(notices.contains { $0.lowercased().contains("compact") })
        // "allowed" is the normal state and must stay silent.
        #expect(notices.filter { $0.lowercased().contains("rate limit") }.count == 1)

        eventTask.cancel()
        await driver.stop()
    }
```

- [x] **Step 2: Run test to verify it fails**

Run: `cd Packages/TillerACP && swift test --filter ClaudeDriverTests`
Expected: FAIL — no `.notice` is ever emitted; `systemEvent` and `rateLimit`
hit the temporary `break` added in Task 1.

- [x] **Step 3: Write minimal implementation**

Add the formatter and the emitters to the driver:

```swift
    private func turnStatsNotice(_ result: ClaudeResult) -> String? {
        var parts: [String] = []
        if let duration = result.durationMs {
            parts.append(String(format: "%.1fs", Double(duration) / 1000))
        }
        if let ttft = result.ttftMs {
            parts.append(String(format: "TTFT %.1fs", Double(ttft) / 1000))
        }
        if let cost = result.totalCostUsd {
            parts.append(String(format: "$%.2f", cost))
        }
        if let usage = result.usage {
            parts.append("\(usage.inputTokens ?? 0)↑ \(usage.outputTokens ?? 0)↓")
        }
        if let turns = result.numTurns, turns > 1 {
            parts.append("\(turns) turns")
        }
        return parts.isEmpty ? nil : parts.joined(separator: " · ")
    }

    private func emitNotice(_ text: String) {
        eventContinuation.yield(.update(.notice(text)))
    }
```

In the `.result` arm, after the context-usage handling:

```swift
            if let stats = turnStatsNotice(result) { emitNotice(stats) }
            if let denials = result.permissionDenials, !denials.isEmpty {
                emitNotice("\(denials.count) permission request(s) denied")
            }
```

Replace the temporary `break` from Task 1 with real arms:

```swift
        case .systemEvent(let event):
            switch event.subtype {
            case "compact_boundary":
                emitNotice("Context compacted")
            case "model_fallback", "model_refusal_fallback",
                 "model_consent_fallback", "model_refusal_no_fallback":
                let model = event.payload["model"]?.stringValue ?? "another model"
                emitNotice("Model fell back to \(model)")
            default:
                break
            }

        case .rateLimit(let info):
            guard info.status != "allowed" else { return }
            emitNotice("Rate limit \(info.status) (\(info.rateLimitType ?? "unknown"))")

        case .promptSuggestion, .streamEvent, .commandsChanged:
            break
```

(`.commandsChanged` already has its own arm from Task 3; keep that one and
leave only `.promptSuggestion, .streamEvent` here.)

- [x] **Step 4: Run test to verify it passes**

Run: `cd Packages/TillerACP && swift test --filter ClaudeDriverTests`
Expected: PASS.

- [x] **Step 5: Commit**

```bash
git add Packages/TillerACP
git commit -m "feat: surface claude turn stats, compaction and rate limits inline"
```

---

### Task 9: Wire the diagnostic signals to the package edge

**Files:**
- Create: `Packages/TillerACP/Sources/TillerACP/Drivers/ClaudeDiagnostics.swift`
- Modify: `Packages/TillerACP/Sources/TillerACP/Drivers/ClaudeStreamJSONDriver.swift` (`.systemEvent` arm, `makeHandle`)
- Test: `Packages/TillerACP/Tests/TillerACPTests/ClaudeDiagnosticsTests.swift`

**Interfaces:**
- Consumes: `ClaudeSystemEvent` (Task 1).
- Produces: `ClaudeDiagnostic` (fields: `kind: Kind`, `label: String`,
  `detail: String?`) and `ClaudeDiagnostics.diagnostic(for:) -> ClaudeDiagnostic?`.
  No UI consumes these yet — the panel is a separate piece of work.

Wiring only, by explicit scope decision. The signals reach the edge of
`TillerACP` and stop there.

- [x] **Step 1: Write the failing test**

Create `ClaudeDiagnosticsTests.swift`:

```swift
import Testing
import Foundation
@testable import TillerACP

@Suite struct ClaudeDiagnosticsTests {
    private func event(_ json: String) throws -> ClaudeSystemEvent {
        guard case .systemEvent(let event) = try JSONDecoder()
            .decode(ClaudeWireMessage.self, from: Data(json.utf8)) else {
            throw DecodingError.dataCorrupted(
                .init(codingPath: [], debugDescription: "not a system event"))
        }
        return event
    }

    @Test func mapsHookLifecycleToDiagnostics() throws {
        let started = try event(#"{"type":"system","subtype":"hook_started","hook_name":"SessionStart:startup"}"#)
        let diagnostic = ClaudeDiagnostics.diagnostic(for: started)
        #expect(diagnostic?.kind == .hook)
        #expect(diagnostic?.label == "SessionStart:startup")
    }

    @Test func mapsThinkingTokens() throws {
        let tokens = try event(#"{"type":"system","subtype":"thinking_tokens","estimated_tokens":86,"estimated_tokens_delta":36}"#)
        let diagnostic = ClaudeDiagnostics.diagnostic(for: tokens)
        #expect(diagnostic?.kind == .thinkingTokens)
        #expect(diagnostic?.detail == "86")
    }

    @Test func ignoresSubtypesRenderedInline() throws {
        let compact = try event(#"{"type":"system","subtype":"compact_boundary"}"#)
        #expect(ClaudeDiagnostics.diagnostic(for: compact) == nil)
    }
}
```

- [x] **Step 2: Run test to verify it fails**

Run: `cd Packages/TillerACP && swift test --filter ClaudeDiagnosticsTests`
Expected: FAIL — no such type `ClaudeDiagnostics`.

- [x] **Step 3: Write minimal implementation**

Create `ClaudeDiagnostics.swift`:

```swift
import Foundation

/// A non-conversational signal from the agent: hook lifecycle, request
/// status, thinking-token counters, background task progress. These reach
/// the edge of TillerACP and are not rendered in the transcript; the
/// diagnostic panel that consumes them is separate work.
public struct ClaudeDiagnostic: Sendable, Equatable {
    public enum Kind: String, Sendable, Equatable {
        case hook
        case status
        case thinkingTokens
        case taskProgress
        case informational
    }

    public let kind: Kind
    public let label: String
    public let detail: String?

    public init(kind: Kind, label: String, detail: String? = nil) {
        self.kind = kind
        self.label = label
        self.detail = detail
    }
}

public enum ClaudeDiagnostics {
    /// Returns nil for subtypes the transcript already renders inline, so a
    /// signal is never reported twice.
    public static func diagnostic(for event: ClaudeSystemEvent) -> ClaudeDiagnostic? {
        switch event.subtype {
        case "hook_started", "hook_progress", "hook_response":
            return ClaudeDiagnostic(
                kind: .hook,
                label: event.payload["hook_name"]?.stringValue ?? event.subtype,
                detail: event.subtype)
        case "status":
            return ClaudeDiagnostic(
                kind: .status,
                label: event.payload["status"]?.stringValue ?? "status")
        case "thinking_tokens":
            let total = event.payload["estimated_tokens"]?.intValue
            return ClaudeDiagnostic(
                kind: .thinkingTokens,
                label: "thinking",
                detail: total.map(String.init))
        case "task_progress":
            return ClaudeDiagnostic(
                kind: .taskProgress,
                label: event.payload["task_id"]?.stringValue ?? "task",
                detail: event.payload["summary"]?.stringValue)
        case "informational", "notification":
            return ClaudeDiagnostic(
                kind: .informational,
                label: event.subtype,
                detail: event.payload["message"]?.stringValue)
        default:
            return nil
        }
    }
}
```

In the driver's `.systemEvent` arm, add a `default` branch that routes through
the mapper. Store the most recent diagnostics on the actor so a future panel
has a source, without adding an event type:

```swift
    private(set) var diagnostics: [ClaudeDiagnostic] = []

    private func record(_ diagnostic: ClaudeDiagnostic) {
        diagnostics.append(diagnostic)
        if diagnostics.count > 500 { diagnostics.removeFirst(diagnostics.count - 500) }
    }
```

and in the `default:` of the `.systemEvent` switch:

```swift
            default:
                if let diagnostic = ClaudeDiagnostics.diagnostic(for: event) {
                    record(diagnostic)
                }
```

Do **not** add `--include-hook-events` to `launchTransport`. A two-step turn
produced 55 hook events; it stays opt-in.

- [x] **Step 4: Run test to verify it passes**

Run: `cd Packages/TillerACP && swift test`
Expected: PASS.

- [x] **Step 5: Commit**

```bash
git add Packages/TillerACP
git commit -m "feat: map claude diagnostic signals to a typed channel"
```

---

### Task 10: Use the structured tool result

**Files:**
- Modify: `Packages/TillerACP/Sources/TillerACP/Drivers/ClaudeStreamJSONDriver.swift:262-271`
- Test: `Packages/TillerACP/Tests/TillerACPTests/ClaudeDriverTests.swift`

**Interfaces:**
- Consumes: `ClaudeUserMessage.toolUseResult`, already decoded at
  `ClaudeWire.swift:121` and currently discarded.
- Produces: nothing later tasks depend on.

The rendered `content` carries flattened text; `tool_use_result` carries the
structured form — `structuredPatch` for Edit, `{filePath, numLines,
totalLines}` for Read, separated stdout/stderr for Bash.

- [x] **Step 1: Write the failing test**

```swift
    @Test func toolResultCarriesStructuredPayload() async throws {
        let mock = MockTransport()
        let driver = ClaudeStreamJSONDriver(transport: mock, permissionMode: .ask,
                                            model: nil, resumeSessionId: nil)
        let collector = EventCollector()
        let eventTask = collect(driver, into: collector)
        try await driver.start()
        _ = try await connectWithInitializeResponse(driver, mock: mock)

        await mock.emit(#"{"type":"user","message":{"role":"user","content":[{"tool_use_id":"tu-1","type":"tool_result","content":"1\thi\n"}]},"tool_use_result":{"type":"text","file":{"filePath":"/w/f.txt","numLines":2,"totalLines":2}}}"#)

        let events = await waitForEvent(collector) { event in
            if case .update(.toolCallUpdate) = event { return true }
            return false
        }
        let update = events.compactMap { event -> ToolCallUpdate? in
            guard case .update(.toolCallUpdate(let update)) = event else { return nil }
            return update
        }.first
        #expect(update?.toolCallId == "tu-1")
        #expect(update?.rawOutput?["file"]?["numLines"]?.intValue == 2)

        eventTask.cancel()
        await driver.stop()
    }
```

- [x] **Step 2: Run test to verify it fails**

Run: `cd Packages/TillerACP && swift test --filter toolResultCarriesStructuredPayload`
Expected: FAIL — `ToolCallUpdate` has no `rawOutput` member, and the driver
never reads `toolUseResult`.

- [x] **Step 3: Write minimal implementation**

Add `rawOutput` to `ToolCallUpdate` in `ToolCall.swift`, mirroring the
existing `rawInput` property exactly — same optionality, same `CodingKeys`
entry, same position in the memberwise initialiser. Read the file first and
match its style; do not invent a different shape.

Then in the driver's `.user` arm, attach the structured payload:

```swift
        case .user(let user):
            for block in user.message.content {
                if case .toolResult(let result) = block {
                    let output = result.content.map(renderJSON) ?? ""
                    eventContinuation.yield(.update(.toolCallUpdate(ToolCallUpdate(
                        toolCallId: result.toolUseId,
                        status: result.isError ? .failed : .completed,
                        content: [.content(.text(output))],
                        rawOutput: user.toolUseResult))))
                }
            }
```

- [x] **Step 4: Run test to verify it passes**

Run: `cd Packages/TillerACP && swift test`
Expected: PASS.

- [x] **Step 5: Commit**

```bash
git add Packages/TillerACP
git commit -m "feat: carry claude structured tool results through tool call updates"
```

---

## Final verification

- [ ] Run the whole package: `cd Packages/TillerACP && swift test`
- [ ] Run the gate: `Scripts/ci.sh` — expect `CI OK`
- [ ] If the gate is red, exonerate the diff by running it twice on the same
      tree rather than comparing against `HEAD`; several suites fail on
      baseline as of 2026-08-06
- [ ] Manual smoke, which no test covers: open a chat on Claude, confirm the
      effort picker lists five levels on Sonnet/Opus and **disappears** on
      Haiku, send a prompt, and confirm a turn-stats notice appears with a
      plausible cost and duration
