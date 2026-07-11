/// Flag dispatch for the dual-mode `notify` CLI command: agent-status
/// (--session/--status, used by agent hooks) vs user notification
/// (--title/--body, cmux parity). Pure so the usage-error matrix is
/// testable without ArgumentParser.
public enum NotifyMode: Equatable, Sendable {
    case agentStatus
    case userNotification

    public static func resolve(
        session: String?, status: String?, title: String?, body: String?
    ) throws -> NotifyMode {
        switch (session != nil, title != nil) {
        case (true, true): throw NotifyModeError.ambiguousMode
        case (false, false): throw NotifyModeError.missingMode
        case (true, false):
            guard status != nil else { throw NotifyModeError.missingStatus }
            return .agentStatus
        case (false, true):
            guard body != nil else { throw NotifyModeError.missingBody }
            return .userNotification
        }
    }
}

public enum NotifyModeError: Error, Equatable, Sendable {
    case ambiguousMode, missingMode, missingStatus, missingBody

    public var usageMessage: String {
        switch self {
        case .ambiguousMode, .missingMode:
            "use either --session/--status (agent status) or --title/--body (user notification)"
        case .missingStatus: "--status is required with --session"
        case .missingBody: "--body is required with --title"
        }
    }
}
