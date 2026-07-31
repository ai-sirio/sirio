import Foundation

/// Native session reference reported by an agent for one terminal, used to
/// resume the agent's conversation after a Tiller restart.
///
/// Keyed by `TerminalContentID`, which is stable across relaunches. The live
/// pane id is a `ResourceGenerationID`, minted anew every time the terminal
/// spawns, so a ref stored under it could never be matched after a restart.
public struct AgentSessionRef: Sendable, Equatable {
    public let contentID: TerminalContentID
    public let agentId: String
    public let sessionRef: String

    public init(contentID: TerminalContentID, agentId: String, sessionRef: String) {
        self.contentID = contentID
        self.agentId = agentId
        self.sessionRef = sessionRef
    }
}
