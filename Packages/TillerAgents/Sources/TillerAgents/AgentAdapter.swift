import Foundation

/// Interface for an AI agent adapter that can be launched in a Tiller worktree pane.
public protocol AgentAdapter: Sendable {
    /// Stable identifier, e.g. "claude" or "codex".
    var id: String { get }
    /// Human-readable name, e.g. "Claude Code" or "Codex".
    var displayName: String { get }
    /// True when the agent notifies lifecycle events itself (hooks calling
    /// `tillerctl notify`). False → Tiller watches the pane's exit code instead.
    var hasNativeHooks: Bool { get }

    /// Writes per-worktree hook configuration.
    /// - Note: NEVER touches user-global config (`~/.claude/settings.json`,
    ///   `~/.codex/config.toml`, etc.).
    func prepare(worktreePath: String, paneId: UUID, tillerctlPath: String) throws

    /// Full shell command to run inside the pane.
    func command(worktreePath: String, paneId: UUID, tillerctlPath: String) -> String

    /// Full shell command that relaunches the agent resuming a previously
    /// captured native session, or nil when the agent cannot resume by
    /// reference. `sessionRef` comes from the agentSession table.
    func resumeCommand(worktreePath: String, paneId: UUID,
                       tillerctlPath: String, sessionRef: String) -> String?
}

/// Well-known adapters shipped with Tiller.
public enum AgentCatalog {
    /// All registered adapters, in display order.
    public static let all: [any AgentAdapter] = [
        ClaudeCodeAdapter(),
        CodexAdapter(),
        OpenCodeAdapter(),
        PiAdapter(),
        OhMyPiAdapter(),
    ]
}
