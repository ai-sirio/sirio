import Foundation

/// Params of the agent-initiated `fs/read_text_file` request.
public struct ReadTextFileParams: Sendable, Equatable, Codable {
    public var sessionId: String
    public var path: String
    public var line: Int?
    public var limit: Int?
}

public struct ReadTextFileResult: Sendable, Equatable, Codable {
    public var content: String
    public init(content: String) { self.content = content }
}

/// Params of the agent-initiated `fs/write_text_file` request.
public struct WriteTextFileParams: Sendable, Equatable, Codable {
    public var sessionId: String
    public var path: String
    public var content: String
}
