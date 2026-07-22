# Native Chat Transports Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Chat with Claude Code, Codex and OpenCode through their native wire protocols (stream-json, app-server JSON-RPC, HTTP+SSE) instead of ACP adapters; every other agent keeps ACP.

**Architecture:** A new `AgentDriver` protocol in `TillerACP` abstracts what `ChatController` needs from a chat backend. The existing `ACPSession` conforms to it unchanged; three new drivers implement it over the native protocols and map wire events into the existing canonical `SessionUpdate` / `ACPSessionEvent` model, so `TranscriptReducer` and the whole UI are untouched. A factory picks the driver by agent id. DB migration v13 adds per-session mode/model/effort/transport columns.

**Tech Stack:** Swift 6, swift-testing (`@Test`/`#expect`), GRDB (TillerPersistence), Foundation `Process`/`URLSession`. No new SPM dependencies.

**Spec:** `docs/superpowers/specs/2026-07-22-native-chat-transports-design.md`

## Global Constraints

- Package name stays `TillerACP` (rename to TillerChat deferred — cosmetic churn).
- Tests first (`swift-testing`, never XCTest). Run per-package: `cd Packages/TillerACP && swift test --filter <Name>`.
- Full gate before declaring done: `Scripts/ci.sh` → prints `CI OK`. PTY tests are flaky; retry up to 5-6 times.
- After adding files to the App target: `xcodegen generate`. Package-only files need no regen.
- Launching CLIs always goes through login shell `zsh -lc exec …` (PATH parity with terminal panes).
- Never touch user-global agent config (`~/.claude/settings.json`, `~/.codex/config.toml`, …).
- Codex Full-auto always pairs `approvalPolicy: never` with `sandbox: workspace-write`, never `danger-full-access`.
- No secrets in argv. stderr logs must not dump the environment.
- Commit style: Conventional Commits, lower-case imperative.
- UI strings in English.

## Wire-protocol ground truth

The three protocols are re-implemented from fixtures captured from the real CLIs, not
from documentation. Tasks 4, 7 and 9 start by running a capture script and committing
the recorded NDJSON/SSE under `Packages/TillerACP/Tests/TillerACPTests/Fixtures/`.
Where this plan shows wire JSON, treat the shape as expected but **trust the fixture**:
if a captured message differs, adapt the decoder and the fixture-based test, not the
capture. This is the empirical-verification requirement from the spec.

---

### Task 1: `AgentDriver` protocol + ACP conformance

**Files:**
- Create: `Packages/TillerACP/Sources/TillerACP/AgentDriver.swift`
- Test: `Packages/TillerACP/Tests/TillerACPTests/AgentDriverTests.swift`

**Interfaces:**
- Consumes: `ACPSession`, `ACPSessionEvent`, `SessionHandle`, `ContentBlock`, `StopReason`, `McpServerSpec`, `SessionConfigOption`, `PermissionOutcome`, `JSONRPCID` (all existing in TillerACP).
- Produces: `protocol AgentDriver` (exact shape below) and `extension ACPSession: AgentDriver`. Every later driver task conforms to this protocol; Task 11 makes ChatController hold `any AgentDriver`.

- [ ] **Step 1: Write the failing test**

```swift
// AgentDriverTests.swift
import Testing
@testable import TillerACP

struct AgentDriverTests {
    @Test func acpSessionConformsToAgentDriver() async throws {
        let transport = MockTransport()
        let session = ACPSession(
            client: ACPClient(transport: transport),
            fileSystem: DenyAllFileSystem())
        let driver: any AgentDriver = session   // compiles only with conformance
        _ = driver.events                        // protocol exposes the stream
    }
}
```

`DenyAllFileSystem` already exists in the test target (used by ACPSessionTests); if the
name differs, use whatever stub `ACPSessionTests.swift` constructs.

- [ ] **Step 2: Run test to verify it fails**

Run: `cd Packages/TillerACP && swift test --filter AgentDriverTests`
Expected: compile FAILURE — `cannot convert ACPSession to any AgentDriver` / `AgentDriver not found`.

- [ ] **Step 3: Write the protocol and conformance**

```swift
// AgentDriver.swift
import Foundation

/// What ChatController needs from a chat backend, regardless of wire protocol.
/// ACPSession satisfies it as-is; native drivers (Claude stream-json, Codex
/// app-server, OpenCode HTTP) implement the same surface and emit the same
/// canonical ACPSessionEvent/SessionUpdate stream.
public protocol AgentDriver: Actor {
    nonisolated var events: AsyncStream<ACPSessionEvent> { get }
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

extension ACPSession: AgentDriver {}
```

`ACPSession.connect` has a default parameter for `mcpServers`; the protocol requirement
without defaults is satisfied by the same method. If the compiler complains about the
default-argument mismatch, add a trivial forwarding method in an `ACPSession` extension.

- [ ] **Step 4: Run test to verify it passes**

Run: `cd Packages/TillerACP && swift test --filter AgentDriverTests`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add Packages/TillerACP/Sources/TillerACP/AgentDriver.swift Packages/TillerACP/Tests/TillerACPTests/AgentDriverTests.swift
git commit -m "feat: add AgentDriver protocol with ACPSession conformance"
```

---

### Task 2: `PermissionMode` + per-driver mapping

**Files:**
- Create: `Packages/TillerACP/Sources/TillerACP/PermissionMode.swift`
- Test: `Packages/TillerACP/Tests/TillerACPTests/PermissionModeTests.swift`

**Interfaces:**
- Produces:
  - `enum PermissionMode: String, CaseIterable, Sendable, Codable` — `ask`, `acceptEdits`, `plan`, `fullAuto`.
  - `PermissionMode.claudeValue: String`, `codexApprovalPolicy: String`, `codexSandbox: String`.
  - `static func supported(byDriverFor agentId: String) -> [PermissionMode]`
  - `var sessionMode: SessionMode` — bridges into the existing ACP `SessionModeState` so the current mode dropdown renders native modes with zero UI changes.

- [ ] **Step 1: Write the failing test**

```swift
import Testing
@testable import TillerACP

struct PermissionModeTests {
    @Test func claudeMapping() {
        #expect(PermissionMode.ask.claudeValue == "default")
        #expect(PermissionMode.acceptEdits.claudeValue == "acceptEdits")
        #expect(PermissionMode.plan.claudeValue == "plan")
        #expect(PermissionMode.fullAuto.claudeValue == "bypassPermissions")
    }

    @Test func codexMapping() {
        #expect(PermissionMode.ask.codexApprovalPolicy == "untrusted")
        #expect(PermissionMode.acceptEdits.codexApprovalPolicy == "on-request")
        #expect(PermissionMode.fullAuto.codexApprovalPolicy == "never")
        // Security invariant from the spec: full auto never escapes the workspace.
        #expect(PermissionMode.fullAuto.codexSandbox == "workspace-write")
    }

    @Test func supportedModesPerDriver() {
        #expect(PermissionMode.supported(byDriverFor: "claude-acp")
            == [.ask, .acceptEdits, .plan, .fullAuto])
        #expect(PermissionMode.supported(byDriverFor: "codex-acp")
            == [.ask, .acceptEdits, .fullAuto])
        #expect(PermissionMode.supported(byDriverFor: "opencode")
            == [.ask, .acceptEdits, .fullAuto])
        #expect(PermissionMode.supported(byDriverFor: "omp").isEmpty) // ACP: no dropdown
    }

    @Test func bridgesToSessionMode() {
        let mode = PermissionMode.acceptEdits.sessionMode
        #expect(mode.id == "acceptEdits")
        #expect(!mode.name.isEmpty)
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd Packages/TillerACP && swift test --filter PermissionModeTests`
Expected: compile FAILURE — `PermissionMode` not found.

- [ ] **Step 3: Implement**

```swift
// PermissionMode.swift
import Foundation

/// Tiller's unified permission mode (spec table). Raw values are persisted in
/// chatSession.permissionMode — do not rename cases.
public enum PermissionMode: String, CaseIterable, Sendable, Codable {
    case ask, acceptEdits, plan, fullAuto

    public var claudeValue: String {
        switch self {
        case .ask: "default"
        case .acceptEdits: "acceptEdits"
        case .plan: "plan"
        case .fullAuto: "bypassPermissions"
        }
    }

    public var codexApprovalPolicy: String {
        switch self {
        case .ask: "untrusted"
        case .acceptEdits: "on-request"
        case .plan: "untrusted" // unreachable: plan is not offered for codex
        case .fullAuto: "never"
        }
    }

    /// Full auto is always confined to the worktree (spec security section).
    public var codexSandbox: String {
        switch self {
        case .fullAuto: "workspace-write"
        default: "workspace-write"
        }
    }

    public var displayName: String {
        switch self {
        case .ask: "Ask"
        case .acceptEdits: "Accept edits"
        case .plan: "Plan"
        case .fullAuto: "Full auto"
        }
    }

    /// Bridge into the ACP mode model so the existing dropdown renders these.
    public var sessionMode: SessionMode {
        SessionMode(id: rawValue, name: displayName)
    }

    public static func supported(byDriverFor agentId: String) -> [PermissionMode] {
        switch AgentIdMigration.canonical(agentId) {
        case "claude-acp": [.ask, .acceptEdits, .plan, .fullAuto]
        case "codex-acp", "opencode": [.ask, .acceptEdits, .fullAuto]
        default: [] // ACP agents keep their agent-provided modes
        }
    }
}
```

Check `SessionMode`'s real memberwise init in `ACPTypes.swift` and match it (it may
carry a `description`). Adjust the bridge accordingly.

- [ ] **Step 4: Run test to verify it passes**

Run: `cd Packages/TillerACP && swift test --filter PermissionModeTests`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add Packages/TillerACP/Sources/TillerACP/PermissionMode.swift Packages/TillerACP/Tests/TillerACPTests/PermissionModeTests.swift
git commit -m "feat: add unified PermissionMode with per-driver mappings"
```

---

### Task 3: DB migration v13 + store accessors

**Files:**
- Modify: `Packages/TillerPersistence/Sources/TillerPersistence/` — the file registering migrations (search: `grep -rn "registerMigration" Packages/TillerPersistence/Sources`; v12 is the latest) and `ChatSessionRecord`.
- Modify: `Packages/TillerACP/Sources/TillerACP/ChatSessionStore.swift`
- Test: `Packages/TillerPersistence/Tests/.../MigrationTests` (follow the existing v12 test's file) and `Packages/TillerACP/Tests/TillerACPTests/ChatSessionStoreTests.swift`

**Interfaces:**
- Consumes: `PermissionMode` (Task 2) — stored by raw value, but persistence layer stores plain `String` to avoid a package dependency cycle.
- Produces:
  - `ChatSessionRecord` gains `permissionMode: String?`, `selectedModel: String?`, `selectedEffort: String?`, `transportKind: String` (default `"acp"`).
  - `ChatSessionStore.setSessionSettings(permissionMode:selectedModel:selectedEffort:sessionId:)` and `setTransportKind(_:sessionId:)`.

- [ ] **Step 1: Write the failing migration test** (in the persistence package, mirroring the v12 test style)

```swift
@Test func v13AddsSessionSettingsColumns() throws {
    let db = try makeMigratedDatabase() // existing helper in the test file
    try db.read { conn in
        let columns = try conn.columns(in: "chatSession").map(\.name)
        #expect(columns.contains("permissionMode"))
        #expect(columns.contains("selectedModel"))
        #expect(columns.contains("selectedEffort"))
        #expect(columns.contains("transportKind"))
    }
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cd Packages/TillerPersistence && swift test --filter v13`
Expected: FAIL — columns missing.

- [ ] **Step 3: Register the migration and extend the record**

In the migrations file, after v12:

```swift
migrator.registerMigration("v13") { db in
    try db.alter(table: "chatSession") { t in
        t.add(column: "permissionMode", .text)
        t.add(column: "selectedModel", .text)
        t.add(column: "selectedEffort", .text)
        t.add(column: "transportKind", .text).notNull().defaults(to: "acp")
    }
    // Existing sessions for the three native harnesses flip to native;
    // ids here are the canonical registry ids (AgentIdMigration).
    try db.execute(sql: """
        UPDATE chatSession SET transportKind = 'native'
        WHERE agentId IN ('claude-acp', 'codex-acp', 'opencode', 'claude', 'codex')
        """)
}
```

Add the four properties to `ChatSessionRecord` (optional `String`s, `transportKind: String = "acp"`), including `CodingKeys`/column definitions in whatever style the record already uses.

- [ ] **Step 4: Run migration test — PASS.** Also run the full persistence suite: `cd Packages/TillerPersistence && swift test`. Expected: PASS (v12 fixture DBs migrate cleanly through v13).

- [ ] **Step 5: Write the failing store test** (TillerACP)

```swift
@Test func sessionSettingsRoundTrip() throws {
    let store = try makeStore() // existing helper in ChatSessionStoreTests
    let record = try store.createSession(worktreeId: "w1", agentId: "claude-acp")
    try store.setSessionSettings(permissionMode: "acceptEdits",
                                 selectedModel: "claude-sonnet-5",
                                 selectedEffort: "high",
                                 sessionId: record.id)
    let reloaded = try store.latestSessionIncludingEmpty(worktreeId: "w1")
    #expect(reloaded?.permissionMode == "acceptEdits")
    #expect(reloaded?.selectedModel == "claude-sonnet-5")
    #expect(reloaded?.selectedEffort == "high")
}
```

If `ChatSessionStoreTests` has no `latestSessionIncludingEmpty`, fetch the record by
key through a small test helper instead — do not weaken `latestSession`'s has-items filter.

- [ ] **Step 6: Implement store accessors** (same pattern as `setContextUsage`)

```swift
public func setSessionSettings(permissionMode: String?, selectedModel: String?,
                               selectedEffort: String?, sessionId: String) throws {
    try database.write { db in
        guard var record = try ChatSessionRecord.fetchOne(db, key: sessionId) else { return }
        record.permissionMode = permissionMode
        record.selectedModel = selectedModel
        record.selectedEffort = selectedEffort
        try record.update(db)
    }
}
```

- [ ] **Step 7: Run both packages' tests — PASS.**

Run: `cd Packages/TillerPersistence && swift test && cd ../TillerACP && swift test --filter ChatSessionStore`

- [ ] **Step 8: Commit**

```bash
git add Packages/TillerPersistence Packages/TillerACP
git commit -m "feat: add v13 session settings columns and store accessors"
```

---

### Task 4: Claude wire fixtures + decoder

**Files:**
- Create: `Scripts/record-claude-fixture.sh`
- Create: `Packages/TillerACP/Tests/TillerACPTests/Fixtures/claude-init-turn.ndjson` (captured)
- Create: `Packages/TillerACP/Sources/TillerACP/Drivers/ClaudeWire.swift`
- Test: `Packages/TillerACP/Tests/TillerACPTests/ClaudeWireTests.swift`

**Interfaces:**
- Produces: `enum ClaudeWireMessage: Decodable` with cases `systemInit(ClaudeInit)`, `assistant(ClaudeAssistantMessage)`, `user(ClaudeUserMessage)`, `result(ClaudeResult)`, `controlRequest(id: String, ClaudeControlRequest)`, `controlResponse(id: String, JSONValue?)`, `unknown(String)`; plus `ClaudeInit` (`sessionId`, `model`, `tools: [String]`, `slashCommands: [String]`), `ClaudeResult` (`sessionId`, `usage: ClaudeUsage?`, `isError: Bool`), `ClaudeControlRequest` (`subtype: String`, `toolName: String?`, `input: JSONValue?`, `permissionSuggestions: JSONValue?`). Task 5 consumes all of these.

- [ ] **Step 1: Write the capture script**

```bash
#!/bin/bash
# Records one Claude stream-json conversation turn into a fixture file.
# Requires: claude CLI installed and authenticated. Run from repo root.
set -euo pipefail
out="Packages/TillerACP/Tests/TillerACPTests/Fixtures/claude-init-turn.ndjson"
mkdir -p "$(dirname "$out")"
printf '{"type":"user","message":{"role":"user","content":[{"type":"text","text":"Run: echo hello. Then reply with one word."}]}}\n' \
  | claude -p --input-format stream-json --output-format stream-json --verbose \
      --permission-mode acceptEdits --model claude-haiku-4-5-20251001 \
  > "$out"
echo "captured $(wc -l < "$out") lines -> $out"
```

- [ ] **Step 2: Run it and inspect**

Run: `chmod +x Scripts/record-claude-fixture.sh && Scripts/record-claude-fixture.sh && head -c 2000 Packages/TillerACP/Tests/TillerACPTests/Fixtures/claude-init-turn.ndjson`
Expected: first line is `{"type":"system","subtype":"init",...}` containing `session_id`, `model`, `tools`, `slash_commands`; last line is `{"type":"result",...}` with `usage`. **Read the captured lines carefully — they are the contract the decoder implements.** Redact nothing (fixture runs in a scratch dir with no secrets), but check before committing.

- [ ] **Step 3: Write the failing decoder test**

```swift
import Testing
import Foundation
@testable import TillerACP

struct ClaudeWireTests {
    func fixtureLines() throws -> [Data] {
        let url = Bundle.module.url(forResource: "claude-init-turn",
                                    withExtension: "ndjson")!
        return try String(contentsOf: url, encoding: .utf8)
            .split(separator: "\n").map { Data($0.utf8) }
    }

    @Test func decodesInitAndResult() throws {
        let messages = try fixtureLines().map {
            try JSONDecoder().decode(ClaudeWireMessage.self, from: $0)
        }
        guard case .systemInit(let initMsg) = messages.first else {
            Issue.record("first message is not init"); return
        }
        #expect(!initMsg.sessionId.isEmpty)
        #expect(!initMsg.model.isEmpty)
        guard case .result(let result) = messages.last else {
            Issue.record("last message is not result"); return
        }
        #expect(result.sessionId == initMsg.sessionId)
        #expect(result.usage != nil)
    }

    @Test func unknownTypesDecodeAsUnknown() throws {
        let data = Data(#"{"type":"totally_new_thing"}"#.utf8)
        let message = try JSONDecoder().decode(ClaudeWireMessage.self, from: data)
        guard case .unknown = message else { Issue.record("expected unknown"); return }
    }
}
```

Register the Fixtures directory as a test resource in `Packages/TillerACP/Package.swift`
(`resources: [.copy("Fixtures")]` on the test target) if not already present.

- [ ] **Step 4: Run to verify it fails, then implement `ClaudeWire.swift`**

Decode by `type` discriminator with snake_case keys (`session_id`, `slash_commands`,
`permission_suggestions`, `request_id`), tolerant of missing fields (`unknown` for
unrecognized `type`). Shape every struct to the captured fixture, not to this plan.

- [ ] **Step 5: Run to verify PASS, commit**

```bash
git add Scripts/record-claude-fixture.sh Packages/TillerACP
git commit -m "feat: add claude stream-json wire decoder with recorded fixture"
```

---

### Task 5: ClaudeStreamJSONDriver

**Files:**
- Create: `Packages/TillerACP/Sources/TillerACP/Drivers/ClaudeStreamJSONDriver.swift`
- Test: `Packages/TillerACP/Tests/TillerACPTests/ClaudeDriverTests.swift`

**Interfaces:**
- Consumes: `ClaudeWireMessage` et al. (Task 4), `AgentDriver` (Task 1), `PermissionMode` (Task 2), `ACPTransport`/`MockTransport`, `SessionUpdate`, `ToolCall`/`ToolCallUpdate`, `ContentBlock`.
- Produces: `actor ClaudeStreamJSONDriver: AgentDriver`, `init(transport: any ACPTransport, permissionMode: PermissionMode, model: String?, resumeSessionId: String?)`, plus a factory `static func launchTransport(worktreePath: String, permissionMode: PermissionMode, model: String?, resumeSessionId: String?) -> ProcessTransport` that builds `zsh -lc exec claude -p --input-format stream-json --output-format stream-json --verbose --permission-mode <mode> [--model <m>] [--resume <id>]`. Task 11 consumes both.

Behavior contract (all covered by tests below):

1. `connect` waits for the `init` message → `SessionHandle(sessionId: init.session_id, modes: synthesized from PermissionMode.supported, models: from init.model, didResume: resumeSessionId != nil)`. Emits `.update(.availableCommandsUpdate(...))` from `slash_commands`.
2. `prompt(blocks)` writes a `{"type":"user",...}` line; streams `assistant` message content blocks as `.update(.agentMessageChunk(...))` (text), `.update(.toolCall(...))` (tool_use blocks, id = tool_use id, title = tool name + primary input); `user` messages containing `tool_result` map to `.update(.toolCallUpdate(...))` with `status: completed` and output content. Thinking blocks map to `.agentThoughtChunk`. Returns when `result` arrives: `.endTurn` (or `.cancelled` after `cancel()`); result usage maps to `.update(.usageUpdate(ContextUsage(...)))` when context tokens are present.
3. Incoming `control_request` with subtype `can_use_tool` → `.permissionRequested(requestId:, toolCall:, options:)` with synthesized options (`allow_once`, `reject_once`; plus `allow_always` when the request carries permission suggestions). `answerPermission` writes the matching `control_response` (`{"behavior":"allow"}` / `{"behavior":"deny"}`).
4. `setMode` sends a client→CLI `control_request` `{"subtype":"set_permission_mode","mode":<claudeValue>}` and emits `.update(.currentModeUpdate(mode.rawValue))` on success. `setModel` same with `set_model`. `cancel` sends `{"subtype":"interrupt"}`.
5. Transport line stream ending → `.disconnected`.

- [ ] **Step 1: Write the failing tests.** Reuse `MockTransport` (script incoming lines, record outgoing). Core tests — write all of these:

```swift
struct ClaudeDriverTests {
    // Feed the recorded fixture through MockTransport; expect connect() to
    // return the fixture's session id and prompt() to produce, in order:
    // at least one .agentMessageChunk, one .toolCall, one .toolCallUpdate
    // (completed), and finish with StopReason.endTurn.
    @Test func fixtureTurnMapsToCanonicalUpdates() async throws { ... }

    // Emit a can_use_tool control_request line; expect .permissionRequested
    // with allow/reject options; answerPermission(.selected("allow_once"))
    // must write a control_response line with behavior "allow" and the same
    // request_id.
    @Test func canUseToolRoundTrip() async throws { ... }

    // setMode(.acceptEdits) writes control_request set_permission_mode with
    // mode "acceptEdits"; a scripted success control_response resolves it and
    // the driver emits currentModeUpdate("acceptEdits").
    @Test func setModeWritesControlRequest() async throws { ... }

    // Transport EOF mid-turn: prompt() throws or returns, and .disconnected
    // is emitted exactly once.
    @Test func eofEmitsDisconnected() async throws { ... }

    // launchTransport builds the expected zsh -lc exec command line.
    @Test func launchCommandLine() {
        let transport = ClaudeStreamJSONDriver.launchTransport(
            worktreePath: "/tmp/w", permissionMode: .plan,
            model: "claude-sonnet-5", resumeSessionId: "abc")
        #expect(transport.arguments.last!.contains("--permission-mode plan"))
        #expect(transport.arguments.last!.contains("--resume abc"))
        #expect(transport.arguments.last!.contains("--model claude-sonnet-5"))
    }
}
```

Flesh each `...` out with real event collection: pump `driver.events` into an array via a
task, then assert ordered expectations (same style as `ACPSessionTests`).

- [ ] **Step 2: Run to verify failure** (`swift test --filter ClaudeDriverTests`) — types missing.

- [ ] **Step 3: Implement the driver.** Single actor, ~250 lines. Internal structure: a
`run()` read-loop task decoding `ClaudeWireMessage` per line; `pending` continuations for
control responses keyed by request id (mirror `ACPClient`'s correlation pattern); a
`promptContinuation` resolved by `result`. Tool-call mapping: build `ToolCall` with
`id`, `title`, `kind` derived from tool name (`Bash` → execute, `Edit`/`Write` → edit,
`Read` → read, rest → other), `status: .inProgress`, raw input attached the way
`ToolCall`'s existing initializer expects (check `ToolCall.swift` for exact fields).

- [ ] **Step 4: Run tests until PASS.** Then run the whole package: `swift test`. Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add Packages/TillerACP
git commit -m "feat: add ClaudeStreamJSONDriver mapping stream-json to canonical updates"
```

---

### Task 6: Claude context usage + effort prefix

**Files:**
- Modify: `Packages/TillerACP/Sources/TillerACP/Drivers/ClaudeStreamJSONDriver.swift`
- Test: extend `Packages/TillerACP/Tests/TillerACPTests/ClaudeDriverTests.swift`

**Interfaces:**
- Produces: `func setEffort(_ effort: String?)` on the driver (stored; next `prompt` prepends the prefix), context usage emission after each result.

- [ ] **Step 1: Failing tests**

```swift
// After a result message with usage, the driver sends control_request
// get_context_usage; a scripted response {"used":1000,"size":200000} makes it
// emit .update(.usageUpdate(ContextUsage(used: 1000, size: 200000))).
// If the CLI answers with an error (older versions), no usageUpdate is emitted
// and nothing crashes — graceful degradation.
@Test func contextUsageProbeAfterResult() async throws { ... }

// setEffort("high") then prompt(["hi"]) → the outgoing user line's text starts
// with the effort prefix, and the prefix is NOT echoed into the transcript
// (userMessageChunk suppressed for the injected prefix — the caller renders
// the user's original text).
@Test func effortPrefixInjectedIntoPrompt() async throws { ... }
```

- [ ] **Step 2: Run — FAIL. Step 3: Implement.** Effort prefix (t3code's trick, adapted):

```swift
private func effortPrefix(_ effort: String) -> String {
    "Reasoning effort for this and following turns: \(effort). "
}
```

Probe subtype: try `get_context_usage`; if the real CLI (fixture capture from Task 4 —
re-run with a second capture that includes the probe if needed) names it differently,
follow the wire, and leave a one-line comment with the CLI version probed.

- [ ] **Step 4: PASS + package suite green. Step 5: Commit**

```bash
git commit -am "feat: add claude context usage probe and effort prompt prefix"
```

---

### Task 7: Codex app-server driver

**Files:**
- Create: `Scripts/record-codex-fixture.sh` (same pattern as Task 4: run `codex app-server`, drive `initialize` → `newConversation` → `sendUserTurn` with a trivial prompt via a here-doc, tee traffic to `Fixtures/codex-init-turn.ndjson`)
- Create: `Packages/TillerACP/Sources/TillerACP/Drivers/CodexAppServerDriver.swift`
- Create: `Fixtures/codex-init-turn.ndjson` (captured)
- Test: `Packages/TillerACP/Tests/TillerACPTests/CodexDriverTests.swift`

**Interfaces:**
- Consumes: `ACPClient` (reused as-is — codex app-server is plain JSON-RPC over stdio lines, exactly what ACPClient speaks), `ProcessTransport`, `AgentDriver`, `PermissionMode`.
- Produces: `actor CodexAppServerDriver: AgentDriver`, `init(client: ACPClient, permissionMode: PermissionMode, model: String?, effort: String?, resumeConversationId: String?)`, `static func launchTransport(worktreePath: String) -> ProcessTransport` (`zsh -lc exec codex app-server`).

Behavior contract:

1. `connect`: `initialize` request (client info params), then `resumeConversation(conversationId:)` when a resume id is given — on error fall back to `newConversation` — else `newConversation` with `{cwd, model?, approvalPolicy: mode.codexApprovalPolicy, sandbox: mode.codexSandbox}`. Handle = conversation id; `didResume` accordingly. Subscribe to conversation events (`addConversationListener` request if the protocol requires explicit subscription — the fixture decides).
2. Event notifications map: `agent_message_delta` → `.agentMessageChunk`; `agent_reasoning_delta`/`agent_reasoning` → `.agentThoughtChunk`; `exec_command_begin` → `.toolCall` (kind execute, title = joined command); `exec_command_output_delta` → `.toolCallUpdate` (content append); `exec_command_end` → `.toolCallUpdate` (completed/failed by exit code); `patch_apply_begin/end` → edit-kind tool call pair; `token_count` → `.usageUpdate` (used = total tokens in context, size = model context window from the event or handle); `task_complete` → resolves `prompt` with `.endTurn`; `turn_aborted` → `.cancelled`.
3. Server→client requests `execCommandApproval` / `applyPatchApproval` → `.permissionRequested` with allow/reject options; `answerPermission` responds `{"decision":"approved"}` / `{"decision":"denied"}` on the JSON-RPC id.
4. `prompt` sends `sendUserTurn` with text items + stored model/effort/mode; `setMode`/`setModel`/`setEffort` update stored values applied on the next turn (Codex policy is per-turn — spec) and emit `.currentModeUpdate` immediately.
5. `stop` terminates the transport; EOF → `.disconnected`.

- [ ] **Step 1: Capture fixture, inspect, commit shapes.** Run the script; verify method/notification names against what the plan assumed; adjust the mapping table comments in the test to the real names.

- [ ] **Step 2: Write failing tests** — same structure as ClaudeDriverTests: fixture replay → ordered canonical updates; approval round-trip; per-turn mode application (`setMode` then `prompt` → outgoing `sendUserTurn` carries the new approvalPolicy); EOF → disconnected.

- [ ] **Step 3: Implement (~250 lines), reusing `ACPClient` for correlation. Step 4: PASS + package green. Step 5: Commit**

```bash
git commit -am "feat: add CodexAppServerDriver over app-server json-rpc"
```

---

### Task 8: OpenCode server connection abstraction

**Files:**
- Create: `Packages/TillerACP/Sources/TillerACP/Drivers/OpenCodeConnection.swift`
- Test: `Packages/TillerACP/Tests/TillerACPTests/OpenCodeConnectionTests.swift`

**Interfaces:**
- Produces:

```swift
/// HTTP+SSE surface of one `opencode serve` process. Protocol-ized so the
/// driver is testable without sockets (t3code's OpenCodeServerConnection idea).
public protocol OpenCodeConnection: Sendable {
    func request(method: String, path: String, body: Data?) async throws -> Data
    /// Server-sent events from GET /event, one JSON object per event.
    func events() -> AsyncThrowingStream<Data, Error>
    func close() async
}

/// Spawns `opencode serve --port 0 --hostname 127.0.0.1` via zsh -lc, parses
/// the bound URL from stdout, then serves URLSession-backed requests/SSE.
public final class OpenCodeProcessConnection: OpenCodeConnection { ... }
```

- [ ] **Step 1: Failing tests** for the pure parts (no process): URL parse (`parseServerURL(fromStdoutLine:)` extracts `http://127.0.0.1:PORT` from the CLI's startup line) and SSE framing (`SSEParser.feed(_:) -> [Data]` splits `data: {...}\n\n` frames across chunk boundaries, ignores `:heartbeat` comments).

```swift
@Test func parsesServerURL() {
    let url = OpenCodeProcessConnection.parseServerURL(
        fromStdoutLine: "opencode server listening on http://127.0.0.1:53422")
    #expect(url?.absoluteString == "http://127.0.0.1:53422")
}

@Test func sseParserHandlesSplitFrames() {
    var parser = SSEParser()
    let first = parser.feed(Data("data: {\"a\":".utf8))
    let second = parser.feed(Data("1}\n\ndata: {\"b\":2}\n\n".utf8))
    #expect(first.isEmpty)
    #expect(second.count == 2)
}
```

- [ ] **Step 2: FAIL. Step 3: Implement** parser + connection (URLSession `bytes(for:)` for SSE, plain `data(for:)` for requests; process spawn mirrors `ProcessTransport`'s zsh/exec + cwd pattern). **Step 4: PASS. Step 5: Commit**

```bash
git commit -am "feat: add OpenCode server connection with sse parser"
```

---

### Task 9: OpenCodeHTTPDriver

**Files:**
- Create: `Scripts/record-opencode-fixture.sh` (start `opencode serve`, create session, send one prompt via curl, dump `/event` stream and responses to `Fixtures/opencode-init-turn/`)
- Create: `Packages/TillerACP/Sources/TillerACP/Drivers/OpenCodeHTTPDriver.swift`
- Test: `Packages/TillerACP/Tests/TillerACPTests/OpenCodeDriverTests.swift` (fake `OpenCodeConnection` scripted from fixtures)

**Interfaces:**
- Consumes: `OpenCodeConnection` (Task 8), `AgentDriver`, `PermissionMode`.
- Produces: `actor OpenCodeHTTPDriver: AgentDriver`, `init(connection: any OpenCodeConnection, permissionMode: PermissionMode, resumeSessionId: String?)`.

Behavior contract:

1. `connect`: reuse `resumeSessionId` (`ses_…`) directly when given (didResume = true — OpenCode scopes history by session id); else `POST /session` → new id. Model list from `GET /config/providers` → `SessionModelState`; config options (effort etc.) surfaced as `configOptions` exactly like the ACP adapter did.
2. `prompt`: `POST /session/{id}/message` with text parts + selected model/effort; SSE events for the session map: message part text deltas → `.agentMessageChunk`; reasoning parts → `.agentThoughtChunk`; tool parts → `.toolCall`/`.toolCallUpdate` (status from part state); step-finish/idle event for the session resolves `prompt` with `.endTurn`; usage payloads → `.usageUpdate`.
3. Permission events → `.permissionRequested` (requestId synthesized as `JSONRPCID.string(permissionID)`); `answerPermission` → `POST /session/{id}/permissions/{permissionID}` with `once`/`always`/`reject` derived from the chosen `PermissionOptionKind`.
4. `cancel` → `POST /session/{id}/abort`. `setMode` rebuilds permission rules for subsequent requests and emits `.currentModeUpdate`. `stop` closes the connection and kills the process.
5. Connection stream ending → `.disconnected`.

- [ ] **Step 1: Capture fixtures; adjust event names in the contract above to the capture.**
- [ ] **Step 2: Failing tests** — fixture replay through fake connection → ordered updates; permission round-trip asserts the exact reply path+body; resume path (given `ses_x`, no `POST /session` request is made); abort path.
- [ ] **Step 3: Implement (~220 lines). Step 4: PASS + package green. Step 5: Commit**

```bash
git commit -am "feat: add OpenCodeHTTPDriver over http+sse"
```

---

### Task 10: Driver factory + transport kind

**Files:**
- Create: `Packages/TillerACP/Sources/TillerACP/AgentDriverFactory.swift`
- Test: `Packages/TillerACP/Tests/TillerACPTests/AgentDriverFactoryTests.swift`

**Interfaces:**
- Consumes: all drivers (Tasks 1, 5, 7, 9), `AgentLaunchSpec`, `AgentInstallStore`, `PermissionMode`.
- Produces:

```swift
public enum AgentTransportKind: String, Sendable, Codable { case native, acp }

public enum AgentDriverFactory {
    public static func transportKind(for agentId: String) -> AgentTransportKind
    /// nil when the agent is neither native nor ACP-installed → caller shows
    /// the existing "Agent not installed" state.
    public static func makeDriver(
        agentId: String, worktreePath: String,
        installStore: AgentInstallStore,
        permissionMode: PermissionMode, model: String?, effort: String?,
        resumeSessionId: String?) -> (any AgentDriver)?
}
```

- Native ids: `claude-acp`, `codex-acp`, `opencode` (post-`AgentIdMigration.canonical`). Native drivers need the CLI on PATH, not an install-store manifest: availability check is `zsh -lc "command -v claude"` (cached per launch) — expose `static func nativeBinary(for agentId: String) -> String` (`claude`/`codex`/`opencode`) so the App layer can build install hints.

- [ ] **Step 1: Failing tests** — kind routing (three native ids + omp/pi → acp), factory returns `ClaudeStreamJSONDriver` for claude-acp, `ACPSession` for omp (with manifest present in a temp install store), nil for an unknown uninstalled id.
- [ ] **Step 2: FAIL. Step 3: Implement.** ACP branch reuses today's `AgentLaunchSpec.resolved` + `ProcessTransport` + `ACPSession` wiring (move it here from ChatController mostly verbatim — Task 11 deletes it there).
- [ ] **Step 4: PASS. Step 5: Commit**

```bash
git commit -am "feat: add AgentDriverFactory routing native vs acp transports"
```

---

### Task 11: ChatController on AgentDriver + settings persistence + legacy resume

**Files:**
- Modify: `App/Chat/ChatController.swift` (start/send/stop paths; `private var session: ACPSession?` becomes `private var driver: (any AgentDriver)?`)
- Test: `App/Tests/…` — extend the existing ChatController test file (find it: `grep -rln "ChatController" App/Tests`)

**Interfaces:**
- Consumes: `AgentDriverFactory` (Task 10), `PermissionMode` (2), store accessors (3).
- Produces (for Spec 2's UI): `ChatController.permissionMode: PermissionMode?` (nil for ACP agents — dropdown hidden), `func setPermissionMode(_:)`, `func setEffort(_:)`, persisted via `setSessionSettings` at every change; existing `modes`/`models`/`effortOption`/`contextUsage` properties keep their types and meaning.

Key edits (surgical, following the existing structure):

1. `start()`: replace transport/session construction with `AgentDriverFactory.makeDriver(...)`, passing `permissionMode` (from record or `.ask`), `selectedModel`, `selectedEffort`, and `resumeSessionId` = `record?.acpSessionId` under the existing same-agent guard. `transportKind` written back via store.
2. Native session id from `SessionHandle` keeps flowing into `setACPSessionId` (the column now stores "the driver's resume token" for both kinds — claude session id / codex conversation id / opencode ses_ id; no schema change needed beyond v13).
3. Resume failure path (native driver throws on resume, factory retried without resume id): keep old transcript in `restored`, emit a `TranscriptItem.systemNotice("Session resumed as history — new conversation started.")`, continue with the fresh handle. Mirror of today's ACP fallback.
4. `setPermissionMode(_ mode:)`: `try await driver.setMode(mode.rawValue)`, update local state + persist. `currentModeUpdate` events already flow through the reducer → no extra UI plumbing.
5. `pendingHandoff` / agent-switch logic untouched.

- [ ] **Step 1: Failing tests** (hosted App tests — NEVER `UserDefaults.standard`, use the injected suite like existing AppTests):
  - controller with a stub driver (conform a test actor to `AgentDriver`) reaches `.ready`, forwards prompt, folds events — proves the protocol seam.
  - permission mode set → driver received `setMode("acceptEdits")` and store row updated.
  - resume-failure path → transcript keeps restored items and contains the history systemNotice.
- [ ] **Step 2: FAIL. Step 3: Implement edits. Step 4: `cd` App tests via `Scripts/ci.sh` (full gate, retries allowed). Expected: CI OK. Step 5: Commit**

```bash
git commit -am "feat: drive chat through AgentDriver with native transports and legacy resume"
```

---

### Task 12: Registry/UI wiring for native agents + adapter cutover

**Files:**
- Modify: `App/AcpAgentCenter.swift` (native ids listed as built-in — installed when the CLI resolves on PATH; install/update buttons hidden for them, replaced by "requires <cli> on PATH" hint when missing)
- Modify: `Packages/TillerACP/Sources/TillerACP/AgentLaunchSpec.swift` — `resolved(id:installStore:)` returns nil for native ids (they no longer launch through ACP manifests)
- Test: extend `AgentInstallStoreTests` / the AcpAgentCenter test file

**Interfaces:**
- Consumes: `AgentDriverFactory.transportKind`, `nativeBinary(for:)` (Task 10).

- [ ] **Step 1: Failing tests** — `AgentLaunchSpec.resolved(id: "claude-acp", …)` returns nil even with a manifest present; agent center marks `claude-acp` available when a fake PATH probe returns true, unavailable with hint otherwise.
- [ ] **Step 2: FAIL. Step 3: Implement. Step 4: tests PASS, `xcodegen generate` if files were added, `Scripts/ci.sh` → CI OK. Step 5: Commit**

```bash
git commit -am "feat: route native agents through path probe and cut acp adapters over"
```

---

### Task 13: Full gate + manual smoke checklist

**Files:**
- Modify: `docs/superpowers/plans/2026-07-22-native-chat-transports.md` (check off), memory update.

- [ ] **Step 1:** `Scripts/ci.sh` until `CI OK` (PTY retries allowed).
- [ ] **Step 2:** Manual smoke (real CLIs, from the spec):
  - one tool-using turn per native driver (Claude, Codex, OpenCode)
  - live permission-mode switch (Claude immediate; Codex next turn)
  - model switch mid-session (Claude `set_model`)
  - context meter moves after a turn
  - mid-turn cancel
  - `kill -9` the CLI process → chat shows disconnected banner, no crash
  - open a pre-migration session per agent → resumes, or shows history notice
  - one ACP agent (omp) → unchanged behavior end to end
- [ ] **Step 3: Commit any fixes; final commit**

```bash
git commit -am "chore: native chat transports smoke fixes"
```

---

## Self-review notes

- Spec coverage: drivers (T4-9), abstraction (T1), factory (T10), permission modes (T2, applied T5/T7/T9/T11), v13 + settings (T3, persisted T11), legacy resume + banner (T11), adapter cutover (T12), security invariants (T2 test + global constraints), stderr logging stays on the existing `onStderrLine` NSLog path (ProcessTransport) — per-session file logging is deliberately dropped to YAGNI; NSLog + Console.app suffices today.
- Fixture-first discipline replaces guessed wire shapes; every driver task starts at the capture script.
- Out of scope (per spec): multi-account HOME isolation, minimap, Spec 2 UI work.
