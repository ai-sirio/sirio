import Foundation

/// Native session reference reported by an agent for one pane, used to
/// resume the agent's conversation after a Tiller restart.
public struct AgentSessionRef: Sendable, Equatable {
    public let paneId: UUID
    public let agentId: String
    public let sessionRef: String

    public init(paneId: UUID, agentId: String, sessionRef: String) {
        self.paneId = paneId
        self.agentId = agentId
        self.sessionRef = sessionRef
    }
}
