import Foundation

/// Pi has no lifecycle hook mechanism (see
/// docs/contracts/agent-hook-capabilities.md), so `prepare` is a no-op and
/// Tiller watches the pane's exit code.
public struct PiAdapter: AgentAdapter {
    public let id = "pi"
    public let displayName = "Pi"
    public var hasNativeHooks: Bool { false }

    public init() {}

    public func prepare(
        worktreePath: String,
        paneId: UUID,
        tillerctlPath: String
    ) throws {}

    public func command(worktreePath: String, paneId: UUID, tillerctlPath: String) -> String {
        "pi"
    }

    public func resumeCommand(worktreePath: String, paneId: UUID,
                              tillerctlPath: String, sessionRef: String) -> String? {
        "pi --session \(shellQuote(sessionRef))"
    }
    
    public func summarizerCommand(prompt: String) -> String? {
        "pi --print --no-tools \(shellQuote(prompt))"
    }
}
