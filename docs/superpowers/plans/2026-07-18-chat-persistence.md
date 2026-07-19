# Chat Persistence Implementation Plan (Plan 2 of 3)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Persist chat transcripts and ACP session ids in GRDB (`chatSession`/`chatItem` tables + `ChatSessionStore`), and expose the agent-id → launch-spec mapping, so Plan 3's UI can bind, save, and resume conversations.

**Architecture:** Schema and records live in `TillerPersistence` (migration `v8`, raw `Data` payloads — the package stays protocol-agnostic). `ChatSessionStore` lives in `TillerACP`, which gains a dependency on `TillerPersistence` (as the spec's architecture diagram allows): it encodes/decodes `TranscriptItem` and owns the write policy. Write policy simplification vs the spec: instead of tracking per-item "finalized" state, `saveTranscript` atomically replaces a session's whole transcript in one transaction; Plan 3 calls it at settle points only (turn end, permission resolution, tool-call terminal state, pane close/quit) — the spec's real requirement ("no per-chunk writes, flush on close") holds with far less bookkeeping. Table names use the repo's lowerCamel convention (`chatSession`, not the spec sketch's `chat_session`). The spec's `AgentCatalog.supportsACP` flag becomes `AgentLaunchSpec.forAgent(id:)` returning nil for unsupported agents — single source of truth in TillerACP, zero changes to TillerAgents.

**Tech Stack:** Swift 6, GRDB (already a TillerPersistence dependency), swift-testing.

## Global Constraints

- Swift tools 6.0, `.macOS(.v15)`; swift-testing (`import Testing`, `@Test`, `#expect`) — never XCTest.
- Migrations are append-only: add `v8` after `v7` in `AppDatabase.migrator`; never edit v1–v7.
- Touch only `Packages/TillerPersistence/` and `Packages/TillerACP/`. No project.yml, no App/, no other package.
- Conventional Commits, lower-case imperative subject.
- Final gate: `Scripts/ci.sh` prints `CI OK` (TillerTerminal `spawnCapturesOutput` is a known flake — retry up to 5-6 times if it is the only failure).
- Adapter agent ids are `"claude"` and `"opencode"` (from ClaudeCodeAdapter/OpenCodeAdapter).

---

### Task 1: Migration v8 + chat records (TillerPersistence)

**Files:**
- Modify: `Packages/TillerPersistence/Sources/TillerPersistence/AppDatabase.swift` (append migration after `v7`, before `return migrator`)
- Modify: `Packages/TillerPersistence/Sources/TillerPersistence/Records.swift` (append two records at end)
- Test: `Packages/TillerPersistence/Tests/TillerPersistenceTests/ChatRecordsTests.swift`

**Interfaces:**
- Produces: `ChatSessionRecord{id: String, worktreeId: String, agentId: String, acpSessionId: String?, createdAt: Date, lastActivityAt: Date}` and `ChatItemRecord{sessionId: String, ordinal: Int, kind: String, payload: Data}` — both `Codable, FetchableRecord, PersistableRecord, Sendable, Equatable` with memberwise `init` (defaults: `acpSessionId: String? = nil`). Tables: `chatSession` (PK `id`, FK `worktreeId` → `worktree` cascade), `chatItem` (composite PK `[sessionId, ordinal]`, FK `sessionId` → `chatSession` cascade).

- [ ] **Step 1: Write the failing test**

```swift
// Packages/TillerPersistence/Tests/TillerPersistenceTests/ChatRecordsTests.swift
import Testing
import Foundation
import GRDB
@testable import TillerPersistence

@Suite struct ChatRecordsTests {
    /// Inserts the project + worktree rows chatSession's FK chain needs.
    private func makeDatabaseWithWorktree() throws -> (AppDatabase, worktreeId: String) {
        let database = try AppDatabase.inMemory()
        let worktreeId = UUID().uuidString
        try database.write { db in
            try ProjectRecord(id: "p1", name: "P", rootPath: "/p", createdAt: .init())
                .insert(db)
            try WorktreeRecord(id: worktreeId, projectId: "p1", branch: "main",
                               path: "/p", createdAt: .init()).insert(db)
        }
        return (database, worktreeId)
    }

    @Test func sessionAndItemsRoundTrip() throws {
        let (database, worktreeId) = try makeDatabaseWithWorktree()
        let session = ChatSessionRecord(
            id: "s1", worktreeId: worktreeId, agentId: "claude",
            acpSessionId: "acp-1", createdAt: .init(), lastActivityAt: .init())
        let item = ChatItemRecord(sessionId: "s1", ordinal: 0,
                                  kind: "userMessage", payload: Data("x".utf8))
        try database.write { db in
            try session.insert(db)
            try item.insert(db)
        }
        let fetchedSession = try database.read { db in
            try ChatSessionRecord.fetchOne(db, key: "s1")
        }
        let fetchedItems = try database.read { db in
            try ChatItemRecord.fetchAll(db)
        }
        #expect(fetchedSession?.agentId == "claude")
        #expect(fetchedSession?.acpSessionId == "acp-1")
        #expect(fetchedItems == [item])
    }

    @Test func deletingWorktreeCascadesToSessionsAndItems() throws {
        let (database, worktreeId) = try makeDatabaseWithWorktree()
        try database.write { db in
            try ChatSessionRecord(id: "s1", worktreeId: worktreeId, agentId: "claude",
                                  createdAt: .init(), lastActivityAt: .init()).insert(db)
            try ChatItemRecord(sessionId: "s1", ordinal: 0, kind: "plan",
                               payload: Data()).insert(db)
            _ = try WorktreeRecord.deleteOne(db, key: worktreeId)
        }
        let sessions = try database.read { try ChatSessionRecord.fetchCount($0) }
        let items = try database.read { try ChatItemRecord.fetchCount($0) }
        #expect(sessions == 0)
        #expect(items == 0)
    }

    @Test func duplicateOrdinalInSameSessionIsRejected() throws {
        let (database, worktreeId) = try makeDatabaseWithWorktree()
        try database.write { db in
            try ChatSessionRecord(id: "s1", worktreeId: worktreeId, agentId: "claude",
                                  createdAt: .init(), lastActivityAt: .init()).insert(db)
            try ChatItemRecord(sessionId: "s1", ordinal: 0, kind: "a",
                               payload: Data()).insert(db)
        }
        #expect(throws: (any Error).self) {
            try database.write { db in
                try ChatItemRecord(sessionId: "s1", ordinal: 0, kind: "b",
                                   payload: Data()).insert(db)
            }
        }
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd Packages/TillerPersistence && swift test --filter ChatRecordsTests`
Expected: compile FAILURE — `cannot find 'ChatSessionRecord' in scope`.

- [ ] **Step 3: Add migration v8**

In `AppDatabase.swift`, immediately after the `v7` migration block and before `return migrator`:

```swift
        migrator.registerMigration("v8") { db in
            try db.create(table: "chatSession") { t in
                t.primaryKey("id", .text)
                t.column("worktreeId", .text).notNull()
                    .references("worktree", onDelete: .cascade)
                t.column("agentId", .text).notNull()
                t.column("acpSessionId", .text)
                t.column("createdAt", .datetime).notNull()
                t.column("lastActivityAt", .datetime).notNull()
            }
            try db.create(table: "chatItem") { t in
                t.column("sessionId", .text).notNull()
                    .references("chatSession", onDelete: .cascade)
                t.column("ordinal", .integer).notNull()
                t.column("kind", .text).notNull()
                t.column("payload", .blob).notNull()
                t.primaryKey(["sessionId", "ordinal"])
            }
        }
```

- [ ] **Step 4: Add the records**

Append to `Records.swift`:

```swift
/// One chat conversation bound to a worktree + agent. `acpSessionId` is the
/// agent-side session reference used for `session/load` resume.
public struct ChatSessionRecord: Codable, FetchableRecord, PersistableRecord, Sendable, Equatable {
    public static let databaseTableName = "chatSession"
    public var id: String
    public var worktreeId: String
    public var agentId: String
    public var acpSessionId: String?
    public var createdAt: Date
    public var lastActivityAt: Date

    public init(id: String, worktreeId: String, agentId: String,
                acpSessionId: String? = nil, createdAt: Date, lastActivityAt: Date) {
        self.id = id; self.worktreeId = worktreeId; self.agentId = agentId
        self.acpSessionId = acpSessionId
        self.createdAt = createdAt; self.lastActivityAt = lastActivityAt
    }
}

/// One transcript entry. `payload` is an opaque encoded blob owned by the
/// caller (TillerACP encodes TranscriptItem); `kind` is denormalized for
/// future queries.
public struct ChatItemRecord: Codable, FetchableRecord, PersistableRecord, Sendable, Equatable {
    public static let databaseTableName = "chatItem"
    public var sessionId: String
    public var ordinal: Int
    public var kind: String
    public var payload: Data

    public init(sessionId: String, ordinal: Int, kind: String, payload: Data) {
        self.sessionId = sessionId; self.ordinal = ordinal
        self.kind = kind; self.payload = payload
    }
}
```

- [ ] **Step 5: Run test to verify it passes**

Run: `cd Packages/TillerPersistence && swift test --filter ChatRecordsTests`
Expected: PASS (3 tests). Then full `swift test` for the package — all pass (migration chain intact).

- [ ] **Step 6: Commit**

```bash
git add Packages/TillerPersistence
git commit -m "feat: add chat session and item tables with records"
```

---

### Task 2: ChatSessionStore (TillerACP)

**Files:**
- Modify: `Packages/TillerACP/Package.swift` (add TillerPersistence dependency)
- Create: `Packages/TillerACP/Sources/TillerACP/ChatSessionStore.swift`
- Test: `Packages/TillerACP/Tests/TillerACPTests/ChatSessionStoreTests.swift`

**Interfaces:**
- Consumes: `AppDatabase`, `ChatSessionRecord`, `ChatItemRecord` (Task 1); `TranscriptItem` (Plan 1).
- Produces: `ChatSessionStore` struct — `init(database: AppDatabase)`, `latestSession(worktreeId: String, agentId: String) throws -> ChatSessionRecord?`, `createSession(worktreeId: String, agentId: String, now: Date) throws -> ChatSessionRecord`, `setACPSessionId(_ acpSessionId: String, sessionId: String) throws`, `saveTranscript(sessionId: String, items: [TranscriptItem], now: Date) throws`, `loadTranscript(sessionId: String) throws -> [TranscriptItem]`. Plus internal `TranscriptItem.kindLabel: String`.

- [ ] **Step 1: Add the package dependency**

Replace `Packages/TillerACP/Package.swift` content with:

```swift
// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "TillerACP",
    platforms: [.macOS(.v15)],
    products: [.library(name: "TillerACP", targets: ["TillerACP"])],
    dependencies: [
        .package(path: "../TillerPersistence")
    ],
    targets: [
        .target(name: "TillerACP", dependencies: ["TillerPersistence"]),
        .testTarget(name: "TillerACPTests", dependencies: ["TillerACP", "TillerPersistence"])
    ]
)
```

Run: `cd Packages/TillerACP && swift build` — must still compile.

- [ ] **Step 2: Write the failing test**

```swift
// Packages/TillerACP/Tests/TillerACPTests/ChatSessionStoreTests.swift
import Testing
import Foundation
import TillerPersistence
@testable import TillerACP

@Suite struct ChatSessionStoreTests {
    private func makeStore() throws -> (ChatSessionStore, worktreeId: String) {
        let database = try AppDatabase.inMemory()
        let worktreeId = UUID().uuidString
        try database.write { db in
            try ProjectRecord(id: "p1", name: "P", rootPath: "/p", createdAt: .init())
                .insert(db)
            try WorktreeRecord(id: worktreeId, projectId: "p1", branch: "main",
                               path: "/p", createdAt: .init()).insert(db)
        }
        return (ChatSessionStore(database: database), worktreeId)
    }

    private let sampleItems: [TranscriptItem] = [
        .userMessage(id: "user-0", blocks: [.text("fix the bug")]),
        .toolCall(ToolCallItem(
            toolCallId: "tc1", title: "Edit F.swift", kind: .edit, status: .completed,
            content: [.diff(path: "/w/F.swift", oldText: "a", newText: "b")],
            locations: [ToolCallLocation(path: "/w/F.swift", line: 3)],
            permission: PermissionState(
                requestId: .number(9),
                options: [PermissionOption(optionId: "y", name: "Allow", kind: .allowOnce)],
                resolution: .selected(optionId: "y")))),
        .agentMessage(id: "agent-1", text: "Done.", isComplete: true),
    ]

    @Test func createThenLatestFindsSession() throws {
        let (store, worktreeId) = try makeStore()
        let created = try store.createSession(worktreeId: worktreeId, agentId: "claude",
                                              now: Date(timeIntervalSince1970: 100))
        let latest = try store.latestSession(worktreeId: worktreeId, agentId: "claude")
        #expect(latest?.id == created.id)
        #expect(try store.latestSession(worktreeId: worktreeId, agentId: "opencode") == nil)
    }

    @Test func latestPicksMostRecentActivity() throws {
        let (store, worktreeId) = try makeStore()
        _ = try store.createSession(worktreeId: worktreeId, agentId: "claude",
                                    now: Date(timeIntervalSince1970: 100))
        let newer = try store.createSession(worktreeId: worktreeId, agentId: "claude",
                                            now: Date(timeIntervalSince1970: 200))
        #expect(try store.latestSession(worktreeId: worktreeId, agentId: "claude")?.id
                == newer.id)
    }

    @Test func transcriptRoundTripsIncludingPermissions() throws {
        let (store, worktreeId) = try makeStore()
        let session = try store.createSession(worktreeId: worktreeId, agentId: "claude",
                                              now: .init())
        try store.saveTranscript(sessionId: session.id, items: sampleItems, now: .init())
        #expect(try store.loadTranscript(sessionId: session.id) == sampleItems)
    }

    @Test func saveTranscriptReplacesAndBumpsActivity() throws {
        let (store, worktreeId) = try makeStore()
        let session = try store.createSession(worktreeId: worktreeId, agentId: "claude",
                                              now: Date(timeIntervalSince1970: 100))
        try store.saveTranscript(sessionId: session.id, items: sampleItems,
                                 now: Date(timeIntervalSince1970: 150))
        let shorter: [TranscriptItem] = [.userMessage(id: "user-0", blocks: [.text("hi")])]
        try store.saveTranscript(sessionId: session.id, items: shorter,
                                 now: Date(timeIntervalSince1970: 200))
        #expect(try store.loadTranscript(sessionId: session.id) == shorter)
        let record = try store.latestSession(worktreeId: worktreeId, agentId: "claude")
        #expect(record?.lastActivityAt == Date(timeIntervalSince1970: 200))
    }

    @Test func acpSessionIdIsStoredForResume() throws {
        let (store, worktreeId) = try makeStore()
        let session = try store.createSession(worktreeId: worktreeId, agentId: "opencode",
                                              now: .init())
        try store.setACPSessionId("acp-42", sessionId: session.id)
        #expect(try store.latestSession(worktreeId: worktreeId, agentId: "opencode")?
            .acpSessionId == "acp-42")
    }

    @Test func kindLabelsAreDistinct() {
        let labels = sampleItems.map(\.kindLabel)
        #expect(labels == ["userMessage", "toolCall", "agentMessage"])
    }
}
```

- [ ] **Step 3: Run test to verify it fails**

Run: `cd Packages/TillerACP && swift test --filter ChatSessionStoreTests`
Expected: compile FAILURE — `cannot find 'ChatSessionStore' in scope`.

- [ ] **Step 4: Implement ChatSessionStore**

```swift
// Packages/TillerACP/Sources/TillerACP/ChatSessionStore.swift
import Foundation
import GRDB
import TillerPersistence

/// Persistence facade for chat conversations: session lookup/creation and
/// whole-transcript snapshots. Callers invoke `saveTranscript` at settle
/// points only (turn end, permission resolution, tool-call terminal state,
/// pane close) — never per streaming chunk.
public struct ChatSessionStore: Sendable {
    private let database: AppDatabase

    public init(database: AppDatabase) {
        self.database = database
    }

    /// Most recently active session for a (worktree, agent) pair.
    public func latestSession(worktreeId: String, agentId: String) throws -> ChatSessionRecord? {
        try database.read { db in
            try ChatSessionRecord
                .filter(Column("worktreeId") == worktreeId)
                .filter(Column("agentId") == agentId)
                .order(Column("lastActivityAt").desc)
                .fetchOne(db)
        }
    }

    public func createSession(worktreeId: String, agentId: String,
                              now: Date = Date()) throws -> ChatSessionRecord {
        let record = ChatSessionRecord(
            id: UUID().uuidString, worktreeId: worktreeId, agentId: agentId,
            createdAt: now, lastActivityAt: now)
        try database.write { db in try record.insert(db) }
        return record
    }

    /// Stores the agent-side session reference for later `session/load`.
    public func setACPSessionId(_ acpSessionId: String, sessionId: String) throws {
        try database.write { db in
            guard var record = try ChatSessionRecord.fetchOne(db, key: sessionId) else { return }
            record.acpSessionId = acpSessionId
            try record.update(db)
        }
    }

    /// Atomically replaces the session's transcript and bumps activity.
    public func saveTranscript(sessionId: String, items: [TranscriptItem],
                               now: Date = Date()) throws {
        let encoder = JSONEncoder()
        let records = try items.enumerated().map { ordinal, item in
            ChatItemRecord(sessionId: sessionId, ordinal: ordinal,
                           kind: item.kindLabel, payload: try encoder.encode(item))
        }
        try database.write { db in
            try ChatItemRecord.filter(Column("sessionId") == sessionId).deleteAll(db)
            for record in records { try record.insert(db) }
            if var session = try ChatSessionRecord.fetchOne(db, key: sessionId) {
                session.lastActivityAt = now
                try session.update(db)
            }
        }
    }

    /// Loads the transcript in order; items that no longer decode (schema
    /// drift across app versions) are skipped rather than failing the load.
    public func loadTranscript(sessionId: String) throws -> [TranscriptItem] {
        let records = try database.read { db in
            try ChatItemRecord
                .filter(Column("sessionId") == sessionId)
                .order(Column("ordinal"))
                .fetchAll(db)
        }
        let decoder = JSONDecoder()
        return records.compactMap { try? decoder.decode(TranscriptItem.self, from: $0.payload) }
    }
}

extension TranscriptItem {
    /// Stable discriminator stored in `chatItem.kind` for future queries.
    var kindLabel: String {
        switch self {
        case .userMessage: "userMessage"
        case .agentMessage: "agentMessage"
        case .thought: "thought"
        case .toolCall: "toolCall"
        case .plan: "plan"
        }
    }
}
```

- [ ] **Step 5: Run test to verify it passes**

Run: `cd Packages/TillerACP && swift test --filter ChatSessionStoreTests`
Expected: PASS (6 tests). Then full `swift test` — all suites pass (57 + 6).

- [ ] **Step 6: Commit**

```bash
git add Packages/TillerACP
git commit -m "feat: add chat session store over grdb persistence"
```

---

### Task 3: Agent-id launch mapping + full CI gate

**Files:**
- Modify: `Packages/TillerACP/Sources/TillerACP/AgentLaunchSpec.swift` (add one method)
- Modify: `Packages/TillerACP/Tests/TillerACPTests/AgentLaunchSpecTests.swift` (add one test)

**Interfaces:**
- Produces: `AgentLaunchSpec.forAgent(id: String) -> AgentLaunchSpec?` — nil means the agent has no ACP support (Plan 3 uses this to decide which "Chat" menu entries exist; replaces the spec's `supportsACP` flag with a single source of truth here).

- [ ] **Step 1: Write the failing test**

Append to `AgentLaunchSpecTests`:

```swift
    @Test func forAgentMapsSupportedIdsAndRejectsOthers() {
        #expect(AgentLaunchSpec.forAgent(id: "claude") == .claudeCode())
        #expect(AgentLaunchSpec.forAgent(id: "opencode") == .openCode())
        #expect(AgentLaunchSpec.forAgent(id: "codex") == nil)
        #expect(AgentLaunchSpec.forAgent(id: "pi") == nil)
        #expect(AgentLaunchSpec.forAgent(id: "omp") == nil)
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd Packages/TillerACP && swift test --filter AgentLaunchSpecTests`
Expected: compile FAILURE — `type 'AgentLaunchSpec' has no member 'forAgent'`.

- [ ] **Step 3: Implement forAgent**

Append inside `AgentLaunchSpec`:

```swift
    /// Launch spec for an AgentCatalog id, or nil when the agent has no ACP
    /// support yet. This is the single source of truth Plan 3's UI uses to
    /// build the "Chat" menu (v1: Claude Code and OpenCode).
    public static func forAgent(id: String) -> AgentLaunchSpec? {
        switch id {
        case "claude": claudeCode()
        case "opencode": openCode()
        default: nil
        }
    }
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd Packages/TillerACP && swift test --filter AgentLaunchSpecTests`
Expected: PASS (4 tests).

- [ ] **Step 5: Full CI gate**

Run: `Scripts/ci.sh` from the repo root.
Expected: prints `CI OK` (retry on the known TillerTerminal flake only).

- [ ] **Step 6: Commit**

```bash
git add Packages/TillerACP
git commit -m "feat: map agent ids to acp launch specs"
```

---

## Out of Scope (Plan 3)

`App/Chat/` UI, `ChatViewModel` (wires `ACPSession.events` → `TranscriptReducer` → `ChatSessionStore.saveTranscript` at settle points), resume flow (`latestSession` → show `loadTranscript` → `connect(resumeSessionId:)`), permission cards, composer, pane/tab integration, sidebar status, worktree-removal cleanup call site (cascade already handles the DB side).
