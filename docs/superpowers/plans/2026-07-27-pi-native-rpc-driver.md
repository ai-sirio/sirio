# Pi Native RPC Driver Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add Pi as Tiller's fourth native chat driver by translating the official `pi --mode rpc` JSONL protocol into the existing canonical chat event stream.

**Architecture:** `PiRPCDriver` is a Swift actor over the existing `ACPTransport`/`ProcessTransport` seam. It launches Pi through the user's login shell, correlates Pi command responses by string id, and translates stream events into `ACPSessionEvent`/`SessionUpdate`; the existing transcript, tool-card, model-picker and persistence pipelines remain authoritative. A small canonical text-answer extension lets Pi's Extension UI `input` requests use the existing question card.

**Tech Stack:** Swift 6, Foundation `Process`/`Pipe`, macOS 15+, Pi RPC JSONL protocol, Swift Testing (`@Test`, `#expect`), GRDB-backed existing chat persistence.

**Design spec:** `docs/superpowers/specs/2026-07-27-pi-native-rpc-driver-design.md`

## Global Constraints

- Keep package dependencies flowing in the existing direction: `TillerACP` must not import SwiftUI/AppKit; only `App/` renders controls.
- Use `pi --mode rpc --approve`; do not embed Node/Bun, add npm dependencies, or mutate `~/.pi/agent` configuration.
- Resolve `pi` through `/bin/zsh -lc` because app-launch environments do not reliably inherit the user's CLI PATH.
- Preserve strict JSONL framing: one JSON object per LF-terminated line; accept a trailing CR before decoding.
- Close a canonical turn only on `agent_settled`. `agent_end` is a low-level boundary and may be followed by retry, compaction or queued continuations.
- During streaming, ordinary messages use `prompt` with `streamingBehavior: "steer"`; slash-prefixed extension commands remain plain `prompt` commands.
- Pi has no built-in tool permission prompts. Only Extension UI `select`, `confirm`, and `input` become question cards; `editor` is cancelled explicitly because ignoring a dialog would deadlock the agent.
- Keep terminal-pane `PiAdapter` unchanged.
- Tests use Swift Testing, never XCTest. Every source change follows red → green → commit.
- `Scripts/ci.sh` must print `CI OK` before completion.

## File Structure

**Create**

- `Packages/TillerACP/Sources/TillerACP/Drivers/PiWire.swift` — Pi-specific envelope codec; deliberately separate from JSON-RPC 2.0 `JSONRPCMessage`.
- `Packages/TillerACP/Sources/TillerACP/Drivers/PiRPCDriver.swift` — process lifecycle, command correlation, event mapping and Extension UI bridge.
- `Packages/TillerACP/Tests/TillerACPTests/PiWireTests.swift` — codec and exact response-shape tests.
- `Packages/TillerACP/Tests/TillerACPTests/PiDriverTests.swift` — driver control-plane, stream, tool, question and disconnect tests.
- `Packages/TillerACP/Tests/TillerACPTests/Fixtures/pi-turn.jsonl` — deterministic recorded-shape event fixture.

**Modify**

- `Packages/TillerACP/Sources/TillerACP/ChatQuestion.swift` — canonical optional text-input metadata and structured rejection metadata.
- `Packages/TillerACP/Tests/TillerACPTests/ChatQuestionTests.swift` — text-input/rejection normalization tests.
- `App/Chat/ChatController.swift` — submit a custom textual answer through `PermissionOutcome.answered`.
- `App/Chat/QuestionCardView.swift` — render text field, Send and Cancel for canonical text questions.
- `AppTests/ChatControllerTests.swift` — verify custom text reaches a structured-capable driver.
- `Packages/TillerACP/Sources/TillerACP/AgentDriverFactory.swift` — classify `pi` as native and construct `PiRPCDriver`.
- `Packages/TillerACP/Tests/TillerACPTests/AgentDriverFactoryTests.swift` — native classification, binary and concrete driver tests.
- `App/AcpAgentCenter.swift` — expose Pi in Settings and New Chat when `pi` resolves on PATH.
- `AppTests/AcpAgentCenterTests.swift` — built-in availability/display-name coverage.

---

### Task 1: Add the Pi JSONL wire codec

**Files:**

- Create: `Packages/TillerACP/Sources/TillerACP/Drivers/PiWire.swift`
- Create: `Packages/TillerACP/Tests/TillerACPTests/PiWireTests.swift`

**Interfaces:**

- Produces: `PiWire.Message`, `PiWire.Response`, `PiWire.decode(_:)`, `PiWire.command(id:type:fields:)`, `PiWire.extensionUIValue(id:value:)`, `PiWire.extensionUIConfirmation(id:confirmed:)`, and `PiWire.extensionUICancel(id:)`.
- Consumes: existing `JSONValue`; it must not use `JSONRPCMessage`, because Pi commands use `{id,type,...}` rather than JSON-RPC 2.0 `{jsonrpc,method,params}`.

- [ ] **Step 1: Write failing codec tests**

Create `PiWireTests.swift` with exact envelope, response and CRLF expectations:

```swift
import Foundation
import Testing
@testable import TillerACP

@Suite struct PiWireTests {
    private func value(_ line: Data) throws -> JSONValue {
        let payload = line.last == UInt8(ascii: "\n") ? line.dropLast() : line[...]
        return try JSONDecoder().decode(JSONValue.self, from: payload)
    }

    @Test func commandUsesPiEnvelopeAndOneLF() throws {
        let line = try PiWire.command(
            id: "req-1", type: "set_model",
            fields: ["provider": .string("anthropic"),
                     "modelId": .string("claude-sonnet-4")])
        let decoded = try value(line)

        #expect(line.last == UInt8(ascii: "\n"))
        #expect(decoded["id"]?.stringValue == "req-1")
        #expect(decoded["type"]?.stringValue == "set_model")
        #expect(decoded["provider"]?.stringValue == "anthropic")
        #expect(decoded["modelId"]?.stringValue == "claude-sonnet-4")
        #expect(decoded["jsonrpc"] == nil)
        #expect(decoded["method"] == nil)
    }

    @Test func decodesCorrelatedSuccessAndFailureResponses() throws {
        let success = try PiWire.decode(Data(
            #"{"id":"req-1","type":"response","command":"get_state","success":true,"data":{"sessionId":"abc"}}"#.utf8))
        let failure = try PiWire.decode(Data(
            #"{"id":"req-2","type":"response","command":"set_model","success":false,"error":"Model not found"}"#.utf8))

        #expect(success == .response(.init(
            id: "req-1", command: "get_state", success: true,
            data: .object(["sessionId": .string("abc")]), error: nil)))
        #expect(failure == .response(.init(
            id: "req-2", command: "set_model", success: false,
            data: nil, error: "Model not found")))
    }

    @Test func decodesEventsAndAcceptsTrailingCR() throws {
        let message = try PiWire.decode(Data(
            "{\"type\":\"agent_start\"}\r".utf8))
        guard case .event(let type, let fields) = message else {
            Issue.record("expected event")
            return
        }
        #expect(type == "agent_start")
        #expect(fields["type"]?.stringValue == "agent_start")
    }

    @Test func extensionUIResponsesUseProtocolSpecificFields() throws {
        let selected = try value(PiWire.extensionUIValue(id: "ui-1", value: "Allow"))
        let confirmed = try value(PiWire.extensionUIConfirmation(id: "ui-2", confirmed: false))
        let cancelled = try value(PiWire.extensionUICancel(id: "ui-3"))

        #expect(selected["type"]?.stringValue == "extension_ui_response")
        #expect(selected["id"]?.stringValue == "ui-1")
        #expect(selected["value"]?.stringValue == "Allow")
        #expect(confirmed["confirmed"]?.boolValue == false)
        #expect(cancelled["cancelled"]?.boolValue == true)
    }
}
```

- [ ] **Step 2: Run the tests and confirm the red state**

Run:

```bash
cd Packages/TillerACP
swift test --filter PiWireTests
```

Expected: compilation fails because `PiWire` does not exist.

- [ ] **Step 3: Implement the codec**

Create `PiWire.swift` with this concrete API and decoding behavior:

```swift
import Foundation

enum PiWire {
    struct Response: Sendable, Equatable {
        var id: String?
        var command: String
        var success: Bool
        var data: JSONValue?
        var error: String?
    }

    enum Message: Sendable, Equatable {
        case response(Response)
        case event(type: String, fields: [String: JSONValue])
    }

    struct DecodeFailure: Error, Sendable, Equatable {
        var reason: String
    }

    static func decode(_ rawLine: Data) throws -> Message {
        var line = rawLine
        if line.last == UInt8(ascii: "\r") { line.removeLast() }
        let value = try JSONDecoder().decode(JSONValue.self, from: line)
        guard case .object(let fields) = value,
              case .string(let type)? = fields["type"] else {
            throw DecodeFailure(reason: "top-level object with string type required")
        }
        if type == "response" {
            guard case .string(let command)? = fields["command"],
                  case .bool(let success)? = fields["success"] else {
                throw DecodeFailure(reason: "response requires command and success")
            }
            return .response(Response(
                id: fields["id"]?.stringValue,
                command: command,
                success: success,
                data: fields["data"],
                error: fields["error"]?.stringValue))
        }
        return .event(type: type, fields: fields)
    }

    static func command(id: String, type: String,
                        fields: [String: JSONValue] = [:]) throws -> Data {
        var object = fields
        object["type"] = .string(type)
        object["id"] = .string(id)
        return try encodedLine(object)
    }

    static func extensionUIValue(id: String, value: String) throws -> Data {
        try encodedLine(["type": .string("extension_ui_response"),
                         "id": .string(id), "value": .string(value)])
    }

    static func extensionUIConfirmation(id: String, confirmed: Bool) throws -> Data {
        try encodedLine(["type": .string("extension_ui_response"),
                         "id": .string(id), "confirmed": .bool(confirmed)])
    }

    static func extensionUICancel(id: String) throws -> Data {
        try encodedLine(["type": .string("extension_ui_response"),
                         "id": .string(id), "cancelled": .bool(true)])
    }

    private static func encodedLine(_ fields: [String: JSONValue]) throws -> Data {
        var data = try JSONEncoder().encode(JSONValue.object(fields))
        data.append(UInt8(ascii: "\n"))
        return data
    }
}
```

- [ ] **Step 4: Run the focused tests**

Run:

```bash
cd Packages/TillerACP
swift test --filter PiWireTests
```

Expected: all `PiWireTests` pass.

- [ ] **Step 5: Commit**

```bash
git add Packages/TillerACP/Sources/TillerACP/Drivers/PiWire.swift \
        Packages/TillerACP/Tests/TillerACPTests/PiWireTests.swift
git commit -m "feat: add pi rpc wire codec"
```

---

### Task 2: Implement Pi process, control plane and settled-turn lifecycle

**Files:**

- Create: `Packages/TillerACP/Sources/TillerACP/Drivers/PiRPCDriver.swift`
- Create: `Packages/TillerACP/Tests/TillerACPTests/PiDriverTests.swift`

**Interfaces:**

- Consumes: `PiWire`, `ACPTransport`, `ProcessTransport`, `AgentDriver`, `SessionHandle`, `SessionModelState`, `SessionConfigOption`, `AvailableCommand`.
- Produces: `PiRPCDriver.init(transport:model:effort:resumeSessionId:)` and `PiRPCDriver.launchTransport(worktreePath:model:resumeSessionId:onStderrLine:)` for `AgentDriverFactory` in Task 5.
- Invariant: `agent_settled` emits exactly one `.turnEnded` and resolves every prompt/steer waiter for that settled run; `agent_end` does neither.

- [ ] **Step 1: Write failing launch and connect tests**

Start `PiDriverTests.swift` with a helper that decodes sent Pi envelopes and drives the four connect requests:

```swift
import Foundation
import Testing
@testable import TillerACP

private actor PiEventCollector {
    private(set) var values: [ACPSessionEvent] = []
    func append(_ value: ACPSessionEvent) { values.append(value) }
}

private func collect(_ driver: PiRPCDriver, into collector: PiEventCollector)
    -> Task<Void, Never> {
    Task {
        for await event in driver.events { await collector.append(event) }
    }
}

@Suite struct PiDriverTests {
    private func value(_ data: Data) throws -> JSONValue {
        let line = data.last == UInt8(ascii: "\n") ? data.dropLast() : data[...]
        return try JSONDecoder().decode(JSONValue.self, from: line)
    }

    private func requestId(_ data: Data) throws -> String {
        try #require(value(data)["id"]?.stringValue)
    }

    private func successResponse(id: String, command: String,
                                 data: JSONValue? = nil) throws -> String {
        var fields: [String: JSONValue] = [
            "id": .string(id), "type": .string("response"),
            "command": .string(command), "success": .bool(true)
        ]
        fields["data"] = data
        let encoded = try JSONEncoder().encode(JSONValue.object(fields))
        return String(decoding: encoded, as: UTF8.self)
    }

    @Test func launchUsesLoginShellRpcApproveModelAndResume() {
        let transport = PiRPCDriver.launchTransport(
            worktreePath: "/tmp/work tree", model: "anthropic/claude-sonnet-4",
            resumeSessionId: "/tmp/session file.jsonl", onStderrLine: nil)
        #expect(transport.arguments == [
            "-lc",
            "exec pi --mode rpc --approve --model anthropic/claude-sonnet-4 " +
            "--session '/tmp/session file.jsonl'"
        ])
    }

    @Test func connectBuildsSessionModelsThinkingAndCommands() async throws {
        let mock = MockTransport()
        let driver = PiRPCDriver(transport: mock, model: nil, effort: nil,
                                 resumeSessionId: "/tmp/old-session.jsonl")
        var eventIterator = driver.events.makeAsyncIterator()
        try await driver.start()
        let connect = Task {
            try await driver.connect(
                cwd: "/tmp/work", resumeSessionId: "/tmp/old-session.jsonl",
                mcpServers: [])
        }

        var sent = try await mock.waitForSent(count: 1)
        let stateId = try requestId(sent[0])
        await mock.emit(#"{"id":"\#(stateId)","type":"response","command":"get_state","success":true,"data":{"model":{"id":"claude-sonnet-4","name":"Sonnet 4","provider":"anthropic"},"thinkingLevel":"medium","isStreaming":false,"sessionFile":"/tmp/pi-session.jsonl","sessionId":"pi-id"}}"#)

        sent = try await mock.waitForSent(count: 2)
        let modelsId = try requestId(sent[1])
        await mock.emit(#"{"id":"\#(modelsId)","type":"response","command":"get_available_models","success":true,"data":{"models":[{"id":"claude-sonnet-4","name":"Sonnet 4","provider":"anthropic"},{"id":"gpt-5","name":"GPT-5","provider":"openai"}]}}"#)

        sent = try await mock.waitForSent(count: 3)
        let thinkingId = try requestId(sent[2])
        await mock.emit(#"{"id":"\#(thinkingId)","type":"response","command":"get_available_thinking_levels","success":true,"data":{"levels":["off","medium","high"]}}"#)

        sent = try await mock.waitForSent(count: 4)
        let commandsId = try requestId(sent[3])
        await mock.emit(#"{"id":"\#(commandsId)","type":"response","command":"get_commands","success":true,"data":{"commands":[{"name":"fix-tests","description":"Fix failing tests","source":"prompt"}]}}"#)

        let handle = try await connect.value
        #expect(handle.sessionId == "/tmp/pi-session.jsonl")
        #expect(handle.didResume == true)
        #expect(handle.agentCapabilities.loadSession == true)
        #expect(handle.models?.currentModelId == "anthropic/claude-sonnet-4")
        #expect(handle.models?.availableModels.map(\.modelId) == [
            "anthropic/claude-sonnet-4", "openai/gpt-5"
        ])
        #expect(handle.configOptions.first?.id == "effort")
        #expect(handle.configOptions.first?.currentValue == "medium")
        #expect(handle.configOptions.first?.options?.map(\.value) == ["off", "medium", "high"])
        guard case .some(.update(.availableCommandsUpdate(let commands))) =
                await eventIterator.next() else {
            Issue.record("expected available command update")
            return
        }
        #expect(commands == [.init(name: "fix-tests", description: "Fix failing tests")])
    }
}
```

- [ ] **Step 2: Write failing request-correlation, model and settlement tests**

Add tests proving out-of-order correlation, the exact `set_model` payload, effort updates and settled-turn ordering:

```swift
@Test func setModelSplitsProviderFromModelId() async throws {
    let mock = MockTransport()
    let driver = PiRPCDriver(transport: mock, model: nil, effort: nil,
                             resumeSessionId: nil)
    try await driver.start()
    await driver.markConnectedForTesting(sessionId: "session")

    let change = Task { try await driver.setModel("anthropic/claude-sonnet-4") }
    let sent = try await mock.waitForSent(count: 1)
    let payload = try value(sent[0])
    #expect(payload["type"]?.stringValue == "set_model")
    #expect(payload["provider"]?.stringValue == "anthropic")
    #expect(payload["modelId"]?.stringValue == "claude-sonnet-4")
    let id = try requestId(sent[0])
    await mock.emit(#"{"id":"\#(id)","type":"response","command":"set_model","success":true,"data":{"id":"claude-sonnet-4","provider":"anthropic","name":"Sonnet 4"}}"#)
    try await change.value
}

@Test func concurrentCommandsCorrelateOutOfOrderResponsesById() async throws {
    let mock = MockTransport()
    let driver = PiRPCDriver(transport: mock, model: nil, effort: nil,
                             resumeSessionId: nil)
    try await driver.start()
    await driver.markConnectedForTesting(sessionId: "session")

    let model = Task { try await driver.setModel("anthropic/claude-sonnet-4") }
    let effort = Task {
        try await driver.setConfigOption(id: "effort", value: "high")
    }
    let sent = try await mock.waitForSent(count: 2)
    let first = try value(sent[0])
    let second = try value(sent[1])
    let firstId = try #require(first["id"]?.stringValue)
    let secondId = try #require(second["id"]?.stringValue)
    let firstCommand = try #require(first["type"]?.stringValue)
    let secondCommand = try #require(second["type"]?.stringValue)
    let modelData: JSONValue = .object([
        "id": .string("claude-sonnet-4"), "provider": .string("anthropic"),
        "name": .string("Sonnet 4")
    ])

    await mock.emit(try successResponse(
        id: secondId, command: secondCommand,
        data: secondCommand == "set_model" ? modelData : nil))
    await mock.emit(try successResponse(
        id: firstId, command: firstCommand,
        data: firstCommand == "set_model" ? modelData : nil))

    try await model.value
    let effortOptions = try await effort.value
    #expect(effortOptions?.first?.currentValue == "high")
}

@Test func agentEndDoesNotFinishPromptButAgentSettledDoes() async throws {
    let mock = MockTransport()
    let driver = PiRPCDriver(transport: mock, model: nil, effort: nil,
                             resumeSessionId: nil)
    try await driver.start()
    await driver.markConnectedForTesting(sessionId: "session")

    let prompt = Task { try await driver.prompt([.text("hello")]) }
    let sent = try await mock.waitForSent(count: 1)
    let id = try requestId(sent[0])
    await mock.emit(#"{"id":"\#(id)","type":"response","command":"prompt","success":true}"#)
    await mock.emit(#"{"type":"agent_start"}"#)
    await mock.emit(#"{"type":"agent_end","messages":[],"willRetry":true}"#)

    try await Task.sleep(for: .milliseconds(30))
    #expect(await driver.hasPendingPromptForTesting)

    await mock.emit(#"{"type":"agent_settled"}"#)
    #expect(try await prompt.value == .endTurn)
    #expect(await driver.hasPendingPromptForTesting == false)
}

@Test func oneSettlementResolvesInitialPromptAndSteerWithOneTurnEnd() async throws {
    let mock = MockTransport()
    let driver = PiRPCDriver(transport: mock, model: nil, effort: nil,
                             resumeSessionId: nil)
    let collector = PiEventCollector()
    let collecting = collect(driver, into: collector)
    try await driver.start()
    await driver.markConnectedForTesting(sessionId: "session")

    let initial = Task { try await driver.prompt([.text("start")]) }
    var sent = try await mock.waitForSent(count: 1)
    let initialId = try requestId(sent[0])
    await mock.emit(#"{"id":"\#(initialId)","type":"response","command":"prompt","success":true}"#)
    await mock.emit(#"{"type":"agent_start"}"#)

    let steer = Task { try await driver.prompt([.text("change direction")]) }
    sent = try await mock.waitForSent(count: 2)
    #expect((try value(sent[1]))["streamingBehavior"]?.stringValue == "steer")
    let steerId = try requestId(sent[1])
    await mock.emit(#"{"id":"\#(steerId)","type":"response","command":"prompt","success":true}"#)
    await mock.emit(#"{"type":"agent_settled"}"#)

    #expect(try await initial.value == .endTurn)
    #expect(try await steer.value == .endTurn)
    for _ in 0..<100 {
        if !(await collector.values).isEmpty { break }
        await Task.yield()
    }
    let turnEnds = (await collector.values).filter {
        if case .turnEnded = $0 { return true }
        return false
    }
    #expect(turnEnds.count == 1)
    collecting.cancel()
}

@Test func ordinaryMidStreamPromptSteersButSlashCommandDoesNot() async throws {
    let mock = MockTransport()
    let driver = PiRPCDriver(transport: mock, model: nil, effort: nil,
                             resumeSessionId: nil)
    try await driver.start()
    await driver.markConnectedForTesting(sessionId: "session")
    await mock.emit(#"{"type":"agent_start"}"#)

    let ordinary = Task { try await driver.prompt([.text("change direction")]) }
    var sent = try await mock.waitForSent(count: 1)
    #expect((try value(sent[0]))["streamingBehavior"]?.stringValue == "steer")
    let ordinaryId = try requestId(sent[0])
    await mock.emit(#"{"id":"\#(ordinaryId)","type":"response","command":"prompt","success":true}"#)
    await mock.emit(#"{"type":"agent_settled"}"#)
    _ = try await ordinary.value

    await mock.emit(#"{"type":"agent_start"}"#)
    let slash = Task { try await driver.prompt([.text("/fix-tests")]) }
    sent = try await mock.waitForSent(count: 2)
    #expect((try value(sent[1]))["streamingBehavior"] == nil)
    let slashId = try requestId(sent[1])
    await mock.emit(#"{"id":"\#(slashId)","type":"response","command":"prompt","success":true}"#)
    await mock.emit(#"{"type":"agent_settled"}"#)
    _ = try await slash.value
}
```

Use internal test seams only under `@testable`:

```swift
func markConnectedForTesting(sessionId: String) {
    self.sessionId = sessionId
    effortOption = SessionConfigOption(
        id: "effort", name: "Thinking", currentValue: "medium",
        options: [.init(value: "off", name: "Off"),
                  .init(value: "medium", name: "Medium"),
                  .init(value: "high", name: "High")])
}
var hasPendingPromptForTesting: Bool { !promptContinuations.isEmpty }
```

Keep these internal (not `public`) and label them `// Test seam: avoids four-command connect setup in event-only tests.`

- [ ] **Step 3: Run the new tests and confirm the red state**

Run:

```bash
cd Packages/TillerACP
swift test --filter PiDriverTests
```

Expected: compilation fails because `PiRPCDriver` does not exist.

- [ ] **Step 4: Implement process launch, correlation and connection metadata**

Create `PiRPCDriver.swift` with the following state and protocol surface:

```swift
import Foundation

public enum PiRPCDriverError: Error, Sendable, Equatable {
    case notStarted
    case notConnected
    case invalidModelId(String)
    case commandFailed(command: String, message: String)
    case disconnected
}

public actor PiRPCDriver: AgentDriver {
    private enum UIRequestKind: Equatable { case select, confirm, input }

    private let transport: any ACPTransport
    private let requestedModel: String?
    private let requestedEffort: String?
    private let requestedResumeSessionId: String?
    private var started = false
    private var finished = false
    private var stopping = false
    private var isStreaming = false
    private var cancelRequested = false
    private var sessionId: String?
    private var nextRequestNumber = 1
    private var readTask: Task<Void, Never>?
    private var pendingResponses: [String: CheckedContinuation<PiWire.Response, Error>] = [:]
    private var promptContinuations: [CheckedContinuation<StopReason, Error>] = []
    private var settlementSequence = 0
    private var lastSettlementReason: StopReason = .endTurn
    private var pendingUIRequests: [String: UIRequestKind] = [:]
    private var effortOption: SessionConfigOption?

    private let eventContinuation: AsyncStream<ACPSessionEvent>.Continuation
    public nonisolated let events: AsyncStream<ACPSessionEvent>
    public nonisolated var supportsStructuredAnswers: Bool { true }

    public init(transport: any ACPTransport, model: String?, effort: String?,
                resumeSessionId: String?) {
        self.transport = transport
        requestedModel = model
        requestedEffort = effort
        requestedResumeSessionId = resumeSessionId
        (events, eventContinuation) = AsyncStream.makeStream(of: ACPSessionEvent.self)
    }
}
```

Implement launch with the same quoting rule used by `ClaudeStreamJSONDriver`:

```swift
public static func launchTransport(
    worktreePath: String, model: String?, resumeSessionId: String?,
    onStderrLine: (@Sendable (String) -> Void)? = nil
) -> ProcessTransport {
    var command = "exec pi --mode rpc --approve"
    if let model { command += " --model \(shellArgument(model))" }
    if let resumeSessionId { command += " --session \(shellArgument(resumeSessionId))" }
    return ProcessTransport(
        executable: "/bin/zsh", arguments: ["-lc", command], cwd: worktreePath,
        onStderrLine: onStderrLine)
}

private static func shellArgument(_ value: String) -> String {
    let safe = value.allSatisfy { $0.isLetter || $0.isNumber || "-._/".contains($0) }
    guard !safe else { return value }
    return "'" + value.replacingOccurrences(of: "'", with: "'\\''") + "'"
}
```

Implement `start`, `stop`, request ids, correlated requests and clean disconnect:

```swift
public func start() async throws {
    guard !started else { return }
    try await transport.start()
    started = true
    readTask = Task { [weak self] in await self?.readLoop() }
}

public func stop() async {
    stopping = true
    readTask?.cancel()
    await transport.terminate()
    finish()
}

private func makeRequestId() -> String {
    defer { nextRequestNumber += 1 }
    return "req-\(nextRequestNumber)"
}

private func sendCommand(_ type: String,
                         fields: [String: JSONValue] = [:]) async throws -> PiWire.Response {
    guard started else { throw PiRPCDriverError.notStarted }
    let id = makeRequestId()
    let line = try PiWire.command(id: id, type: type, fields: fields)
    return try await withCheckedThrowingContinuation { continuation in
        pendingResponses[id] = continuation
        let transport = self.transport
        Task { [weak self] in
            do { try await transport.send(line: line) }
            catch { await self?.failResponse(id: id, error: error) }
        }
    }
}

private func requireSuccess(_ response: PiWire.Response) throws -> JSONValue? {
    guard response.success else {
        throw PiRPCDriverError.commandFailed(
            command: response.command, message: response.error ?? "Unknown Pi RPC error")
    }
    return response.data
}

private func failResponse(id: String, error: Error) {
    pendingResponses.removeValue(forKey: id)?.resume(throwing: error)
}
```

Decode and dispatch on the actor so a response cannot race its continuation registration:

```swift
private func readLoop() async {
    do {
        for try await line in transport.lines() {
            guard !Task.isCancelled else { break }
            do { handle(try PiWire.decode(line)) }
            catch {
                eventContinuation.yield(.update(.agentThoughtChunk(
                    .text("Pi RPC decode error: \(error.localizedDescription)"))))
            }
        }
    } catch {
        // Transport failure and EOF share the same session-level finish path.
    }
    finish()
}

private func handle(_ message: PiWire.Message) {
    switch message {
    case .response(let response):
        guard let id = response.id,
              let continuation = pendingResponses.removeValue(forKey: id) else { return }
        continuation.resume(returning: response)
    case .event(let type, _):
        switch type {
        case "agent_start": isStreaming = true
        case "agent_end": break
        case "agent_settled": settleCurrentRun()
        default: break
        }
    }
}

private func finish() {
    guard !finished else { return }
    finished = true
    let responses = Array(pendingResponses.values)
    pendingResponses.removeAll()
    for continuation in responses {
        continuation.resume(throwing: PiRPCDriverError.disconnected)
    }
    let prompts = promptContinuations
    promptContinuations.removeAll()
    for continuation in prompts {
        continuation.resume(throwing: PiRPCDriverError.disconnected)
    }
    if !stopping { eventContinuation.yield(.disconnected) }
    eventContinuation.finish()
}
```

Unexpected EOF emits `.disconnected` exactly once; `stop()` first sets `stopping = true` and therefore only closes the stream.

Implement `connect` with this fixed request order:

1. `get_state`
2. optional `set_thinking_level` when a persisted `requestedEffort` exists
3. `get_available_models`
4. `get_available_thinking_levels`
5. `get_commands`

Prefer `state.data.sessionFile` over `sessionId` as `SessionHandle.sessionId`, because the full path is directly resumable; throw `.commandFailed(command: "get_state", message: "Missing session reference")` if both are absent. Convert model objects with this exact helper:

```swift
private func modelInfo(_ value: JSONValue) -> ModelInfo? {
    guard let provider = value["provider"]?.stringValue,
          let id = value["id"]?.stringValue else { return nil }
    return ModelInfo(modelId: "\(provider)/\(id)",
                     name: value["name"]?.stringValue ?? id)
}
```

Use the current state model when present, then `requestedModel`, then the first available model. If no current or available model exists, return `models: nil` rather than inventing an id. Create `SessionConfigOption(id: "effort", name: "Thinking", currentValue:, options:)`; emit `.availableCommandsUpdate` after parsing `get_commands`. Return:

```swift
SessionHandle(
    sessionId: resolvedSessionRef,
    agentCapabilities: AgentCapabilities(loadSession: true),
    modes: nil,
    models: SessionModelState(currentModelId: currentModelId,
                              availableModels: models),
    configOptions: effortOption.map { [$0] } ?? [],
    didResume: resumeSessionId != nil || requestedResumeSessionId != nil)
```

`mcpServers` is intentionally ignored in this driver because MCP forwarding is outside the approved scope.

- [ ] **Step 5: Implement prompt, cancel, model and thinking commands**

Build prompt payloads from canonical blocks:

```swift
private func promptFields(_ blocks: [ContentBlock]) -> [String: JSONValue] {
    var textParts: [String] = []
    var images: [JSONValue] = []
    for block in blocks {
        switch block {
        case .text(let text): textParts.append(text)
        case .image(let mimeType, let data):
            images.append(.object(["type": .string("image"),
                                   "data": .string(data),
                                   "mimeType": .string(mimeType)]))
        case .resourceLink(let uri, let name): textParts.append("\(name): \(uri)")
        case .resource(let uri, let text): textParts.append("\(uri)\n\(text)")
        case .unknown: break
        }
    }
    let message = textParts.joined(separator: "\n")
    var fields: [String: JSONValue] = ["message": .string(message)]
    if !images.isEmpty { fields["images"] = .array(images) }
    let slashCommand = message.trimmingCharacters(in: .whitespacesAndNewlines).hasPrefix("/")
    if isStreaming && !slashCommand { fields["streamingBehavior"] = .string("steer") }
    return fields
}
```

`prompt()` snapshots `settlementSequence`, sends `prompt`, and verifies the acceptance response. If settlement advanced while the response was in flight, it returns `lastSettlementReason`; otherwise it appends a continuation to `promptContinuations`:

```swift
public func prompt(_ blocks: [ContentBlock]) async throws -> StopReason {
    guard sessionId != nil else { throw PiRPCDriverError.notConnected }
    let observedSettlement = settlementSequence
    let response = try await sendCommand("prompt", fields: promptFields(blocks))
    _ = try requireSuccess(response)
    if settlementSequence != observedSettlement { return lastSettlementReason }
    return try await withCheckedThrowingContinuation { continuation in
        promptContinuations.append(continuation)
    }
}

private func settleCurrentRun() {
    isStreaming = false
    settlementSequence += 1
    let reason: StopReason = cancelRequested ? .cancelled : .endTurn
    cancelRequested = false
    lastSettlementReason = reason
    eventContinuation.yield(.turnEnded(reason))
    let continuations = promptContinuations
    promptContinuations.removeAll()
    for continuation in continuations { continuation.resume(returning: reason) }
}
```

Call `settleCurrentRun()` only for `agent_settled`; `agent_end` never emits or resolves anything. This supports an initial prompt plus one or more accepted steering prompts without duplicate dividers or stranded waiters.

`cancel()` sets `cancelRequested = true` and sends `abort`. Implement live model switching with an exact first-slash split:

```swift
public func setModel(_ modelId: String) async throws {
    guard sessionId != nil else { throw PiRPCDriverError.notConnected }
    let parts = modelId.split(separator: "/", maxSplits: 1,
                              omittingEmptySubsequences: false)
    guard parts.count == 2, !parts[0].isEmpty, !parts[1].isEmpty else {
        throw PiRPCDriverError.invalidModelId(modelId)
    }
    let response = try await sendCommand("set_model", fields: [
        "provider": .string(String(parts[0])),
        "modelId": .string(String(parts[1]))
    ])
    _ = try requireSuccess(response)
}
```

`setConfigOption(id:value:)` accepts only `id == "effort"`, sends `set_thinking_level`, updates `effortOption.currentValue`, and returns the one-element config array. `setMode(_:)` is a deliberate no-op because `connect` returns `modes: nil`, so the UI never exposes a Pi mode selector.

Install the complete protocol-required answer writer now; Task 4 will populate `pendingUIRequests` when UI requests arrive:

```swift
public func answerPermission(requestId: JSONRPCID,
                             outcome: PermissionOutcome) async {
    guard case .string(let id) = requestId,
          let kind = pendingUIRequests.removeValue(forKey: id) else { return }
    do {
        let line: Data
        switch outcome {
        case .cancelled:
            line = try PiWire.extensionUICancel(id: id)
        case .selected(let optionId):
            if optionId == "__cancel__" {
                line = try PiWire.extensionUICancel(id: id)
            } else if kind == .confirm {
                line = try PiWire.extensionUIConfirmation(
                    id: id, confirmed: optionId == "true")
            } else {
                line = try PiWire.extensionUIValue(id: id, value: optionId)
            }
        case .answered(let optionId, let updatedInput):
            if kind == .confirm {
                line = try PiWire.extensionUIConfirmation(
                    id: id, confirmed: optionId == "true")
            } else if let text = updatedInput["choice"]?.stringValue {
                line = try PiWire.extensionUIValue(id: id, value: text)
            } else {
                line = try PiWire.extensionUICancel(id: id)
            }
        }
        try await transport.send(line: line)
    } catch {
        eventContinuation.yield(.update(.agentThoughtChunk(
            .text("Pi UI response failed: \(error.localizedDescription)"))))
    }
}
```

- [ ] **Step 6: Run focused tests**

Run:

```bash
cd Packages/TillerACP
swift test --filter PiDriverTests
```

Expected: launch, connect, correlation, model, thinking and settlement tests pass.

- [ ] **Step 7: Commit**

```bash
git add Packages/TillerACP/Sources/TillerACP/Drivers/PiRPCDriver.swift \
        Packages/TillerACP/Tests/TillerACPTests/PiDriverTests.swift
git commit -m "feat: add pi rpc driver control plane"
```

---

### Task 3: Map Pi stream and tool events to canonical chat updates

**Files:**

- Modify: `Packages/TillerACP/Sources/TillerACP/Drivers/PiRPCDriver.swift`
- Modify: `Packages/TillerACP/Tests/TillerACPTests/PiDriverTests.swift`
- Create: `Packages/TillerACP/Tests/TillerACPTests/Fixtures/pi-turn.jsonl`

**Interfaces:**

- Consumes: `PiWire.Message.event(type:fields:)` and the driver lifecycle from Task 2.
- Produces: canonical `.agentMessageChunk`, `.agentThoughtChunk`, `.toolCall`, `.toolCallUpdate`, retry/compaction notices, and `.disconnected` events.
- Invariant: `tool_execution_update.partialResult` is cumulative and replaces displayed content; it is never appended as a delta.

- [ ] **Step 1: Add the deterministic Pi event fixture**

Create `Fixtures/pi-turn.jsonl` with exactly these LF-delimited objects:

```jsonl
{"type":"agent_start"}
{"type":"message_update","message":{},"assistantMessageEvent":{"type":"thinking_delta","contentIndex":0,"delta":"Check the file","partial":{}}}
{"type":"message_update","message":{},"assistantMessageEvent":{"type":"text_delta","contentIndex":1,"delta":"Updating it.","partial":{}}}
{"type":"tool_execution_start","toolCallId":"call-edit","toolName":"edit","args":{"path":"Sources/App.swift","edits":[{"oldText":"let old = 1","newText":"let value = 2"}]}}
{"type":"tool_execution_update","toolCallId":"call-edit","toolName":"edit","args":{"path":"Sources/App.swift"},"partialResult":{"content":[{"type":"text","text":"Applying edit"}],"details":{}}}
{"type":"tool_execution_end","toolCallId":"call-edit","toolName":"edit","result":{"content":[{"type":"text","text":"Updated Sources/App.swift"}],"details":{"diff":"-let old = 1\n+let value = 2","patch":"@@ -1 +1 @@\n-let old = 1\n+let value = 2"}},"isError":false}
{"type":"agent_end","messages":[],"willRetry":false}
{"type":"agent_settled"}
```

- [ ] **Step 2: Write failing fixture and cumulative-output tests**

Reuse the `PiEventCollector` created in Task 2 and assert the canonical sequence:

```swift
@Test func fixtureMapsTextThinkingEditDiffAndSettledTurn() async throws {
    let mock = MockTransport()
    let driver = PiRPCDriver(transport: mock, model: nil, effort: nil,
                             resumeSessionId: nil)
    let collector = PiEventCollector()
    let collecting = collect(driver, into: collector)
    try await driver.start()
    await driver.markConnectedForTesting(sessionId: "session")

    let prompt = Task { try await driver.prompt([.text("edit the file")]) }
    let sent = try await mock.waitForSent(count: 1)
    let id = try requestId(sent[0])
    await mock.emit(#"{"id":"\#(id)","type":"response","command":"prompt","success":true}"#)

    let fixtureURL = Bundle.module.url(
        forResource: "pi-turn", withExtension: "jsonl", subdirectory: "Fixtures")!
    let lines = try String(contentsOf: fixtureURL, encoding: .utf8)
        .split(separator: "\n").map(String.init)
    for line in lines { await mock.emit(line) }
    #expect(try await prompt.value == .endTurn)

    let events = await collector.values
    let updates = events.compactMap { event -> SessionUpdate? in
        guard case .update(let update) = event else { return nil }
        return update
    }
    #expect(updates.contains(.agentThoughtChunk(.text("Check the file"))))
    #expect(updates.contains(.agentMessageChunk(.text("Updating it."))))
    #expect(updates.contains(.toolCall(ToolCall(
        toolCallId: "call-edit", title: "Edit Sources/App.swift", kind: .edit,
        status: .inProgress, locations: [.init(path: "Sources/App.swift")],
        rawInput: .object(["path": .string("Sources/App.swift"), "edits": .array([
            .object(["oldText": .string("let old = 1"),
                     "newText": .string("let value = 2")])
        ])]))))
    #expect(updates.contains(.toolCallUpdate(ToolCallUpdate(
        toolCallId: "call-edit", status: .completed,
        content: [.diff(path: "Sources/App.swift", oldText: "let old = 1",
                        newText: "let value = 2")])))
    #expect(events.contains { if case .turnEnded(.endTurn) = $0 { true } else { false } })
    collecting.cancel()
}
```

Add a cumulative replacement test:

```swift
@Test func cumulativeToolUpdatesReplaceInsteadOfAppend() async throws {
    let mock = MockTransport()
    let driver = PiRPCDriver(transport: mock, model: nil, effort: nil,
                             resumeSessionId: nil)
    let collector = PiEventCollector()
    let collecting = collect(driver, into: collector)
    try await driver.start()

    await mock.emit(#"{"type":"tool_execution_start","toolCallId":"call-bash","toolName":"bash","args":{"command":"printf test"}}"#)
    await mock.emit(#"{"type":"tool_execution_update","toolCallId":"call-bash","toolName":"bash","args":{"command":"printf test"},"partialResult":{"content":[{"type":"text","text":"one"}],"details":{}}}"#)
    await mock.emit(#"{"type":"tool_execution_update","toolCallId":"call-bash","toolName":"bash","args":{"command":"printf test"},"partialResult":{"content":[{"type":"text","text":"one two"}],"details":{}}}"#)
    for _ in 0..<100 {
        if (await collector.values).count >= 3 { break }
        await Task.yield()
    }

    let contents = (await collector.values).compactMap { event -> [ToolCallContent]? in
        guard case .update(.toolCallUpdate(let update)) = event else { return nil }
        return update.content
    }
    #expect(contents == [[.content(.text("one"))],
                         [.content(.text("one two"))]])
    collecting.cancel()
}
```

Add explicit EOF coverage:

```swift
@Test func unexpectedEOFDisconnectsButIntentionalStopDoesNot() async throws {
    let crashTransport = MockTransport()
    let crashed = PiRPCDriver(transport: crashTransport, model: nil, effort: nil,
                              resumeSessionId: nil)
    var crashEvents = crashed.events.makeAsyncIterator()
    try await crashed.start()
    await crashTransport.close()
    guard case .some(.disconnected) = await crashEvents.next() else {
        Issue.record("unexpected EOF must emit disconnected")
        return
    }

    let stopTransport = MockTransport()
    let stopped = PiRPCDriver(transport: stopTransport, model: nil, effort: nil,
                              resumeSessionId: nil)
    var stopEvents = stopped.events.makeAsyncIterator()
    try await stopped.start()
    await stopped.stop()
    guard case nil = await stopEvents.next() else {
        Issue.record("intentional stop must only finish the event stream")
        return
    }
}
```

- [ ] **Step 3: Run the event tests and confirm failure**

Run:

```bash
cd Packages/TillerACP
swift test --filter PiDriverTests
```

Expected: fixture assertions fail because message/tool events are not mapped yet.

- [ ] **Step 4: Implement message and lifecycle notices**

In the event switch:

- `agent_start`: set `isStreaming = true`.
- `agent_end`: read `willRetry` for diagnostics only; do not resolve prompt.
- `agent_settled`: use Task 2 settlement logic.
- `message_update`: inspect `assistantMessageEvent.type`; emit text for `text_delta`, thought for `thinking_delta`, and ignore start/end/done structural markers.
- `compaction_start`: emit `.agentThoughtChunk(.text("⟳ Compacting context…"))`.
- `compaction_end`: emit `"Context compacted."` on success, `errorMessage` on failure, and `"Compaction cancelled."` when `aborted == true`.
- `auto_retry_start`: include attempt/maxAttempts in `.agentThoughtChunk` when present.
- `auto_retry_end`: emit success or `finalError`; never close the turn here because `agent_settled` follows the full retry sequence.
- `extension_error`: emit a thought containing extension path and error.
- `queue_update`, message start/end and turn start/end: no canonical event; ignore explicitly.

- [ ] **Step 5: Implement tool mapping with retained arguments**

Add:

```swift
private var toolArguments: [String: JSONValue] = [:]
private var toolNames: [String: String] = [:]
```

On `tool_execution_start`, retain `args`, map kind and title, and emit `ToolCall`:

```swift
private func toolKind(_ name: String) -> ToolKind {
    switch name {
    case "read": .read
    case "edit", "write": .edit
    case "bash": .execute
    case "grep", "find", "ls": .search
    default: .other
    }
}

private func toolTitle(name: String, args: JSONValue?) -> String {
    let subject = args?["path"]?.stringValue ?? args?["command"]?.stringValue
    let label = name.prefix(1).uppercased() + name.dropFirst()
    return subject.map { "\(label) \($0)" } ?? label
}

private func locations(_ args: JSONValue?) -> [ToolCallLocation] {
    guard let path = args?["path"]?.stringValue else { return [] }
    return [.init(path: path)]
}
```

On `tool_execution_update`, flatten `partialResult.content` text blocks and emit a replacing `.toolCallUpdate(content:)`.

On `tool_execution_end`:

- status is `.failed` when `isError == true`, otherwise `.completed`;
- for `edit`, use retained `args.path` and every retained `args.edits[]` entry to create one `.diff(path:oldText:newText:)` per edit;
- for `write`, use retained `args.path` and `args.content` to create `.diff(path:oldText:nil,newText:content)`;
- otherwise use result text blocks as `.content(.text(...))`;
- if edit args are malformed, fall back to `result.details.diff` as text content;
- remove retained name/args after completion.

- [ ] **Step 6: Run stream/tool tests**

Run:

```bash
cd Packages/TillerACP
swift test --filter PiDriverTests
```

Expected: all lifecycle, text, thinking, cumulative-output, tool and diff tests pass.

- [ ] **Step 7: Commit**

```bash
git add Packages/TillerACP/Sources/TillerACP/Drivers/PiRPCDriver.swift \
        Packages/TillerACP/Tests/TillerACPTests/PiDriverTests.swift \
        Packages/TillerACP/Tests/TillerACPTests/Fixtures/pi-turn.jsonl
git commit -m "feat: map pi rpc stream to chat events"
```

---

### Task 4: Bridge Extension UI questions and canonical text answers

**Files:**

- Modify: `Packages/TillerACP/Sources/TillerACP/Drivers/PiRPCDriver.swift`
- Modify: `Packages/TillerACP/Tests/TillerACPTests/PiDriverTests.swift`
- Modify: `Packages/TillerACP/Sources/TillerACP/ChatQuestion.swift`
- Modify: `Packages/TillerACP/Tests/TillerACPTests/ChatQuestionTests.swift`
- Modify: `App/Chat/ChatController.swift`
- Modify: `App/Chat/QuestionCardView.swift`
- Modify: `AppTests/ChatControllerTests.swift`

**Interfaces:**

- Produces: `ChatQuestion.TextInput`, `ChatQuestion.textInput`, and `ChatController.answerQuestion(_:text:)`.
- Driver protocol mapping: Pi `select` → value response; `confirm` → boolean `confirmed`; `input` → textual value; cancel → `cancelled: true`; `editor` → immediate cancellation.
- Canonical metadata contract in `ToolCallUpdate.rawInput`: `_tillerTextInput: {placeholder,prefill}` marks a text-answer question.

- [ ] **Step 1: Write failing normalization tests**

Add to `ChatQuestionTests.swift`:

```swift
@Test func structuredTextQuestionAllowsNoOptionsAndCarriesPlaceholder() throws {
    var call = ToolCallItem(toolCallId: "pi-ui-input", title: "Enter branch",
                            kind: .other, status: .pending)
    call.rawInput = .object([
        "questions": .array([.object([
            "header": .string("Enter branch"),
            "question": .string("Branch name"),
            "options": .array([])
        ])]),
        "_tillerTextInput": .object([
            "placeholder": .string("feature/native-pi"),
            "prefill": .string("feature/")
        ])
    ])
    call.permission = PermissionState(requestId: .string("ui-1"), options: [])

    let question = try #require(ChatQuestion.from(call))
    #expect(question.options.isEmpty)
    #expect(question.textInput == .init(
        placeholder: "feature/native-pi", prefill: "feature/"))
}

@Test func structuredOptionCanBeMarkedAsRejection() throws {
    var call = ToolCallItem(toolCallId: "pi-ui-select", title: "Choose",
                            kind: .other, status: .pending)
    call.rawInput = .object(["questions": .array([.object([
        "question": .string("Choose"),
        "options": .array([
            .object(["id": .string("proceed"), "label": .string("Proceed")]),
            .object(["id": .string("__cancel__"), "label": .string("Cancel"),
                     "isRejection": .bool(true)])
        ])
    ])])])
    call.permission = PermissionState(requestId: .string("ui-2"), options: [])

    let question = try #require(ChatQuestion.from(call))
    #expect(question.options.last?.id == "__cancel__")
    #expect(question.options.last?.isRejection == true)
}
```

- [ ] **Step 2: Write failing driver roundtrip tests**

Add select, confirm, input and editor tests to `PiDriverTests.swift`. The confirm assertion must check `confirmed`, not `value`:

```swift
@Test func confirmRequestRoundTripsAsConfirmedBoolean() async throws {
    let mock = MockTransport()
    let driver = PiRPCDriver(transport: mock, model: nil, effort: nil,
                             resumeSessionId: nil)
    let collector = PiEventCollector()
    let collecting = collect(driver, into: collector)
    try await driver.start()

    await mock.emit(#"{"type":"extension_ui_request","id":"ui-confirm","method":"confirm","title":"Clear session?","message":"All messages will be lost."}"#)
    for _ in 0..<100 {
        if !(await collector.values).isEmpty { break }
        await Task.yield()
    }
    await driver.answerPermission(requestId: .string("ui-confirm"),
                                  outcome: .selected(optionId: "true"))

    let sent = try await mock.waitForSent(count: 1)
    let reply = try value(sent[0])
    #expect(reply["type"]?.stringValue == "extension_ui_response")
    #expect(reply["id"]?.stringValue == "ui-confirm")
    #expect(reply["confirmed"]?.boolValue == true)
    #expect(reply["value"] == nil)
    collecting.cancel()
}

@Test func customInputAnswerUsesValueAndEditorIsCancelled() async throws {
    let mock = MockTransport()
    let driver = PiRPCDriver(transport: mock, model: nil, effort: nil,
                             resumeSessionId: nil)
    let collector = PiEventCollector()
    let collecting = collect(driver, into: collector)
    try await driver.start()

    await mock.emit(#"{"type":"extension_ui_request","id":"ui-input","method":"input","title":"Branch","placeholder":"feature/name"}"#)
    for _ in 0..<100 {
        if !(await collector.values).isEmpty { break }
        await Task.yield()
    }
    await driver.answerPermission(
        requestId: .string("ui-input"),
        outcome: .answered(optionId: "feature/pi",
                           updatedInput: .object(["choice": .string("feature/pi")])))
    await mock.emit(#"{"type":"extension_ui_request","id":"ui-editor","method":"editor","title":"Edit text","prefill":"hello"}"#)

    let sent = try await mock.waitForSent(count: 2)
    #expect((try value(sent[0]))["value"]?.stringValue == "feature/pi")
    #expect((try value(sent[1]))["cancelled"]?.boolValue == true)
    collecting.cancel()
}
```

Add exact select and cancellation coverage:

```swift
@Test func selectUsesValueAndCancellationUsesCancelledField() async throws {
    let mock = MockTransport()
    let driver = PiRPCDriver(transport: mock, model: nil, effort: nil,
                             resumeSessionId: nil)
    let collector = PiEventCollector()
    let collecting = collect(driver, into: collector)
    try await driver.start()

    await mock.emit(#"{"type":"extension_ui_request","id":"ui-select","method":"select","title":"Mode","options":["Fast","Safe"]}"#)
    for _ in 0..<100 {
        if !(await collector.values).isEmpty { break }
        await Task.yield()
    }
    guard case .some(.permissionRequested(_, _, let options)) =
            (await collector.values).first else {
        Issue.record("expected canonical select question")
        return
    }
    #expect(options.map(\.optionId) == ["Fast", "Safe", "__cancel__"])
    #expect(options.last?.kind == .rejectOnce)
    await driver.answerPermission(requestId: .string("ui-select"),
                                  outcome: .selected(optionId: "Fast"))

    await mock.emit(#"{"type":"extension_ui_request","id":"ui-cancel","method":"select","title":"Mode","options":["Fast"]}"#)
    for _ in 0..<100 {
        if (await collector.values).count >= 2 { break }
        await Task.yield()
    }
    await driver.answerPermission(requestId: .string("ui-cancel"), outcome: .cancelled)

    let sent = try await mock.waitForSent(count: 2)
    #expect((try value(sent[0]))["value"]?.stringValue == "Fast")
    #expect((try value(sent[1]))["cancelled"]?.boolValue == true)
    collecting.cancel()
}
```

- [ ] **Step 3: Run package tests and confirm failure**

Run:

```bash
cd Packages/TillerACP
swift test --filter ChatQuestionTests
swift test --filter PiDriverTests
```

Expected: new metadata and Extension UI tests fail.

- [ ] **Step 4: Extend the canonical question model**

Add to `ChatQuestion`:

```swift
public struct TextInput: Sendable, Equatable {
    public var placeholder: String?
    public var prefill: String?
    public init(placeholder: String? = nil, prefill: String? = nil) {
        self.placeholder = placeholder
        self.prefill = prefill
    }
}

public var textInput: TextInput?
```

Parse `_tillerTextInput` before parsing `questions`. Permit an empty structured option array only when `textInput != nil`. While mapping structured options, use optional `id` before falling back to the label, and set `isRejection` from optional `isRejection: true`:

```swift
let baseID = option["id"]?.stringValue ?? (label.isEmpty ? "option-\(index)" : label)
let rejection = option["isRejection"]?.boolValue ?? false
```

Pass `textInput` into both `ChatQuestion` initializers; fallback permission questions receive `nil`.

- [ ] **Step 5: Implement Extension UI event and response mapping**

For `extension_ui_request`:

- `select`: store `.select`; emit a synthetic structured question containing every string option plus `Cancel` with `isRejection: true` and id `__cancel__`.
- `confirm`: store `.confirm`; emit `Yes` (`true`) and `No` (`false`, rejection) structured options, preserving `message` as the prompt.
- `input`: store `.input`; emit a structured question with no options and `_tillerTextInput.placeholder`.
- `editor`: send `PiWire.extensionUICancel(id:)` immediately and do not emit a card.
- fire-and-forget methods: ignore without response.

Build synthetic structured raw input with stable ids using this helper:

```swift
private func structuredQuestionInput(
    header: String, prompt: String,
    options: [(id: String, label: String, isRejection: Bool)],
    textInput: ChatQuestion.TextInput? = nil
) -> JSONValue {
    let optionValues = options.map { option in
        JSONValue.object(["id": .string(option.id), "label": .string(option.label),
                          "isRejection": .bool(option.isRejection)])
    }
    var raw: [String: JSONValue] = ["questions": .array([.object([
        "header": .string(header), "question": .string(prompt),
        "options": .array(optionValues)
    ])])]
    if let textInput {
        var metadata: [String: JSONValue] = [:]
        if let placeholder = textInput.placeholder {
            metadata["placeholder"] = .string(placeholder)
        }
        if let prefill = textInput.prefill { metadata["prefill"] = .string(prefill) }
        raw["_tillerTextInput"] = .object(metadata)
    }
    return .object(raw)
}
```

Each dialog event uses `.permissionRequested(requestId: .string(id), toolCall: syntheticUpdate, options: mirroredPermissionOptions)` with `toolCallId = "pi-ui-\(id)"`. These handlers populate `pendingUIRequests`; the complete answer writer installed in Task 2 then serializes the protocol-specific value, confirmation, or cancellation response without changing its wire shapes.

- [ ] **Step 6: Add canonical text submission to the app**

Add to `ChatController` beside the existing option answer method:

```swift
func answerQuestion(_ question: ChatQuestion, text: String) async {
    let answer = text.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !answer.isEmpty, let driver else { return }
    reducer.permissionResolved(requestId: question.requestId,
                               resolution: .selected(optionId: answer))
    rebuildPresentationSnapshot()
    await driver.answerPermission(
        requestId: question.requestId,
        outcome: .answered(optionId: answer,
                           updatedInput: .object(["choice": .string(answer)])))
    onStatusChange?(.running)
    persist()
}
```

Extend `QuestionCardView` with state initialized from the canonical prefill:

```swift
@State private var textAnswer: String

init(question: ChatQuestion, controller: ChatController) {
    self.question = question
    self.controller = controller
    _textAnswer = State(initialValue: question.textInput?.prefill ?? "")
}
```

Replace the unanswered `options` branch with this canonical control:

```swift
@ViewBuilder
private var unansweredControls: some View {
    if let input = question.textInput {
        HStack(spacing: 6) {
            TextField(input.placeholder ?? "Type an answer", text: $textAnswer)
                .textFieldStyle(.roundedBorder)
                .onSubmit { submitTextAnswer() }
            Button("Send") { submitTextAnswer() }
                .disabled(textAnswer.trimmingCharacters(
                    in: .whitespacesAndNewlines).isEmpty)
            Button("Cancel", role: .cancel) {
                Task {
                    await controller.answerPermission(
                        requestId: question.requestId, optionId: nil)
                }
            }
        }
    } else {
        options
    }
}

private func submitTextAnswer() {
    let answer = textAnswer
    guard !answer.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else { return }
    Task { await controller.answerQuestion(question, text: answer) }
}
```

In `body`, render `unansweredControls` instead of `options` for an unresolved question. Keep the existing option button implementation unchanged.

- [ ] **Step 7: Add an app-controller test for custom text**

Extend the private `ChatTestDriver` in `AppTests/ChatControllerTests.swift` with recorded outcomes and replace its existing initializer with a constructor that accepts `supportsStructuredAnswers`:

```swift
nonisolated let supportsStructuredAnswers: Bool
private(set) var permissionAnswers: [(JSONRPCID, PermissionOutcome)] = []

init(handle: SessionHandle, failsResume: Bool = false,
     promptEvents: [ACPSessionEvent] = [], holdPromptOpen: Bool = false,
     supportsStructuredAnswers: Bool = false) {
    let (events, continuation) = AsyncStream.makeStream(of: ACPSessionEvent.self)
    self.events = events
    self.continuation = continuation
    self.handle = handle
    self.failsResume = failsResume
    self.promptEvents = promptEvents
    self.holdPromptOpen = holdPromptOpen
    self.supportsStructuredAnswers = supportsStructuredAnswers
}

func answerPermission(requestId: JSONRPCID, outcome: PermissionOutcome) async {
    permissionAnswers.append((requestId, outcome))
}
```

Add the full controller test:

```swift
@Test func customTextQuestionSendsStructuredAnswer() async throws {
    let worktreeId = UUID()
    let (store, installStore, root) = try makeChatTestFixture(worktreeId: worktreeId)
    defer { try? FileManager.default.removeItem(at: root) }
    let driver = ChatTestDriver(
        handle: makeChatTestHandle(sessionId: "pi-session"),
        supportsStructuredAnswers: true)
    let controller = ChatController(
        tabId: UUID(), agentId: "pi", worktreeId: worktreeId,
        worktreePath: root.path, store: store, installStore: installStore,
        driverFactory: { _, _, _, _, _, _, _ in driver })
    await controller.start()

    var call = ToolCallItem(toolCallId: "pi-ui-input", title: "Branch",
                            kind: .other, status: .pending)
    call.rawInput = .object([
        "questions": .array([.object([
            "header": .string("Branch"), "question": .string("Branch name"),
            "options": .array([])
        ])]),
        "_tillerTextInput": .object(["placeholder": .string("feature/name")])
    ])
    call.permission = PermissionState(requestId: .string("ui-input"), options: [])
    let question = try #require(ChatQuestion.from(call))

    await controller.answerQuestion(question, text: " feature/pi ")

    #expect(await driver.permissionAnswers.last?.1 == .answered(
        optionId: "feature/pi",
        updatedInput: .object(["choice": .string("feature/pi")])))
}
```

- [ ] **Step 8: Run package and app tests**

Run:

```bash
cd Packages/TillerACP
swift test --filter ChatQuestionTests
swift test --filter PiDriverTests
cd ../..
Scripts/ci.sh
```

Expected: focused tests pass and the repository gate ends with `CI OK`.

- [ ] **Step 9: Commit**

```bash
git add Packages/TillerACP/Sources/TillerACP/Drivers/PiRPCDriver.swift \
        Packages/TillerACP/Tests/TillerACPTests/PiDriverTests.swift \
        Packages/TillerACP/Sources/TillerACP/ChatQuestion.swift \
        Packages/TillerACP/Tests/TillerACPTests/ChatQuestionTests.swift \
        App/Chat/ChatController.swift App/Chat/QuestionCardView.swift \
        AppTests/ChatControllerTests.swift
git commit -m "feat: bridge pi extension ui questions"
```

---

### Task 5: Register Pi as a native chat agent and pass the final gate

**Files:**

- Modify: `Packages/TillerACP/Sources/TillerACP/AgentDriverFactory.swift`
- Modify: `Packages/TillerACP/Tests/TillerACPTests/AgentDriverFactoryTests.swift`
- Modify: `App/AcpAgentCenter.swift`
- Modify: `AppTests/AcpAgentCenterTests.swift`

**Interfaces:**

- Consumes: `PiRPCDriver.launchTransport` and initializer from Task 2.
- Produces: native chat agent id `pi`; leaves legacy/registry `pi-acp` on the ACP path.

- [ ] **Step 1: Write failing factory tests**

Update `AgentDriverFactoryTests`:

```swift
@Test func transportKindRoutesNativeAndACPIds() {
    #expect(AgentDriverFactory.transportKind(for: "claude-acp") == .native)
    #expect(AgentDriverFactory.transportKind(for: "codex-acp") == .native)
    #expect(AgentDriverFactory.transportKind(for: "opencode") == .native)
    #expect(AgentDriverFactory.transportKind(for: "pi") == .native)
    #expect(AgentDriverFactory.transportKind(for: "pi-acp") == .acp)
    #expect(AgentDriverFactory.transportKind(for: "omp") == .acp)
    #expect(AgentDriverFactory.transportKind(for: "gemini") == .acp)
}

@Test func nativeBinaryUsesCanonicalNativeNames() {
    #expect(AgentDriverFactory.nativeBinary(for: "claude-acp") == "claude")
    #expect(AgentDriverFactory.nativeBinary(for: "codex") == "codex")
    #expect(AgentDriverFactory.nativeBinary(for: "opencode") == "opencode")
    #expect(AgentDriverFactory.nativeBinary(for: "pi") == "pi")
}

@Test func piFactoryReturnsNativeDriverWhenCLIIsAvailable() throws {
    let driver = makeDriver(agentId: "pi", store: try tempStore())
    #expect(driver is PiRPCDriver)
}
```

- [ ] **Step 2: Write failing agent-center tests**

In `AcpAgentCenterTests.nativeIdsAreBuiltInWhenTheirBinariesResolve`, change all three expected-native lists to:

```swift
["claude-acp", "codex-acp", "opencode", "pi"]
```

Add:

```swift
#expect(center.displayName(for: "pi") == "Pi")
```

Add a missing-binary assertion using `pathProbe: { $0 != "pi" }`:

```swift
#expect(center.statuses["pi"] == .builtin(available: false))
#expect(center.rows.first(where: { $0.id == "pi" })?.description
        == "Requires pi on PATH")
#expect(!center.installedAgents.contains { $0.id == "pi" })
```

- [ ] **Step 3: Run tests and confirm failure**

Run:

```bash
cd Packages/TillerACP
swift test --filter AgentDriverFactoryTests
cd ../..
Scripts/ci.sh
```

Expected: factory and center expectations fail because `pi` is not registered.

- [ ] **Step 4: Register the native driver**

In `AgentDriverFactory`:

- add `"pi"` to native cases in `transportKind(for:)`;
- map `"pi"` to binary `"pi"` in `nativeBinary(for:)`;
- add this `makeNativeDriver` case:

```swift
case "pi":
    let transport = PiRPCDriver.launchTransport(
        worktreePath: worktreePath, model: model,
        resumeSessionId: resumeSessionId, onStderrLine: onStderrLine)
    return PiRPCDriver(
        transport: transport, model: model, effort: effort,
        resumeSessionId: resumeSessionId)
```

In `AcpAgentCenter`:

```swift
private static let nativeAgentIDs = ["claude-acp", "codex-acp", "opencode", "pi"]
```

and add `case "pi": return "Pi"` to `displayName(for:)`.

- [ ] **Step 5: Run proactive diagnostics before builds**

Run `lsp_diagnostics` over these paths through the coding harness:

- `Packages/TillerACP/Sources/TillerACP/Drivers/PiWire.swift`
- `Packages/TillerACP/Sources/TillerACP/Drivers/PiRPCDriver.swift`
- `Packages/TillerACP/Sources/TillerACP/ChatQuestion.swift`
- `Packages/TillerACP/Sources/TillerACP/AgentDriverFactory.swift`
- `App/Chat/ChatController.swift`
- `App/Chat/QuestionCardView.swift`
- `App/AcpAgentCenter.swift`

Expected: no Swift errors.

- [ ] **Step 6: Run all focused package tests**

```bash
cd Packages/TillerACP
swift test --filter PiWireTests
swift test --filter PiDriverTests
swift test --filter ChatQuestionTests
swift test --filter AgentDriverFactoryTests
```

Expected: all focused suites pass.

- [ ] **Step 7: Run the repository verification gate**

```bash
cd /Users/enzopiopalmisano/Desktop/Progetti/tiller
Scripts/ci.sh
```

Expected final line: `CI OK`.

- [ ] **Step 8: Perform a real Pi RPC smoke test**

With a configured Pi model/account:

1. Start Tiller from Xcode.
2. Open a git worktree and choose **New Chat → Pi**.
3. Send `List the files in this project.` and verify streaming text and read/search tool cards.
4. Send a request that edits a disposable file and verify an edit diff card.
5. Trigger an installed extension using `ctx.ui.select`, then `ctx.ui.input`; verify selection, custom text and cancellation resume the agent.
6. Switch model and thinking level; send another prompt and verify the selected values remain visible.
7. Send an ordinary second message while Pi is working and verify it is steered; send a slash command while working and verify it executes as a prompt command.
8. Cancel a running prompt and verify the turn ends as cancelled.
9. Close and reopen the chat tab; verify `ChatSessionStore` passes the saved session file back and Pi resumes.
10. Kill the Pi child process and verify the chat reports disconnect without hanging.

- [ ] **Step 9: Commit registration and integration**

```bash
git add Packages/TillerACP/Sources/TillerACP/AgentDriverFactory.swift \
        Packages/TillerACP/Tests/TillerACPTests/AgentDriverFactoryTests.swift \
        App/AcpAgentCenter.swift AppTests/AcpAgentCenterTests.swift
git commit -m "feat: expose pi as a native chat agent"
```

- [ ] **Step 10: Verify the final working tree and diagnostics**

```bash
git status --short
git log --oneline -5
```

Expected: no uncommitted implementation files. Then run `lens_diagnostics(mode: "all")`; expected: no blocking errors in edited files.
