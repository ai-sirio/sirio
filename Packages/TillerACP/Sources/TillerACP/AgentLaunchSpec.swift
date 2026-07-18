import Foundation

/// How to launch an ACP agent process for a worktree. Commands run through a
/// login shell (`zsh -lc`) so the user's PATH (node via nvm/homebrew, the
/// opencode binary) resolves exactly as it does in their terminal panes;
/// `exec` replaces the shell so the child IS the agent process.
public struct AgentLaunchSpec: Sendable, Equatable {
    public var executable: String
    public var arguments: [String]

    public init(executable: String, arguments: [String]) {
        self.executable = executable
        self.arguments = arguments
    }

    /// Pinned adapter version: a protocol-stable, reproducible launch.
    /// Update deliberately via `npm view @zed-industries/claude-code-acp version`.
    public static let claudeCodeACPVersion = "0.16.2"

    /// Claude Code via Zed's official ACP adapter, fetched on demand by npx
    /// (cached by npm after the first run).
    public static func claudeCode() -> AgentLaunchSpec {
        AgentLaunchSpec(
            executable: "/bin/zsh",
            arguments: ["-lc",
                "exec npx -y @zed-industries/claude-code-acp@\(claudeCodeACPVersion)"])
    }

    /// OpenCode's native ACP mode (binary installed by the user).
    public static func openCode() -> AgentLaunchSpec {
        AgentLaunchSpec(executable: "/bin/zsh", arguments: ["-lc", "exec opencode acp"])
    }

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
}
