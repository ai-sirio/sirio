# Piano A — Chat ACP quick wins: fix 3 puntini, font, MCP config

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** chiudere i tre item "quick win" della spec `docs/superpowers/specs/2026-07-19-chat-acp-feature-complete-design.md`: RunningDots perenne sotto le risposte, font chat di sistema 13pt, passaggio dei server MCP di progetto all'agente.

**Architecture:** fix puro nel reducer (TillerACP) + normalizzazione al load (ChatSessionStore); tema MarkdownUI custom nel target App; parser `.mcp.json` puro in TillerACP con wiring in `ACPSession.connect` e `ChatController.start`.

**Tech Stack:** Swift 6, swift-testing (`@Test`/`#expect`), MarkdownUI 2.x, GRDB (già presenti — nessuna dipendenza nuova).

## Global Constraints

- Test first (RED→GREEN), swift-testing, mai XCTest.
- `Scripts/ci.sh` deve stampare `CI OK` a fine piano (il test PTY `spawnCapturesOutput` è flaky: fino a 5-6 retry sono normali, rilanciare).
- Commit conventional (`fix:`, `feat:`, `refactor:`), soggetto imperativo minuscolo.
- Package test rapidi: `cd Packages/TillerACP && swift test`.
- Modifiche al target App verificate con `xcodebuild -project Tiller.xcodeproj -scheme Tiller -configuration Debug -derivedDataPath DerivedData build -quiet` (exit 0). Nessun file nuovo va aggiunto a `project.yml` per il target App: `App/**` è già incluso come glob, ma dopo aver creato file nuovi serve `xcodegen generate`.
- Testo UI utente in italiano (convenzione esistente: "Piano", "Nuova conversazione").

---

### Task 1: TranscriptReducer — chiudere i messaggi agente quando lo stream ruota

**Files:**
- Modify: `Packages/TillerACP/Sources/TillerACP/TranscriptReducer.swift`
- Test: `Packages/TillerACP/Tests/TillerACPTests/TranscriptReducerTests.swift`

**Interfaces:**
- Consumes: `TranscriptReducer.apply(_:)`, `SessionUpdate` (esistenti).
- Produces: nessuna API nuova — solo semantica: dopo `apply` di un update che interrompe un messaggio agente aperto, quel messaggio ha `isComplete == true`.

Il bug: i rami `agentThoughtChunk` e `userMessageChunk` fanno `openAgentMessageIndex = nil` senza marcare il messaggio completo; il messaggio resta `isComplete: false` per sempre (RunningDots perenne in `TranscriptView`, stato sporco persistito).

- [ ] **Step 1: Scrivere i test che falliscono**

Aggiungere a `TranscriptReducerTests.swift` (stesso stile dei test esistenti nel file):

```swift
/// Un thought che interrompe un messaggio agente deve chiuderlo: lasciarlo
/// isComplete=false mostra RunningDots per sempre sotto la risposta.
@Test func thoughtChunkCompletesOpenAgentMessage() {
    var reducer = TranscriptReducer()
    reducer.apply(.agentMessageChunk(.text("Risposta")))
    reducer.apply(.agentThoughtChunk(.text("pensiero")))
    guard case .agentMessage(_, "Risposta", let isComplete) = reducer.items[0] else {
        Issue.record("expected agentMessage first"); return
    }
    #expect(isComplete)
}

/// Il replay di session/load alterna user e agent chunk: nessun messaggio
/// agente già streamato deve restare aperto.
@Test func replayLeavesNoOpenAgentMessages() {
    var reducer = TranscriptReducer()
    reducer.apply(.userMessageChunk(.text("domanda 1")))
    reducer.apply(.agentMessageChunk(.text("risposta 1")))
    reducer.apply(.userMessageChunk(.text("domanda 2")))
    reducer.apply(.agentMessageChunk(.text("risposta 2")))
    reducer.turnEnded(.endTurn)
    let incomplete = reducer.items.filter {
        if case .agentMessage(_, _, false) = $0 { return true }
        return false
    }
    #expect(incomplete.isEmpty)
}
```

- [ ] **Step 2: Verificare che falliscano**

Run: `cd Packages/TillerACP && swift test --filter TranscriptReducerTests`
Expected: FAIL su `thoughtChunkCompletesOpenAgentMessage` e `replayLeavesNoOpenAgentMessages` (isComplete false).

- [ ] **Step 3: Implementazione minima**

In `TranscriptReducer.swift`, estrarre la chiusura del messaggio agente e usarla ovunque l'indice venga azzerato:

```swift
private mutating func closeAgentMessage() {
    if let index = openAgentMessageIndex,
       case .agentMessage(let id, let text, _) = items[index] {
        items[index] = .agentMessage(id: id, text: text, isComplete: true)
    }
    openAgentMessageIndex = nil
}

private mutating func closeOpenStreams() {
    closeAgentMessage()
    openThoughtIndex = nil
    openUserMessageIndex = nil
}
```

Nel ramo `.agentThoughtChunk`, sostituire `openAgentMessageIndex = nil` con `closeAgentMessage()`. Stessa sostituzione nel ramo `.userMessageChunk`.

- [ ] **Step 4: Verificare che passino**

Run: `cd Packages/TillerACP && swift test --filter TranscriptReducerTests`
Expected: PASS, tutti i test della suite.

- [ ] **Step 5: Commit**

```bash
git add Packages/TillerACP/Sources/TillerACP/TranscriptReducer.swift \
        Packages/TillerACP/Tests/TillerACPTests/TranscriptReducerTests.swift
git commit -m "fix: complete open agent message when stream rotates to thought or replayed user chunk"
```

---

### Task 2: ChatSessionStore — normalizzare isComplete al load

**Files:**
- Modify: `Packages/TillerACP/Sources/TillerACP/ChatSessionStore.swift` (funzione `loadTranscript`, righe ~71-80)
- Test: `Packages/TillerACP/Tests/TillerACPTests/ChatSessionStoreTests.swift`

**Interfaces:**
- Consumes: `ChatSessionStore.saveTranscript`/`loadTranscript` (esistenti).
- Produces: `loadTranscript` restituisce ogni `.agentMessage` con `isComplete == true` (sana i dati già sporchi nel DB senza migrazione).

- [ ] **Step 1: Scrivere il test che fallisce**

Aggiungere a `ChatSessionStoreTests.swift`:

```swift
/// I transcript persistiti sono di turni finiti: un agentMessage salvato
/// isComplete=false (bug pre-fix) deve tornare completo al load, altrimenti
/// la UI mostra RunningDots su conversazioni ripristinate.
@Test func loadTranscriptNormalizesIncompleteAgentMessages() throws {
    let (store, worktreeId) = try makeStore()
    let session = try store.createSession(worktreeId: worktreeId, agentId: "claude",
                                          now: .init())
    let dirty: [TranscriptItem] = [
        .agentMessage(id: "agent-0", text: "Risposta", isComplete: false),
    ]
    try store.saveTranscript(sessionId: session.id, items: dirty, now: .init())
    let loaded = try store.loadTranscript(sessionId: session.id)
    #expect(loaded == [.agentMessage(id: "agent-0", text: "Risposta", isComplete: true)])
}
```

- [ ] **Step 2: Verificare che fallisca**

Run: `cd Packages/TillerACP && swift test --filter loadTranscriptNormalizesIncompleteAgentMessages`
Expected: FAIL (`isComplete` resta false).

- [ ] **Step 3: Implementazione minima**

In `ChatSessionStore.loadTranscript`, sostituire il `compactMap` finale:

```swift
let decoder = JSONDecoder()
return records.compactMap { record in
    guard let item = try? decoder.decode(TranscriptItem.self, from: record.payload)
    else { return nil }
    // Persisted transcripts are finished turns; normalize agent messages
    // left isComplete=false by the pre-fix reducer so restored chats don't
    // render a perpetual streaming indicator.
    if case .agentMessage(let id, let text, false) = item {
        return .agentMessage(id: id, text: text, isComplete: true)
    }
    return item
}
```

- [ ] **Step 4: Verificare che passi**

Run: `cd Packages/TillerACP && swift test --filter ChatSessionStoreTests`
Expected: PASS, tutta la suite.

- [ ] **Step 5: Commit**

```bash
git add Packages/TillerACP/Sources/TillerACP/ChatSessionStore.swift \
        Packages/TillerACP/Tests/TillerACPTests/ChatSessionStoreTests.swift
git commit -m "fix: normalize persisted agent messages to complete on transcript load"
```

---

### Task 3: McpConfig — parser di `.mcp.json` in wire format ACP

**Files:**
- Create: `Packages/TillerACP/Sources/TillerACP/McpConfig.swift`
- Test: `Packages/TillerACP/Tests/TillerACPTests/McpConfigTests.swift`

**Interfaces:**
- Consumes: niente (leaf, solo Foundation).
- Produces:
  - `public struct McpServerSpec: Sendable, Equatable, Codable` con `init(type: String? = nil, name: String, command: String? = nil, args: [String]? = nil, env: [NamedValue]? = nil, url: String? = nil, headers: [NamedValue]? = nil)` e `public struct NamedValue: Sendable, Equatable, Codable { public var name: String; public var value: String; public init(name:value:) }`
  - `public enum McpConfig { public static func load(worktreeRoot: String) throws -> [McpServerSpec] }` — file assente → `[]`; JSON malformato o entry invalida → throw.

Formato input (convenzione Claude Code, `<worktree>/.mcp.json`):

```json
{"mcpServers": {
  "tokensave": {"command": "npx", "args": ["-y", "tokensave-mcp"], "env": {"KEY": "V"}},
  "docs":      {"type": "http", "url": "https://example.com/mcp", "headers": {"Authorization": "Bearer x"}}
}}
```

Wire format ACP atteso da claude-agent-acp (verificato sul dist 0.59.0): stdio = `{name, command, args, env: [{name, value}]}`; http/sse = `{type, name, url, headers: [{name, value}]}`.

- [ ] **Step 1: Scrivere i test che falliscono**

Creare `McpConfigTests.swift`:

```swift
import Testing
import Foundation
@testable import TillerACP

@Suite struct McpConfigTests {
    private func writeConfig(_ json: String) throws -> String {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: root,
                                                withIntermediateDirectories: true)
        try json.data(using: .utf8)!
            .write(to: root.appendingPathComponent(".mcp.json"))
        return root.path
    }

    @Test func missingFileYieldsEmptyList() throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: root,
                                                withIntermediateDirectories: true)
        #expect(try McpConfig.load(worktreeRoot: root.path) == [])
    }

    @Test func parsesStdioServerWithEnv() throws {
        let root = try writeConfig(#"""
        {"mcpServers": {"tokensave": {"command": "npx",
          "args": ["-y", "tokensave-mcp"], "env": {"KEY": "V"}}}}
        """#)
        let specs = try McpConfig.load(worktreeRoot: root)
        #expect(specs == [McpServerSpec(
            name: "tokensave", command: "npx", args: ["-y", "tokensave-mcp"],
            env: [.init(name: "KEY", value: "V")])])
    }

    @Test func parsesHttpServerWithHeaders() throws {
        let root = try writeConfig(#"""
        {"mcpServers": {"docs": {"type": "http",
          "url": "https://example.com/mcp",
          "headers": {"Authorization": "Bearer x"}}}}
        """#)
        let specs = try McpConfig.load(worktreeRoot: root)
        #expect(specs == [McpServerSpec(
            type: "http", name: "docs", url: "https://example.com/mcp",
            headers: [.init(name: "Authorization", value: "Bearer x")])])
    }

    @Test func serversAreSortedByNameForDeterminism() throws {
        let root = try writeConfig(#"""
        {"mcpServers": {"zeta": {"command": "z"}, "alfa": {"command": "a"}}}
        """#)
        #expect(try McpConfig.load(worktreeRoot: root).map(\.name)
                == ["alfa", "zeta"])
    }

    @Test func malformedJSONThrows() throws {
        let root = try writeConfig("{not json")
        #expect(throws: (any Error).self) {
            try McpConfig.load(worktreeRoot: root)
        }
    }

    @Test func stdioEntryWithoutCommandThrows() throws {
        let root = try writeConfig(#"{"mcpServers": {"broken": {"args": ["x"]}}}"#)
        #expect(throws: (any Error).self) {
            try McpConfig.load(worktreeRoot: root)
        }
    }

    /// Il wire format ACP: env/headers come array {name,value}, niente
    /// chiavi null per i campi assenti (Codable sintetizzato usa
    /// encodeIfPresent — il test lo fissa contro regressioni).
    @Test func specEncodesToACPWireFormat() throws {
        let spec = McpServerSpec(name: "s", command: "cmd",
                                 env: [.init(name: "K", value: "V")])
        let data = try JSONEncoder().encode(spec)
        let object = try JSONSerialization.jsonObject(with: data) as? [String: Any]
        #expect(object?["name"] as? String == "s")
        #expect(object?["command"] as? String == "cmd")
        #expect((object?["env"] as? [[String: String]])?.first?["name"] == "K")
        #expect(object?["type"] == nil)
        #expect(object?["url"] == nil)
    }
}
```

- [ ] **Step 2: Verificare che falliscano**

Run: `cd Packages/TillerACP && swift test --filter McpConfigTests`
Expected: FAIL di compilazione ("cannot find 'McpConfig'").

- [ ] **Step 3: Implementazione**

Creare `McpConfig.swift`:

```swift
import Foundation

/// One MCP server in the ACP wire format `session/new`/`session/load`
/// expect: stdio entries carry command/args/env, http/sse entries carry
/// type/url/headers. Optional fields are omitted when nil (synthesized
/// Codable uses encodeIfPresent), which is what claude-agent-acp parses.
public struct McpServerSpec: Sendable, Equatable, Codable {
    public struct NamedValue: Sendable, Equatable, Codable {
        public var name: String
        public var value: String
        public init(name: String, value: String) {
            self.name = name
            self.value = value
        }
    }

    public var type: String?
    public var name: String
    public var command: String?
    public var args: [String]?
    public var env: [NamedValue]?
    public var url: String?
    public var headers: [NamedValue]?

    public init(type: String? = nil, name: String, command: String? = nil,
                args: [String]? = nil, env: [NamedValue]? = nil,
                url: String? = nil, headers: [NamedValue]? = nil) {
        self.type = type
        self.name = name
        self.command = command
        self.args = args
        self.env = env
        self.url = url
        self.headers = headers
    }
}

public enum McpConfigError: Error, Equatable {
    /// Entry missing the fields its type requires (stdio without command,
    /// http/sse without url).
    case invalidEntry(name: String)
}

/// Reads the project-level MCP configuration (`<worktree>/.mcp.json`,
/// Claude Code convention) into ACP wire-format specs. Missing file means
/// "no MCP servers"; a malformed file or entry throws so the caller can
/// surface a warning without blocking the session.
public enum McpConfig {
    private struct File: Decodable {
        var mcpServers: [String: Entry]?
    }
    private struct Entry: Decodable {
        var type: String?
        var command: String?
        var args: [String]?
        var env: [String: String]?
        var url: String?
        var headers: [String: String]?
    }

    public static func load(worktreeRoot: String) throws -> [McpServerSpec] {
        let path = (worktreeRoot as NSString).appendingPathComponent(".mcp.json")
        guard FileManager.default.fileExists(atPath: path) else { return [] }
        let data = try Data(contentsOf: URL(fileURLWithPath: path))
        let file = try JSONDecoder().decode(File.self, from: data)
        return try (file.mcpServers ?? [:])
            .sorted { $0.key < $1.key }
            .map { name, entry in try spec(name: name, entry: entry) }
    }

    private static func spec(name: String, entry: Entry) throws -> McpServerSpec {
        func namedValues(_ dict: [String: String]?) -> [McpServerSpec.NamedValue]? {
            guard let dict, !dict.isEmpty else { return nil }
            return dict.sorted { $0.key < $1.key }
                .map { .init(name: $0.key, value: $0.value) }
        }
        if entry.type == "http" || entry.type == "sse" {
            guard let url = entry.url else {
                throw McpConfigError.invalidEntry(name: name)
            }
            return McpServerSpec(type: entry.type, name: name, url: url,
                                 headers: namedValues(entry.headers))
        }
        guard let command = entry.command else {
            throw McpConfigError.invalidEntry(name: name)
        }
        return McpServerSpec(name: name, command: command, args: entry.args,
                             env: namedValues(entry.env))
    }
}
```

- [ ] **Step 4: Verificare che passino**

Run: `cd Packages/TillerACP && swift test --filter McpConfigTests`
Expected: PASS, 7 test.

- [ ] **Step 5: Commit**

```bash
git add Packages/TillerACP/Sources/TillerACP/McpConfig.swift \
        Packages/TillerACP/Tests/TillerACPTests/McpConfigTests.swift
git commit -m "feat: parse project .mcp.json into ACP wire-format server specs"
```

---

### Task 4: Wiring MCP — ACPSession.connect e ChatController.start

**Files:**
- Modify: `Packages/TillerACP/Sources/TillerACP/ACPTypes.swift` (`NewSessionParams` ~riga 90, `LoadSessionParams` ~riga 200)
- Modify: `Packages/TillerACP/Sources/TillerACP/ACPSession.swift` (`connect`, ~riga 68)
- Modify: `App/Chat/ChatController.swift` (`start`, zona `session.connect`; proprietà)
- Modify: `App/Chat/ChatPaneView.swift` (banner)
- Test: `Packages/TillerACP/Tests/TillerACPTests/ACPSessionTests.swift`

**Interfaces:**
- Consumes: `McpServerSpec`, `McpConfig.load(worktreeRoot:)` dal Task 3; `FakeAgent`/`MockTransport` esistenti in `ACPSessionTests.swift`.
- Produces: `ACPSession.connect(cwd:resumeSessionId:mcpServers:)` con `mcpServers: [McpServerSpec] = []`; `ChatController.mcpWarning: String?`.

- [ ] **Step 1: Scrivere il test che fallisce**

Aggiungere a `ACPSessionTests.swift`:

```swift
/// I server MCP del progetto viaggiano in session/new nel wire format ACP.
@Test func connectSendsMcpServersInSessionNew() async throws {
    let agent = FakeAgent()
    let session = try await makeSession(agent: agent)
    let spec = McpServerSpec(name: "docs", command: "npx",
                             args: ["-y", "docs-mcp"])
    _ = try await session.connect(cwd: "/w", resumeSessionId: nil,
                                  mcpServers: [spec])
    var sent: NewSessionParams?
    for index in 0..<(await agent.mock.sent.count) {
        guard case .request(_, "session/new", let params) =
                try await agent.mock.sentMessage(index) else { continue }
        sent = try params?.decoded(NewSessionParams.self)
    }
    #expect(sent?.mcpServers == [spec])
}
```

- [ ] **Step 2: Verificare che fallisca**

Run: `cd Packages/TillerACP && swift test --filter connectSendsMcpServersInSessionNew`
Expected: FAIL di compilazione (`connect` non ha il parametro `mcpServers`; `NewSessionParams.mcpServers` è `[JSONValue]`).

Nota: se `MockTransport.sent`/`sentMessage` hanno firme diverse da quelle usate sopra, adattare il test alla firma reale (vedere l'uso in `FakeAgent.run()` nello stesso file) mantenendo l'assert su `sent?.mcpServers == [spec]`.

- [ ] **Step 3: Implementazione**

In `ACPTypes.swift`, tipizzare i parametri (era `[JSONValue]`):

```swift
public struct NewSessionParams: Sendable, Equatable, Codable {
    public var cwd: String
    public var mcpServers: [McpServerSpec]
    public init(cwd: String, mcpServers: [McpServerSpec] = []) {
        self.cwd = cwd
        self.mcpServers = mcpServers
    }
}
```

e in `LoadSessionParams` sostituire `public var mcpServers: [JSONValue]` con `public var mcpServers: [McpServerSpec]` (init: `mcpServers: [McpServerSpec] = []`).

In `ACPSession.connect`, aggiungere il parametro e propagarlo a entrambe le chiamate:

```swift
public func connect(cwd: String, resumeSessionId: String?,
                    mcpServers: [McpServerSpec] = []) async throws -> SessionHandle {
```

- `session/load`: `LoadSessionParams(sessionId: resumeSessionId, cwd: cwd, mcpServers: mcpServers)`
- `session/new`: `NewSessionParams(cwd: cwd, mcpServers: mcpServers)`

In `ChatController`:

```swift
/// Warning non bloccante da .mcp.json invalido; la sessione parte senza MCP.
var mcpWarning: String?
```

e in `start()`, prima della chiamata a `session.connect` (zona riga ~98):

```swift
var mcpServers: [McpServerSpec] = []
do {
    mcpServers = try McpConfig.load(worktreeRoot: worktreePath)
} catch {
    mcpWarning = "File .mcp.json non valido: la sessione parte senza server MCP."
}
let handle = try await session.connect(
    cwd: worktreePath, resumeSessionId: record?.acpSessionId,
    mcpServers: mcpServers)
```

In `ChatPaneView.body`, dopo il blocco `if let promptError ...`:

```swift
if let mcpWarning = controller.mcpWarning {
    banner("Configurazione MCP",
           detail: mcpWarning,
           actionTitle: "OK") {
        controller.mcpWarning = nil
    }
}
```

- [ ] **Step 4: Verificare che passino package test e build App**

Run: `cd Packages/TillerACP && swift test`
Expected: PASS, tutta la suite (inclusi i test connect esistenti: il default `= []` non li rompe).

Run: `xcodebuild -project Tiller.xcodeproj -scheme Tiller -configuration Debug -derivedDataPath DerivedData build -quiet; echo "exit=$?"`
Expected: `exit=0`.

- [ ] **Step 5: Commit**

```bash
git add Packages/TillerACP/Sources/TillerACP/ACPTypes.swift \
        Packages/TillerACP/Sources/TillerACP/ACPSession.swift \
        Packages/TillerACP/Tests/TillerACPTests/ACPSessionTests.swift \
        App/Chat/ChatController.swift App/Chat/ChatPaneView.swift
git commit -m "feat: pass project .mcp.json servers to the agent in session/new and session/load"
```

---

### Task 5: Font chat — tema MarkdownUI `tiller` a 13pt di sistema

**Files:**
- Create: `App/Chat/TillerMarkdownTheme.swift`
- Modify: `App/Chat/TranscriptView.swift` (righe ~42-43 `.markdownTheme(.gitHub)`; bolla utente ~riga 89; `ThoughtRow` ~riga 160)

**Interfaces:**
- Consumes: MarkdownUI 2.x `Theme` DSL (dipendenza App già presente in `project.yml`).
- Produces: `Theme.tiller` usato da `TranscriptView`. Nessun consumer successivo.

UI pura: niente unit test (il gate è build + verifica visiva). Dopo la creazione del file: `xcodegen generate`.

- [ ] **Step 1: Creare il tema**

`App/Chat/TillerMarkdownTheme.swift`:

```swift
import MarkdownUI
import SwiftUI

extension Theme {
    /// Compact system-font markdown theme for the chat transcript: 13pt body
    /// aligned with the rest of the Tiller UI (the stock .gitHub theme
    /// renders ~16px web-page typography that clashes with native panes).
    static let tiller = Theme.gitHub
        .text {
            FontSize(13)
        }
        .code {
            FontFamilyVariant(.monospaced)
            FontSize(12)
        }
        .heading1 { configuration in
            configuration.label
                .markdownMargin(top: 12, bottom: 4)
                .markdownTextStyle {
                    FontWeight(.semibold)
                    FontSize(15)
                }
        }
        .heading2 { configuration in
            configuration.label
                .markdownMargin(top: 10, bottom: 4)
                .markdownTextStyle {
                    FontWeight(.semibold)
                    FontSize(14)
                }
        }
        .heading3 { configuration in
            configuration.label
                .markdownMargin(top: 8, bottom: 2)
                .markdownTextStyle {
                    FontWeight(.semibold)
                    FontSize(13)
                }
        }
}
```

Nota per l'implementatore: se il compilatore rifiuta un modificatore (l'API `Theme` di MarkdownUI varia tra minor version), controllare la versione risolta in `Tiller.xcodeproj` e i nomi esatti in `DerivedData/SourcePackages/checkouts/swift-markdown-ui/Sources/MarkdownUI/Theme/Theme.swift` — l'obiettivo fisso è: body 13pt sistema, code 12pt monospace, heading 15/14/13 semibold.

- [ ] **Step 2: Applicare il tema nel transcript**

In `TranscriptView.swift`:

- riga ~43: `.markdownTheme(.gitHub)` → `.markdownTheme(.tiller)`
- bolla utente, `userBlockView` caso `.text` (~riga 89): `Text(text).textSelection(.enabled)` → `Text(text).font(.system(size: 13)).textSelection(.enabled)`
- `ThoughtRow`, testo espanso (~riga 160): `.font(.callout)` → `.font(.system(size: 13))`

- [ ] **Step 3: Rigenerare progetto e buildare**

Run: `xcodegen generate && xcodebuild -project Tiller.xcodeproj -scheme Tiller -configuration Debug -derivedDataPath DerivedData build -quiet; echo "exit=$?"`
Expected: `exit=0`.

- [ ] **Step 4: Commit**

```bash
git add App/Chat/TillerMarkdownTheme.swift App/Chat/TranscriptView.swift
git commit -m "feat: compact 13pt system-font markdown theme for chat transcript"
```

---

### Task 6: Gate finale

- [ ] **Step 1: CI completa**

Run: `Scripts/ci.sh`
Expected: `CI OK` (retry se fallisce solo il test PTY flaky `spawnCapturesOutput`).

- [ ] **Step 2: Rebuild e rilancio app per verifica manuale**

```bash
xcodebuild -project Tiller.xcodeproj -scheme Tiller -configuration Debug \
  -derivedDataPath DerivedData build -quiet
pkill -x Tiller; sleep 1; open DerivedData/Build/Products/Debug/Tiller.app
```

Verifica manuale (umano): (1) risposta agente senza puntini a fine turno, anche dopo chiudi/riapri pane; (2) font transcript 13pt uniforme; (3) con un `.mcp.json` nel worktree l'agente vede i tool MCP (chiedigli "che tool MCP hai?"); (4) `.mcp.json` malformato → banner "Configurazione MCP".
