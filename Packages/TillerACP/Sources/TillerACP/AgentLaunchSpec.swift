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
    /// Update deliberately via `npm view @agentclientprotocol/claude-agent-acp version`.
    /// (Successor of `@zed-industries/claude-code-acp`, which is frozen at
    /// 0.16.2 with a stale model list; the renamed package tracks current
    /// Claude Agent SDK releases and exposes the model as a `configOptions`
    /// select instead of `session/set_model`.)
    public static let claudeCodeACPVersion = "0.59.0"

    /// Claude Code via the official ACP adapter, fetched on demand by npx
    /// (cached by npm after the first run).
    public static func claudeCode() -> AgentLaunchSpec {
        AgentLaunchSpec(
            executable: "/bin/zsh",
            arguments: ["-lc",
                "exec npx -y @agentclientprotocol/claude-agent-acp@\(claudeCodeACPVersion)"])
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

    /// Environment for the adapter process. Strips the markers Claude Code
    /// sets in its own shells: when Tiller is launched from such a shell the
    /// child inherits them and the Claude Agent SDK inside claude-code-acp
    /// refuses to run ("Query closed before response received").
    public static func launchEnvironment(
        base: [String: String] = ProcessInfo.processInfo.environment
    ) -> [String: String] {
        var environment = base
        environment.removeValue(forKey: "CLAUDECODE")
        environment.removeValue(forKey: "CLAUDE_CODE_ENTRYPOINT")
        return environment
    }
}
