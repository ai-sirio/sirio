# Auto-rename tab e agenti Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rinominare automaticamente `WorkspaceTab.title` (e quindi il nodo root corrispondente nell'Agents panel) al termine di un turno agente, riassumendo la conversazione con la CLI dell'agente stesso, con precedenza assoluta ai rename manuali — strategia di [manaflow-ai/cmux](https://github.com/manaflow-ai/cmux/blob/main/docs/workspace-auto-naming.md).

**Architecture:** Riuso del trigger esistente `AppModel.notifyTransition` (già chiamato per ogni `AgentTransition`, sia da hook Layer A sia dal path di stato delle chat tab ACP) come unico punto di innesco. Sorgente transcript pluggabile via protocollo `TranscriptSource`: `ChatTranscriptSource` legge da `ChatController` in memoria (tutti e 5 gli agenti sulle chat tab), `FileTranscriptSource` fa il tail del JSONL su disco per Claude Code e Codex (via `AgentSessionRef.sessionRef`, terminal tab). `AutoNamer` orchestra: throttle → transcript → prompt → CLI dell'adapter in modalità non interattiva (`Process` detached) → parsing → applicazione del titolo senza toccare la provenance.

**Tech Stack:** Swift 6, swift-testing, GRDB (migration v11), Foundation `Process`.

## Global Constraints

- Feature interamente off by default (toggle in Settings), come da spec.
- Nessun nuovo hook: il trigger riusa `AgentTransition` esistente.
- Rename manuale vince sempre e per sempre su una tab (nessun "clear name" in v1).
- Terminal tab OpenCode/Pi restano fuori scope (nessuna sorgente transcript nota) — non toccarle.
- Convenzione transcript Oh-My-Pi non verificata — `summarizerCommand` esiste per omp (serve alle chat tab), ma nessun `FileTranscriptSource` per omp in questo piano.
- Ogni fallimento del summarizer (binario assente, timeout, output vuoto, tab chiusa) degrada silenziosamente: titolo invariato, nessun errore visibile.
- `Scripts/ci.sh` deve stampare `CI OK` prima di considerare il lavoro concluso.

---

## File Structure

**Nuovi file:**
- `Packages/TillerCore/Sources/TillerCore/AutoNamingThrottle.swift` — throttle puro (intervallo + crescita transcript).
- `Packages/TillerCore/Sources/TillerCore/TranscriptSource.swift` — protocollo condiviso.
- `Packages/TillerAgents/Sources/TillerAgents/ClaudeTranscriptSource.swift` — `FileTranscriptSource` per Claude Code.
- `Packages/TillerAgents/Sources/TillerAgents/CodexTranscriptSource.swift` — `FileTranscriptSource` per Codex.
- `App/Chat/ChatTranscriptSource.swift` — `TranscriptSource` che legge da `ChatController`.
- `App/AutoNaming/AutoNamer.swift` — orchestratore (prompt, spawn, parsing, timeout).
- Test corrispondenti in `Packages/TillerCore/Tests/TillerCoreTests/`, `Packages/TillerAgents/Tests/TillerAgentsTests/`, `AppTests/`.

**File modificati:**
- `Packages/TillerCore/Sources/TillerCore/WorkspaceTab.swift` — campo `titleIsAutoNamed`.
- `Packages/TillerPersistence/Sources/TillerPersistence/Records.swift` — colonna su `TerminalTabRecord`.
- `Packages/TillerPersistence/Sources/TillerPersistence/AppDatabase.swift` — migration `v11`.
- `Packages/TillerCore/Sources/TillerCore/ProjectStore.swift` — threading del campo in `saveTabs`/`loadTabs`.
- `Packages/TillerAgents/Sources/TillerAgents/AgentAdapter.swift` — `summarizerCommand(prompt:)`.
- `Packages/TillerAgents/Sources/TillerAgents/{ClaudeCodeAdapter,CodexAdapter,OpenCodeAdapter,PiAdapter,OhMyPiAdapter}.swift` — override.
- `Packages/TillerCore/Sources/TillerCore/AppSettings.swift` — chiave toggle.
- `App/AppModel.swift` — `renameTab` flippa il flag, nuovo `applyAutoTitle`, wiring in `notifyTransition`.
- `App/GeneralSettingsView.swift` — sezione "Automation".

---

### Task 1: `WorkspaceTab.titleIsAutoNamed`

**Files:**
- Modify: `Packages/TillerCore/Sources/TillerCore/WorkspaceTab.swift`
- Test: `Packages/TillerCore/Tests/TillerCoreTests/WorkspaceTabTests.swift`

**Interfaces:**
- Produces: `WorkspaceTab.titleIsAutoNamed: Bool` (default `true`), `WorkspaceTab.init(id:title:content:titleIsAutoNamed:)`, `WorkspaceTab.init(id:title:tree:titleIsAutoNamed:)`.

- [ ] **Step 1: Write the failing test**

```swift
@Test func newTabDefaultsToAutoNamed() {
    let tab = WorkspaceTab(id: UUID(), title: "Terminale 1", tree: SplitTree.leaf(UUID()))
    #expect(tab.titleIsAutoNamed == true)
}

@Test func explicitTitleIsAutoNamedOverridesDefault() {
    let tab = WorkspaceTab(id: UUID(), title: "my tab", tree: SplitTree.leaf(UUID()), titleIsAutoNamed: false)
    #expect(tab.titleIsAutoNamed == false)
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd Packages/TillerCore && swift test --filter WorkspaceTabTests`
Expected: FAIL — `value of type 'WorkspaceTab' has no member 'titleIsAutoNamed'`

- [ ] **Step 3: Write minimal implementation**

```swift
/// Una tab dentro un worktree. L'identità della tab è stabile; title e
/// content sono mutabili (rinomina, split).
public struct WorkspaceTab: Identifiable, Equatable, Sendable {
    public let id: UUID
    public var title: String
    public var content: TabContent
    /// True quando `title` è ancora eleggibile per l'auto-naming: placeholder
    /// di creazione o ultimo titolo scritto dall'auto-naming stesso. Un
    /// rename manuale (`AppModel.renameTab`) lo flippa a false per sempre.
    public var titleIsAutoNamed: Bool

    public init(id: UUID, title: String, content: TabContent, titleIsAutoNamed: Bool = true) {
        self.id = id; self.title = title; self.content = content
        self.titleIsAutoNamed = titleIsAutoNamed
    }

    /// Convenience per il caso terminale, il più comune nei call site.
    public init(id: UUID, title: String, tree: SplitTree, titleIsAutoNamed: Bool = true) {
        self.init(id: id, title: title, content: .terminal(tree), titleIsAutoNamed: titleIsAutoNamed)
    }

    // ... resto invariato (leafIds, terminalTree, markdownFileURL, chatAgentId,
    // activityPaneIds, nextShellTitle) ...
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd Packages/TillerCore && swift test --filter WorkspaceTabTests`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add Packages/TillerCore/Sources/TillerCore/WorkspaceTab.swift Packages/TillerCore/Tests/TillerCoreTests/WorkspaceTabTests.swift
git commit -m "feat: add titleIsAutoNamed provenance flag to WorkspaceTab"
```

---

### Task 2: Persistenza — migration v11 + `TerminalTabRecord` + `ProjectStore`

**Files:**
- Modify: `Packages/TillerPersistence/Sources/TillerPersistence/AppDatabase.swift`
- Modify: `Packages/TillerPersistence/Sources/TillerPersistence/Records.swift`
- Modify: `Packages/TillerCore/Sources/TillerCore/ProjectStore.swift`
- Test: `Packages/TillerPersistence/Tests/TillerPersistenceTests/AppDatabaseTests.swift`
- Test: `Packages/TillerCore/Tests/TillerCoreTests/WorkspaceTabTests.swift` (persistenza round-trip, se lì testata già `saveTabs`/`loadTabs` altrimenti aggiungere in un test dedicato `ProjectStoreTests.swift` — verificare quale file testa oggi `saveTabs`/`loadTabs` con `grep -rn "saveTabs" Packages/TillerCore/Tests`)

**Interfaces:**
- Consumes: `WorkspaceTab.titleIsAutoNamed` (Task 1).
- Produces: `TerminalTabRecord.titleIsAutoNamed: Bool`, colonna DB `titleIsAutoNamed` con default SQL `false` (grandfathering righe esistenti), `ProjectStore.saveTabs`/`loadTabs` che leggono/scrivono il campo.

- [ ] **Step 1: Write the failing test**

```swift
// Packages/TillerPersistence/Tests/TillerPersistenceTests/AppDatabaseTests.swift
@Test func migrationV11AddsTitleIsAutoNamedDefaultingFalse() throws {
    let db = try AppDatabase.inMemory()
    try db.write { database in
        try database.execute(sql: "INSERT INTO worktree (id, projectId, branch, path, createdAt) VALUES ('w1','p1','main','/tmp',?)", arguments: [Date()])
        try database.execute(
            sql: """
            INSERT INTO terminalTab (id, worktreeId, title, orderIdx, isActive, treeJSON, updatedAt, kind)
            VALUES ('t1','w1','Terminale 1',0,1,'{}',?,'terminal')
            """, arguments: [Date()])
    }
    let value = try db.read { database in
        try Bool.fetchOne(database, sql: "SELECT titleIsAutoNamed FROM terminalTab WHERE id = 't1'")
    }
    #expect(value == false)
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd Packages/TillerPersistence && swift test --filter AppDatabaseTests`
Expected: FAIL — `no such column: titleIsAutoNamed`

- [ ] **Step 3: Write minimal implementation**

In `AppDatabase.swift`, dopo `migrator.registerMigration("v10")`:

```swift
migrator.registerMigration("v11") { db in
    try db.alter(table: "terminalTab") { t in
        t.add(column: "titleIsAutoNamed", .boolean).notNull().defaults(to: false)
    }
}
```

In `Records.swift`, `TerminalTabRecord`:

```swift
public struct TerminalTabRecord: Codable, FetchableRecord, PersistableRecord, Sendable, Equatable {
    public static let databaseTableName = "terminalTab"
    public var id: String
    public var worktreeId: String
    public var title: String
    public var orderIdx: Int
    public var isActive: Bool
    public var treeJSON: String
    public var updatedAt: Date
    public var kind: String
    public var filePath: String?
    public var chatAgentId: String?
    public var titleIsAutoNamed: Bool

    public init(id: String, worktreeId: String, title: String, orderIdx: Int,
                isActive: Bool, treeJSON: String, updatedAt: Date,
                kind: String = "terminal", filePath: String? = nil,
                chatAgentId: String? = nil, titleIsAutoNamed: Bool = true) {
        self.id = id; self.worktreeId = worktreeId; self.title = title
        self.orderIdx = orderIdx; self.isActive = isActive
        self.treeJSON = treeJSON; self.updatedAt = updatedAt
        self.kind = kind; self.filePath = filePath; self.chatAgentId = chatAgentId
        self.titleIsAutoNamed = titleIsAutoNamed
    }
}
```

In `ProjectStore.saveTabs`, aggiungere `titleIsAutoNamed: tab.titleIsAutoNamed` a ciascuno dei tre `TerminalTabRecord(...)` costruiti (caso `.terminal`, `.markdown`, `.chat`). In `ProjectStore.loadTabs`, propagare `titleIsAutoNamed: record.titleIsAutoNamed` in ciascuna delle tre chiamate `WorkspaceTab(...)`.

- [ ] **Step 4: Run test to verify it passes**

Run: `cd Packages/TillerPersistence && swift test --filter AppDatabaseTests && cd ../TillerCore && swift test`
Expected: PASS (tutti i test esistenti di `ProjectStore`/tab persistence continuano a passare — `titleIsAutoNamed` ha default `true` lato Swift quindi i call site di test esistenti che non lo passano restano validi)

- [ ] **Step 5: Commit**

```bash
git add Packages/TillerPersistence Packages/TillerCore/Sources/TillerCore/ProjectStore.swift
git commit -m "feat: persist titleIsAutoNamed (migration v11, grandfathers existing tabs)"
```

---

### Task 3: `AppModel.renameTab` flips provenance + `applyAutoTitle`

**Files:**
- Modify: `App/AppModel.swift:923-929` (`renameTab`)
- Test: `AppTests/AppModelTabTests.swift` (creare se non esiste — verificare con `grep -rln "func renameTab" AppTests` quale file testa oggi `renameTab`)

**Interfaces:**
- Consumes: `WorkspaceTab.titleIsAutoNamed` (Task 1).
- Produces: `AppModel.applyAutoTitle(tabId: UUID, in worktreeId: UUID, title: String)` — usato solo da `AutoNamer` (Task 9), non tocca `titleIsAutoNamed`.

- [ ] **Step 1: Write the failing test**

```swift
@MainActor @Test func renameTabDisablesAutoNaming() async throws {
    let model = try AppModel.forTesting()
    let worktree = try model.addTestWorktree()
    let tab = model.tabs[worktree.id]!.first!
    #expect(tab.titleIsAutoNamed == true)

    model.renameTab(tab.id, in: worktree.id, to: "My custom name")

    let updated = model.tabs[worktree.id]!.first { $0.id == tab.id }!
    #expect(updated.title == "My custom name")
    #expect(updated.titleIsAutoNamed == false)
}

@MainActor @Test func applyAutoTitleLeavesProvenanceUntouched() async throws {
    let model = try AppModel.forTesting()
    let worktree = try model.addTestWorktree()
    let tab = model.tabs[worktree.id]!.first!

    model.applyAutoTitle(tab.id, in: worktree.id, title: "Fix login bug")

    let updated = model.tabs[worktree.id]!.first { $0.id == tab.id }!
    #expect(updated.title == "Fix login bug")
    #expect(updated.titleIsAutoNamed == true)
}
```

Nota: `AppModel.forTesting()` / `addTestWorktree()` sono placeholder di comodo — usare l'helper di test già esistente per costruire un `AppModel` con `ProjectStore` in-memory e un worktree con almeno una tab (cercarlo con `grep -rln "AppModel(" AppTests` e riusare lo stesso pattern degli altri test di `AppModel`, es. quelli di `renameTab`/`moveTab` già presenti).

- [ ] **Step 2: Run test to verify it fails**

Run: `Scripts/ci.sh` oppure, per iterare rapido, il test target App via Xcode (`xcodebuild test -scheme Tiller -only-testing:AppTests/...`) — il progetto App non ha uno `swift test` standalone, verificare in `CLAUDE.md`/`project.yml` il comando corretto per il target `AppTests` prima di eseguire.
Expected: FAIL — `value of type 'AppModel' has no member 'applyAutoTitle'` e assert `titleIsAutoNamed == false` fallito (renameTab non lo tocca ancora).

- [ ] **Step 3: Write minimal implementation**

```swift
func renameTab(_ tabId: UUID, in worktreeId: UUID, to title: String) {
    guard let idx = tabs[worktreeId]?.firstIndex(where: { $0.id == tabId }) else { return }
    let trimmed = title.trimmingCharacters(in: .whitespaces)
    guard !trimmed.isEmpty else { return }
    tabs[worktreeId]?[idx].title = trimmed
    tabs[worktreeId]?[idx].titleIsAutoNamed = false
    persistTabs(for: worktreeId)
}

/// Applica un titolo generato dall'auto-naming (Task 9). A differenza di
/// `renameTab`, non tocca `titleIsAutoNamed`: resta eleggibile per il
/// prossimo pass finché l'utente non rinomina manualmente.
func applyAutoTitle(_ tabId: UUID, in worktreeId: UUID, title: String) {
    guard let idx = tabs[worktreeId]?.firstIndex(where: { $0.id == tabId }) else { return }
    let trimmed = title.trimmingCharacters(in: .whitespaces)
    guard !trimmed.isEmpty, tabs[worktreeId]?[idx].titleIsAutoNamed == true else { return }
    tabs[worktreeId]?[idx].title = trimmed
    persistTabs(for: worktreeId)
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: come Step 2.
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add App/AppModel.swift AppTests/
git commit -m "feat: renameTab disables auto-naming; add applyAutoTitle setter"
```

---

### Task 4: `AutoNamingThrottle`

**Files:**
- Create: `Packages/TillerCore/Sources/TillerCore/AutoNamingThrottle.swift`
- Test: `Packages/TillerCore/Tests/TillerCoreTests/AutoNamingThrottleTests.swift`

**Interfaces:**
- Produces: `AutoNamingThrottle` (struct), `AutoNamingThrottle.shouldRun(transcriptLength:now:) -> Bool`, `AutoNamingThrottle.recording(transcriptLength:now:) -> AutoNamingThrottle`.

- [ ] **Step 1: Write the failing test**

```swift
import Testing
@testable import TillerCore

@Test func firstRunAlwaysAllowed() {
    let throttle = AutoNamingThrottle()
    #expect(throttle.shouldRun(transcriptLength: 50, now: Date()) == true)
}

@Test func blocksWithinMinInterval() {
    let now = Date()
    let throttle = AutoNamingThrottle().recording(transcriptLength: 100, now: now)
    #expect(throttle.shouldRun(transcriptLength: 400, now: now.addingTimeInterval(5)) == false)
}

@Test func blocksWhenGrowthTooSmall() {
    let now = Date()
    let throttle = AutoNamingThrottle().recording(transcriptLength: 100, now: now)
    #expect(throttle.shouldRun(transcriptLength: 150, now: now.addingTimeInterval(60)) == false)
}

@Test func allowsAfterIntervalAndGrowth() {
    let now = Date()
    let throttle = AutoNamingThrottle().recording(transcriptLength: 100, now: now)
    #expect(throttle.shouldRun(transcriptLength: 400, now: now.addingTimeInterval(60)) == true)
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd Packages/TillerCore && swift test --filter AutoNamingThrottleTests`
Expected: FAIL — `cannot find 'AutoNamingThrottle' in scope`

- [ ] **Step 3: Write minimal implementation**

```swift
import Foundation

/// Doppio gate per l'auto-naming: non ripartire prima di `minInterval`
/// secondi *e* prima che il transcript sia cresciuto di almeno `minGrowth`
/// caratteri dall'ultimo pass. Stato puro, tenuto in memoria per pane da
/// AppModel — un riavvio dell'app al più causa un pass extra, mai un bug.
public struct AutoNamingThrottle: Sendable, Equatable {
    public static let minInterval: TimeInterval = 30
    public static let minGrowth = 200

    public var lastRunAt: Date?
    public var lastTranscriptLength: Int

    public init(lastRunAt: Date? = nil, lastTranscriptLength: Int = 0) {
        self.lastRunAt = lastRunAt
        self.lastTranscriptLength = lastTranscriptLength
    }

    public func shouldRun(transcriptLength: Int, now: Date) -> Bool {
        guard let lastRunAt else { return true }
        guard now.timeIntervalSince(lastRunAt) >= Self.minInterval else { return false }
        return transcriptLength - lastTranscriptLength >= Self.minGrowth
    }

    public func recording(transcriptLength: Int, now: Date) -> AutoNamingThrottle {
        AutoNamingThrottle(lastRunAt: now, lastTranscriptLength: transcriptLength)
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd Packages/TillerCore && swift test --filter AutoNamingThrottleTests`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add Packages/TillerCore/Sources/TillerCore/AutoNamingThrottle.swift Packages/TillerCore/Tests/TillerCoreTests/AutoNamingThrottleTests.swift
git commit -m "feat: add AutoNamingThrottle pure throttle logic"
```

---

### Task 5: `AgentAdapter.summarizerCommand`

**Files:**
- Modify: `Packages/TillerAgents/Sources/TillerAgents/AgentAdapter.swift`
- Modify: `Packages/TillerAgents/Sources/TillerAgents/ClaudeCodeAdapter.swift`
- Modify: `Packages/TillerAgents/Sources/TillerAgents/CodexAdapter.swift`
- Modify: `Packages/TillerAgents/Sources/TillerAgents/OpenCodeAdapter.swift`
- Modify: `Packages/TillerAgents/Sources/TillerAgents/PiAdapter.swift`
- Modify: `Packages/TillerAgents/Sources/TillerAgents/OhMyPiAdapter.swift`
- Test: `Packages/TillerAgents/Tests/TillerAgentsTests/SummarizerCommandTests.swift`

**Interfaces:**
- Produces: `AgentAdapter.summarizerCommand(prompt: String) -> String?` (default `nil` via extension, override per adapter). Consumato da `AutoNamer` (Task 9).

- [ ] **Step 1: Write the failing test**

```swift
import Testing
@testable import TillerAgents

@Test func claudeSummarizerCommandUsesPrintMode() {
    let cmd = ClaudeCodeAdapter().summarizerCommand(prompt: "summarize this")
    #expect(cmd == "claude -p 'summarize this'")
}

@Test func codexSummarizerCommandUsesExecOutputLastMessage() {
    let cmd = CodexAdapter().summarizerCommand(prompt: "summarize this")
    #expect(cmd == "codex exec --output-last-message /dev/stdout 'summarize this'")
}

@Test func openCodeSummarizerCommandUsesRunPure() {
    let cmd = OpenCodeAdapter().summarizerCommand(prompt: "summarize this")
    #expect(cmd == "opencode run --pure 'summarize this'")
}

@Test func piSummarizerCommandUsesPrintNoTools() {
    let cmd = PiAdapter().summarizerCommand(prompt: "summarize this")
    #expect(cmd == "pi --print --no-tools 'summarize this'")
}

@Test func ohMyPiSummarizerCommandUsesPrintNoTools() {
    let cmd = OhMyPiAdapter().summarizerCommand(prompt: "summarize this")
    #expect(cmd == "omp --print --no-tools 'summarize this'")
}

@Test func promptWithSingleQuoteIsShellSafe() {
    let cmd = ClaudeCodeAdapter().summarizerCommand(prompt: "it's a test")
    #expect(cmd == "claude -p 'it'\\''s a test'")
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd Packages/TillerAgents && swift test --filter SummarizerCommandTests`
Expected: FAIL — `value of type 'ClaudeCodeAdapter' has no member 'summarizerCommand'`

- [ ] **Step 3: Write minimal implementation**

In `AgentAdapter.swift`, aggiungere al protocollo e all'extension esistente:

```swift
public protocol AgentAdapter: Sendable {
    var id: String { get }
    var displayName: String { get }
    var hasNativeHooks: Bool { get }
    func prepare(worktreePath: String, paneId: UUID, tillerctlPath: String) throws
    func command(worktreePath: String, paneId: UUID, tillerctlPath: String) -> String
    func resumeCommand(worktreePath: String, paneId: UUID,
                       tillerctlPath: String, sessionRef: String) -> String?

    /// Full shell command that runs this adapter's own CLI in non-interactive
    /// "print" mode, summarizing `prompt` and writing the result to stdout.
    /// Nil when no safe non-interactive invocation exists yet for this adapter.
    func summarizerCommand(prompt: String) -> String?
}

public extension AgentAdapter {
    func summarizerCommand(prompt: String) -> String? { nil }
}
```

In ciascun adapter, aggiungere:

```swift
// ClaudeCodeAdapter
public func summarizerCommand(prompt: String) -> String? {
    "claude -p \(shellQuote(prompt))"
}

// CodexAdapter
public func summarizerCommand(prompt: String) -> String? {
    "codex exec --output-last-message /dev/stdout \(shellQuote(prompt))"
}

// OpenCodeAdapter
public func summarizerCommand(prompt: String) -> String? {
    "opencode run --pure \(shellQuote(prompt))"
}

// PiAdapter
public func summarizerCommand(prompt: String) -> String? {
    "pi --print --no-tools \(shellQuote(prompt))"
}

// OhMyPiAdapter
public func summarizerCommand(prompt: String) -> String? {
    "omp --print --no-tools \(shellQuote(prompt))"
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd Packages/TillerAgents && swift test --filter SummarizerCommandTests`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add Packages/TillerAgents
git commit -m "feat: add AgentAdapter.summarizerCommand for all 5 adapters"
```

---

### Task 6: `TranscriptSource` protocol

**Files:**
- Create: `Packages/TillerCore/Sources/TillerCore/TranscriptSource.swift`
- Test: nessuno (protocollo puro, coperto dai test dei conformanti nei Task 7-8-9)

**Interfaces:**
- Produces: `public protocol TranscriptSource { func recentText() -> String? }`.

- [ ] **Step 1: Write the minimal implementation** (protocollo puro, nessun test dedicato ha senso — è coperto dai conformanti)

```swift
import Foundation

/// Sorgente di transcript recente per l'auto-naming. Implementazioni:
/// `ChatTranscriptSource` (App, chat tab ACP), `FileTranscriptSource`
/// (TillerAgents, terminal tab con adapter a hook nativi).
///
/// Chiamato sempre in modo sincrono dal chiamante prima di passare il
/// risultato a un contesto concorrente — nessuna garanzia Sendable richiesta
/// sul conformante stesso (permette a `ChatTranscriptSource` di restare
/// @MainActor senza friction).
public protocol TranscriptSource {
    /// Testo piano delle ultime battute della conversazione, o nil quando
    /// non disponibile (file assente, sessione vuota, tab senza sessione).
    func recentText() -> String?
}
```

- [ ] **Step 2: Commit**

```bash
git add Packages/TillerCore/Sources/TillerCore/TranscriptSource.swift
git commit -m "feat: add TranscriptSource protocol"
```

---

### Task 7: `FileTranscriptSource` — Claude Code

**Files:**
- Create: `Packages/TillerAgents/Sources/TillerAgents/ClaudeTranscriptSource.swift`
- Test: `Packages/TillerAgents/Tests/TillerAgentsTests/ClaudeTranscriptSourceTests.swift`

**Interfaces:**
- Consumes: `TranscriptSource` (Task 6).
- Produces: `ClaudeTranscriptSource(worktreePath: String, sessionRef: String, homeDirectory: URL = FileManager.default.homeDirectoryForCurrentUser)`, conforme a `TranscriptSource`.

- [ ] **Step 1: Write the failing test**

```swift
import Testing
import Foundation
@testable import TillerAgents

@Test func readsLastAssistantAndUserLinesFromJSONL() throws {
    let tmp = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    let worktreePath = "/Users/tester/project"
    // Claude Code escapa il path della working dir sostituendo "/" con "-".
    let escaped = worktreePath.replacingOccurrences(of: "/", with: "-")
    let projectDir = tmp.appendingPathComponent(".claude/projects/\(escaped)")
    try FileManager.default.createDirectory(at: projectDir, withIntermediateDirectories: true)
    let sessionRef = "abc-123"
    let jsonl = """
    {"type":"user","message":{"role":"user","content":"fix the login bug"}}
    {"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"Looking at auth.ts now"}]}}
    """
    try jsonl.write(to: projectDir.appendingPathComponent("\(sessionRef).jsonl"), atomically: true, encoding: .utf8)
    defer { try? FileManager.default.removeItem(at: tmp) }

    let source = ClaudeTranscriptSource(worktreePath: worktreePath, sessionRef: sessionRef, homeDirectory: tmp)
    let text = source.recentText()

    #expect(text?.contains("fix the login bug") == true)
    #expect(text?.contains("Looking at auth.ts now") == true)
}

@Test func returnsNilWhenTranscriptFileMissing() {
    let source = ClaudeTranscriptSource(worktreePath: "/nope", sessionRef: "missing", homeDirectory: FileManager.default.temporaryDirectory)
    #expect(source.recentText() == nil)
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd Packages/TillerAgents && swift test --filter ClaudeTranscriptSourceTests`
Expected: FAIL — `cannot find 'ClaudeTranscriptSource' in scope`

- [ ] **Step 3: Write minimal implementation**

```swift
import Foundation
import TillerCore

/// Legge il transcript JSONL nativo di Claude Code per il pane corrente.
/// Convenzione: `~/.claude/projects/<worktreePath con "/" sostituito da "-">/<sessionRef>.jsonl`.
public struct ClaudeTranscriptSource: TranscriptSource {
    private let worktreePath: String
    private let sessionRef: String
    private let homeDirectory: URL

    public init(worktreePath: String, sessionRef: String,
                homeDirectory: URL = FileManager.default.homeDirectoryForCurrentUser) {
        self.worktreePath = worktreePath
        self.sessionRef = sessionRef
        self.homeDirectory = homeDirectory
    }

    public func recentText() -> String? {
        let escaped = worktreePath.replacingOccurrences(of: "/", with: "-")
        let path = homeDirectory
            .appendingPathComponent(".claude/projects/\(escaped)/\(sessionRef).jsonl")
        guard let data = try? Data(contentsOf: path), !data.isEmpty else { return nil }
        let lines = String(decoding: data, as: UTF8.self).split(separator: "\n")
        var chunks: [String] = []
        for line in lines {
            guard let obj = try? JSONSerialization.jsonObject(with: Data(line.utf8)) as? [String: Any],
                  let message = obj["message"] as? [String: Any] else { continue }
            if let content = message["content"] as? String {
                chunks.append(content)
            } else if let blocks = message["content"] as? [[String: Any]] {
                for block in blocks where block["type"] as? String == "text" {
                    if let text = block["text"] as? String { chunks.append(text) }
                }
            }
        }
        guard !chunks.isEmpty else { return nil }
        return chunks.joined(separator: "\n")
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd Packages/TillerAgents && swift test --filter ClaudeTranscriptSourceTests`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add Packages/TillerAgents/Sources/TillerAgents/ClaudeTranscriptSource.swift Packages/TillerAgents/Tests/TillerAgentsTests/ClaudeTranscriptSourceTests.swift
git commit -m "feat: add ClaudeTranscriptSource (reads Claude Code JSONL transcript)"
```

---

### Task 8: `FileTranscriptSource` — Codex

**Files:**
- Create: `Packages/TillerAgents/Sources/TillerAgents/CodexTranscriptSource.swift`
- Test: `Packages/TillerAgents/Tests/TillerAgentsTests/CodexTranscriptSourceTests.swift`

**Interfaces:**
- Consumes: `TranscriptSource` (Task 6).
- Produces: `CodexTranscriptSource(sessionRef: String, homeDirectory: URL = ...)`, conforme a `TranscriptSource`.

- [ ] **Step 1: Write the failing test**

```swift
import Testing
import Foundation
@testable import TillerAgents

@Test func findsRolloutFileByIdAcrossDateSubdirectories() throws {
    let tmp = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    let sessionsDir = tmp.appendingPathComponent(".codex/sessions/2026/07/20")
    try FileManager.default.createDirectory(at: sessionsDir, withIntermediateDirectories: true)
    let id = "abcd1234-0000-0000-0000-000000000000"
    let jsonl = """
    {"type":"message","role":"user","content":[{"type":"text","text":"add rate limiting"}]}
    {"type":"message","role":"assistant","content":[{"type":"text","text":"Added a token bucket limiter"}]}
    """
    try jsonl.write(
        to: sessionsDir.appendingPathComponent("rollout-2026-07-20T12-00-00-\(id).jsonl"),
        atomically: true, encoding: .utf8)
    defer { try? FileManager.default.removeItem(at: tmp) }

    let source = CodexTranscriptSource(sessionRef: id, homeDirectory: tmp)
    let text = source.recentText()

    #expect(text?.contains("add rate limiting") == true)
    #expect(text?.contains("Added a token bucket limiter") == true)
}

@Test func returnsNilWhenNoMatchingRollout() {
    let source = CodexTranscriptSource(sessionRef: "missing", homeDirectory: FileManager.default.temporaryDirectory)
    #expect(source.recentText() == nil)
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd Packages/TillerAgents && swift test --filter CodexTranscriptSourceTests`
Expected: FAIL — `cannot find 'CodexTranscriptSource' in scope`

- [ ] **Step 3: Write minimal implementation**

```swift
import Foundation
import TillerCore

/// Legge il rollout JSONL nativo di Codex per il pane corrente. Il nome
/// file (`rollout-<data>-<uuid>.jsonl`) incorpora l'id di sessione ma non
/// la sua directory-per-data, quindi la ricerca fa una enumerazione
/// ricorsiva di `~/.codex/sessions` filtrando per suffisso `-<id>.jsonl`.
public struct CodexTranscriptSource: TranscriptSource {
    private let sessionRef: String
    private let homeDirectory: URL

    public init(sessionRef: String,
                homeDirectory: URL = FileManager.default.homeDirectoryForCurrentUser) {
        self.sessionRef = sessionRef
        self.homeDirectory = homeDirectory
    }

    public func recentText() -> String? {
        let sessionsRoot = homeDirectory.appendingPathComponent(".codex/sessions")
        guard let enumerator = FileManager.default.enumerator(
            at: sessionsRoot, includingPropertiesForKeys: nil,
            options: [.skipsHiddenFiles]
        ) else { return nil }
        let suffix = "-\(sessionRef.lowercased()).jsonl"
        guard let match = (enumerator.compactMap { $0 as? URL })
            .first(where: { $0.lastPathComponent.lowercased().hasSuffix(suffix) })
        else { return nil }
        guard let data = try? Data(contentsOf: match), !data.isEmpty else { return nil }
        let lines = String(decoding: data, as: UTF8.self).split(separator: "\n")
        var chunks: [String] = []
        for line in lines {
            guard let obj = try? JSONSerialization.jsonObject(with: Data(line.utf8)) as? [String: Any],
                  let blocks = obj["content"] as? [[String: Any]] else { continue }
            for block in blocks where block["type"] as? String == "text" {
                if let text = block["text"] as? String { chunks.append(text) }
            }
        }
        guard !chunks.isEmpty else { return nil }
        return chunks.joined(separator: "\n")
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd Packages/TillerAgents && swift test --filter CodexTranscriptSourceTests`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add Packages/TillerAgents/Sources/TillerAgents/CodexTranscriptSource.swift Packages/TillerAgents/Tests/TillerAgentsTests/CodexTranscriptSourceTests.swift
git commit -m "feat: add CodexTranscriptSource (reads Codex rollout JSONL)"
```

---

### Task 9: `ChatTranscriptSource`

**Files:**
- Create: `App/Chat/ChatTranscriptSource.swift`
- Test: `AppTests/ChatTranscriptSourceTests.swift`

**Interfaces:**
- Consumes: `TranscriptSource` (Task 6), `ChatController.items: [TranscriptItem]` (esistente), `TranscriptItem.userMessage(id:blocks:)` / `.agentMessage(id:text:isComplete:)` (esistenti, `Packages/TillerACP/Sources/TillerACP/TranscriptItem.swift`), `ContentBlock.text(String)` (esistente).
- Produces: `ChatTranscriptSource(controller: ChatController)`, conforme a `TranscriptSource`, `@MainActor`.

- [ ] **Step 1: Write the failing test**

```swift
@MainActor @Test func extractsUserAndAgentTextFromRecentItems() {
    let controller = ChatController(tabId: UUID(), agentId: "claude", worktreeId: UUID(),
                                    worktreePath: "/tmp", store: nil)
    // Il reducer non è direttamente iniettabile da fuori: il test usa
    // l'API pubblica di ChatController per popolare `items` — verificare
    // con `grep -n "func apply\|func userPrompted" App/Chat/ChatController.swift`
    // qual è il metodo pubblico/internal usato per iniettare eventi ACP nei
    // test esistenti di ChatController e riusare lo stesso pattern.

    let source = ChatTranscriptSource(controller: controller)
    let text = source.recentText()
    #expect(text == nil) // nessun turno ancora avvenuto
}
```

Nota: questo task dipende dal pattern di test già usato per `ChatController` altrove (cercare `AppTests/ChatControllerTests.swift` o simile con `grep -rln "ChatController(" AppTests`) per sapere come iniettare `SessionUpdate` di test nel reducer senza un vero processo ACP. Riusare esattamente quell'helper per un secondo test che verifica testo non-nil dopo un turno con un messaggio utente e uno risposta agente.

- [ ] **Step 2: Run test to verify it fails**

Run: comando di test del target App (vedi nota Task 3, Step 2).
Expected: FAIL — `cannot find 'ChatTranscriptSource' in scope`

- [ ] **Step 3: Write minimal implementation**

```swift
import Foundation
import TillerACP
import TillerCore

/// Estrae il testo delle ultime battute (utente + agente) dal transcript
/// già in memoria di una chat tab ACP — nessuna lettura da disco.
@MainActor
struct ChatTranscriptSource: TranscriptSource {
    let controller: ChatController

    func recentText() -> String? {
        var chunks: [String] = []
        for item in controller.items {
            switch item {
            case .userMessage(_, let blocks):
                for block in blocks {
                    if case .text(let text) = block { chunks.append(text) }
                }
            case .agentMessage(_, let text, _):
                chunks.append(text)
            default:
                continue
            }
        }
        guard !chunks.isEmpty else { return nil }
        return chunks.joined(separator: "\n")
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: come Step 2.
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add App/Chat/ChatTranscriptSource.swift AppTests/ChatTranscriptSourceTests.swift
git commit -m "feat: add ChatTranscriptSource (reads in-memory ACP transcript)"
```

---

### Task 10: Toggle Settings

**Files:**
- Modify: `Packages/TillerCore/Sources/TillerCore/AppSettings.swift`
- Modify: `App/GeneralSettingsView.swift`
- Test: `Packages/TillerCore/Tests/TillerCoreTests/AppSettingsTests.swift`

**Interfaces:**
- Produces: `AppSettings.autoNamingEnabledKey: String = "autoNaming.enabled"` (missing = disabled, cmux-style opt-in — a differenza di `controlSocketEnabledKey` il default è `false`).

- [ ] **Step 1: Write the failing test**

```swift
@Test func autoNamingEnabledKeyDefaultsToFalseWhenAbsent() {
    // Convenzione: la UI legge la chiave via @AppStorage(default: false).
    #expect(AppSettings.autoNamingEnabledKey == "autoNaming.enabled")
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd Packages/TillerCore && swift test --filter AppSettingsTests`
Expected: FAIL — `type 'AppSettings' has no member 'autoNamingEnabledKey'`

- [ ] **Step 3: Write minimal implementation**

In `AppSettings.swift`, aggiungere:

```swift
/// UserDefaults key per il toggle di auto-naming di tab/agenti. Missing
/// value significa disabilitato (default false) — a differenza degli altri
/// toggle qui, opt-in perché genera chiamate CLI extra a carico dell'utente.
public static let autoNamingEnabledKey = "autoNaming.enabled"
```

In `GeneralSettingsView.swift`, aggiungere lo state e una nuova sezione:

```swift
@AppStorage(AppSettings.autoNamingEnabledKey) private var autoNamingEnabled = false
```

```swift
Section("Automation") {
    Toggle(isOn: $autoNamingEnabled) {
        Text("Auto-rename tabs and agents")
        Text("Summarizes each session's conversation into a short tab title using the session's own agent CLI. Manual renames always win.")
    }
}
```

(posizionare la nuova `Section` dopo `Section("Agents")`, prima di `Section("Performance")`, seguendo l'ordine esistente del `Form`.)

- [ ] **Step 4: Run test to verify it passes**

Run: `cd Packages/TillerCore && swift test --filter AppSettingsTests`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add Packages/TillerCore/Sources/TillerCore/AppSettings.swift App/GeneralSettingsView.swift Packages/TillerCore/Tests/TillerCoreTests/AppSettingsTests.swift
git commit -m "feat: add auto-naming Settings toggle (off by default)"
```

---

### Task 11: `AutoNamer` orchestratore

**Files:**
- Create: `App/AutoNaming/AutoNamer.swift`
- Test: `AppTests/AutoNamerTests.swift`

**Interfaces:**
- Consumes: `TranscriptSource.recentText()` (Task 6-9), `AgentAdapter.summarizerCommand(prompt:)` (Task 5).
- Produces: `AutoNamer.summarize(transcript: String, worktreePath: String, adapter: any AgentAdapter, timeout: TimeInterval = 10) async -> String?` — testabile senza un vero processo iniettando un adapter fittizio il cui `summarizerCommand` esegue `echo` invece di una vera CLI.

- [ ] **Step 1: Write the failing test**

```swift
import Testing
@testable import Tiller // sostituire con il nome reale del modulo App/target — verificare con `grep -n "PRODUCT_NAME\|name:" project.yml`

private struct EchoAdapter: AgentAdapter {
    let id = "echo"
    let displayName = "Echo"
    var hasNativeHooks: Bool { false }
    func prepare(worktreePath: String, paneId: UUID, tillerctlPath: String) throws {}
    func command(worktreePath: String, paneId: UUID, tillerctlPath: String) -> String { "" }
    func resumeCommand(worktreePath: String, paneId: UUID, tillerctlPath: String, sessionRef: String) -> String? { nil }
    func summarizerCommand(prompt: String) -> String? { "echo 'Fix login bug'" }
}

private struct MissingBinaryAdapter: AgentAdapter {
    let id = "missing"
    let displayName = "Missing"
    var hasNativeHooks: Bool { false }
    func prepare(worktreePath: String, paneId: UUID, tillerctlPath: String) throws {}
    func command(worktreePath: String, paneId: UUID, tillerctlPath: String) -> String { "" }
    func resumeCommand(worktreePath: String, paneId: UUID, tillerctlPath: String, sessionRef: String) -> String? { nil }
    func summarizerCommand(prompt: String) -> String? { "definitely-not-a-real-binary-xyz" }
}

private struct UnsupportedAdapter: AgentAdapter {
    let id = "unsupported"
    let displayName = "Unsupported"
    var hasNativeHooks: Bool { false }
    func prepare(worktreePath: String, paneId: UUID, tillerctlPath: String) throws {}
    func command(worktreePath: String, paneId: UUID, tillerctlPath: String) -> String { "" }
    func resumeCommand(worktreePath: String, paneId: UUID, tillerctlPath: String, sessionRef: String) -> String? { nil }
    // summarizerCommand non sovrascritto → default nil
}

@Test func summarizeReturnsTrimmedStdout() async {
    let result = await AutoNamer.summarize(
        transcript: "user asked to fix login bug", worktreePath: "/tmp",
        adapter: EchoAdapter())
    #expect(result == "Fix login bug")
}

@Test func summarizeReturnsNilWhenAdapterUnsupported() async {
    let result = await AutoNamer.summarize(
        transcript: "anything", worktreePath: "/tmp", adapter: UnsupportedAdapter())
    #expect(result == nil)
}

@Test func summarizeReturnsNilWhenBinaryMissing() async {
    let result = await AutoNamer.summarize(
        transcript: "anything", worktreePath: "/tmp", adapter: MissingBinaryAdapter())
    #expect(result == nil)
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: comando di test del target App (vedi nota Task 3, Step 2).
Expected: FAIL — `cannot find 'AutoNamer' in scope`

- [ ] **Step 3: Write minimal implementation**

```swift
import Foundation
import TillerAgents

/// Orchestra il pass di auto-naming: costruisce il prompt, lancia il
/// comando di riassunto dell'adapter come processo detached, parsa
/// l'output. Ogni fallimento (adapter non supportato, binario assente,
/// timeout, output vuoto) restituisce nil — mai un errore visibile.
enum AutoNamer {
    static let maxTitleLength = 60

    static func summarize(
        transcript: String, worktreePath: String,
        adapter: any AgentAdapter, timeout: TimeInterval = 10
    ) async -> String? {
        let prompt = """
        Summarize this coding-agent conversation into a short title, \
        2-5 words, in the conversation's own language, no quotes, no punctuation \
        at the end. Reply with only the title.

        \(transcript)
        """
        guard let command = adapter.summarizerCommand(prompt: prompt) else { return nil }

        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/bin/zsh")
        process.arguments = ["-lc", command]
        process.currentDirectoryURL = URL(fileURLWithPath: worktreePath)
        let outputPipe = Pipe()
        process.standardOutput = outputPipe
        process.standardError = Pipe() // scartato: mai propagare stderr nella UI

        do {
            try process.run()
        } catch {
            return nil // binario assente o non eseguibile
        }

        let output: Data? = await withTaskGroup(of: Data?.self) { group in
            group.addTask {
                outputPipe.fileHandleForReading.readDataToEndOfFile()
            }
            group.addTask {
                try? await Task.sleep(nanoseconds: UInt64(timeout * 1_000_000_000))
                if process.isRunning { process.terminate() }
                return nil
            }
            let first = await group.next() ?? nil
            group.cancelAll()
            return first
        }
        process.waitUntilExit()

        guard let output else { return nil }
        let title = String(decoding: output, as: UTF8.self)
            .trimmingCharacters(in: .whitespacesAndNewlines)
        guard !title.isEmpty else { return nil }
        return String(title.prefix(maxTitleLength))
    }
}
```

Nota implementativa: la race fra "lettura stdout completata" e "timeout" con `withTaskGroup` restituisce il primo che completa; se il timeout vince, la lettura di `outputPipe` resta appesa in background finché il processo terminato non chiude la pipe — accettabile perché `Task` non trattenuto blocca solo se stesso, non il chiamante. Se in review emergesse una leak di task non voluta, sostituire con `FileHandle` non bloccante o `AsyncBytes`; verificare con `pr-review-toolkit:code-reviewer` prima di considerare il task concluso.

- [ ] **Step 4: Run test to verify it passes**

Run: come Step 2.
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add App/AutoNaming/AutoNamer.swift AppTests/AutoNamerTests.swift
git commit -m "feat: add AutoNamer orchestrator (spawn, timeout, parse)"
```

---

### Task 12: Wiring del trigger in `AppModel.notifyTransition`

**Files:**
- Modify: `App/AppModel.swift` (aggiungere stato `autoNamingThrottle`, nuova `requestAutoRename`, chiamata in `notifyTransition`)
- Test: `AppTests/AutoRenameWiringTests.swift`

**Interfaces:**
- Consumes: `AutoNamingThrottle` (Task 4), `TranscriptSource` via `ChatTranscriptSource`/`ClaudeTranscriptSource`/`CodexTranscriptSource` (Task 7-9), `AutoNamer.summarize` (Task 11), `AppModel.applyAutoTitle` (Task 3), `AppSettings.autoNamingEnabledKey` (Task 10), `ProjectStore.agentSessionRefs(of:)` (esistente), `AgentActivityModel.paneAgents` (esistente).
- Produces: nessuna nuova API pubblica — comportamento osservabile via `model.tabs[...]![...].title` dopo una transizione `.running → .done/.needsInput`.

- [ ] **Step 1: Write the failing test**

```swift
@MainActor @Test func autoRenameFiresOnRunningToNeedsInputWhenEnabled() async throws {
    UserDefaults.standard.set(true, forKey: AppSettings.autoNamingEnabledKey)
    defer { UserDefaults.standard.removeObject(forKey: AppSettings.autoNamingEnabledKey) }

    let model = try AppModel.forTesting()
    let worktree = try model.addTestWorktree()
    let tab = try model.addTestChatTab(agentId: "claude", worktree: worktree) // helper da verificare/creare accanto agli altri test-helper di AppModel

    model.chatController(for: tab, in: worktree)?.onStatusChange?(.running)
    // Simula un turno con contenuto sufficiente a superare il throttle di crescita.
    // Il meccanismo esatto per iniettare testo nel reducer di test dipende
    // dall'helper già usato in AppTests/ChatControllerTests.swift (Task 9) —
    // riusarlo qui per popolare `controller.items` prima della transizione finale.
    model.chatController(for: tab, in: worktree)?.onStatusChange?(.needsInput)

    try await Task.sleep(nanoseconds: 500_000_000) // il pass gira in un Task detached
    let updated = model.tabs[worktree.id]!.first { $0.id == tab.id }!
    #expect(updated.title != tab.title) // rinominata rispetto al placeholder iniziale
}

@MainActor @Test func autoRenameSkipsWhenSettingDisabled() async throws {
    UserDefaults.standard.removeObject(forKey: AppSettings.autoNamingEnabledKey) // default false
    let model = try AppModel.forTesting()
    let worktree = try model.addTestWorktree()
    let tab = try model.addTestChatTab(agentId: "claude", worktree: worktree)
    let originalTitle = tab.title

    model.chatController(for: tab, in: worktree)?.onStatusChange?(.running)
    model.chatController(for: tab, in: worktree)?.onStatusChange?(.needsInput)
    try await Task.sleep(nanoseconds: 500_000_000)

    let updated = model.tabs[worktree.id]!.first { $0.id == tab.id }!
    #expect(updated.title == originalTitle)
}

@MainActor @Test func autoRenameSkipsManuallyRenamedTab() async throws {
    UserDefaults.standard.set(true, forKey: AppSettings.autoNamingEnabledKey)
    defer { UserDefaults.standard.removeObject(forKey: AppSettings.autoNamingEnabledKey) }
    let model = try AppModel.forTesting()
    let worktree = try model.addTestWorktree()
    let tab = try model.addTestChatTab(agentId: "claude", worktree: worktree)
    model.renameTab(tab.id, in: worktree.id, to: "Kept by user")

    model.chatController(for: tab, in: worktree)?.onStatusChange?(.running)
    model.chatController(for: tab, in: worktree)?.onStatusChange?(.needsInput)
    try await Task.sleep(nanoseconds: 500_000_000)

    let updated = model.tabs[worktree.id]!.first { $0.id == tab.id }!
    #expect(updated.title == "Kept by user")
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: comando di test del target App (vedi nota Task 3, Step 2).
Expected: FAIL — il titolo non cambia mai (nessun wiring ancora presente).

- [ ] **Step 3: Write minimal implementation**

Nuovo stato in `AppModel` (accanto a `chatControllers`/`chatStore`):

```swift
private var autoNamingThrottle: [UUID: AutoNamingThrottle] = [:]
```

Modifica di `notifyTransition`:

```swift
private func notifyTransition(paneId: UUID, from old: AgentStatus?, to new: AgentStatus) {
    requestAutoRename(paneId: paneId, from: old, to: new)
    let visible = isSelectedWorktreeContaining(paneId: paneId)
    guard NotificationPolicy.shouldNotify(
        old: old, new: new, appActive: NSApp.isActive, paneVisible: visible
    ) else { return }
    guard let payload = buildPayload(paneId: paneId, status: new) else { return }
    notifier.post(payload)
}

/// Innesca un pass di auto-naming quando un turno finisce (running → done
/// o needs-input). Copre le chat tab ACP per tutti e 5 gli agenti e le
/// terminal tab dei 3 adapter con hook nativi (Claude Code, Codex,
/// Oh-My-Pi) — vedi docs/superpowers/specs/2026-07-20-auto-rename-tabs-agents-design.md.
private func requestAutoRename(paneId: UUID, from old: AgentStatus?, to new: AgentStatus) {
    guard old == .running, new == .done || new == .needsInput else { return }
    guard AppSettings.autoNamingEnabled(
        defaultsValue: UserDefaults.standard.object(forKey: AppSettings.autoNamingEnabledKey) as? Bool
    ) else { return }
    guard let worktree = worktreeContaining(paneId: paneId),
          let idx = tabs[worktree.id]?.firstIndex(where: { $0.activityPaneIds.contains(paneId) })
    else { return }
    let tab = tabs[worktree.id]![idx]
    guard tab.titleIsAutoNamed,
          let agentId = agentActivity.paneAgents[paneId],
          let adapter = AgentCatalog.all.first(where: { $0.id == agentId })
    else { return }

    let source: TranscriptSource?
    switch tab.content {
    case .chat:
        source = chatControllers[tab.id].map { ChatTranscriptSource(controller: $0) }
    case .terminal:
        source = resolveFileTranscriptSource(paneId: paneId, worktree: worktree, agentId: agentId)
    case .markdown:
        source = nil
    }
    guard let source, let text = source.recentText() else { return }

    let throttle = autoNamingThrottle[paneId] ?? AutoNamingThrottle()
    let now = Date()
    guard throttle.shouldRun(transcriptLength: text.count, now: now) else { return }
    autoNamingThrottle[paneId] = throttle.recording(transcriptLength: text.count, now: now)

    let worktreePath = worktree.path
    Task { [weak self] in
        guard let title = await AutoNamer.summarize(
            transcript: text, worktreePath: worktreePath, adapter: adapter
        ) else { return }
        await MainActor.run {
            self?.applyAutoTitle(tab.id, in: worktree.id, title: title)
        }
    }
}

/// Sorgente file-based per le 3 terminal-tab adapter con hook nativi
/// (Claude Code, Codex, Oh-My-Pi) — vedi spec per il gap dichiarato su
/// OpenCode/Pi (nessun `AgentSessionRef` addressabile) e su Oh-My-Pi
/// (convenzione transcript ancora da verificare, non implementata qui).
private func resolveFileTranscriptSource(
    paneId: UUID, worktree: Worktree, agentId: String
) -> TranscriptSource? {
    guard let store,
          let refs = try? store.agentSessionRefs(of: worktree.id),
          let ref = refs.first(where: { $0.paneId == paneId })
    else { return nil }
    switch agentId {
    case "claude": return ClaudeTranscriptSource(worktreePath: worktree.path, sessionRef: ref.sessionRef)
    case "codex": return CodexTranscriptSource(sessionRef: ref.sessionRef)
    default: return nil
    }
}
```

Aggiungere `AppSettings.autoNamingEnabled(defaultsValue:) -> Bool` accanto a `controlSocketEnabled` (Task 10 non l'ha ancora prodotta — completare qui):

```swift
public static func autoNamingEnabled(defaultsValue: Bool?) -> Bool {
    defaultsValue ?? false
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: come Step 2.
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add App/AppModel.swift Packages/TillerCore/Sources/TillerCore/AppSettings.swift AppTests/AutoRenameWiringTests.swift
git commit -m "feat: wire auto-rename trigger into notifyTransition"
```

---

### Task 13: Verifica finale

**Files:** nessuno — solo verifica.

- [ ] **Step 1: Full CI**

Run: `Scripts/ci.sh`
Expected: stampa `CI OK`

- [ ] **Step 2: Xcodegen + build check**

Run: `xcodegen generate && open Tiller.xcodeproj` — build manuale con ⌘R, verificare che:
1. Settings → Automation mostri il nuovo toggle, off di default.
2. Con toggle ON, una chat tab Claude Code rinomina il proprio titolo dopo un turno completo (richiede autenticazione Claude reale — QA manuale, non automatizzabile in CI).
3. Un rename manuale della tab blocchi l'auto-naming successivo (verificabile anche senza autenticazione: rinominare, aspettare, controllare che il titolo non torni al placeholder).

- [ ] **Step 3: Commit finale se emergono fix**

```bash
git add -A
git commit -m "fix: address CI/build issues found in final verification"
```

---

## Self-Review

**1. Copertura spec:**
- Trigger via `AgentTransition` esistente → Task 12. ✓
- Provenance (`titleIsAutoNamed`, rename manuale vince) → Task 1, 2, 3. ✓
- `ChatTranscriptSource` (tutti e 5 gli agenti, chat tab) → Task 9. ✓
- `FileTranscriptSource` Claude/Codex (terminal tab) → Task 7, 8. ✓
- Oh-My-Pi terminal transcript → dichiarato esplicitamente fuori scope in Task 12 (commento + `default: nil` nello switch). ✓
- OpenCode/Pi terminal → mai raggiunti (non hanno hook nativi, `hasNativeHooks == false`; il trigger di Task 12 dipende solo da `notifyTransition`, chiamato per il loro stato via Layer B/C ma `resolveFileTranscriptSource` ritorna nil per adapter diversi da claude/codex, quindi no-op corretto). ✓
- `AgentAdapter.summarizerCommand` per tutti e 5 → Task 5. ✓
- Errori silenziosi (binario assente, timeout, output vuoto, tab chiusa) → Task 11 (`AutoNamer.summarize` ritorna sempre `nil` su fallimento) + Task 12 (`guard let title` non applica nulla su nil; `applyAutoTitle` in Task 3 fa già guard su tab ancora esistente/ancora auto-named). ✓
- Settings toggle off-by-default → Task 10. ✓
- Test unit per throttle, provenance, path resolution, parsing → Task 4, 3, 7, 8, 5, 11. ✓

**2. Placeholder scan:** nessun TBD/TODO. Le uniche note "verificare con grep" (Task 3, 9, 11 Step 1) indicano dove l'implementatore deve riusare un helper di test esistente invece di inventarne uno nuovo — non sono lavoro non specificato, è codice reale con un solo dettaglio (il nome esatto dell'helper) da confermare leggendo un file adiacente già nel repo.

**3. Coerenza dei tipi:** `TranscriptSource.recentText() -> String?` usato identicamente da `ChatTranscriptSource` (Task 9), `ClaudeTranscriptSource`/`CodexTranscriptSource` (Task 7-8) e consumato in Task 12. `AgentAdapter.summarizerCommand(prompt: String) -> String?` firma identica in protocollo (Task 5) e nei 5 adapter. `AutoNamer.summarize(transcript:worktreePath:adapter:timeout:) async -> String?` firma identica fra Task 11 e la chiamata in Task 12. `applyAutoTitle(_:in:title:)` firma identica fra Task 3 e Task 12.

---

**Plan complete and saved to `docs/superpowers/plans/2026-07-20-auto-rename-tabs-agents.md`. Two execution options:**

**1. Subagent-Driven (recommended)** - dispatch di un subagent fresco per task, review fra un task e l'altro, iterazione rapida.

**2. Inline Execution** - esecuzione dei task in questa sessione via executing-plans, batch execution con checkpoint.

**Quale preferisci?**
