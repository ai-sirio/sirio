import Foundation

public enum PermissionOptionKind: String, Sendable, Equatable, Codable {
    case allowOnce = "allow_once"
    case allowAlways = "allow_always"
    case rejectOnce = "reject_once"
    case rejectAlways = "reject_always"
}

public struct PermissionOption: Sendable, Equatable, Codable {
    public var optionId: String
    public var name: String
    public var kind: PermissionOptionKind
    public init(optionId: String, name: String, kind: PermissionOptionKind) {
        self.optionId = optionId
        self.name = name
        self.kind = kind
    }
}

/// Params of the agent-initiated `session/request_permission` request.
public struct RequestPermissionParams: Sendable, Equatable, Decodable {
    public var sessionId: String
    public var toolCall: ToolCallUpdate
    public var options: [PermissionOption]
}

/// User decision, encoded as `{outcome: {outcome: "selected", optionId}}`
/// or `{outcome: {outcome: "cancelled"}}`.
public enum PermissionOutcome: Sendable, Equatable {
    case selected(optionId: String)
    /// A chosen answer to a question, carried back to the agent as the tool's
    /// updated input. Encodes as `selected` on the ACP wire, which has no
    /// equivalent field.
    case answered(optionId: String, updatedInput: JSONValue)
    case cancelled
}

extension PermissionOutcome: Codable {
    private enum CodingKeys: String, CodingKey { case outcome, optionId }

    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        switch try container.decode(String.self, forKey: .outcome) {
        case "selected":
            self = .selected(optionId: try container.decode(String.self, forKey: .optionId))
        default:
            self = .cancelled
        }
    }

    public func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        switch self {
        case .selected(let optionId):
            try container.encode("selected", forKey: .outcome)
            try container.encode(optionId, forKey: .optionId)
        case .answered(let optionId, _):
            try container.encode("selected", forKey: .outcome)
            try container.encode(optionId, forKey: .optionId)
        case .cancelled:
            try container.encode("cancelled", forKey: .outcome)
        }
    }
}

public struct RequestPermissionResult: Sendable, Equatable, Codable {
    public var outcome: PermissionOutcome
    public init(outcome: PermissionOutcome) { self.outcome = outcome }
}
