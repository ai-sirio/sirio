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


/// Exit status of an agent-side terminal command (claude-agent-acp
/// `_meta.terminal_exit`).
public struct TerminalExitStatus: Sendable, Equatable, Codable {
    public var exitCode: Int?
    public var signal: String?
    public init(exitCode: Int? = nil, signal: String? = nil) {
        self.exitCode = exitCode
        self.signal = signal
    }
}

/// claude-agent-acp terminal extension riding in `_meta` on
/// tool_call/tool_call_update: the SDK runs the command agent-side and
/// streams output here — the client never spawns a process.
public struct TerminalMeta: Sendable, Equatable, Codable {
    public struct Info: Sendable, Equatable, Codable {
        public var terminalId: String
        enum CodingKeys: String, CodingKey { case terminalId = "terminal_id" }
        public init(terminalId: String) { self.terminalId = terminalId }
    }
    public struct Output: Sendable, Equatable, Codable {
        public var terminalId: String
        public var data: String
        enum CodingKeys: String, CodingKey {
            case terminalId = "terminal_id", data
        }
        public init(terminalId: String, data: String) {
            self.terminalId = terminalId
            self.data = data
        }
    }
    public struct Exit: Sendable, Equatable, Codable {
        public var terminalId: String
        public var exitCode: Int?
        public var signal: String?
        enum CodingKeys: String, CodingKey {
            case terminalId = "terminal_id", exitCode = "exit_code", signal
        }
        public init(terminalId: String, exitCode: Int? = nil,
                    signal: String? = nil) {
            self.terminalId = terminalId
            self.exitCode = exitCode
            self.signal = signal
        }
    }

    public var terminalInfo: Info?
    public var terminalOutput: Output?
    public var terminalExit: Exit?

    enum CodingKeys: String, CodingKey {
        case terminalInfo = "terminal_info"
        case terminalOutput = "terminal_output"
        case terminalExit = "terminal_exit"
    }

    public init(terminalInfo: Info? = nil, terminalOutput: Output? = nil,
                terminalExit: Exit? = nil) {
        self.terminalInfo = terminalInfo
        self.terminalOutput = terminalOutput
        self.terminalExit = terminalExit
    }
}

 
/// Tool call output, discriminated by `type`: nested content block, a diff,
/// or an agent-side terminal stream.
public enum ToolCallContent: Sendable, Equatable {
    case content(ContentBlock)
    case diff(path: String, oldText: String?, newText: String)
    case terminal(terminalId: String)
    case unknown(type: String)
}

extension ToolCallContent: Codable {
    private enum CodingKeys: String, CodingKey {
        case type, content, path, oldText, newText, terminalId
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
        case "terminal":
            self = .terminal(terminalId: try container.decode(String.self,
                                                               forKey: .terminalId))
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
        case .terminal(let terminalId):
            try container.encode("terminal", forKey: .type)
            try container.encode(terminalId, forKey: .terminalId)
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
    public var terminalMeta: TerminalMeta?
    public var parentToolCallId: String?
    private enum CodingKeys: String, CodingKey {
        case toolCallId, title, kind, status, content, locations, rawInput
        case parentToolCallId
        case terminalMeta = "_meta"
    }

    public init(toolCallId: String, title: String, kind: ToolKind,
                status: ToolCallStatus, content: [ToolCallContent] = [],
                locations: [ToolCallLocation] = [], rawInput: JSONValue? = nil,
                terminalMeta: TerminalMeta? = nil,
                parentToolCallId: String? = nil) {
        self.toolCallId = toolCallId
        self.title = title
        self.kind = kind
        self.status = status
        self.content = content
        self.locations = locations
        self.rawInput = rawInput
        self.terminalMeta = terminalMeta
        self.parentToolCallId = parentToolCallId
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
        terminalMeta = try container.decodeIfPresent(TerminalMeta.self, forKey: .terminalMeta)
        parentToolCallId = try container.decodeIfPresent(String.self,
                                                         forKey: .parentToolCallId)
    }
}

/// Partial tool call: every field except the id is optional.
public struct ToolCallUpdate: Sendable, Equatable, Codable {
    private enum CodingKeys: String, CodingKey {
        case toolCallId, title, kind, status, content, locations, rawInput
        case parentToolCallId
        case terminalMeta = "_meta"
    }

    public var toolCallId: String
    public var title: String?
    public var kind: ToolKind?
    public var status: ToolCallStatus?
    public var content: [ToolCallContent]?
    public var locations: [ToolCallLocation]?
    public var rawInput: JSONValue?
    public var terminalMeta: TerminalMeta?
    public var parentToolCallId: String?

    public init(toolCallId: String, title: String? = nil, kind: ToolKind? = nil,
                status: ToolCallStatus? = nil, content: [ToolCallContent]? = nil,
                locations: [ToolCallLocation]? = nil, rawInput: JSONValue? = nil,
                terminalMeta: TerminalMeta? = nil,
                parentToolCallId: String? = nil) {
        self.toolCallId = toolCallId
        self.title = title
        self.kind = kind
        self.status = status
        self.content = content
        self.locations = locations
        self.rawInput = rawInput
        self.terminalMeta = terminalMeta
        self.parentToolCallId = parentToolCallId
    }
}
