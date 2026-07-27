import Foundation

/// How to launch an ACP agent process for a worktree. Commands run through a
/// login shell (`zsh -lc`) so the user's PATH (node via nvm/homebrew, the
/// opencode binary) resolves exactly as it does in their terminal panes;
/// `exec` replaces the shell so the child IS the agent process.
public struct AgentLaunchSpec: Sendable, Equatable {
    public var executable: String
    public var arguments: [String]
    /// Extra environment from the install manifest, merged over the base
    /// environment at launch.
    public var environment: [String: String]

    public init(executable: String, arguments: [String],
                environment: [String: String] = [:]) {
        self.executable = executable
        self.arguments = arguments
        self.environment = environment
    }

    /// Environment for the adapter process. Strips the markers Claude Code
    /// sets in its own shells: when Tiller is launched from such a shell the
    /// child inherits them and the Claude Agent SDK inside claude-code-acp
    /// refuses to run ("Query closed before response received").
    public static func launchEnvironment(
        base: [String: String] = ProcessInfo.processInfo.environment,
        extra: [String: String] = [:]
    ) -> [String: String] {
        var environment = base.merging(extra) { _, new in new }
        environment.removeValue(forKey: "CLAUDECODE")
        environment.removeValue(forKey: "CLAUDE_CODE_ENTRYPOINT")
        return environment
    }
}

/// Registry ids are canonical for ACP agents; Tiller's historical short ids
/// (persisted in sessions and tabs from earlier versions) map onto them.
public enum AgentIdMigration {
    public static func canonical(_ id: String) -> String {
        switch id {
        case "claude": "claude-acp"
        case "codex": "codex-acp"
        case "pi", "pi-acp": "pi"
        default: id
        }
    }

    /// Inverse of `canonical`: the AgentCatalog short id for a registry id.
    /// The old Pi ACP registry id remains accepted as an input alias.
    public static func catalogId(_ id: String) -> String {
        switch id {
        case "claude-acp": "claude"
        case "codex-acp": "codex"
        case "pi-acp": "pi"
        default: id
        }
    }
}

extension AgentLaunchSpec {
    /// Launch spec resolved from the install store and built-ins.
    public static func resolved(id: String,
                                installStore: AgentInstallStore) -> AgentLaunchSpec? {
        let canonical = AgentIdMigration.canonical(id)
        if ["claude-acp", "codex-acp", "opencode", "pi"].contains(canonical) {
            return nil
        }
        if canonical == "omp" {
            return AgentLaunchSpec(executable: "/bin/zsh",
                                   arguments: ["-lc", "exec omp acp"])
        }
        guard let manifest = installStore.manifest(id: canonical) else { return nil }
        // Through the login shell like omp: npm bins are node scripts whose
        // `#!/usr/bin/env node` shebang needs the user's PATH, which a
        // Finder-launched app doesn't have.
        let command = ([manifest.executable] + manifest.arguments)
            .map(posixQuoted).joined(separator: " ")
        return AgentLaunchSpec(executable: "/bin/zsh",
                               arguments: ["-lc", "exec \(command)"],
                               environment: manifest.environment)
    }
}
