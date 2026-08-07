import Foundation

/// Tiller's unified permission mode (spec table). Raw values are persisted in
/// chatSession.permissionMode — do not rename cases.
public enum PermissionMode: String, CaseIterable, Sendable, Codable {
    case ask, acceptEdits, plan, fullAuto

    /// Claude requires `--dangerously-skip-permissions` at launch for
    /// `bypassPermissions`. Tiller implements full auto as `acceptEdits` plus
    /// automatic tool approval instead.
    public var claudeValue: String {
        switch self {
        case .ask: "default"
        case .acceptEdits: "acceptEdits"
        case .plan: "plan"
        case .fullAuto: "acceptEdits"
        }
    }

    public var codexApprovalPolicy: String {
        switch self {
        case .ask: "untrusted"
        case .acceptEdits: "on-request"
        case .plan: "untrusted" // unreachable: plan is not offered for codex
        case .fullAuto: "never"
        }
    }

    /// Full auto is always confined to the worktree (spec security section).
    public var codexSandbox: String {
        switch self {
        case .fullAuto: "workspace-write"
        default: "workspace-write"
        }
    }

    public var displayName: String {
        switch self {
        case .ask: "Ask"
        case .acceptEdits: "Accept edits"
        case .plan: "Plan"
        case .fullAuto: "Full auto"
        }
    }

    /// Bridge into the ACP mode model so the existing dropdown renders these.
    public var sessionMode: SessionMode {
        SessionMode(id: rawValue, name: displayName)
    }

    public static func supported(byDriverFor agentId: String) -> [PermissionMode] {
        switch AgentIdMigration.canonical(agentId) {
        case "claude": [.ask, .acceptEdits, .plan, .fullAuto]
        case "codex", "opencode": [.ask, .acceptEdits, .fullAuto]
        // Pi is native but has neither a mode call on its RPC nor a mode flag
        // on its CLI, so it belongs here with the ACP agents.
        default: [] // ACP agents keep their agent-provided modes
        }
    }

    /// What the unified mode pill shows, or nil when the driver has no modes
    /// and the pill must stay hidden. Being a native transport is not the
    /// same thing: Pi is native and has none.
    public static func pillSelection(forAgent agentId: String,
                                     requested: PermissionMode) -> PermissionMode? {
        let modes = supported(byDriverFor: agentId)
        return modes.contains(requested) ? requested : modes.first
    }
}
