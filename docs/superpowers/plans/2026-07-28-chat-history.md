# Chat History and Session Restore Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Chat tabs come back with their own transcript after a restart, and past conversations of the current worktree are reachable from a history menu next to the tab bar `+`.

**Architecture:** Three independent moves. (1) Split the overloaded `didResume` flag so Tiller stops discarding its own transcript when a native agent resumes without replaying it. (2) Give every chat tab a `chatSessionId` column so it reloads its own session instead of guessing via `latestSession(worktreeId:)`. (3) Add a `.detached` controller state so restored and history-opened chats show their transcript without spawning an agent process.

**Tech Stack:** Swift 6, SwiftUI, GRDB/SQLite (migrations in `TillerPersistence`), swift-testing (`@Test` / `#expect`), macOS 15+.

## Global Constraints

- Tests first, swift-testing (`@Test` / `#expect`), never XCTest.
- Domain models are value types; classes only for real identity.
- Commit messages follow Conventional Commits, lower-case imperative subject.
- All user-facing strings in the app are **English**, even though this plan's prose is Italian-adjacent. No Italian in UI copy.
- `TillerCore` must not import `TillerTerminal`, `TillerControl`, or `TillerAgents`.
- `Scripts/ci.sh` must print `CI OK` before the branch is considered done.
- `AppTests` is **not** run by `ci.sh`. Run it explicitly (see below) and confirm the `Test run with N tests` line — a wrong selector exits green having run zero tests.

Running tests:

```bash
# One package, one test
cd Packages/TillerACP && swift test --filter chatHistoryExcludesEmptySessions

# App target tests — target is TillerTests, selector is the struct name
xcodebuild test -project Tiller.xcodeproj -scheme Tiller -configuration Debug \
  -skipPackagePluginValidation \
  -only-testing:TillerTests/ChatControllerTests

# Whole gate
Scripts/ci.sh
```

The `xcodebuild test` line above is exact and was verified on 2026-07-28. Do not
"improve" it:

- `-skipPackagePluginValidation` gets past the SwiftLint package plugin, which
  otherwise refuses to run. Two SwiftLint failures from third-party CodeEdit
  packages are expected noise; the binary still links.
- **Never add `CODE_SIGNING_ALLOWED=NO` or `-derivedDataPath DerivedData`.**
  With either of them the app test host hangs in dyld `open()` *before test
  discovery* and xcodebuild gives up after ~5 min with "The test runner hung
  before establishing connection" — which reads as a stuck machine, not a
  failing test. Signed, in the default DerivedData, the same suite passes.
- Expect it to be slow: ~13 min for 21 tests on a cold build. That is this
  machine's endpoint-security scanning, not a hung run — wait it out.

`TillerTerminal`'s PTY tests are flaky under parallel load. If `ci.sh` fails only there, rerun before blaming these changes.

## File Structure

**Created:**
- `App/Chat/ChatHistoryRows.swift` — pure row-model builder (title fallback), no SwiftUI.
- `App/Chat/ChatHistoryMenu.swift` — the menu view placed beside the tab bar `+`.
- `AppTests/ChatHistoryRowsTests.swift` — tests for the builder.

**Modified:**
- `Packages/TillerACP/Sources/TillerACP/ACPSession.swift` — `SessionHandle.didReplayHistory`.
- `Packages/TillerACP/Sources/TillerACP/ChatSessionStore.swift` — history/delete/prune API.
- `Packages/TillerPersistence/Sources/TillerPersistence/AppDatabase.swift` — migration v15.
- `Packages/TillerPersistence/Sources/TillerPersistence/Records.swift` — two new columns.
- `Packages/TillerCore/Sources/TillerCore/WorkspaceTab.swift` — `TabContent.chat` gains `sessionId`.
- `Packages/TillerCore/Sources/TillerCore/ProjectStore.swift` — persist/load that id.
- `App/Chat/ChatController.swift` — replay split, own-session load, `.detached`.
- `App/Chat/ChatPaneView.swift`, `App/Chat/ChatComposerView.swift`, `App/Chat/ComposerControlBar.swift` — `.detached` handling.
- `App/AppModel.swift` — eager session creation, history/open/delete, title mirror, prune.
- `App/TabBarView.swift` — the history icon.
- `App/GeneralSettingsView.swift`, `Packages/TillerCore/Sources/TillerCore/AppSettings.swift` — retention setting.

---

### Task 1: Stop discarding the transcript on native resume

`ChatController` clears its persisted transcript whenever the agent reports a resume, assuming the agent will replay the conversation. Only ACP `session/load` actually does. `ClaudeStreamJSONDriver`, `PiRPCDriver`, `CodexAppServerDriver` and `OpenCodeHTTPDriver` report `didResume: true` and send nothing back, so the chat renders empty. This task fixes the visible bug on its own.

**Files:**
- Modify: `Packages/TillerACP/Sources/TillerACP/ACPSession.swift:5-13` (struct), `:95-99` (load branch)
- Modify: `App/Chat/ChatController.swift:291-294`
- Test: `AppTests/ChatControllerTests.swift`

**Interfaces:**
- Produces: `SessionHandle.didReplayHistory: Bool` — defaults to `false`, so the four native drivers keep compiling unchanged and keep their current `didResume` semantics.

- [ ] **Step 1: Add the `didReplayHistory` parameter to the test helper**

In `AppTests/ChatControllerTests.swift`, replace the existing `makeChatTestHandle` (near line 719):

```swift
private func makeChatTestHandle(sessionId: String, didResume: Bool = false,
                                didReplayHistory: Bool = false) -> SessionHandle {
    SessionHandle(sessionId: sessionId, agentCapabilities: AgentCapabilities(), modes: nil,
                  models: nil, configOptions: [], didResume: didResume,
                  didReplayHistory: didReplayHistory)
}
```

- [ ] **Step 2: Write the failing tests**

Add to `struct ChatControllerTests` in `AppTests/ChatControllerTests.swift`:

```swift
@Test func nativeResumeKeepsPersistedTranscript() async throws {
    let worktreeId = UUID()
    let (store, installStore, root) = try makeChatTestFixture(worktreeId: worktreeId)
    defer { try? FileManager.default.removeItem(at: root) }
    let record = try store.createSession(worktreeId: worktreeId.uuidString,
                                         agentId: "claude-acp")
    try store.setACPSessionId("agent-session", sessionId: record.id)
    try store.saveTranscript(sessionId: record.id, items: [
        .agentMessage(id: "old-message", text: "from last launch", isComplete: true)
    ])
    // Native driver: resumes the session but never replays it.
    let driver = ChatTestDriver(handle: makeChatTestHandle(
        sessionId: "agent-session", didResume: true, didReplayHistory: false))
    let controller = ChatController(
        tabId: UUID(), agentId: "claude-acp", worktreeId: worktreeId,
        worktreePath: root.path, store: store, installStore: installStore,
        driverFactory: { _, _, _, _, _, _, _ in driver })

    await controller.start()

    #expect(controller.items.contains { item in
        if case .agentMessage(_, let text, _) = item { return text == "from last launch" }
        return false
    })
}

@Test func acpReplayDropsPersistedTranscript() async throws {
    let worktreeId = UUID()
    let (store, installStore, root) = try makeChatTestFixture(worktreeId: worktreeId)
    defer { try? FileManager.default.removeItem(at: root) }
    let record = try store.createSession(worktreeId: worktreeId.uuidString,
                                         agentId: "claude-acp")
    try store.setACPSessionId("agent-session", sessionId: record.id)
    try store.saveTranscript(sessionId: record.id, items: [
        .agentMessage(id: "old-message", text: "from last launch", isComplete: true)
    ])
    // ACP session/load replays the conversation itself — ours would duplicate it.
    let driver = ChatTestDriver(handle: makeChatTestHandle(
        sessionId: "agent-session", didResume: true, didReplayHistory: true))
    let controller = ChatController(
        tabId: UUID(), agentId: "claude-acp", worktreeId: worktreeId,
        worktreePath: root.path, store: store, installStore: installStore,
        driverFactory: { _, _, _, _, _, _, _ in driver })

    await controller.start()

    #expect(controller.items.isEmpty)
}
```

- [ ] **Step 3: Run the tests to verify they fail**

```bash
xcodebuild test -project Tiller.xcodeproj -scheme Tiller -configuration Debug \
  -skipPackagePluginValidation \
  -only-testing:TillerTests/ChatControllerTests
```

Expected: compile error, `SessionHandle` has no `didReplayHistory`.

- [ ] **Step 4: Add the field to `SessionHandle`**

In `Packages/TillerACP/Sources/TillerACP/ACPSession.swift`, replace the struct:

```swift
public struct SessionHandle: Sendable, Equatable {
    public var sessionId: String
    public var agentCapabilities: AgentCapabilities
    public var modes: SessionModeState?
    public var models: SessionModelState?
    /// OpenCode's non-model tunables (effort, …); empty for standard agents.
    public var configOptions: [SessionConfigOption]
    /// The agent picked up an existing conversation instead of creating one.
    public var didResume: Bool
    /// The agent re-sent that conversation, so our persisted copy must be
    /// dropped to avoid rendering it twice. True only for ACP `session/load`:
    /// the native drivers restore context inside their own CLI without
    /// reprinting it, and clearing our copy there leaves an empty chat.
    public var didReplayHistory: Bool = false
}
```

- [ ] **Step 5: Set it on the ACP load branch only**

In the same file, in the `session/load` branch (around line 95), change the returned handle:

```swift
            return SessionHandle(sessionId: liveSessionId,
                                 agentCapabilities: initialize.agentCapabilities,
                                 modes: loaded.modes, models: loaded.resolvedModels,
                                 configOptions: loaded.configOptions ?? [],
                                 didResume: true, didReplayHistory: true)
```

Leave all four native drivers untouched — the default `false` is already correct for them.

- [ ] **Step 6: Gate the clear on the new flag**

In `App/Chat/ChatController.swift`, replace lines 291-294:

```swift
            if handle.didResume, let record {
                if handle.didReplayHistory { restored = [] }
                sessionRecordId = record.id
                try? store?.setACPSessionId(handle.sessionId, sessionId: record.id)
            } else if let sessionRecordId {
```

- [ ] **Step 7: Run the tests to verify they pass**

```bash
xcodebuild test -project Tiller.xcodeproj -scheme Tiller -configuration Debug \
  -skipPackagePluginValidation \
  -only-testing:TillerTests/ChatControllerTests
```

Expected: PASS. Confirm the `Test run with N tests` line shows a non-zero N.

- [ ] **Step 8: Commit**

```bash
git add Packages/TillerACP/Sources/TillerACP/ACPSession.swift \
        App/Chat/ChatController.swift AppTests/ChatControllerTests.swift
git commit -m "fix: keep persisted transcript when a native agent resumes without replay"
```

---

### Task 2: Schema v15 — session title and per-tab session id

**Files:**
- Modify: `Packages/TillerPersistence/Sources/TillerPersistence/AppDatabase.swift:171-184`
- Modify: `Packages/TillerPersistence/Sources/TillerPersistence/Records.swift:69-92` (`TerminalTabRecord`), `:130-159` (`ChatSessionRecord`)
- Test: `Packages/TillerPersistence/Tests/TillerPersistenceTests/MigrationRoundTripTests.swift`

**Interfaces:**
- Produces: `TerminalTabRecord.chatSessionId: String?` and `ChatSessionRecord.title: String?`, both appended last in their memberwise inits with `nil` defaults so existing call sites keep compiling.

- [ ] **Step 1: Write the failing migration test**

Append to `Packages/TillerPersistence/Tests/TillerPersistenceTests/MigrationRoundTripTests.swift`:

```swift
/// v15 binds each worktree's active chat tab to that worktree's most recent
/// non-empty session, so a restored tab loads its own transcript instead of
/// guessing at runtime. Sessions with no items are skipped: the agent side
/// cannot resume them either.
@Test func v15BindsActiveChatTabToLatestNonEmptySession() throws {
    let queue = try DatabaseQueue()
    try AppDatabase.migrator.migrate(queue, upTo: "v14")
    try queue.write { db in
        try db.execute(sql: """
            INSERT INTO project (id, name, rootPath, createdAt)
            VALUES ('p1', 'proj', '/tmp/p', '2026-07-01 10:00:00')
            """)
        try db.execute(sql: """
            INSERT INTO worktree (id, projectId, branch, path, createdAt)
            VALUES ('w1', 'p1', 'main', '/tmp/p', '2026-07-01 10:00:00')
            """)
        // Older session WITH items — the one a resume can actually use.
        try db.execute(sql: """
            INSERT INTO chatSession (id, worktreeId, agentId, createdAt, lastActivityAt)
            VALUES ('s-used', 'w1', 'claude-acp', '2026-07-01 10:00:00', '2026-07-01 11:00:00')
            """)
        try db.execute(sql: """
            INSERT INTO chatItem (sessionId, ordinal, kind, payload)
            VALUES ('s-used', 0, 'agentMessage', X'7B7D')
            """)
        // Newer session with NO items — must be ignored.
        try db.execute(sql: """
            INSERT INTO chatSession (id, worktreeId, agentId, createdAt, lastActivityAt)
            VALUES ('s-empty', 'w1', 'claude-acp', '2026-07-01 12:00:00', '2026-07-01 12:00:00')
            """)
        try db.execute(sql: """
            INSERT INTO terminalTab (id, worktreeId, title, orderIdx, isActive, treeJSON,
                                     updatedAt, kind, chatAgentId)
            VALUES ('t-active', 'w1', 'Chat', 0, 1, '', '2026-07-01 12:00:00', 'chat', 'claude-acp')
            """)
        try db.execute(sql: """
            INSERT INTO terminalTab (id, worktreeId, title, orderIdx, isActive, treeJSON,
                                     updatedAt, kind, chatAgentId)
            VALUES ('t-other', 'w1', 'Chat', 1, 0, '', '2026-07-01 12:00:00', 'chat', 'claude-acp')
            """)
    }

    try AppDatabase.migrator.migrate(queue)

    try queue.read { db in
        let active = try Row.fetchOne(db, sql: "SELECT * FROM terminalTab WHERE id = 't-active'")
        #expect(active?["chatSessionId"] as? String == "s-used")
        let other = try Row.fetchOne(db, sql: "SELECT * FROM terminalTab WHERE id = 't-other'")
        #expect(other?["chatSessionId"] as? String == nil)
        let session = try Row.fetchOne(db, sql: "SELECT * FROM chatSession WHERE id = 's-used'")
        #expect(session?["title"] as? String == nil)
    }
}
```

- [ ] **Step 2: Run it to verify it fails**

```bash
cd Packages/TillerPersistence && swift test --filter v15BindsActiveChatTabToLatestNonEmptySession
```

Expected: FAIL — `chatSessionId` column does not exist.

- [ ] **Step 3: Add migration v15**

In `Packages/TillerPersistence/Sources/TillerPersistence/AppDatabase.swift`, insert after the `v14` block (before `return migrator`):

```swift
        migrator.registerMigration("v15") { db in
            try db.alter(table: "chatSession") { t in
                t.add(column: "title", .text)
            }
            try db.alter(table: "terminalTab") { t in
                t.add(column: "chatSessionId", .text)
            }
            // Bind each worktree's active chat tab to its most recent non-empty
            // session. Empty sessions are skipped for the same reason
            // latestSession skipped them: the agent cannot resume one either.
            // Other legacy chat tabs stay NULL and open a fresh session — the
            // ambiguity is settled once here rather than re-guessed on every launch.
            try db.execute(sql: """
                UPDATE terminalTab SET chatSessionId = (
                    SELECT s.id FROM chatSession s
                    WHERE s.worktreeId = terminalTab.worktreeId
                      AND EXISTS (SELECT 1 FROM chatItem i WHERE i.sessionId = s.id)
                    ORDER BY s.lastActivityAt DESC
                    LIMIT 1
                )
                WHERE kind = 'chat' AND isActive = 1
                """)
        }
```

- [ ] **Step 4: Add the record fields**

In `Packages/TillerPersistence/Sources/TillerPersistence/Records.swift`, in `TerminalTabRecord` add the stored property after `chatAgentId` and the parameter last in the init:

```swift
    public var chatAgentId: String?
    /// Chat session this tab renders. NULL for non-chat tabs and for legacy
    /// chat tabs that predate v15 and had no session to adopt.
    public var chatSessionId: String?
    public var titleIsAutoNamed: Bool

    public init(id: String, worktreeId: String, title: String, orderIdx: Int,
                isActive: Bool, treeJSON: String, updatedAt: Date,
                kind: String = "terminal", filePath: String? = nil,
                chatAgentId: String? = nil, titleIsAutoNamed: Bool = true,
                chatSessionId: String? = nil) {
        self.id = id; self.worktreeId = worktreeId; self.title = title
        self.orderIdx = orderIdx; self.isActive = isActive
        self.treeJSON = treeJSON; self.updatedAt = updatedAt
        self.kind = kind; self.filePath = filePath; self.chatAgentId = chatAgentId
        self.titleIsAutoNamed = titleIsAutoNamed
        self.chatSessionId = chatSessionId
    }
```

In `ChatSessionRecord` add the property after `transportKind` and the parameter last:

```swift
    public var transportKind: String
    /// Mirror of the tab's auto-generated title, so history rows stay
    /// readable after the tab is closed. NULL until auto-naming runs.
    public var title: String?

    public init(id: String, worktreeId: String, agentId: String,
                acpSessionId: String? = nil, createdAt: Date, lastActivityAt: Date,
                contextUsageUsed: Int? = nil, contextUsageSize: Int? = nil,
                permissionMode: String? = nil, selectedModel: String? = nil,
                selectedEffort: String? = nil, transportKind: String = "acp",
                title: String? = nil) {
        self.id = id; self.worktreeId = worktreeId; self.agentId = agentId
        self.acpSessionId = acpSessionId
        self.createdAt = createdAt; self.lastActivityAt = lastActivityAt
        self.contextUsageUsed = contextUsageUsed; self.contextUsageSize = contextUsageSize
        self.permissionMode = permissionMode; self.selectedModel = selectedModel
        self.selectedEffort = selectedEffort; self.transportKind = transportKind
        self.title = title
    }
```

- [ ] **Step 5: Run the test to verify it passes**

```bash
cd Packages/TillerPersistence && swift test
```

Expected: PASS, including the pre-existing `v1DataSurvivesFullMigration` and `v14BackfillsManualOrderFromInsertionOrder`.

- [ ] **Step 6: Commit**

```bash
git add Packages/TillerPersistence
git commit -m "feat: add v15 migration for chat session title and per-tab session id"
```

---

### Task 3: History, delete and prune in `ChatSessionStore`

**Files:**
- Modify: `Packages/TillerACP/Sources/TillerACP/ChatSessionStore.swift`
- Test: `Packages/TillerACP/Tests/TillerACPTests/ChatSessionStoreTests.swift`

**Interfaces:**
- Consumes: `ChatSessionRecord.title` from Task 2.
- Produces, all on `ChatSessionStore`:
  - `func sessions(worktreeId: String) throws -> [ChatSessionRecord]` — non-empty only, `lastActivityAt` descending.
  - `func setTitle(_ title: String, sessionId: String) throws`
  - `func deleteSession(id: String) throws`
  - `func deleteIfEmpty(sessionId: String) throws`
  - `func prune(worktreeId: String, keeping: Int) throws` — `keeping <= 0` is a no-op.

- [ ] **Step 1: Write the failing tests**

Append to `Packages/TillerACP/Tests/TillerACPTests/ChatSessionStoreTests.swift`, inside `struct ChatSessionStoreTests`:

```swift
    @Test func chatHistoryExcludesEmptySessions() throws {
        let (store, worktreeId) = try makeStore()
        let used = try store.createSession(worktreeId: worktreeId, agentId: "claude",
                                           now: Date(timeIntervalSince1970: 100))
        try store.saveTranscript(sessionId: used.id, items: sampleItems,
                                 now: Date(timeIntervalSince1970: 100))
        _ = try store.createSession(worktreeId: worktreeId, agentId: "claude",
                                    now: Date(timeIntervalSince1970: 200))

        let history = try store.sessions(worktreeId: worktreeId)

        #expect(history.map(\.id) == [used.id])
    }

    @Test func chatHistoryOrdersByMostRecentActivity() throws {
        let (store, worktreeId) = try makeStore()
        let older = try store.createSession(worktreeId: worktreeId, agentId: "claude",
                                            now: Date(timeIntervalSince1970: 100))
        try store.saveTranscript(sessionId: older.id, items: sampleItems,
                                 now: Date(timeIntervalSince1970: 100))
        let newer = try store.createSession(worktreeId: worktreeId, agentId: "codex",
                                            now: Date(timeIntervalSince1970: 200))
        try store.saveTranscript(sessionId: newer.id, items: sampleItems,
                                 now: Date(timeIntervalSince1970: 200))

        let history = try store.sessions(worktreeId: worktreeId)

        #expect(history.map(\.id) == [newer.id, older.id])
    }

    @Test func setTitleIsReadBackOnTheSession() throws {
        let (store, worktreeId) = try makeStore()
        let session = try store.createSession(worktreeId: worktreeId, agentId: "claude")
        try store.saveTranscript(sessionId: session.id, items: sampleItems)

        try store.setTitle("Fix the parser", sessionId: session.id)

        #expect(try store.sessions(worktreeId: worktreeId).first?.title == "Fix the parser")
    }

    @Test func deleteSessionRemovesItsTranscript() throws {
        let (store, worktreeId) = try makeStore()
        let session = try store.createSession(worktreeId: worktreeId, agentId: "claude")
        try store.saveTranscript(sessionId: session.id, items: sampleItems)

        try store.deleteSession(id: session.id)

        #expect(try store.sessions(worktreeId: worktreeId).isEmpty)
        #expect(try store.loadTranscript(sessionId: session.id).isEmpty)
    }

    @Test func deleteIfEmptyKeepsSessionsThatHaveTurns() throws {
        let (store, worktreeId) = try makeStore()
        let empty = try store.createSession(worktreeId: worktreeId, agentId: "claude")
        let used = try store.createSession(worktreeId: worktreeId, agentId: "claude")
        try store.saveTranscript(sessionId: used.id, items: sampleItems)

        try store.deleteIfEmpty(sessionId: empty.id)
        try store.deleteIfEmpty(sessionId: used.id)

        #expect(try store.sessions(worktreeId: worktreeId).map(\.id) == [used.id])
        #expect(try store.latestSession(worktreeId: worktreeId)?.id == used.id)
    }

    @Test func pruneKeepsTheMostRecentSessions() throws {
        let (store, worktreeId) = try makeStore()
        var ids: [String] = []
        for tick in 1...4 {
            let session = try store.createSession(
                worktreeId: worktreeId, agentId: "claude",
                now: Date(timeIntervalSince1970: TimeInterval(tick * 100)))
            try store.saveTranscript(sessionId: session.id, items: sampleItems,
                                     now: Date(timeIntervalSince1970: TimeInterval(tick * 100)))
            ids.append(session.id)
        }

        try store.prune(worktreeId: worktreeId, keeping: 2)

        #expect(try store.sessions(worktreeId: worktreeId).map(\.id) == [ids[3], ids[2]])
    }

    @Test func pruneWithZeroKeepsEverything() throws {
        let (store, worktreeId) = try makeStore()
        let session = try store.createSession(worktreeId: worktreeId, agentId: "claude")
        try store.saveTranscript(sessionId: session.id, items: sampleItems)

        try store.prune(worktreeId: worktreeId, keeping: 0)

        #expect(try store.sessions(worktreeId: worktreeId).count == 1)
    }
```

- [ ] **Step 2: Run them to verify they fail**

```bash
cd Packages/TillerACP && swift test --filter ChatSessionStoreTests
```

Expected: FAIL — `sessions(worktreeId:)` does not exist.

- [ ] **Step 3: Implement the API**

In `Packages/TillerACP/Sources/TillerACP/ChatSessionStore.swift`, add after `latestSession` (which stays for now, callers are removed in Task 5):

```swift
    /// History for the worktree, newest first. Sessions with no persisted turn
    /// are excluded: they carry nothing to show and the agent side cannot
    /// resume them either.
    public func sessions(worktreeId: String) throws -> [ChatSessionRecord] {
        try database.read { db in
            try ChatSessionRecord
                .filter(Column("worktreeId") == worktreeId)
                .filter(sql: "EXISTS (SELECT 1 FROM chatItem WHERE chatItem.sessionId = chatSession.id)")
                .order(Column("lastActivityAt").desc)
                .fetchAll(db)
        }
    }

    /// Mirrors the tab's auto-generated title so the history row stays
    /// readable once the tab is gone.
    public func setTitle(_ title: String, sessionId: String) throws {
        try database.write { db in
            guard var record = try ChatSessionRecord.fetchOne(db, key: sessionId) else { return }
            record.title = title
            try record.update(db)
        }
    }

    public func deleteSession(id: String) throws {
        try database.write { db in
            try ChatItemRecord.filter(Column("sessionId") == id).deleteAll(db)
            _ = try ChatSessionRecord.deleteOne(db, key: id)
        }
    }

    /// Drops a session that never got a turn — the row a chat tab leaves
    /// behind when it is closed without being used.
    public func deleteIfEmpty(sessionId: String) throws {
        try database.write { db in
            let hasItems = try ChatItemRecord
                .filter(Column("sessionId") == sessionId).fetchCount(db) > 0
            guard !hasItems else { return }
            _ = try ChatSessionRecord.deleteOne(db, key: sessionId)
        }
    }

    /// Retention: keeps the `keeping` most recent sessions of a worktree.
    /// `keeping <= 0` means unlimited and does nothing.
    public func prune(worktreeId: String, keeping: Int) throws {
        guard keeping > 0 else { return }
        try database.write { db in
            let doomed = try ChatSessionRecord
                .filter(Column("worktreeId") == worktreeId)
                .order(Column("lastActivityAt").desc)
                .fetchAll(db)
                .dropFirst(keeping)
                .map(\.id)
            guard !doomed.isEmpty else { return }
            try ChatItemRecord.filter(doomed.contains(Column("sessionId"))).deleteAll(db)
            _ = try ChatSessionRecord.deleteAll(db, keys: doomed)
        }
    }
```

- [ ] **Step 4: Run the tests to verify they pass**

```bash
cd Packages/TillerACP && swift test --filter ChatSessionStoreTests
```

Expected: PASS, all pre-existing tests in the suite included.

- [ ] **Step 5: Commit**

```bash
git add Packages/TillerACP
git commit -m "feat: add chat history listing, delete and retention to ChatSessionStore"
```

---

### Task 4: Carry the session id on the tab

**Files:**
- Modify: `Packages/TillerCore/Sources/TillerCore/WorkspaceTab.swift:5-10` (enum), `:61-65` (accessor)
- Modify: `Packages/TillerCore/Sources/TillerCore/ProjectStore.swift:279-288` (save), `:326-331` (load)
- Modify: `App/AppModel.swift:1542-1543` (the one construction site outside TillerCore)
- Test: `Packages/TillerCore/Tests/TillerCoreTests/MarkdownTabPersistenceTests.swift`

**Interfaces:**
- Consumes: `TerminalTabRecord.chatSessionId` from Task 2.
- Produces: `TabContent.chat(agentId: String, sessionId: String?)` and `WorkspaceTab.chatSessionId: String?`.

Only three places bind the payload of `.chat` (`WorkspaceTab.chatAgentId`, `ProjectStore` save and load); every other `case .chat` in the codebase ignores its associated values and needs no edit.

- [ ] **Step 1: Write the failing test**

Append to `Packages/TillerCore/Tests/TillerCoreTests/MarkdownTabPersistenceTests.swift`:

```swift
@Test func chatTabRoundTripsItsSessionId() async throws {
    let db = try AppDatabase.inMemory()
    let store = ProjectStore(database: db)
    let project = try await store.addProject(name: "p", rootPath: "/tmp/p")
    let worktree = try await store.addWorktree(projectId: project.id, branch: "main", path: "/tmp/p")

    let bound = WorkspaceTab(id: UUID(), title: "Chat",
                             content: .chat(agentId: "claude-acp", sessionId: "s-1"))
    let legacy = WorkspaceTab(id: UUID(), title: "Chat",
                              content: .chat(agentId: "claude-acp", sessionId: nil))
    try await store.saveTabs(worktreeId: worktree.id, tabs: [bound, legacy],
                             activeTabId: bound.id)

    let loaded = try await store.loadTabs(of: worktree.id)

    #expect(loaded.tabs == [bound, legacy])
    #expect(loaded.tabs.first?.chatSessionId == "s-1")
    #expect(loaded.tabs.last?.chatSessionId == nil)
}
```

- [ ] **Step 2: Run it to verify it fails**

```bash
cd Packages/TillerCore && swift test --filter chatTabRoundTripsItsSessionId
```

Expected: FAIL — `.chat` takes one argument.

- [ ] **Step 3: Widen the enum and add the accessor**

In `Packages/TillerCore/Sources/TillerCore/WorkspaceTab.swift`:

```swift
public enum TabContent: Equatable, Sendable {
    case terminal(SplitTree)
    case markdown(fileURL: URL)
    case code(fileURL: URL)
    /// `sessionId` is the chat conversation this tab renders. Nil only for
    /// legacy tabs restored from before v15; they adopt a fresh session on
    /// first open.
    case chat(agentId: String, sessionId: String?)
}
```

and replace the accessor at line 62:

```swift
    /// Agent id when this is a chat tab.
    public var chatAgentId: String? {
        if case .chat(let agentId, _) = content { return agentId }
        return nil
    }

    /// Chat session this tab renders, when it has one.
    public var chatSessionId: String? {
        if case .chat(_, let sessionId) = content { return sessionId }
        return nil
    }
```

- [ ] **Step 4: Persist and restore the id**

In `Packages/TillerCore/Sources/TillerCore/ProjectStore.swift`, replace the `.chat` case of `saveTabs` (line 279):

```swift
                case .chat(let agentId, let sessionId):
                    record = TerminalTabRecord(
                        id: tab.id.uuidString, worktreeId: worktreeId.uuidString,
                        title: tab.title, orderIdx: idx,
                        isActive: tab.id == activeTabId,
                        treeJSON: "", updatedAt: Date(),
                        kind: "chat", chatAgentId: agentId,
                        titleIsAutoNamed: tab.titleIsAutoNamed,
                        chatSessionId: sessionId
                    )
```

and the `"chat"` case of `loadTabs` (line 326):

```swift
                case "chat":
                    guard let agentId = record.chatAgentId else { continue }
                    tabs.append(WorkspaceTab(
                        id: id, title: record.title,
                        content: .chat(agentId: agentId, sessionId: record.chatSessionId),
                        titleIsAutoNamed: record.titleIsAutoNamed))
```

- [ ] **Step 5: Fix the one App-side construction site**

In `App/AppModel.swift:1542`, inside `openChatTab`, change the tab construction so it compiles (Task 5 replaces `nil` with a real id):

```swift
        let tab = WorkspaceTab(id: UUID(), title: "Chat",
                               content: .chat(agentId: agentId, sessionId: nil))
```

- [ ] **Step 6: Run the tests to verify they pass**

```bash
cd Packages/TillerCore && swift test
```

Expected: PASS, including `saveAndLoadRoundTripsTerminalAndMarkdownTabs`.

- [ ] **Step 7: Build the app target to catch missed call sites**

```bash
xcodebuild -project Tiller.xcodeproj -scheme Tiller -configuration Debug \
  -skipPackagePluginValidation build
```

Expected: BUILD SUCCEEDED.

- [ ] **Step 8: Commit**

```bash
git add Packages/TillerCore App/AppModel.swift
git commit -m "feat: carry the chat session id on the tab"
```

---

### Task 5: Each chat tab loads its own session

**Files:**
- Modify: `App/Chat/ChatController.swift:146-183` (init), `:187-213` (start)
- Modify: `App/AppModel.swift:1539-1579` (`openChatTab`, `chatController(for:in:)`), `:1587-1592` (`teardownChatController`), `:1057-1063` (`applyAutoTitle`)
- Test: `AppTests/ChatControllerTests.swift`

**Interfaces:**
- Consumes: `WorkspaceTab.chatSessionId` (Task 4), `ChatSessionStore.deleteIfEmpty` / `setTitle` (Task 3).
- Produces: `ChatController.init(..., sessionId: String?, ...)` and `ChatController.sessionId: String?` (read-only, the record id the controller is bound to).

- [ ] **Step 1: Write the failing test**

Add to `struct ChatControllerTests` in `AppTests/ChatControllerTests.swift`:

```swift
@Test func chatTabLoadsItsOwnSessionNotTheWorktreeLatest() async throws {
    let worktreeId = UUID()
    let (store, installStore, root) = try makeChatTestFixture(worktreeId: worktreeId)
    defer { try? FileManager.default.removeItem(at: root) }
    let mine = try store.createSession(worktreeId: worktreeId.uuidString,
                                       agentId: "claude-acp",
                                       now: Date(timeIntervalSince1970: 100))
    try store.saveTranscript(sessionId: mine.id, items: [
        .agentMessage(id: "mine", text: "my conversation", isComplete: true)
    ], now: Date(timeIntervalSince1970: 100))
    // A newer session in the same worktree: the old lookup would pick this one.
    let other = try store.createSession(worktreeId: worktreeId.uuidString,
                                        agentId: "claude-acp",
                                        now: Date(timeIntervalSince1970: 900))
    try store.saveTranscript(sessionId: other.id, items: [
        .agentMessage(id: "other", text: "someone else's", isComplete: true)
    ], now: Date(timeIntervalSince1970: 900))

    let driver = ChatTestDriver(handle: makeChatTestHandle(sessionId: "live"))
    let controller = ChatController(
        tabId: UUID(), agentId: "claude-acp", worktreeId: worktreeId,
        worktreePath: root.path, store: store, installStore: installStore,
        sessionId: mine.id,
        driverFactory: { _, _, _, _, _, _, _ in driver })

    await controller.start()

    #expect(controller.sessionId == mine.id)
    #expect(controller.items.contains { item in
        if case .agentMessage(_, let text, _) = item { return text == "my conversation" }
        return false
    })
    #expect(!controller.items.contains { item in
        if case .agentMessage(_, let text, _) = item { return text == "someone else's" }
        return false
    })
}
```

- [ ] **Step 2: Run it to verify it fails**

```bash
xcodebuild test -project Tiller.xcodeproj -scheme Tiller -configuration Debug \
  -skipPackagePluginValidation \
  -only-testing:TillerTests/ChatControllerTests
```

Expected: compile error — no `sessionId:` parameter.

- [ ] **Step 3: Take the session id in the controller's init**

In `App/Chat/ChatController.swift`, expose the bound id next to the existing private field (replace line 139):

```swift
    /// The chat session record this controller renders. Assigned by the owning
    /// tab; created on first connect only for legacy tabs that arrived without one.
    private(set) var sessionRecordId: String?
    var sessionId: String? { sessionRecordId }
```

Add the parameter to `init`, after `installStore:` and before `persistenceCoordinator:`:

```swift
    init(tabId: UUID, agentId: String, worktreeId: UUID,
         worktreePath: String, store: ChatSessionStore?,
         installStore: AgentInstallStore,
         sessionId: String? = nil,
         persistenceCoordinator: PersistenceCoordinator? = nil,
```

and assign it in the body, next to `self.installStore = installStore`:

```swift
        self.sessionRecordId = sessionId
```

- [ ] **Step 4: Add the by-id lookup to the store**

In `Packages/TillerACP/Sources/TillerACP/ChatSessionStore.swift`, add next to `sessions(worktreeId:)`:

```swift
    public func session(id: String) throws -> ChatSessionRecord? {
        try database.read { db in try ChatSessionRecord.fetchOne(db, key: id) }
    }
```

- [ ] **Step 5: Load that session instead of the worktree's latest**

In `start()`, replace lines 191-207 (from `var record = …` down to and including the `restoreContextUsage` block):

```swift
        // The tab owns its session. A nil id means a legacy tab from before
        // v15: it starts empty and adopts the session created on connect.
        let stored = sessionRecordId.flatMap { id -> ChatSessionRecord? in
            guard let store else { return nil }
            return try? store.session(id: id)
        }
        if forceNewSession { sessionRecordId = nil }
        let resumable = forceNewSession ? nil : stored
        // First start of a reopened chat: adopt the session's last-used agent.
        if let resumable, restored.isEmpty, reducer.items.isEmpty {
            agentId = AgentIdMigration.canonical(resumable.agentId)
        }
        state = .connecting

        if let resumable, restored.isEmpty, reducer.items.isEmpty,
           let transcript = try? store?.loadTranscript(sessionId: resumable.id) {
            restored = transcript
            reducer = TranscriptReducer(existingIDs: Set(transcript.map(\.id)))
            rebuildPresentationSnapshot()
        }
        if let resumable, let used = resumable.contextUsageUsed,
           let size = resumable.contextUsageSize {
            reducer.restoreContextUsage(ContextUsage(used: used, size: size))
        }
```

Then replace the block that follows (currently lines 209-220), which still says `record`:

```swift
        let requestedMode = PermissionMode(rawValue: resumable?.permissionMode ?? "") ?? .ask
        permissionMode = PermissionMode.pillSelection(forAgent: agentId,
                                                      requested: requestedMode)
        selectedModel = resumable?.selectedModel
        selectedEffort = resumable?.selectedEffort
        // Resume only a session created by this same agent. A persisted
        // pi-acp record has an ACP token incompatible with Pi RPC, even
        // though its identity migrates to native Pi.
        let migratedFromPiACP = resumable?.agentId == "pi-acp"
        let resumeId = !migratedFromPiACP
            && AgentIdMigration.canonical(resumable?.agentId ?? "") == agentId
            ? resumable?.acpSessionId : nil
```

and the resume branch inside the `do` block (edited in Task 1), which becomes:

```swift
            if handle.didResume, let resumable {
                if handle.didReplayHistory { restored = [] }
                sessionRecordId = resumable.id
                try? store?.setACPSessionId(handle.sessionId, sessionId: resumable.id)
            } else if let sessionRecordId {
```

After these edits no `record` identifier remains in `start()`, and `latestSession` has no caller left in `App/`.

- [ ] **Step 6: Create the session when the tab is created**

In `App/AppModel.swift`, replace the body of `openChatTab` (lines 1540-1551):

```swift
    @discardableResult
    func openChatTab(agentId: String, in worktree: Worktree) -> WorkspaceTab? {
        rememberChatAgent(agentId)
        // The session row exists from the start so the tab knows which
        // conversation it owns even before the first turn is persisted.
        let sessionId = try? chatStore?.createSession(
            worktreeId: worktree.id.uuidString, agentId: agentId).id
        let tab = WorkspaceTab(id: UUID(), title: "Chat",
                               content: .chat(agentId: agentId, sessionId: sessionId))
        selectedWorktree = worktree
        tabs[worktree.id, default: []].append(tab)
        activeTabId[worktree.id] = tab.id
        agentActivity.agentSpawned(paneId: tab.id, agentId: agentId, now: Date())
        persistTabs(for: worktree.id)
        _ = chatController(for: tab, in: worktree, startNewConversation: true)
        return tab
    }
```

- [ ] **Step 7: Pass the id when building the controller**

In `chatController(for:in:startNewConversation:)` (line 1560), add the argument:

```swift
        let controller = ChatController(
            tabId: tab.id, agentId: agentId, worktreeId: worktree.id,
            worktreePath: worktree.path, store: chatStore,
            installStore: agentInstallStore,
            sessionId: tab.chatSessionId,
            persistenceCoordinator: persistenceCoordinator,
            startNewConversation: startNewConversation)
```

- [ ] **Step 8: Drop the empty session when its tab closes**

Replace `teardownChatController` (line 1587):

```swift
    func teardownChatController(tabId: UUID) {
        guard let controller = chatControllers[tabId] else { return }
        chatControllers[tabId] = nil
        paneClosed(paneId: tabId)
        // A chat closed before its first turn leaves a row nothing can show.
        if let sessionId = controller.sessionId {
            try? chatStore?.deleteIfEmpty(sessionId: sessionId)
        }
        Task { await controller.stop() }
    }
```

- [ ] **Step 9: Mirror the auto-generated title onto the session**

Replace `applyAutoTitle` (line 1057):

```swift
    func applyAutoTitle(_ tabId: UUID, in worktreeId: UUID, title: String) {
        guard let idx = tabs[worktreeId]?.firstIndex(where: { $0.id == tabId }) else { return }
        let trimmed = title.trimmingCharacters(in: .whitespaces)
        guard !trimmed.isEmpty, tabs[worktreeId]?[idx].titleIsAutoNamed == true else { return }
        tabs[worktreeId]?[idx].title = trimmed
        // The history row outlives the tab, so the title has to live on the
        // session too — no extra summarizer call, same string.
        if let sessionId = tabs[worktreeId]?[idx].chatSessionId {
            try? chatStore?.setTitle(trimmed, sessionId: sessionId)
        }
        persistTabs(for: worktreeId)
    }
```

- [ ] **Step 10: Run the tests to verify they pass**

```bash
xcodebuild test -project Tiller.xcodeproj -scheme Tiller -configuration Debug \
  -skipPackagePluginValidation \
  -only-testing:TillerTests/ChatControllerTests
cd Packages/TillerACP && swift test --filter ChatSessionStoreTests
```

Expected: PASS. `newChatDoesNotResumeLatestWorktreeSession` must still pass — it asserts an empty transcript for a fresh chat, which the new code satisfies by construction.

- [ ] **Step 11: Commit**

```bash
git add App Packages/TillerACP
git commit -m "feat: bind each chat tab to its own session record"
```

---

### Task 6: Detached chats

**Files:**
- Modify: `App/Chat/ChatController.swift:12-15` (state), `:187-188` (start guard), `:383-392` (send)
- Modify: `App/Chat/ChatPaneView.swift:98-103`
- Modify: `App/Chat/ChatComposerView.swift:24`
- Modify: `App/Chat/ComposerControlBar.swift:149-156`
- Modify: `App/AppModel.swift` (`restoreWorktree`, `chatController(for:in:)`)
- Test: `AppTests/ChatControllerTests.swift`

**Interfaces:**
- Consumes: `ChatController.init(sessionId:)` from Task 5.
- Produces: `ChatController.ChatState.detached`, `ChatController.activate() async`, and `ChatController.init(..., startDetached: Bool = false, ...)`.

- [ ] **Step 1: Write the failing tests**

Add to `struct ChatControllerTests`:

```swift
@Test func detachedControllerShowsTranscriptWithoutStartingAnAgent() async throws {
    let worktreeId = UUID()
    let (store, installStore, root) = try makeChatTestFixture(worktreeId: worktreeId)
    defer { try? FileManager.default.removeItem(at: root) }
    let session = try store.createSession(worktreeId: worktreeId.uuidString,
                                          agentId: "claude-acp")
    try store.saveTranscript(sessionId: session.id, items: [
        .agentMessage(id: "old", text: "yesterday's answer", isComplete: true)
    ])
    let driver = ChatTestDriver(handle: makeChatTestHandle(sessionId: "live"))
    let controller = ChatController(
        tabId: UUID(), agentId: "claude-acp", worktreeId: worktreeId,
        worktreePath: root.path, store: store, installStore: installStore,
        sessionId: session.id, startDetached: true,
        driverFactory: { _, _, _, _, _, _, _ in driver })

    await controller.activate()

    #expect(controller.state == ChatController.ChatState.detached)
    #expect(await driver.promptCount == 0)
    #expect(controller.items.contains { item in
        if case .agentMessage(_, let text, _) = item { return text == "yesterday's answer" }
        return false
    })
}

@Test func sendingFromDetachedStartsTheAgentAndDeliversThePrompt() async throws {
    let worktreeId = UUID()
    let (store, installStore, root) = try makeChatTestFixture(worktreeId: worktreeId)
    defer { try? FileManager.default.removeItem(at: root) }
    let session = try store.createSession(worktreeId: worktreeId.uuidString,
                                          agentId: "claude-acp")
    try store.saveTranscript(sessionId: session.id, items: [
        .agentMessage(id: "old", text: "yesterday's answer", isComplete: true)
    ])
    let driver = ChatTestDriver(handle: makeChatTestHandle(sessionId: "live"))
    let controller = ChatController(
        tabId: UUID(), agentId: "claude-acp", worktreeId: worktreeId,
        worktreePath: root.path, store: store, installStore: installStore,
        sessionId: session.id, startDetached: true,
        driverFactory: { _, _, _, _, _, _, _ in driver })

    controller.send(text: "carry on", mentionPaths: [], images: [])
    while await driver.promptCount == 0 { await Task.yield() }

    #expect(await driver.promptCount == 1)
    #expect(controller.items.contains { item in
        if case .userMessage(_, let blocks) = item {
            return blocks.contains { if case .text(let t) = $0 { return t == "carry on" } else { return false } }
        }
        return false
    })
}
```

- [ ] **Step 2: Run them to verify they fail**

```bash
xcodebuild test -project Tiller.xcodeproj -scheme Tiller -configuration Debug \
  -skipPackagePluginValidation \
  -only-testing:TillerTests/ChatControllerTests
```

Expected: compile error — no `.detached`, no `startDetached:`, no `activate()`.

- [ ] **Step 3: Add the state and the init flag**

In `App/Chat/ChatController.swift`, extend the state enum:

```swift
    enum ChatState: Equatable {
        case idle, connecting, ready, prompting, needsAuth
        /// Transcript on screen, no agent process. Restored and history-opened
        /// chats sit here until the user actually sends something.
        case detached
        case disconnected(message: String?)
    }
```

Add the init parameter after `sessionId:`:

```swift
         sessionId: String? = nil,
         startDetached: Bool = false,
```

and in the init body, after `self.forceNewSession = startNewConversation`:

```swift
        if startDetached {
            state = .detached
            if let sessionId, let stored = try? store?.loadTranscript(sessionId: sessionId) {
                restored = stored
                reducer = TranscriptReducer(existingIDs: Set(stored.map(\.id)))
                rebuildPresentationSnapshot()
            }
        }
```

`state` is `private(set)`, so this assignment must live inside the type — it does.

- [ ] **Step 4: Add `activate()` and let `start()` leave detached**

Add above `start()`:

```swift
    /// Called when a chat view appears. A detached chat stays asleep: its
    /// transcript is already on screen and the agent costs a subprocess.
    func activate() async {
        guard state != .detached else { return }
        await start()
    }
```

and widen the `start()` guard at line 188:

```swift
        guard state == .idle || state == .detached || isDisconnected else { return }
```

- [ ] **Step 5: Wake up on send**

Add the pending-prompt field next to the other `@ObservationIgnored` fields:

```swift
    /// Prompt typed into a detached chat, replayed once the agent is ready.
    /// Kept whole rather than pushed onto `queued`, which carries only text
    /// and would silently drop mentions and images.
    @ObservationIgnored private var pendingDetachedPrompt:
        (text: String, mentionPaths: [String], images: [ImageAttachment])?
```

and put the wake-up at the top of `send(text:mentionPaths:images:)`:

```swift
    func send(text: String, mentionPaths: [String], images: [ImageAttachment]) {
        if state == .detached {
            pendingDetachedPrompt = (text, mentionPaths, images)
            Task { [weak self] in
                await self?.start()
                self?.sendPendingDetachedPrompt()
            }
            return
        }
        if state == .prompting {
```

Add the replay helper next to `dispatchQueued()`:

```swift
    private func sendPendingDetachedPrompt() {
        guard state == .ready, let pending = pendingDetachedPrompt else { return }
        pendingDetachedPrompt = nil
        send(text: pending.text, mentionPaths: pending.mentionPaths, images: pending.images)
    }
```

- [ ] **Step 6: Stop the view from starting the agent on mount**

In `App/Chat/ChatPaneView.swift`, change the `.task` block (line 98):

```swift
        .task {
            controller.onFollowLocation = { [weak appModel] path in
                appModel?.requestChatFollow(path: path, worktreeId: worktree.id)
            }
            await controller.activate()
        }
```

- [ ] **Step 7: Keep the composer usable while detached**

In `App/Chat/ChatComposerView.swift`, line 24:

```swift
        (controller.state == .ready || isPrompting || controller.state == .detached)
            && !controller.presentationSnapshot.hasPendingPermission
```

In `App/Chat/ComposerControlBar.swift`, add a case to `stateLabel` (line 150):

```swift
    private var stateLabel: String {
        switch controller.state {
        case .ready: "ready"
        case .prompting: "working"
        case .connecting: "connecting…"
        case .detached: "idle"
        default: "offline"
        }
    }
```

`trailingControl(for:)` already falls through to `.send` and `statusDotColor` to the dimmed dot, so both are correct for `.detached` without edits.

- [ ] **Step 8: Make restored tabs detached**

In `App/AppModel.swift`, `chatController(for:in:startNewConversation:)` gains a parameter and forwards it:

```swift
    func chatController(for tab: WorkspaceTab, in worktree: Worktree,
                     startNewConversation: Bool = false,
                     startDetached: Bool = false) -> ChatController? {
        guard let agentId = tab.chatAgentId else { return nil }
        if let controller = chatControllers[tab.id] { return controller }
        let controller = ChatController(
            tabId: tab.id, agentId: agentId, worktreeId: worktree.id,
            worktreePath: worktree.path, store: chatStore,
            installStore: agentInstallStore,
            sessionId: tab.chatSessionId,
            startDetached: startDetached,
            persistenceCoordinator: persistenceCoordinator,
            startNewConversation: startNewConversation)
```

In `App/ContentView.swift:339`, the lazy build for a restored tab becomes detached:

```swift
                                if let controller = model.chatControllers[tab.id]
                                    ?? (isVisible ? model.chatController(
                                            for: tab, in: worktree,
                                            startDetached: true) : nil) {
```

`openChatTab` keeps calling `chatController(for:in:startNewConversation: true)` with `startDetached` defaulting to false, so a chat created from the `+` still starts its agent immediately while the user types the first message.

- [ ] **Step 9: Run the tests to verify they pass**

```bash
xcodebuild test -project Tiller.xcodeproj -scheme Tiller -configuration Debug \
  -skipPackagePluginValidation \
  -only-testing:TillerTests/ChatControllerTests
```

Expected: PASS.

- [ ] **Step 10: Commit**

```bash
git add App
git commit -m "feat: keep restored chats detached until the user sends a message"
```

---

### Task 7: The history menu

**Files:**
- Create: `App/Chat/ChatHistoryRows.swift`
- Create: `App/Chat/ChatHistoryMenu.swift`
- Create: `AppTests/ChatHistoryRowsTests.swift`
- Modify: `App/AppModel.swift` (history/open/delete methods)
- Modify: `App/TabBarView.swift:79-89`

This task adds new source files, so `xcodegen generate` **is** required before
they compile: `project.pbxproj` lists files explicitly, and a file that exists on
disk but is not registered produces `cannot find type … in scope` at build time —
or, worse, a test selector that exits green having run zero tests. Run it once
after creating the three files, and commit the regenerated `project.pbxproj`
along with them. Never hand-edit that file.

**Interfaces:**
- Consumes: `ChatSessionStore.sessions(worktreeId:)`, `deleteSession(id:)` (Task 3); `WorkspaceTab.chatSessionId` (Task 4); `chatController(for:in:startDetached:)` (Task 6).
- Produces:
  - `struct ChatHistoryRow: Identifiable, Equatable { let id: String; let title: String; let agentId: String; let lastActivityAt: Date }`
  - `enum ChatHistoryRows { static func make(sessions:displayName:timeFormatter:) -> [ChatHistoryRow] }`
  - `AppModel.chatHistory(for:) -> [ChatHistoryRow]`, `AppModel.openChatSession(sessionId:in:)`, `AppModel.deleteChatSession(sessionId:in:)`

- [ ] **Step 1: Write the failing test for the row builder**

Create `AppTests/ChatHistoryRowsTests.swift`:

```swift
import Foundation
import Testing
import TillerPersistence
@testable import Tiller

@Suite struct ChatHistoryRowsTests {
    private func session(id: String, title: String?, agentId: String = "claude-acp",
                         at seconds: TimeInterval) -> ChatSessionRecord {
        ChatSessionRecord(id: id, worktreeId: "w1", agentId: agentId,
                          createdAt: Date(timeIntervalSince1970: seconds),
                          lastActivityAt: Date(timeIntervalSince1970: seconds),
                          title: title)
    }

    @Test func titledSessionKeepsItsTitle() {
        let rows = ChatHistoryRows.make(
            sessions: [session(id: "s1", title: "Fix the parser", at: 100)],
            displayName: { _ in "Claude Code" },
            timeFormatter: { _ in "14:32" })

        #expect(rows.map(\.title) == ["Fix the parser"])
    }

    @Test func untitledSessionFallsBackToAgentAndTime() {
        let rows = ChatHistoryRows.make(
            sessions: [session(id: "s1", title: nil, at: 100)],
            displayName: { _ in "Claude Code" },
            timeFormatter: { _ in "14:32" })

        #expect(rows.map(\.title) == ["Claude Code · 14:32"])
    }

    @Test func blankTitleIsTreatedAsMissing() {
        let rows = ChatHistoryRows.make(
            sessions: [session(id: "s1", title: "   ", at: 100)],
            displayName: { _ in "Codex" },
            timeFormatter: { _ in "09:05" })

        #expect(rows.map(\.title) == ["Codex · 09:05"])
    }

    @Test func rowsCarrySessionIdentityAndOrder() {
        let rows = ChatHistoryRows.make(
            sessions: [session(id: "s2", title: "b", at: 200),
                       session(id: "s1", title: "a", at: 100)],
            displayName: { _ in "Claude Code" },
            timeFormatter: { _ in "" })

        #expect(rows.map(\.id) == ["s2", "s1"])
        #expect(rows.first?.agentId == "claude-acp")
    }
}
```

- [ ] **Step 2: Run it to verify it fails**

```bash
xcodebuild test -project Tiller.xcodeproj -scheme Tiller -configuration Debug \
  -skipPackagePluginValidation \
  -only-testing:TillerTests/ChatHistoryRowsTests
```

Expected: compile error — no `ChatHistoryRows`.

- [ ] **Step 3: Write the row builder**

Create `App/Chat/ChatHistoryRows.swift`:

```swift
import Foundation
import TillerPersistence

/// One entry in the chat history menu.
struct ChatHistoryRow: Identifiable, Equatable {
    let id: String
    let title: String
    let agentId: String
    let lastActivityAt: Date
}

/// Turns session records into menu rows. Kept free of SwiftUI and of any
/// locale-dependent formatting so the title fallback can be tested directly;
/// the view injects the formatter.
enum ChatHistoryRows {
    static func make(sessions: [ChatSessionRecord],
                     displayName: (String) -> String,
                     timeFormatter: (Date) -> String) -> [ChatHistoryRow] {
        sessions.map { session in
            let trimmed = session.title?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
            let title = trimmed.isEmpty
                ? "\(displayName(session.agentId)) · \(timeFormatter(session.lastActivityAt))"
                : trimmed
            return ChatHistoryRow(id: session.id, title: title,
                                  agentId: session.agentId,
                                  lastActivityAt: session.lastActivityAt)
        }
    }
}
```

- [ ] **Step 4: Run the test to verify it passes**

```bash
xcodebuild test -project Tiller.xcodeproj -scheme Tiller -configuration Debug \
  -skipPackagePluginValidation \
  -only-testing:TillerTests/ChatHistoryRowsTests
```

Expected: PASS with 4 tests.

- [ ] **Step 5: Add the AppModel entry points**

In `App/AppModel.swift`, add next to `openChatTab`:

```swift
    /// History rows for the worktree's chat menu, newest first.
    func chatHistory(for worktree: Worktree) -> [ChatHistoryRow] {
        guard let chatStore else { return [] }
        let sessions = (try? chatStore.sessions(worktreeId: worktree.id.uuidString)) ?? []
        return ChatHistoryRows.make(
            sessions: sessions,
            displayName: { [agentCenter] id in agentCenter.displayName(for: id) },
            timeFormatter: { date in
                date.formatted(date: .abbreviated, time: .shortened)
            })
    }

    /// Focus the tab already showing this conversation, or open it in a new
    /// detached tab.
    func openChatSession(sessionId: String, in worktree: Worktree) {
        if let existing = (tabs[worktree.id] ?? []).first(where: {
            $0.chatSessionId == sessionId
        }) {
            focusTab(tabId: existing.id, in: worktree)
            return
        }
        guard let chatStore, let record = try? chatStore.session(id: sessionId)
        else { return }
        let agentId = AgentIdMigration.canonical(record.agentId)
        let trimmed = record.title?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
        let tab = WorkspaceTab(
            id: UUID(),
            title: trimmed.isEmpty ? "Chat" : trimmed,
            content: .chat(agentId: agentId, sessionId: sessionId))
        selectedWorktree = worktree
        tabs[worktree.id, default: []].append(tab)
        activeTabId[worktree.id] = tab.id
        // Identity without status: a detached chat has no process to report on.
        agentActivity.registerAgentId(paneId: tab.id, agentId: agentId)
        persistTabs(for: worktree.id)
        _ = chatController(for: tab, in: worktree, startDetached: true)
    }

    /// Deletes a conversation and closes the tab rendering it, if any.
    func deleteChatSession(sessionId: String, in worktree: Worktree) {
        if let open = (tabs[worktree.id] ?? []).first(where: {
            $0.chatSessionId == sessionId
        }) {
            closeTab(open.id, in: worktree)
        }
        try? chatStore?.deleteSession(id: sessionId)
    }
```

- [ ] **Step 6: Write the menu view**

Create `App/Chat/ChatHistoryMenu.swift`:

```swift
import SwiftUI
import TillerCore

/// Chat history for the current worktree, opened from the icon beside the tab
/// bar "+". Delete lives in a submenu because NSMenu items take no context
/// menu on macOS — this keeps opening a chat at a single click.
struct ChatHistoryMenu: View {
    @Bindable var model: AppModel
    let worktree: Worktree
    @State private var pendingDeletion: ChatHistoryRow?

    var body: some View {
        let rows = model.chatHistory(for: worktree)
        Menu {
            if rows.isEmpty {
                Text("No past chats")
            } else {
                ForEach(rows) { row in
                    Button {
                        model.openChatSession(sessionId: row.id, in: worktree)
                    } label: {
                        if let icon = AgentMenuIconCache.image(for: row.agentId) {
                            Label { Text(row.title) } icon: { Image(nsImage: icon) }
                        } else {
                            Text(row.title)
                        }
                    }
                }
                Divider()
                Menu("Delete") {
                    ForEach(rows) { row in
                        Button(row.title) { pendingDeletion = row }
                    }
                }
            }
        } label: {
            Image(systemName: "clock.arrow.circlepath")
                .font(.system(size: 11))
                .foregroundStyle(AppTheme.meta)
        }
        .buttonStyle(.plain)
        .menuIndicator(.hidden)
        .help("Chat history")
        .alert("Delete this chat?",
               isPresented: Binding(get: { pendingDeletion != nil },
                                    set: { if !$0 { pendingDeletion = nil } })) {
            Button("Cancel", role: .cancel) { pendingDeletion = nil }
            Button("Delete", role: .destructive) {
                if let row = pendingDeletion {
                    model.deleteChatSession(sessionId: row.id, in: worktree)
                }
                pendingDeletion = nil
            }
        } message: {
            Text(pendingDeletion.map { "\"\($0.title)\" and its transcript will be removed." } ?? "")
        }
    }
}
```

- [ ] **Step 7: Place it beside the `+`**

In `App/TabBarView.swift`, insert the menu immediately before the existing `+` `Menu` (line 79):

```swift
            ChatHistoryMenu(model: model, worktree: worktree)

            Menu {
                NewTabMenuItems(model: model, worktree: worktree)
            } label: {
```

- [ ] **Step 8: Build and run the App tests**

```bash
xcodebuild test -project Tiller.xcodeproj -scheme Tiller -configuration Debug \
  -skipPackagePluginValidation \
  -only-testing:TillerTests/ChatHistoryRowsTests \
  -only-testing:TillerTests/ChatControllerTests
```

Expected: BUILD SUCCEEDED and both suites pass.

- [ ] **Step 9: Commit**

```bash
git add App AppTests
git commit -m "feat: add chat history menu beside the tab bar plus button"
```

---

### Task 8: Retention setting

**Files:**
- Modify: `Packages/TillerCore/Sources/TillerCore/AppSettings.swift`
- Modify: `App/GeneralSettingsView.swift:15-19`, `:57-68`
- Modify: `App/AppModel.swift` (`bootstrap`, near the existing restore code around line 396)
- Test: `Packages/TillerCore/Tests/TillerCoreTests/AppSettingsTests.swift`

**Interfaces:**
- Consumes: `ChatSessionStore.prune(worktreeId:keeping:)` (Task 3).
- Produces: `AppSettings.chatHistoryRetentionKey` and `AppSettings.defaultChatHistoryRetention`.

- [ ] **Step 1: Write the failing test**

Append to `Packages/TillerCore/Tests/TillerCoreTests/AppSettingsTests.swift`:

```swift
@Test func chatHistoryRetentionKeyAndDefault() {
    #expect(AppSettings.chatHistoryRetentionKey == "chat.history.retentionCount")
    #expect(AppSettings.defaultChatHistoryRetention == 50)
}
```

- [ ] **Step 2: Run it to verify it fails**

```bash
cd Packages/TillerCore && swift test --filter chatHistoryRetentionKeyAndDefault
```

Expected: FAIL — no such member.

- [ ] **Step 3: Add the key**

In `Packages/TillerCore/Sources/TillerCore/AppSettings.swift`, next to `maxMountedWorktreesKey`:

```swift
    /// UserDefaults key for how many chat conversations to keep per worktree.
    /// 0 means unlimited. Pruning runs once at bootstrap.
    public static let chatHistoryRetentionKey = "chat.history.retentionCount"
    public static let defaultChatHistoryRetention = 50
```

- [ ] **Step 4: Run the test to verify it passes**

```bash
cd Packages/TillerCore && swift test --filter chatHistoryRetentionKeyAndDefault
```

Expected: PASS.

- [ ] **Step 5: Prune at bootstrap**

In `App/AppModel.swift`, inside `bootstrap()` after the worktrees are loaded and before `usage.updatePolling()` (around line 396), add:

```swift
            pruneChatHistory()
```

and add the method next to `chatHistory(for:)`:

```swift
    /// Applies the retention limit once per launch. Missing preference means
    /// the default; 0 means unlimited and prune() returns immediately.
    private func pruneChatHistory() {
        guard let chatStore else { return }
        let stored = defaults.object(forKey: AppSettings.chatHistoryRetentionKey) as? Int
        let keeping = stored ?? AppSettings.defaultChatHistoryRetention
        for worktree in worktrees.values.flatMap({ $0 }) {
            try? chatStore.prune(worktreeId: worktree.id.uuidString, keeping: keeping)
        }
    }
```

- [ ] **Step 6: Add the setting to General**

In `App/GeneralSettingsView.swift`, add the storage next to the others (line 16):

```swift
    @AppStorage(AppSettings.chatHistoryRetentionKey)
    private var chatHistoryRetention = AppSettings.defaultChatHistoryRetention
```

and a section after `Section("Automation")`:

```swift
            Section("Chat history") {
                Toggle(isOn: Binding(
                    get: { chatHistoryRetention > 0 },
                    set: { chatHistoryRetention = $0 ? AppSettings.defaultChatHistoryRetention : 0 }
                )) {
                    Text("Limit stored chats")
                    Text("Keeps only the most recent conversations per worktree. Older ones are deleted at launch.")
                }
                if chatHistoryRetention > 0 {
                    Stepper("Keep \(chatHistoryRetention) chats per worktree",
                            value: $chatHistoryRetention, in: 5...500, step: 5)
                }
            }
```

- [ ] **Step 7: Run the full gate**

```bash
Scripts/ci.sh
xcodebuild test -project Tiller.xcodeproj -scheme Tiller -configuration Debug \
  -skipPackagePluginValidation \
  -only-testing:TillerTests/ChatControllerTests \
  -only-testing:TillerTests/ChatHistoryRowsTests
```

Expected: `CI OK`, and both App suites report a non-zero `Test run with N tests`. If `ci.sh` fails only in `TillerTerminal` PTY timing tests, rerun — that flakiness predates this work.

- [ ] **Step 8: Commit**

```bash
git add App Packages/TillerCore
git commit -m "feat: add chat history retention setting"
```

---

## Manual verification

None of these are covered by automated tests. Run them against a Debug build before merging.

1. Chat with 2-3 turns → ⌘Q → reopen: the transcript is still there, and Activity Monitor shows no agent process for it.
2. Type into that restored chat: the agent starts and answers in the same thread; the earlier turns stay above the new one.
3. History menu → open an old chat: it opens detached. Open the same entry again: it focuses the existing tab rather than duplicating it.
4. Delete a chat that is currently open: its tab closes and the entry disappears from the menu.
5. Set retention to its minimum, restart twice, confirm older conversations are gone and the newest survive.
6. Create a chat from `+`, close it without sending anything, reopen the history menu: it is not listed.
