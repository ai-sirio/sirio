import Foundation

enum AgentMessagePresentation: Equatable, Sendable {
    case streaming
    case rich

    static func mode(isComplete: Bool) -> Self {
        isComplete ? .rich : .streaming
    }
}
