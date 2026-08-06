import Foundation

/// Display state of one Activity row. Distinct from `AgentStatus` because a
/// terminal running a bare shell has no agent, and `AgentStatus` has no case
/// for "nothing is running" — that gap is what `.idle` fills.
public enum ActivityStatus: String, Sendable, Equatable {
    case running
    case needsInput
    case done
    case error
    case idle

    public static func from(_ status: AgentStatus?) -> ActivityStatus {
        switch status {
        case .running: .running
        case .needsInput: .needsInput
        case .done: .done
        case .error: .error
        case nil: .idle
        }
    }

    /// Closing a row terminates its process, so states that imply live work ask
    /// first. `.error` is included: an agent that reported a failure may still
    /// be sitting at a prompt.
    public var requiresCloseConfirmation: Bool {
        switch self {
        case .running, .needsInput, .error: true
        case .done, .idle: false
        }
    }
}
