import Foundation

/// Tiller's unified permission mode (spec table). Raw values are persisted in
/// chatSession.permissionMode — do not rename cases.
public enum PermissionMode: String, CaseIterable, Sendable, Codable {
    case ask, acceptEdits, plan, fullAuto

    public var claudeValue: String {
        switch self {
        case .ask: "default"
        case .acceptEdits: "acceptEdits"
        case .plan: "plan"
        case .fullAuto: "bypassPermissions"
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
        case "claude-acp": [.ask, .acceptEdits, .plan, .fullAuto]
        case "codex-acp", "opencode": [.ask, .acceptEdits, .fullAuto]
        default: [] // ACP agents keep their agent-provided modes
        }
    }
}
