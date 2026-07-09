import Foundation
import TillerCore
import TillerPersistence

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

    /// `~/Library/Application Support/Tiller/agent-accounts/<provider>/<id>/`
    /// — created lazily by the add-account flow (Task 4), never referenced
    /// before that directory exists.
    static func newAccountConfigDir(provider: String, id: String) throws -> URL {
        let base = try FileManager.default.url(
            for: .applicationSupportDirectory, in: .userDomainMask, appropriateFor: nil, create: true)
        let dir = base.appendingPathComponent("Tiller/agent-accounts/\(provider)/\(id)", isDirectory: true)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        return dir
    }

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
}
