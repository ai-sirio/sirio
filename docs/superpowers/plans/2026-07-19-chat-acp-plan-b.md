# Piano B — Terminali in chat via estensione `_meta terminal_output`

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** rendere visibile in chat l'output dei comandi eseguiti dall'agente (Bash tool, anche background) come card terminale con streaming live, aderendo all'estensione `_meta terminal_output` di claude-agent-acp (sezione 3 rivista della spec `docs/superpowers/specs/2026-07-19-chat-acp-feature-complete-design.md`).

**Architecture:** Tiller non esegue processi: dichiara la capability `_meta {"terminal_output": true}` nell'`initialize` e decodifica tre payload `_meta` sulle notifiche tool_call/tool_call_update (`terminal_info` → id, `terminal_output` → chunk testo, `terminal_exit` → exit status). Lo stato si accumula su `ToolCallItem` (già persistito); la UI è una vista monospace nel card esistente.

**Tech Stack:** Swift 6, swift-testing, SwiftUI. Nessuna dipendenza nuova.

## Global Constraints

- Test first (RED→GREEN), swift-testing (`@Test`/`#expect`), mai XCTest.
- `Scripts/ci.sh` → `CI OK` a fine piano (test PTY `spawnCapturesOutput` flaky: retry fino a 5-6 volte è normale).
- Commit conventional, soggetto imperativo minuscolo.
- Package test: `cd Packages/TillerACP && swift test [--filter <nome>]`.
- Build App: `xcodebuild -project Tiller.xcodeproj -scheme Tiller -configuration Debug -derivedDataPath DerivedData build -quiet` (exit 0). Dopo aver creato file nuovi in `App/`: `xcodegen generate` prima della build.
- Wire format verificato sul dist di claude-agent-acp 0.59.0: i tre `_meta` stanno DENTRO l'oggetto `update` della notifica `session/update`, sibling di `sessionUpdate`/`toolCallId`. Chiavi snake_case: `terminal_info.terminal_id`, `terminal_output.{terminal_id, data}`, `terminal_exit.{terminal_id, exit_code, signal}` (exit_code numero o null, signal stringa o null).

---

### Task 1: Tipi wire — TerminalMeta, ToolCallContent.terminal, decodifica `_meta`

**Files:**
- Modify: `Packages/TillerACP/Sources/TillerACP/ToolCall.swift`
- Test: `Packages/TillerACP/Tests/TillerACPTests/SessionUpdateTests.swift`

**Interfaces:**
- Consumes: `SessionUpdate` decoding esistente (i casi `tool_call`/`tool_call_update` decodificano `ToolCall`/`ToolCallUpdate` dallo stesso decoder).
- Produces:
  - `public struct TerminalExitStatus: Sendable, Equatable, Codable { public var exitCode: Int?; public var signal: String?; public init(exitCode: Int? = nil, signal: String? = nil) }`
  - `public struct TerminalMeta: Sendable, Equatable, Codable` con `terminalInfo: Info?` (`Info.terminalId`), `terminalOutput: Output?` (`Output.terminalId/.data`), `terminalExit: Exit?` (`Exit.terminalId/.exitCode/.signal`)
  - `ToolCall.terminalMeta: TerminalMeta?` e `ToolCallUpdate.terminalMeta: TerminalMeta?` (chiave wire `_meta`)
  - Nuovo case `ToolCallContent.terminal(terminalId: String)`

- [ ] **Step 1: Scrivere i test che falliscono**

Aggiungere a `SessionUpdateTests.swift`:

```swift
/// tool_call Bash: content {type:"terminal"} + _meta.terminal_info.
@Test func decodesTerminalContentAndInfoMeta() throws {
    let json = #"""
    {"sessionUpdate": "tool_call", "toolCallId": "t1", "title": "Bash",
     "kind": "execute", "status": "in_progress",
     "content": [{"type": "terminal", "terminalId": "t1"}],
     "_meta": {"terminal_info": {"terminal_id": "t1"}}}
    """#
    let update = try JSONDecoder().decode(SessionUpdate.self,
                                          from: Data(json.utf8))
    guard case .toolCall(let call) = update else {
        Issue.record("expected toolCall"); return
    }
    #expect(call.content == [.terminal(terminalId: "t1")])
    #expect(call.terminalMeta?.terminalInfo?.terminalId == "t1")
}

/// tool_call_update con chunk di output in streaming.
@Test func decodesTerminalOutputMetaOnUpdate() throws {
    let json = #"""
    {"sessionUpdate": "tool_call_update", "toolCallId": "t1",
     "_meta": {"terminal_output": {"terminal_id": "t1", "data": "riga 1\n"}}}
    """#
    let update = try JSONDecoder().decode(SessionUpdate.self,
                                          from: Data(json.utf8))
    guard case .toolCallUpdate(let partial) = update else {
        Issue.record("expected toolCallUpdate"); return
    }
    #expect(partial.terminalMeta?.terminalOutput?.data == "riga 1\n")
}

/// tool_call_update finale con exit status (exit_code può essere null).
@Test func decodesTerminalExitMeta() throws {
    let json = #"""
    {"sessionUpdate": "tool_call_update", "toolCallId": "t1",
     "status": "completed",
     "_meta": {"terminal_exit": {"terminal_id": "t1", "exit_code": 0,
               "signal": null}}}
    """#
    let update = try JSONDecoder().decode(SessionUpdate.self,
                                          from: Data(json.utf8))
    guard case .toolCallUpdate(let partial) = update else {
        Issue.record("expected toolCallUpdate"); return
    }
    #expect(partial.terminalMeta?.terminalExit?.exitCode == 0)
    #expect(partial.terminalMeta?.terminalExit?.signal == nil)
}

/// _meta con sole chiavi estranee (es. claudeCode) non deve rompere nulla.
@Test func ignoresForeignMetaKeys() throws {
    let json = #"""
    {"sessionUpdate": "tool_call_update", "toolCallId": "t1",
     "_meta": {"claudeCode": {"parentToolUseId": "x"}}}
    """#
    let update = try JSONDecoder().decode(SessionUpdate.self,
                                          from: Data(json.utf8))
    guard case .toolCallUpdate(let partial) = update else {
        Issue.record("expected toolCallUpdate"); return
    }
    #expect(partial.terminalMeta?.terminalOutput == nil)
    #expect(partial.terminalMeta?.terminalExit == nil)
}

/// Round-trip Codable del nuovo case terminal (persistenza transcript).
@Test func terminalContentRoundTrips() throws {
    let content = ToolCallContent.terminal(terminalId: "t9")
    let data = try JSONEncoder().encode(content)
    #expect(try JSONDecoder().decode(ToolCallContent.self, from: data) == content)
}
```

- [ ] **Step 2: Verificare che falliscano**

Run: `cd Packages/TillerACP && swift test --filter SessionUpdateTests`
Expected: FAIL di compilazione (`terminalMeta`, `.terminal` non esistono).

- [ ] **Step 3: Implementazione**

In `ToolCall.swift`:

Aggiungere i tipi (sopra `ToolCallContent`):

```swift
/// Exit status of an agent-side terminal command (claude-agent-acp
/// `_meta.terminal_exit`).
public struct TerminalExitStatus: Sendable, Equatable, Codable {
    public var exitCode: Int?
    public var signal: String?
    public init(exitCode: Int? = nil, signal: String? = nil) {
        self.exitCode = exitCode
        self.signal = signal
    }
}

/// claude-agent-acp terminal extension riding in `_meta` on
/// tool_call/tool_call_update: the SDK runs the command agent-side and
/// streams output here — the client never spawns a process.
public struct TerminalMeta: Sendable, Equatable, Codable {
    public struct Info: Sendable, Equatable, Codable {
        public var terminalId: String
        enum CodingKeys: String, CodingKey { case terminalId = "terminal_id" }
        public init(terminalId: String) { self.terminalId = terminalId }
    }
    public struct Output: Sendable, Equatable, Codable {
        public var terminalId: String
        public var data: String
        enum CodingKeys: String, CodingKey {
            case terminalId = "terminal_id", data
        }
        public init(terminalId: String, data: String) {
            self.terminalId = terminalId
            self.data = data
        }
    }
    public struct Exit: Sendable, Equatable, Codable {
        public var terminalId: String
        public var exitCode: Int?
        public var signal: String?
        enum CodingKeys: String, CodingKey {
            case terminalId = "terminal_id", exitCode = "exit_code", signal
        }
        public init(terminalId: String, exitCode: Int? = nil,
                    signal: String? = nil) {
            self.terminalId = terminalId
            self.exitCode = exitCode
            self.signal = signal
        }
    }

    public var terminalInfo: Info?
    public var terminalOutput: Output?
    public var terminalExit: Exit?

    enum CodingKeys: String, CodingKey {
        case terminalInfo = "terminal_info"
        case terminalOutput = "terminal_output"
        case terminalExit = "terminal_exit"
    }

    public init(terminalInfo: Info? = nil, terminalOutput: Output? = nil,
                terminalExit: Exit? = nil) {
        self.terminalInfo = terminalInfo
        self.terminalOutput = terminalOutput
        self.terminalExit = terminalExit
    }
}
```

In `ToolCallContent`: aggiungere il case e i rami Codable:

```swift
case terminal(terminalId: String)
```

- in `CodingKeys` aggiungere `terminalId`
- in `init(from:)`, nuovo caso: `case "terminal": self = .terminal(terminalId: try container.decode(String.self, forKey: .terminalId))`
- in `encode(to:)`: `case .terminal(let terminalId): try container.encode("terminal", forKey: .type); try container.encode(terminalId, forKey: .terminalId)`

In `ToolCall`: aggiungere `public var terminalMeta: TerminalMeta?`, la chiave `case terminalMeta = "_meta"` nel suo `CodingKeys`, `terminalMeta = try container.decodeIfPresent(TerminalMeta.self, forKey: .terminalMeta)` nell'`init(from:)`, e il parametro `terminalMeta: TerminalMeta? = nil` nell'init memberwise.

In `ToolCallUpdate`: aggiungere `public var terminalMeta: TerminalMeta?` con `CodingKeys` esplicito (tutte le chiavi esistenti invariate + `case terminalMeta = "_meta"`) e parametro `terminalMeta: TerminalMeta? = nil` nell'init.

- [ ] **Step 4: Verificare che passino**

Run: `cd Packages/TillerACP && swift test --filter SessionUpdateTests`
Expected: PASS, tutta la suite.

Run: `cd Packages/TillerACP && swift test`
Expected: PASS (nessuna regressione su ToolCall/TranscriptReducer).

- [ ] **Step 5: Commit**

```bash
git add Packages/TillerACP/Sources/TillerACP/ToolCall.swift \
        Packages/TillerACP/Tests/TillerACPTests/SessionUpdateTests.swift
git commit -m "feat: decode claude-agent-acp terminal _meta extension and terminal content blocks"
```

---

### Task 2: Stato terminale su ToolCallItem — accumulo output nel reducer

**Files:**
- Modify: `Packages/TillerACP/Sources/TillerACP/TranscriptItem.swift` (`ToolCallItem`, righe ~25-61)
- Modify: `Packages/TillerACP/Sources/TillerACP/TranscriptReducer.swift` (`upsert`, righe ~126-135)
- Test: `Packages/TillerACP/Tests/TillerACPTests/TranscriptReducerToolTests.swift`

**Interfaces:**
- Consumes: `TerminalMeta`, `TerminalExitStatus`, `ToolCallUpdate.terminalMeta` dal Task 1.
- Produces: `ToolCallItem.terminalOutput: String?` (output accumulato) e `ToolCallItem.terminalExit: TerminalExitStatus?`; `merge(_:)` accumula i chunk; `upsert` li preserva.

- [ ] **Step 1: Scrivere i test che falliscono**

Aggiungere a `TranscriptReducerToolTests.swift`:

```swift
/// I chunk _meta.terminal_output si accumulano sul ToolCallItem.
@Test func terminalOutputChunksAccumulate() {
    var reducer = TranscriptReducer()
    reducer.apply(.toolCall(ToolCall(
        toolCallId: "t1", title: "Bash", kind: .execute, status: .inProgress,
        content: [.terminal(terminalId: "t1")])))
    reducer.apply(.toolCallUpdate(ToolCallUpdate(
        toolCallId: "t1",
        terminalMeta: TerminalMeta(terminalOutput: .init(terminalId: "t1",
                                                          data: "riga 1\n")))))
    reducer.apply(.toolCallUpdate(ToolCallUpdate(
        toolCallId: "t1",
        terminalMeta: TerminalMeta(terminalOutput: .init(terminalId: "t1",
                                                          data: "riga 2\n")))))
    guard case .toolCall(let item) = reducer.items[0] else {
        Issue.record("expected toolCall"); return
    }
    #expect(item.terminalOutput == "riga 1\nriga 2\n")
}

/// terminal_exit registra l'exit status; un update successivo che rimpiazza
/// il content (upsert dal tool_call finale) non deve cancellare l'accumulo.
@Test func terminalExitAndContentReplacePreserveState() {
    var reducer = TranscriptReducer()
    reducer.apply(.toolCall(ToolCall(
        toolCallId: "t1", title: "Bash", kind: .execute, status: .inProgress,
        content: [.terminal(terminalId: "t1")])))
    reducer.apply(.toolCallUpdate(ToolCallUpdate(
        toolCallId: "t1",
        terminalMeta: TerminalMeta(terminalOutput: .init(terminalId: "t1",
                                                          data: "ok\n")))))
    reducer.apply(.toolCall(ToolCall(
        toolCallId: "t1", title: "Bash", kind: .execute, status: .completed,
        content: [.terminal(terminalId: "t1")])))
    reducer.apply(.toolCallUpdate(ToolCallUpdate(
        toolCallId: "t1", status: .completed,
        terminalMeta: TerminalMeta(terminalExit: .init(terminalId: "t1",
                                                        exitCode: 0)))))
    guard case .toolCall(let item) = reducer.items[0] else {
        Issue.record("expected toolCall"); return
    }
    #expect(item.terminalOutput == "ok\n")
    #expect(item.terminalExit == TerminalExitStatus(exitCode: 0, signal: nil))
    #expect(item.status == .completed)
}
```

- [ ] **Step 2: Verificare che falliscano**

Run: `cd Packages/TillerACP && swift test --filter TranscriptReducerToolTests`
Expected: FAIL di compilazione (`terminalOutput` non esiste su `ToolCallItem`).

- [ ] **Step 3: Implementazione**

In `TranscriptItem.swift`, su `ToolCallItem`:

- proprietà nuove (dopo `permission`):

```swift
/// Accumulated agent-side terminal output (claude-agent-acp _meta
/// extension); nil for tool calls without a terminal.
public var terminalOutput: String?
public var terminalExit: TerminalExitStatus?
```

- init memberwise: aggiungere `terminalOutput: String? = nil, terminalExit: TerminalExitStatus? = nil` e le assegnazioni.
- `merge(_ update: ToolCallUpdate)`, in coda:

```swift
if let meta = update.terminalMeta {
    if let chunk = meta.terminalOutput {
        terminalOutput = (terminalOutput ?? "") + chunk.data
    }
    if let exit = meta.terminalExit {
        terminalExit = TerminalExitStatus(exitCode: exit.exitCode,
                                          signal: exit.signal)
    }
}
```

In `TranscriptReducer.upsert`, accanto alla preservazione della permission:

```swift
merged.permission = existing.permission
merged.terminalOutput = existing.terminalOutput
merged.terminalExit = existing.terminalExit
```

- [ ] **Step 4: Verificare che passino**

Run: `cd Packages/TillerACP && swift test`
Expected: PASS, tutta la suite (i vecchi payload persistiti decodificano con i nuovi campi nil: proprietà opzionali, Codable sintetizzato).

- [ ] **Step 5: Commit**

```bash
git add Packages/TillerACP/Sources/TillerACP/TranscriptItem.swift \
        Packages/TillerACP/Sources/TillerACP/TranscriptReducer.swift \
        Packages/TillerACP/Tests/TillerACPTests/TranscriptReducerToolTests.swift
git commit -m "feat: accumulate terminal output and exit status on tool call items"
```

---

### Task 3: Capability `_meta terminal_output` nell'initialize

**Files:**
- Modify: `Packages/TillerACP/Sources/TillerACP/ACPTypes.swift` (`ClientCapabilities`, righe ~14-21)
- Modify: `Packages/TillerACP/Sources/TillerACP/ACPSession.swift` (`connect`, costruzione `InitializeParams`, righe ~69-76)
- Test: `Packages/TillerACP/Tests/TillerACPTests/ACPSessionTests.swift`

**Interfaces:**
- Consumes: `JSONValue` esistente; `FakeAgent`/`MockTransport` in `ACPSessionTests.swift`.
- Produces: `ClientCapabilities.meta: JSONValue?` (chiave wire `_meta`); l'initialize di Tiller dichiara `{"terminal_output": true}`.

- [ ] **Step 1: Scrivere il test che fallisce**

Aggiungere a `ACPSessionTests.swift` (stesso pattern di `connectSendsMcpServersInSessionNew`):

```swift
/// Tiller aderisce all'estensione terminale di claude-agent-acp:
/// initialize dichiara _meta {"terminal_output": true}.
@Test func initializeAdvertisesTerminalOutputMetaCapability() async throws {
    let agent = FakeAgent()
    let session = try await makeSession(agent: agent)
    _ = try await session.connect(cwd: "/w", resumeSessionId: nil)
    var sent: InitializeParams?
    for index in 0..<(await agent.mock.sent.count) {
        guard case .request(_, "initialize", let params) =
                try await agent.mock.sentMessage(index) else { continue }
        sent = try params?.decoded(InitializeParams.self)
    }
    #expect(sent?.clientCapabilities.meta
            == .object(["terminal_output": .bool(true)]))
}
```

- [ ] **Step 2: Verificare che fallisca**

Run: `cd Packages/TillerACP && swift test --filter initializeAdvertisesTerminalOutputMetaCapability`
Expected: FAIL di compilazione (`meta` non esiste su `ClientCapabilities`).

- [ ] **Step 3: Implementazione**

In `ACPTypes.swift`, `ClientCapabilities`:

```swift
public struct ClientCapabilities: Sendable, Equatable, Codable {
    public var fs: FileSystemCapability
    public var terminal: Bool
    /// Extension capabilities (claude-agent-acp reads
    /// `_meta.terminal_output` to stream Bash output into tool calls).
    public var meta: JSONValue?

    enum CodingKeys: String, CodingKey {
        case fs, terminal, meta = "_meta"
    }

    public init(fs: FileSystemCapability, terminal: Bool,
                meta: JSONValue? = nil) {
        self.fs = fs
        self.terminal = terminal
        self.meta = meta
    }
}
```

In `ACPSession.connect`, costruzione delle capabilities:

```swift
clientCapabilities: ClientCapabilities(
    fs: FileSystemCapability(readTextFile: true, writeTextFile: true),
    terminal: false,
    meta: .object(["terminal_output": .bool(true)]))
```

- [ ] **Step 4: Verificare che passino**

Run: `cd Packages/TillerACP && swift test`
Expected: PASS, tutta la suite.

- [ ] **Step 5: Commit**

```bash
git add Packages/TillerACP/Sources/TillerACP/ACPTypes.swift \
        Packages/TillerACP/Sources/TillerACP/ACPSession.swift \
        Packages/TillerACP/Tests/TillerACPTests/ACPSessionTests.swift
git commit -m "feat: advertise terminal_output meta capability in initialize"
```

---

### Task 4: TerminalOutputView e wiring nel card

**Files:**
- Create: `App/Chat/TerminalOutputView.swift`
- Modify: `App/Chat/ToolCallCardView.swift` (switch `contentBody`, righe ~78-92)

**Interfaces:**
- Consumes: `ToolCallItem.terminalOutput`/`.terminalExit` (Task 2), `ToolCallContent.terminal` (Task 1), `TerminalExitStatus`.
- Produces: `TerminalOutputView(output:exit:isRunning:)`. Nessun consumer successivo.

UI pura: gate = build. Dopo il file nuovo: `xcodegen generate`.

- [ ] **Step 1: Creare la vista**

`App/Chat/TerminalOutputView.swift`:

```swift
import SwiftUI
import TillerACP

/// Live output of an agent-side terminal command inside a tool call card:
/// monospace tail of the accumulated output plus a running/exit-code chip.
/// The process runs inside the agent — cancelling the turn is the only
/// interruption, so no kill button here.
struct TerminalOutputView: View {
    let output: String
    let exit: TerminalExitStatus?
    let isRunning: Bool

    private static let maxLines = 50

    private var tailLines: [Substring] {
        let lines = output.split(separator: "\n", omittingEmptySubsequences: false)
        return Array(lines.suffix(Self.maxLines))
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            statusChip
            if !output.isEmpty {
                VStack(alignment: .leading, spacing: 0) {
                    ForEach(Array(tailLines.enumerated()), id: \.offset) { _, line in
                        Text(line.isEmpty ? " " : String(line))
                            .font(.system(size: 12, design: .monospaced))
                            .frame(maxWidth: .infinity, alignment: .leading)
                    }
                }
                .textSelection(.enabled)
                .padding(6)
                .background(Color.black.opacity(0.85),
                            in: RoundedRectangle(cornerRadius: 6))
                .foregroundStyle(Color.white.opacity(0.92))
            }
        }
    }

    @ViewBuilder
    private var statusChip: some View {
        if isRunning {
            Label("in esecuzione", systemImage: "circle.dotted")
                .font(.caption2)
                .foregroundStyle(.orange)
        } else if let exitCode = exit?.exitCode {
            Label("exit \(exitCode)",
                  systemImage: exitCode == 0
                      ? "checkmark.circle" : "xmark.circle")
                .font(.caption2)
                .foregroundStyle(exitCode == 0 ? .green : .red)
        } else if let signal = exit?.signal {
            Label("segnale \(signal)", systemImage: "exclamationmark.triangle")
                .font(.caption2)
                .foregroundStyle(.orange)
        }
    }
}
```

- [ ] **Step 2: Wiring nel card**

In `ToolCallCardView.contentBody`, nel `switch content` aggiungere prima di `case .content, .unknown`:

```swift
case .terminal:
    TerminalOutputView(
        output: item.terminalOutput ?? "",
        exit: item.terminalExit,
        isRunning: item.status == .pending || item.status == .inProgress)
```

- [ ] **Step 3: Rigenerare progetto e buildare**

Run: `xcodegen generate && xcodebuild -project Tiller.xcodeproj -scheme Tiller -configuration Debug -derivedDataPath DerivedData build -quiet; echo "exit=$?"`
Expected: `exit=0`.

- [ ] **Step 4: Commit**

```bash
git add App/Chat/TerminalOutputView.swift App/Chat/ToolCallCardView.swift \
        Tiller.xcodeproj/project.pbxproj
git commit -m "feat: render agent terminal output as live card in chat transcript"
```

(Includere `Tiller.xcodeproj/project.pbxproj` solo se `git status` lo mostra modificato dopo xcodegen.)

---

### Task 5: Gate finale

- [ ] **Step 1: CI completa**

Run: `Scripts/ci.sh`
Expected: `CI OK` (retry sul flaky PTY se serve).

- [ ] **Step 2: Rebuild e rilancio app**

```bash
xcodebuild -project Tiller.xcodeproj -scheme Tiller -configuration Debug \
  -derivedDataPath DerivedData build -quiet
pkill -x Tiller; sleep 1; open DerivedData/Build/Products/Debug/Tiller.app
```

Verifica manuale (umano): in una chat Claude chiedere di eseguire un comando (es. "esegui ls -la e dimmi cosa vedi") → il card Bash espanso mostra chip "in esecuzione", poi l'output monospace e infine "exit 0"; con un comando lungo (es. `sleep 2 && echo fatto`) l'output arriva in streaming; dopo chiudi/riapri pane l'output resta (persistito nel transcript).
