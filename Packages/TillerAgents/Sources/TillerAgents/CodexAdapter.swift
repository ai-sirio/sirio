import Foundation

/// Adapter for Cursor's Codex CLI.
///
/// `prepare` is a no-op because Codex configuration is global. Hook behaviour
/// is delivered via the `-c` CLI override in the command string.
///
/// `command` returns a shell command that launches Codex with a `notify`
/// override so it calls `tillerctl` on lifecycle events.
public struct CodexAdapter: AgentAdapter, Sendable {
    public let id = "codex"
    public let displayName = "Codex"

    public var hasNativeHooks: Bool { true }
    public init() {}

    public func prepare(
        worktreePath: String,
        paneId: UUID,
        tillerctlPath: String
    ) throws {}

    public func command(worktreePath: String, paneId: UUID, tillerctlPath: String) -> String {
        "codex -c \(notifyOverride(paneId: paneId, tillerctlPath: tillerctlPath))"
    }

    public func resumeCommand(worktreePath: String, paneId: UUID,
                              tillerctlPath: String, sessionRef: String) -> String? {
        "codex -c \(notifyOverride(paneId: paneId, tillerctlPath: tillerctlPath)) resume \(shellQuote(sessionRef))"
    }

    private func notifyOverride(paneId: UUID, tillerctlPath: String) -> String {
        let args = [tillerctlPath, "notify", "--session", paneId.uuidString, "--status", "needs-input"]
        let jsonArgs = args.map(jsonStringLiteral).joined(separator: ",")
        return shellQuote("notify=[\(jsonArgs)]")
    }
}
