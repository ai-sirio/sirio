# Claude/Codex Agent Account Management Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let users add, switch between, re-authenticate, and remove
isolated Claude Code / Codex CLI accounts from Settings, with the globally
active account's credentials injected into every newly-spawned agent
terminal.

**Architecture:** A new GRDB table (`agentAccount`) persists one row per
added account (provider, isolated config-dir path, captured
email/org). A new `AgentAccountStore` (App target, `@Observable`) owns
CRUD against that table plus the two auth-process flows (spawn `claude
auth login` / `codex login` hidden, wait for exit, then run `claude auth
status` / `codex login status` to capture identity). `AppModel.spawnAgent`
consults the store's active-account pointer (two `UserDefaults` strings)
and prefixes the adapter's command string with `CLAUDE_CONFIG_DIR=...`/
`CODEX_HOME=...` when set — no changes to the `AgentAdapter` protocol or
`PtyTerminalPane`/`PtyRuntime` at all, since the override is just a shell
env-var prefix on the existing command string.

**Tech Stack:** Swift 6, SwiftUI, GRDB (HelmPersistence package),
`Foundation.Process` (no PTY needed — `claude auth login`/`status` and
`codex login`/`status` are plain foreground processes), Swift Testing
(`@Test`/`#expect`).

## Global Constraints

- Real, functionally isolated switching only — no cosmetic/tracking-only
  mode (confirmed).
- Active account is global to the whole app, never per-worktree/per-tab
  (confirmed).
- "Add Account" spawns the auth command hidden (no visible terminal); the
  command opens the system browser itself; Settings shows only a
  "Waiting for browser login…" state + Cancel (confirmed).
- Isolation is config-dir-only (`CLAUDE_CONFIG_DIR` / `CODEX_HOME`), never
  a full synthetic `$HOME` (confirmed, approach A).
- OpenCode Go / Ollama Cloud are out of scope — no account list for them.
- Verified CLI behavior (do not re-verify, just rely on it):
  - `claude auth login` — opens browser OAuth, exits 0 on success.
  - `claude auth status` — prints JSON: `{"loggedIn":true,"email":"...",
    "orgName":"...", ...}` (exact real output, captured this session).
  - `codex login` — bare command opens browser OAuth, exits 0 on success.
  - `codex login status` — prints a plain text line, e.g. `Logged in
    using an API key - sk-proj-***Y61sA` (real output, captured this
    session; OAuth logins may print a different but similarly-shaped
    single line — treat the whole trimmed line as the label, do not
    assume a fixed schema).
- `CLAUDE_CONFIG_DIR` / `CODEX_HOME` relocate `~/.claude` / `~/.codex`
  only — every other env var (`$HOME`, `$PATH`, shell rc) stays whatever
  the pane would normally get.

---

### Task 1: Persistence — `AgentAccountRecord` + migration v5

**Files:**
- Modify: `Helm/Packages/HelmPersistence/Sources/HelmPersistence/Records.swift`
- Modify: `Helm/Packages/HelmPersistence/Sources/HelmPersistence/AppDatabase.swift:79-90` (add migration after v4, before `return migrator`)
- Modify: `Helm/Packages/HelmPersistence/Tests/HelmPersistenceTests/AppDatabaseTests.swift`

**Interfaces:**
- Produces: `AgentAccountRecord` (Codable, FetchableRecord, PersistableRecord, Sendable, Equatable) with fields `id: String`, `provider: String`, `configDirPath: String`, `label: String`, `orgName: String?`, `createdAt: Date`, `lastAuthenticatedAt: Date`. Table name `"agentAccount"`.

- [ ] **Step 1: Write the failing test**

Add to `Helm/Packages/HelmPersistence/Tests/HelmPersistenceTests/AppDatabaseTests.swift` (append at end of file, after the last `@Test func`):

```swift
// MARK: - Agent accounts (v5)

@Test func v5CreatesAgentAccountTableAndRoundTripsRecord() throws {
    let db = try AppDatabase.inMemory()
    let account = AgentAccountRecord(
        id: "acct1", provider: "claude", configDirPath: "/tmp/acct1",
        label: "e.palmisano@reply.it", orgName: "e.palmisano@reply.it's Organization",
        createdAt: Date(), lastAuthenticatedAt: Date()
    )
    try db.write { try account.insert($0) }
    let fetched = try db.read { try AgentAccountRecord.fetchOne($0, key: "acct1") }
    #expect(fetched?.provider == "claude")
    #expect(fetched?.label == "e.palmisano@reply.it")
    #expect(fetched?.orgName == "e.palmisano@reply.it's Organization")
}

@Test func agentAccountsFilterByProvider() throws {
    let db = try AppDatabase.inMemory()
    try db.write { database in
        try AgentAccountRecord(
            id: "c1", provider: "claude", configDirPath: "/tmp/c1", label: "a@x.com",
            createdAt: Date(), lastAuthenticatedAt: Date()
        ).insert(database)
        try AgentAccountRecord(
            id: "x1", provider: "codex", configDirPath: "/tmp/x1", label: "b@x.com",
            createdAt: Date(), lastAuthenticatedAt: Date()
        ).insert(database)
    }
    let claudeOnly = try db.read {
        try AgentAccountRecord.filter(Column("provider") == "claude").fetchAll($0)
    }
    #expect(claudeOnly.count == 1)
    #expect(claudeOnly[0].id == "c1")
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd Helm/Packages/HelmPersistence && swift test --filter v5CreatesAgentAccountTableAndRoundTripsRecord`
Expected: FAIL — `AgentAccountRecord` not found in scope (doesn't exist yet).

- [ ] **Step 3: Add the record type**

Append to `Helm/Packages/HelmPersistence/Sources/HelmPersistence/Records.swift` (after `TerminalTabRecord`):

```swift

public struct AgentAccountRecord: Codable, FetchableRecord, PersistableRecord, Sendable, Equatable {
    public static let databaseTableName = "agentAccount"
    public var id: String
    public var provider: String
    public var configDirPath: String
    public var label: String
    public var orgName: String?
    public var createdAt: Date
    public var lastAuthenticatedAt: Date

    public init(id: String, provider: String, configDirPath: String, label: String,
                orgName: String? = nil, createdAt: Date, lastAuthenticatedAt: Date) {
        self.id = id; self.provider = provider; self.configDirPath = configDirPath
        self.label = label; self.orgName = orgName
        self.createdAt = createdAt; self.lastAuthenticatedAt = lastAuthenticatedAt
    }
}
```

- [ ] **Step 4: Add migration v5**

In `Helm/Packages/HelmPersistence/Sources/HelmPersistence/AppDatabase.swift`, insert a new migration between the `migrator.registerMigration("v4")` block and `return migrator`:

```swift
        migrator.registerMigration("v5") { db in
            try db.create(table: "agentAccount") { t in
                t.primaryKey("id", .text)
                t.column("provider", .text).notNull()
                t.column("configDirPath", .text).notNull()
                t.column("label", .text).notNull()
                t.column("orgName", .text)
                t.column("createdAt", .datetime).notNull()
                t.column("lastAuthenticatedAt", .datetime).notNull()
            }
        }
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cd Helm/Packages/HelmPersistence && swift test`
Expected: all tests pass, including the two new ones.

- [ ] **Step 6: Commit**

```bash
git add Helm/Packages/HelmPersistence/Sources/HelmPersistence/Records.swift \
        Helm/Packages/HelmPersistence/Sources/HelmPersistence/AppDatabase.swift \
        Helm/Packages/HelmPersistence/Tests/HelmPersistenceTests/AppDatabaseTests.swift
git commit -m "feat(persistence): add AgentAccountRecord + migration v5"
```

---

### Task 2: Identity-output parsers

**Files:**
- Create: `Helm/Packages/HelmCore/Sources/HelmCore/AgentAccountIdentity.swift`
- Create: `Helm/Packages/HelmCore/Tests/HelmCoreTests/AgentAccountIdentityTests.swift`

**Interfaces:**
- Produces:
  - `ClaudeAuthStatus: Decodable` with `loggedIn: Bool`, `email: String?`, `orgName: String?`.
  - `AgentAccountIdentity.parseClaudeAuthStatus(_ output: String) -> (label: String, orgName: String?)?` — returns `nil` if not logged in or unparseable; `label` is the email.
  - `AgentAccountIdentity.parseCodexLoginStatus(_ output: String) -> String?` — returns the trimmed first non-empty line of output, or `nil` for empty/whitespace-only input.

- [ ] **Step 1: Write the failing tests**

Create `Helm/Packages/HelmCore/Tests/HelmCoreTests/AgentAccountIdentityTests.swift`:

```swift
import Testing
@testable import HelmCore

@Test func parsesClaudeAuthStatusJSON() {
    let output = """
    {
      "loggedIn": true,
      "authMethod": "claude.ai",
      "apiProvider": "firstParty",
      "email": "e.palmisano@reply.it",
      "orgId": "3f3bf2c0-7fc0-4cc7-91ad-b729d8902db2",
      "orgName": "e.palmisano@reply.it's Organization",
      "subscriptionType": "max"
    }
    """
    let identity = AgentAccountIdentity.parseClaudeAuthStatus(output)
    #expect(identity?.label == "e.palmisano@reply.it")
    #expect(identity?.orgName == "e.palmisano@reply.it's Organization")
}

@Test func claudeAuthStatusLoggedOutReturnsNil() {
    let output = #"{"loggedIn": false}"#
    #expect(AgentAccountIdentity.parseClaudeAuthStatus(output) == nil)
}

@Test func claudeAuthStatusMalformedReturnsNil() {
    #expect(AgentAccountIdentity.parseClaudeAuthStatus("not json at all") == nil)
}

@Test func parsesCodexLoginStatusPlainLine() {
    let output = "Logged in using an API key - sk-proj-***Y61sA\n"
    #expect(AgentAccountIdentity.parseCodexLoginStatus(output) == "Logged in using an API key - sk-proj-***Y61sA")
}

@Test func codexLoginStatusEmptyReturnsNil() {
    #expect(AgentAccountIdentity.parseCodexLoginStatus("   \n  ") == nil)
}

@Test func codexLoginStatusTakesFirstNonEmptyLine() {
    let output = "\n\nLogged in using ChatGPT - user@example.com\nextra trailing line\n"
    #expect(AgentAccountIdentity.parseCodexLoginStatus(output) == "Logged in using ChatGPT - user@example.com")
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd Helm/Packages/HelmCore && swift test --filter AgentAccountIdentityTests`
Expected: FAIL — `AgentAccountIdentity` not found in scope.

- [ ] **Step 3: Write the implementation**

Create `Helm/Packages/HelmCore/Sources/HelmCore/AgentAccountIdentity.swift`:

```swift
import Foundation

/// `claude auth status`'s JSON shape. Only the fields this app reads are
/// modeled; unknown keys are ignored by `Decodable` automatically.
struct ClaudeAuthStatus: Decodable {
    let loggedIn: Bool
    let email: String?
    let orgName: String?
}

/// Parses the non-interactive identity-capture commands run after a
/// successful `claude auth login` / `codex login`, so `AgentAccountStore`
/// can label a newly-added account without scraping a TUI.
public enum AgentAccountIdentity {
    /// `claude auth status` prints clean JSON. Returns `nil` when logged
    /// out or when the output can't be decoded (CLI version drift, etc.).
    public static func parseClaudeAuthStatus(_ output: String) -> (label: String, orgName: String?)? {
        guard let data = output.data(using: .utf8),
              let status = try? JSONDecoder().decode(ClaudeAuthStatus.self, from: data),
              status.loggedIn, let email = status.email
        else { return nil }
        return (label: email, orgName: status.orgName)
    }

    /// `codex login status` has no fixed schema (varies by auth method —
    /// API key vs. ChatGPT OAuth). Best-effort: the first non-empty line,
    /// trimmed, stands in as the display label. Returns `nil` for
    /// empty/whitespace-only output.
    public static func parseCodexLoginStatus(_ output: String) -> String? {
        for line in output.split(separator: "\n", omittingEmptySubsequences: false) {
            let trimmed = line.trimmingCharacters(in: .whitespaces)
            if !trimmed.isEmpty { return trimmed }
        }
        return nil
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd Helm/Packages/HelmCore && swift test --filter AgentAccountIdentityTests`
Expected: all 6 tests pass.

- [ ] **Step 5: Commit**

```bash
git add Helm/Packages/HelmCore/Sources/HelmCore/AgentAccountIdentity.swift \
        Helm/Packages/HelmCore/Tests/HelmCoreTests/AgentAccountIdentityTests.swift
git commit -m "feat(core): parse claude auth status / codex login status output"
```

---

### Task 3: `AgentAccountStore` — persistence + active-account pointer

**Files:**
- Create: `Helm/App/AgentAccountStore.swift`

**Interfaces:**
- Consumes: `AgentAccountRecord`, `AppDatabase` (HelmPersistence, Task 1).
- Produces (used by Task 4 and Task 5):
  - `@MainActor @Observable final class AgentAccountStore`
  - `init(database: AppDatabase)`
  - `var claudeAccounts: [AgentAccountRecord]`, `var codexAccounts: [AgentAccountRecord]` (sorted by `createdAt` ascending)
  - `func reload()` — repopulates both arrays from the database
  - `var activeClaudeAccountId: String?`, `var activeCodexAccountId: String?` (get/set, backed by `UserDefaults` keys `"agentAccounts.claude.activeId"` / `"agentAccounts.codex.activeId"`)
  - `func activeClaudeConfigDirPath() -> String?`, `func activeCodexConfigDirPath() -> String?` — `nil` when "System default" or when the pointed-to id no longer exists (self-healing: clears the stale pointer)
  - `func removeClaudeAccount(_ account: AgentAccountRecord)`, `func removeCodexAccount(_ account: AgentAccountRecord)` — delete row + config dir; reset active pointer to nil if it pointed at the removed id

This task does NOT include the auth-process spawning (Add Account /
Re-authenticate) — that's Task 4, layered on top of this file.

- [ ] **Step 1: Write the failing test**

There's no dedicated test target for the `Helm/App` Xcode target (only the
SPM packages have one — see Task 1's pattern). Verification for this task
is a small `#if DEBUG`-free manual smoke check plus the app build, matching
how `UsageStore`/`AIProvidersSettingsView` are verified elsewhere in this
codebase (no unit test target exists for App-target files; `HelmPersistence`
already covers the record/migration logic in Task 1).

Instead of a unit test, write the file directly (Step 2) and verify via
build + a short interactive check in Step 3.

- [ ] **Step 2: Write `AgentAccountStore.swift`**

Create `Helm/App/AgentAccountStore.swift`:

```swift
import Foundation
import HelmPersistence

/// Owns the persisted list of isolated Claude/Codex CLI accounts and which
/// one (if any) is currently active. "Active" is global to the whole app —
/// switching it affects the next agent terminal opened anywhere, not
/// already-running panes (env is read once at spawn time).
///
/// Auth-flow orchestration (Add Account / Re-authenticate) lives in this
/// same type but is added by a later change on top of this file.
@MainActor
@Observable
final class AgentAccountStore {
    private let database: AppDatabase

    var claudeAccounts: [AgentAccountRecord] = []
    var codexAccounts: [AgentAccountRecord] = []

    init(database: AppDatabase) {
        self.database = database
        reload()
    }

    func reload() {
        let all = (try? database.read { try AgentAccountRecord.fetchAll($0) }) ?? []
        claudeAccounts = all.filter { $0.provider == "claude" }.sorted { $0.createdAt < $1.createdAt }
        codexAccounts = all.filter { $0.provider == "codex" }.sorted { $0.createdAt < $1.createdAt }
    }

    var activeClaudeAccountId: String? {
        get { UserDefaults.standard.string(forKey: "agentAccounts.claude.activeId") }
        set { UserDefaults.standard.set(newValue, forKey: "agentAccounts.claude.activeId") }
    }

    var activeCodexAccountId: String? {
        get { UserDefaults.standard.string(forKey: "agentAccounts.codex.activeId") }
        set { UserDefaults.standard.set(newValue, forKey: "agentAccounts.codex.activeId") }
    }

    /// `nil` means "System default" (no env override) — either nothing is
    /// selected, or the selected id no longer exists (self-healing: clears
    /// the stale pointer so future lookups short-circuit immediately).
    func activeClaudeConfigDirPath() -> String? {
        guard let id = activeClaudeAccountId, !id.isEmpty else { return nil }
        guard let record = claudeAccounts.first(where: { $0.id == id }) else {
            activeClaudeAccountId = nil
            return nil
        }
        return record.configDirPath
    }

    func activeCodexConfigDirPath() -> String? {
        guard let id = activeCodexAccountId, !id.isEmpty else { return nil }
        guard let record = codexAccounts.first(where: { $0.id == id }) else {
            activeCodexAccountId = nil
            return nil
        }
        return record.configDirPath
    }

    func removeClaudeAccount(_ account: AgentAccountRecord) {
        _ = try? database.write { try account.delete($0) }
        try? FileManager.default.removeItem(atPath: account.configDirPath)
        if activeClaudeAccountId == account.id { activeClaudeAccountId = nil }
        reload()
    }

    func removeCodexAccount(_ account: AgentAccountRecord) {
        _ = try? database.write { try account.delete($0) }
        try? FileManager.default.removeItem(atPath: account.configDirPath)
        if activeCodexAccountId == account.id { activeCodexAccountId = nil }
        reload()
    }

    /// `~/Library/Application Support/Helm/agent-accounts/<provider>/<id>/`
    /// — created lazily by the add-account flow (Task 4), never referenced
    /// before that directory exists.
    static func newAccountConfigDir(provider: String, id: String) throws -> URL {
        let base = try FileManager.default.url(
            for: .applicationSupportDirectory, in: .userDomainMask, appropriateFor: nil, create: true)
        let dir = base.appendingPathComponent("Helm/agent-accounts/\(provider)/\(id)", isDirectory: true)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        return dir
    }
}
```

- [ ] **Step 3: Build to verify it compiles**

Run: `cd Helm && xcodegen generate && xcodebuild -scheme Helm -configuration Debug -destination 'platform=macOS' build`
Expected: `** BUILD SUCCEEDED **` (this file isn't wired into `AppModel` yet, so nothing calls it — a successful build only confirms it type-checks in isolation).

- [ ] **Step 4: Commit**

```bash
git add Helm/App/AgentAccountStore.swift
git commit -m "feat: add AgentAccountStore (persistence + active-account pointer)"
```

---

### Task 4: Auth-flow orchestration (Add Account / Re-authenticate)

**Files:**
- Modify: `Helm/App/AgentAccountStore.swift` (append to the type from Task 3)

**Interfaces:**
- Consumes: `AgentAccountIdentity.parseClaudeAuthStatus`/`parseCodexLoginStatus` (HelmCore, Task 2), `AgentAccountStore.newAccountConfigDir` (Task 3).
- Produces:
  - `enum AccountAuthState: Equatable { case idle, waitingForBrowser, failed(String) }`
  - `var claudeAuthState: AccountAuthState`, `var codexAuthState: AccountAuthState`
  - `func addClaudeAccount() async`, `func addCodexAccount() async`
  - `func reAuthenticateClaudeAccount(_ account: AgentAccountRecord) async`, `func reAuthenticateCodexAccount(_ account: AgentAccountRecord) async`
  - `func cancelClaudeAuth()`, `func cancelCodexAuth()`

- [ ] **Step 1: Append auth-flow state and process helper to `AgentAccountStore`**

Add to `Helm/App/AgentAccountStore.swift`, inside the `AgentAccountStore` class body (after the existing methods, before the closing `}`):

```swift

    enum AccountAuthState: Equatable {
        case idle
        case waitingForBrowser
        case failed(String)
    }

    var claudeAuthState: AccountAuthState = .idle
    var codexAuthState: AccountAuthState = .idle

    private var claudeAuthProcess: Process?
    private var codexAuthProcess: Process?

    /// Safety-net ceiling on how long "Waiting for browser login…" waits
    /// before giving up on its own — Cancel is the primary way out, this
    /// just guarantees the state never gets stuck forever if the user
    /// walks away mid-flow.
    private static let authTimeout: Duration = .seconds(300)

    /// Runs one CLI command to completion with the given env override,
    /// returning its exit code and captured stdout. `nil` process means the
    /// caller already handled spawn failure (executable not found, etc.).
    private func runProcess(
        executable: String, arguments: [String], envKey: String, envValue: String
    ) async -> (exitCode: Int32, stdout: String)? {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/env")
        process.arguments = [executable] + arguments
        process.environment = ProcessInfo.processInfo.environment.merging(
            [envKey: envValue]) { _, new in new }
        let stdoutPipe = Pipe()
        process.standardOutput = stdoutPipe
        process.standardError = FileHandle.nullDevice

        do {
            try process.run()
        } catch {
            return nil
        }

        let exitCode: Int32 = await withCheckedContinuation { continuation in
            process.terminationHandler = { p in continuation.resume(returning: p.terminationStatus) }
        }
        let data = stdoutPipe.fileHandleForReading.readDataToEndOfFile()
        return (exitCode, String(decoding: data, as: UTF8.self))
    }

    /// Races `runProcess` against the safety-net timeout, terminating the
    /// process if the timeout wins. `processSlot` receives the live
    /// process so Cancel can reach it while waiting.
    private func runAuthCommand(
        executable: String, arguments: [String], envKey: String, envValue: String,
        processSlot: (Process?) -> Void
    ) async -> Bool {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/env")
        process.arguments = [executable] + arguments
        process.environment = ProcessInfo.processInfo.environment.merging(
            [envKey: envValue]) { _, new in new }
        process.standardOutput = FileHandle.nullDevice
        process.standardError = FileHandle.nullDevice

        do {
            try process.run()
        } catch {
            return false
        }
        processSlot(process)

        return await withTaskGroup(of: Bool.self) { group in
            group.addTask {
                await withCheckedContinuation { continuation in
                    process.terminationHandler = { p in
                        continuation.resume(returning: p.terminationStatus == 0)
                    }
                }
            }
            group.addTask {
                try? await Task.sleep(for: Self.authTimeout)
                process.terminate()
                return false
            }
            let result = await group.next() ?? false
            group.cancelAll()
            return result
        }
    }

    func addClaudeAccount() async {
        claudeAuthState = .waitingForBrowser
        let id = UUID().uuidString
        guard let dir = try? Self.newAccountConfigDir(provider: "claude", id: id) else {
            claudeAuthState = .failed("Couldn't create account directory")
            return
        }
        let success = await runAuthCommand(
            executable: "claude", arguments: ["auth", "login"],
            envKey: "CLAUDE_CONFIG_DIR", envValue: dir.path,
            processSlot: { self.claudeAuthProcess = $0 }
        )
        claudeAuthProcess = nil
        guard success else {
            try? FileManager.default.removeItem(at: dir)
            claudeAuthState = .idle
            return
        }
        guard
            let statusResult = await runProcess(
                executable: "claude", arguments: ["auth", "status"],
                envKey: "CLAUDE_CONFIG_DIR", envValue: dir.path),
            let identity = AgentAccountIdentity.parseClaudeAuthStatus(statusResult.stdout)
        else {
            try? FileManager.default.removeItem(at: dir)
            claudeAuthState = .failed("Login succeeded but status could not be read")
            return
        }
        let now = Date()
        let record = AgentAccountRecord(
            id: id, provider: "claude", configDirPath: dir.path,
            label: identity.label, orgName: identity.orgName,
            createdAt: now, lastAuthenticatedAt: now)
        _ = try? database.write { try record.insert($0) }
        reload()
        claudeAuthState = .idle
    }

    func addCodexAccount() async {
        codexAuthState = .waitingForBrowser
        let id = UUID().uuidString
        guard let dir = try? Self.newAccountConfigDir(provider: "codex", id: id) else {
            codexAuthState = .failed("Couldn't create account directory")
            return
        }
        let success = await runAuthCommand(
            executable: "codex", arguments: ["login"],
            envKey: "CODEX_HOME", envValue: dir.path,
            processSlot: { self.codexAuthProcess = $0 }
        )
        codexAuthProcess = nil
        guard success else {
            try? FileManager.default.removeItem(at: dir)
            codexAuthState = .idle
            return
        }
        guard
            let statusResult = await runProcess(
                executable: "codex", arguments: ["login", "status"],
                envKey: "CODEX_HOME", envValue: dir.path),
            let label = AgentAccountIdentity.parseCodexLoginStatus(statusResult.stdout)
        else {
            try? FileManager.default.removeItem(at: dir)
            codexAuthState = .failed("Login succeeded but status could not be read")
            return
        }
        let now = Date()
        let record = AgentAccountRecord(
            id: id, provider: "codex", configDirPath: dir.path,
            label: label, orgName: nil,
            createdAt: now, lastAuthenticatedAt: now)
        _ = try? database.write { try record.insert($0) }
        reload()
        codexAuthState = .idle
    }

    func reAuthenticateClaudeAccount(_ account: AgentAccountRecord) async {
        claudeAuthState = .waitingForBrowser
        let success = await runAuthCommand(
            executable: "claude", arguments: ["auth", "login"],
            envKey: "CLAUDE_CONFIG_DIR", envValue: account.configDirPath,
            processSlot: { self.claudeAuthProcess = $0 }
        )
        claudeAuthProcess = nil
        guard success else {
            claudeAuthState = .idle
            return
        }
        if
            let statusResult = await runProcess(
                executable: "claude", arguments: ["auth", "status"],
                envKey: "CLAUDE_CONFIG_DIR", envValue: account.configDirPath),
            let identity = AgentAccountIdentity.parseClaudeAuthStatus(statusResult.stdout)
        {
            var updated = account
            updated.label = identity.label
            updated.orgName = identity.orgName
            updated.lastAuthenticatedAt = Date()
            _ = try? database.write { try updated.update($0) }
            reload()
        }
        claudeAuthState = .idle
    }

    func reAuthenticateCodexAccount(_ account: AgentAccountRecord) async {
        codexAuthState = .waitingForBrowser
        let success = await runAuthCommand(
            executable: "codex", arguments: ["login"],
            envKey: "CODEX_HOME", envValue: account.configDirPath,
            processSlot: { self.codexAuthProcess = $0 }
        )
        codexAuthProcess = nil
        guard success else {
            codexAuthState = .idle
            return
        }
        if
            let statusResult = await runProcess(
                executable: "codex", arguments: ["login", "status"],
                envKey: "CODEX_HOME", envValue: account.configDirPath),
            let label = AgentAccountIdentity.parseCodexLoginStatus(statusResult.stdout)
        {
            var updated = account
            updated.label = label
            updated.lastAuthenticatedAt = Date()
            _ = try? database.write { try updated.update($0) }
            reload()
        }
        codexAuthState = .idle
    }

    func cancelClaudeAuth() {
        claudeAuthProcess?.terminate()
        claudeAuthProcess = nil
        claudeAuthState = .idle
    }

    func cancelCodexAuth() {
        codexAuthProcess?.terminate()
        codexAuthProcess = nil
        codexAuthState = .idle
    }
```

Also add `import HelmCore` to the top of `Helm/App/AgentAccountStore.swift` (needed for `AgentAccountIdentity`):

```swift
import Foundation
import HelmCore
import HelmPersistence
```

- [ ] **Step 2: Build to verify it compiles**

Run: `cd Helm && xcodebuild -scheme Helm -configuration Debug -destination 'platform=macOS' build`
Expected: `** BUILD SUCCEEDED **`

- [ ] **Step 3: Manual smoke test**

This step needs a real Anthropic/OpenAI login, so it's a manual check, not
an automated one:

1. In a scratch Swift REPL or a temporary debug button, call
   `await store.addClaudeAccount()` (or wire it via Task 6's UI once that
   task lands, and come back to re-run this check then).
2. Confirm: `claudeAuthState` becomes `.waitingForBrowser`, a browser tab
   opens for Anthropic OAuth, completing it flips `claudeAuthState` back
   to `.idle` and `claudeAccounts` gains one entry with the real logged-in
   email as `label`.
3. Confirm the created directory exists: `ls ~/Library/Application\ Support/Helm/agent-accounts/claude/`
   should show one subdirectory containing `.credentials.json` or
   equivalent Claude CLI state, isolated from `~/.claude`.
4. Repeat for `addCodexAccount()`.

(This manual check can be deferred to right after Task 6 ships the UI,
where it's much easier to trigger — noting it here so it isn't forgotten.)

- [ ] **Step 4: Commit**

```bash
git add Helm/App/AgentAccountStore.swift
git commit -m "feat: add auth-flow orchestration to AgentAccountStore"
```

---

### Task 5: Wire active account into agent spawning

**Files:**
- Modify: `Helm/App/AppModel.swift:106` (add property near `database`)
- Modify: `Helm/App/AppModel.swift:108-117` (bootstrap — construct the store)
- Modify: `Helm/App/AppModel.swift:551-565` (`spawnAgent` — inject env override)

**Interfaces:**
- Consumes: `AgentAccountStore` (Task 3/4), `AppDatabase` (already in scope in `AppModel`).
- Produces: `AppModel.agentAccounts: AgentAccountStore?` — consumed by Task 6's UI.

- [ ] **Step 1: Declare the property**

In `Helm/App/AppModel.swift`, change:

```swift
    private var store: ProjectStore?
    private var database: AppDatabase?
```

to:

```swift
    private var store: ProjectStore?
    private var database: AppDatabase?
    var agentAccounts: AgentAccountStore?
```

- [ ] **Step 2: Construct it in `bootstrap()`**

In `Helm/App/AppModel.swift`, change:

```swift
            let db = try AppDatabase(path: dir.appendingPathComponent("helm.sqlite").path)
            let store = ProjectStore(database: db)
            self.store = store
            self.database = db
```

to:

```swift
            let db = try AppDatabase(path: dir.appendingPathComponent("helm.sqlite").path)
            let store = ProjectStore(database: db)
            self.store = store
            self.database = db
            self.agentAccounts = AgentAccountStore(database: db)
```

- [ ] **Step 3: Inject the env override in `spawnAgent`**

In `Helm/App/AppModel.swift`, change:

```swift
    func spawnAgent(_ adapter: any AgentAdapter, in worktree: Worktree) async {
        Task { await notifier.ensureAuthorization() }
        do {
            let hc = helmctlPath()
            let paneId = UUID()
            try adapter.prepare(worktreePath: worktree.path, paneId: paneId, helmctlPath: hc)
            paneCommands[paneId] = adapter.command(worktreePath: worktree.path, paneId: paneId, helmctlPath: hc)
```

to:

```swift
    func spawnAgent(_ adapter: any AgentAdapter, in worktree: Worktree) async {
        Task { await notifier.ensureAuthorization() }
        do {
            let hc = helmctlPath()
            let paneId = UUID()
            try adapter.prepare(worktreePath: worktree.path, paneId: paneId, helmctlPath: hc)
            var command = adapter.command(worktreePath: worktree.path, paneId: paneId, helmctlPath: hc)
            if let override = configDirOverride(forAgentId: adapter.id) {
                command = "\(override.envKey)=\(Self.shellQuote(override.path)) \(command)"
            }
            paneCommands[paneId] = command
```

Then add these two helpers near `spawnAgent` (e.g. right above it, still
inside the `AppModel` class):

```swift
    /// Config-dir env override for the currently active account of the
    /// given agent id, or `nil` for "System default" (today's behavior:
    /// whatever's globally logged into that CLI on this machine).
    private func configDirOverride(forAgentId agentId: String) -> (envKey: String, path: String)? {
        switch agentId {
        case "claude":
            guard let path = agentAccounts?.activeClaudeConfigDirPath() else { return nil }
            return (envKey: "CLAUDE_CONFIG_DIR", path: path)
        case "codex":
            guard let path = agentAccounts?.activeCodexConfigDirPath() else { return nil }
            return (envKey: "CODEX_HOME", path: path)
        default:
            return nil
        }
    }

    /// POSIX single-quote escaping for embedding the config-dir path in the
    /// shell command string built above.
    private static func shellQuote(_ s: String) -> String {
        "'" + s.replacingOccurrences(of: "'", with: "'\\''") + "'"
    }
```

- [ ] **Step 4: Build to verify it compiles**

Run: `cd Helm && xcodebuild -scheme Helm -configuration Debug -destination 'platform=macOS' build`
Expected: `** BUILD SUCCEEDED **`

- [ ] **Step 5: Manual verification**

1. Launch the built app, add a Claude account via Settings (once Task 6
   ships — if doing Task 5 before Task 6, temporarily call
   `model.agentAccounts?.addClaudeAccount()` from anywhere reachable, e.g.
   a debug menu item, to get one account row), and set it Active (once
   Task 6's UI exists) or directly:
   `model.agentAccounts?.activeClaudeAccountId = <the new account's id>`.
2. Spawn a Claude agent in any worktree.
3. In the new pane, run `echo $CLAUDE_CONFIG_DIR` — confirm it prints the
   account's isolated directory path, not empty.
4. Set `activeClaudeAccountId = nil` (System default), spawn another
   Claude agent, confirm `echo $CLAUDE_CONFIG_DIR` prints empty this time.

- [ ] **Step 6: Commit**

```bash
git add Helm/App/AppModel.swift
git commit -m "feat: inject active agent-account config dir when spawning panes"
```

---

### Task 6: Settings UI — Accounts list

**Files:**
- Modify: `Helm/App/AIProvidersSettingsView.swift` (add `accounts` param + Accounts sub-block to the `Claude Code` and `Codex` sections)
- Modify: `Helm/App/SettingsSurface.swift:66` (pass `model.agentAccounts` to the call site)

**Interfaces:**
- Consumes: `AgentAccountStore` (Task 3/4/5), `AgentAccountRecord` (Task 1).

- [ ] **Step 1: Update the `AIProvidersSettingsView` call site**

In `Helm/App/SettingsSurface.swift`, change:

```swift
        case .aiProviders: AIProvidersSettingsView(store: model.usage)
```

to:

```swift
        case .aiProviders: AIProvidersSettingsView(store: model.usage, accounts: model.agentAccounts)
```

- [ ] **Step 2: Add the `accounts` property and Accounts sections**

In `Helm/App/AIProvidersSettingsView.swift`, change the struct's stored
properties from:

```swift
struct AIProvidersSettingsView: View {
    let store: UsageStore
```

to:

```swift
struct AIProvidersSettingsView: View {
    let store: UsageStore
    let accounts: AgentAccountStore?
```

Then, inside the `Section("Claude Code") { ... }` block, immediately after
the existing `Button("Refresh now") { Task { await store.refresh() } }`
line and before the closing `}` of that section, add:

```swift
                if let accounts {
                    AgentAccountsBlock(
                        title: "Accounts",
                        accounts: accounts.claudeAccounts,
                        activeId: accounts.activeClaudeAccountId,
                        authState: accounts.claudeAuthState,
                        onAdd: { Task { await accounts.addClaudeAccount() } },
                        onCancelAdd: { accounts.cancelClaudeAuth() },
                        onSelect: { accounts.activeClaudeAccountId = $0 },
                        onReAuthenticate: { account in Task { await accounts.reAuthenticateClaudeAccount(account) } },
                        onRemove: { accounts.removeClaudeAccount($0) }
                    )
                }
```

And inside the `Section("Codex") { ... }` block, immediately after its
`Button("Refresh now") { Task { await store.refreshCodex() } }` line
(before that section's closing `}`), add the same block wired to the
Codex methods:

```swift
                if let accounts {
                    AgentAccountsBlock(
                        title: "Accounts",
                        accounts: accounts.codexAccounts,
                        activeId: accounts.activeCodexAccountId,
                        authState: accounts.codexAuthState,
                        onAdd: { Task { await accounts.addCodexAccount() } },
                        onCancelAdd: { accounts.cancelCodexAuth() },
                        onSelect: { accounts.activeCodexAccountId = $0 },
                        onReAuthenticate: { account in Task { await accounts.reAuthenticateCodexAccount(account) } },
                        onRemove: { accounts.removeCodexAccount($0) }
                    )
                }
```

Note: this plan assumes the current `AIProvidersSettingsView.swift` still
has separate `Section("Claude Code")` and `Section("Codex")` blocks (the
shape restored by the `f304e65` regression fix earlier this session — the
same shape this plan's spec was written against). If a future change
merges or renames these sections, adapt the two insertion points
accordingly — the `AgentAccountsBlock` call itself does not depend on the
surrounding section structure.

- [ ] **Step 3: Add the `AgentAccountsBlock` view**

Append to the end of `Helm/App/AIProvidersSettingsView.swift` (after the
closing `}` of `AIProvidersSettingsView`, as a new top-level private
struct in the same file — matching how this file already keeps its
view-model-ish helpers colocated):

```swift

/// Reusable "Accounts" list for one provider: System default row (always
/// present, never removable) + one row per `AgentAccountRecord`. Shared by
/// the Claude Code and Codex sections above — same shape, different
/// callbacks per provider.
private struct AgentAccountsBlock: View {
    let title: String
    let accounts: [AgentAccountRecord]
    let activeId: String?
    let authState: AgentAccountStore.AccountAuthState
    let onAdd: () -> Void
    let onCancelAdd: () -> Void
    let onSelect: (String?) -> Void
    let onReAuthenticate: (AgentAccountRecord) -> Void
    let onRemove: (AgentAccountRecord) -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack {
                VStack(alignment: .leading, spacing: 2) {
                    Text(title).font(.headline)
                    Text("Showing accounts for this device. New accounts are added there.")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
                Spacer()
                switch authState {
                case .idle:
                    Button("Add Account", action: onAdd)
                case .waitingForBrowser:
                    HStack(spacing: 6) {
                        ProgressView().controlSize(.small)
                        Text("Waiting for browser login…").font(.caption)
                        Button("Cancel", action: onCancelAdd)
                    }
                case .failed(let message):
                    HStack(spacing: 6) {
                        Text(message).font(.caption).foregroundStyle(.red)
                        Button("Add Account", action: onAdd)
                    }
                }
            }

            row(label: "System default", subtitle: "Use your current CLI login on this device.",
                isActive: activeId == nil, showActions: false,
                onSelect: { onSelect(nil) }, onReAuthenticate: {}, onRemove: {})

            ForEach(accounts, id: \.id) { account in
                row(label: account.label, subtitle: account.orgName,
                    isActive: activeId == account.id, showActions: true,
                    onSelect: { onSelect(account.id) },
                    onReAuthenticate: { onReAuthenticate(account) },
                    onRemove: { onRemove(account) })
            }
        }
        .padding(.vertical, 4)
    }

    @ViewBuilder
    private func row(
        label: String, subtitle: String?, isActive: Bool, showActions: Bool,
        onSelect: @escaping () -> Void, onReAuthenticate: @escaping () -> Void, onRemove: @escaping () -> Void
    ) -> some View {
        HStack {
            VStack(alignment: .leading, spacing: 2) {
                HStack(spacing: 6) {
                    Text(label).fontWeight(.medium)
                    Text("This device").font(.caption2).padding(.horizontal, 6).padding(.vertical, 2)
                        .background(.quaternary, in: Capsule())
                    if isActive {
                        Text("Active").font(.caption2).padding(.horizontal, 6).padding(.vertical, 2)
                            .background(.tint, in: Capsule())
                    }
                }
                if let subtitle {
                    Text(subtitle).font(.caption).foregroundStyle(.secondary)
                }
            }
            Spacer()
            if showActions {
                Button("Re-authenticate", action: onReAuthenticate)
                Button("Remove", role: .destructive, action: onRemove)
            }
        }
        .contentShape(Rectangle())
        .onTapGesture(perform: onSelect)
    }
}
```

- [ ] **Step 4: Build**

Run: `cd Helm && xcodegen generate && xcodebuild -scheme Helm -configuration Debug -destination 'platform=macOS' build`
Expected: `** BUILD SUCCEEDED **`

- [ ] **Step 5: Manual verification in the running app**

```bash
pkill -f "Helm.app/Contents/MacOS/Helm"
open <DerivedData path from the build output>/Helm.app
```

Open Settings → AI Providers. Confirm:
- Claude Code and Codex sections each show an "Accounts" block below
  their existing Status/Refresh controls, with "System default" listed
  first and marked Active by default.
- Clicking "Add Account" under Claude Code shows "Waiting for browser
  login…" + Cancel, opens a real Anthropic OAuth browser tab; completing
  it adds a row with the real email and clears the waiting state (this is
  the deferred manual check from Task 4, Step 3 — do it now if not done
  already).
- Repeat for Codex's "Add Account".
- Clicking an added account's row moves the "Active" badge to it;
  clicking "System default" moves it back.
- "Remove" deletes the row and its directory
  (`~/Library/Application Support/Helm/agent-accounts/<provider>/<id>/`
  should be gone afterward).

- [ ] **Step 6: Commit**

```bash
git add Helm/App/AIProvidersSettingsView.swift Helm/App/SettingsSurface.swift
git commit -m "feat: add Accounts list UI to AI Providers settings"
```
