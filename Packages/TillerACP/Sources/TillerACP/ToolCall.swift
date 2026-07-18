import Foundation

/// Category the agent assigns to a tool call; drives the card icon.
public enum ToolKind: String, Sendable, Equatable, Codable {
    case read, edit, delete, move, search, execute, think, fetch
    case switchMode = "switch_mode"
    case other

    public init(from decoder: Decoder) throws {
        let raw = try decoder.singleValueContainer().decode(String.self)
        self = ToolKind(rawValue: raw) ?? .other
    }
}

public enum ToolCallStatus: String, Sendable, Equatable, Codable {
    case pending
    case inProgress = "in_progress"
    case completed
    case failed
}

public struct ToolCallLocation: Sendable, Equatable, Codable {
    public var path: String
    public var line: Int?
    public init(path: String, line: Int? = nil) {
        self.path = path
        self.line = line
    }
}

/// Tool call output, discriminated by `type`: nested content block or a diff.
public enum ToolCallContent: Sendable, Equatable {
    case content(ContentBlock)
    case diff(path: String, oldText: String?, newText: String)
    case unknown(type: String)
}

extension ToolCallContent: Codable {
    private enum CodingKeys: String, CodingKey {
        case type, content, path, oldText, newText
    }

    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        let type = try container.decode(String.self, forKey: .type)
        switch type {
        case "content":
            self = .content(try container.decode(ContentBlock.self, forKey: .content))
        case "diff":
            self = .diff(path: try container.decode(String.self, forKey: .path),
                         oldText: try container.decodeIfPresent(String.self, forKey: .oldText),
                         newText: try container.decode(String.self, forKey: .newText))
        default:
            self = .unknown(type: type)
        }
    }

    public func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        switch self {
        case .content(let block):
            try container.encode("content", forKey: .type)
            try container.encode(block, forKey: .content)
        case .diff(let path, let oldText, let newText):
            try container.encode("diff", forKey: .type)
            try container.encode(path, forKey: .path)
            try container.encodeIfPresent(oldText, forKey: .oldText)
            try container.encode(newText, forKey: .newText)
        case .unknown(let type):
            try container.encode(type, forKey: .type)
        }
    }
}

public struct ToolCall: Sendable, Equatable, Codable {
    public var toolCallId: String
    public var title: String
    public var kind: ToolKind
    public var status: ToolCallStatus
    public var content: [ToolCallContent]
    public var locations: [ToolCallLocation]
    public var rawInput: JSONValue?

    public init(toolCallId: String, title: String, kind: ToolKind,
                status: ToolCallStatus, content: [ToolCallContent] = [],
                locations: [ToolCallLocation] = [], rawInput: JSONValue? = nil) {
        self.toolCallId = toolCallId
        self.title = title
        self.kind = kind
        self.status = status
        self.content = content
        self.locations = locations
        self.rawInput = rawInput
    }

    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        toolCallId = try container.decode(String.self, forKey: .toolCallId)
        title = try container.decode(String.self, forKey: .title)
        kind = try container.decodeIfPresent(ToolKind.self, forKey: .kind) ?? .other
        status = try container.decodeIfPresent(ToolCallStatus.self, forKey: .status) ?? .pending
        content = try container.decodeIfPresent([ToolCallContent].self, forKey: .content) ?? []
        locations = try container.decodeIfPresent([ToolCallLocation].self, forKey: .locations) ?? []
        rawInput = try container.decodeIfPresent(JSONValue.self, forKey: .rawInput)
    }
}

/// Partial tool call: every field except the id is optional.
public struct ToolCallUpdate: Sendable, Equatable, Codable {
    public var toolCallId: String
    public var title: String?
    public var kind: ToolKind?
    public var status: ToolCallStatus?
    public var content: [ToolCallContent]?
    public var locations: [ToolCallLocation]?
    public var rawInput: JSONValue?

    public init(toolCallId: String, title: String? = nil, kind: ToolKind? = nil,
                status: ToolCallStatus? = nil, content: [ToolCallContent]? = nil,
                locations: [ToolCallLocation]? = nil, rawInput: JSONValue? = nil) {
        self.toolCallId = toolCallId
        self.title = title
        self.kind = kind
        self.status = status
        self.content = content
        self.locations = locations
        self.rawInput = rawInput
    }
}
