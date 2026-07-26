import Foundation

/// Pending or resolved permission attached to a tool call.
public struct PermissionState: Sendable, Equatable, Codable {
    public enum Resolution: Sendable, Equatable, Codable {
        case selected(optionId: String)
        case cancelled
    }

    public var requestId: JSONRPCID
    public var options: [PermissionOption]
    public var resolution: Resolution?

    public init(requestId: JSONRPCID, options: [PermissionOption],
                resolution: Resolution? = nil) {
        self.requestId = requestId
        self.options = options
        self.resolution = resolution
    }

    public var isPending: Bool { resolution == nil }
}

/// A tool call as shown in the transcript, including its permission state.
public struct ToolCallItem: Sendable, Equatable, Codable, Identifiable {
    public var toolCallId: String
    public var title: String
    public var kind: ToolKind
    public var status: ToolCallStatus
    public var content: [ToolCallContent]
    public var locations: [ToolCallLocation]
    public var permission: PermissionState?
    /// Accumulated agent-side terminal output (claude-agent-acp _meta
    /// extension); nil for tool calls without a terminal.
    public var terminalOutput: String?
    public var terminalExit: TerminalExitStatus?
    /// Raw tool input as reported by the agent; carries the Task tool's
    /// `subagent_type` used to detect subagent spawns.
    public var rawInput: JSONValue?
    public var parentToolCallId: String?
    /// Optional UI identity used when a persisted or live tool call's raw
    /// protocol ID collides with another rendered transcript item.
    public var transcriptID: String?

    public var id: String { transcriptID ?? toolCallId }

    public init(toolCallId: String, title: String, kind: ToolKind,
                status: ToolCallStatus, content: [ToolCallContent] = [],
                locations: [ToolCallLocation] = [], permission: PermissionState? = nil,
                terminalOutput: String? = nil, terminalExit: TerminalExitStatus? = nil,
                rawInput: JSONValue? = nil, parentToolCallId: String? = nil,
                transcriptID: String? = nil) {
        self.toolCallId = toolCallId
        self.title = title
        self.kind = kind
        self.status = status
        self.content = content
        self.locations = locations
        self.permission = permission
        self.terminalOutput = terminalOutput
        self.terminalExit = terminalExit
        self.rawInput = rawInput
        self.parentToolCallId = parentToolCallId
        self.transcriptID = transcriptID
    }

    private enum CodingKeys: String, CodingKey {
        case toolCallId, title, kind, status, content, locations, permission
        case terminalOutput, terminalExit, rawInput, parentToolCallId, transcriptID
    }

    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        self.init(
            toolCallId: try container.decode(String.self, forKey: .toolCallId),
            title: try container.decode(String.self, forKey: .title),
            kind: try container.decode(ToolKind.self, forKey: .kind),
            status: try container.decode(ToolCallStatus.self, forKey: .status),
            content: try container.decode([ToolCallContent].self, forKey: .content),
            locations: try container.decode([ToolCallLocation].self, forKey: .locations),
            permission: try container.decodeIfPresent(PermissionState.self, forKey: .permission),
            terminalOutput: try container.decodeIfPresent(String.self, forKey: .terminalOutput),
            terminalExit: try container.decodeIfPresent(TerminalExitStatus.self, forKey: .terminalExit),
            rawInput: try container.decodeIfPresent(JSONValue.self, forKey: .rawInput),
            parentToolCallId: try container.decodeIfPresent(String.self, forKey: .parentToolCallId),
            transcriptID: try container.decodeIfPresent(String.self, forKey: .transcriptID))
    }

    public func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(toolCallId, forKey: .toolCallId)
        try container.encode(title, forKey: .title)
        try container.encode(kind, forKey: .kind)
        try container.encode(status, forKey: .status)
        try container.encode(content, forKey: .content)
        try container.encode(locations, forKey: .locations)
        try container.encodeIfPresent(permission, forKey: .permission)
        try container.encodeIfPresent(terminalOutput, forKey: .terminalOutput)
        try container.encodeIfPresent(terminalExit, forKey: .terminalExit)
        try container.encodeIfPresent(rawInput, forKey: .rawInput)
        try container.encodeIfPresent(parentToolCallId, forKey: .parentToolCallId)
        try container.encodeIfPresent(transcriptID, forKey: .transcriptID)
    }

    init(_ call: ToolCall) {
        self.init(toolCallId: call.toolCallId, title: call.title, kind: call.kind,
                  status: call.status, content: call.content, locations: call.locations,
                  rawInput: call.rawInput, parentToolCallId: call.parentToolCallId)
    }

    /// Merges the non-nil fields of a partial update.
    mutating func merge(_ update: ToolCallUpdate) {
        if let title = update.title { self.title = title }
        if let kind = update.kind { self.kind = kind }
        if let status = update.status { self.status = status }
        if let content = update.content { self.content = content }
        if let locations = update.locations { self.locations = locations }
        if let rawInput = update.rawInput { self.rawInput = rawInput }
        if let parent = update.parentToolCallId { parentToolCallId = parent }
        if let meta = update.terminalMeta {
            if let chunk = meta.terminalOutput {
                terminalOutput = (terminalOutput ?? "") + chunk.data
            }
            if let exit = meta.terminalExit {
                terminalExit = TerminalExitStatus(exitCode: exit.exitCode,
                                                  signal: exit.signal)
            }
        }
    }
}

/// One entry of the chat transcript. Persisted as-is (Codable) by the
/// persistence layer in Plan 2.
public enum TranscriptItem: Sendable, Equatable, Codable, Identifiable {
    case userMessage(id: String, blocks: [ContentBlock])
    case agentMessage(id: String, text: String, isComplete: Bool)
    case thought(id: String, text: String)
    case toolCall(ToolCallItem)
    case plan(id: String, entries: [PlanEntry])
    case turnDivider(id: String, at: Date)
    /// Riepilogo di fine turno dei file modificati dall'agente (kind .edit
    /// completati); sintetizzato dal reducer, non arriva dal protocollo.
    case editSummary(id: String, paths: [String])
    /// Local UI notice (e.g. agent switch); never sent by the protocol,
    /// excluded from handoff preambles.
    case systemNotice(id: String, text: String)

    public var id: String {
        switch self {
        case .userMessage(let id, _): id
        case .agentMessage(let id, _, _): id
        case .thought(let id, _): id
        case .toolCall(let item): item.id
        case .plan(let id, _): id
        case .turnDivider(let id, _): id
        case .editSummary(let id, _): id
        case .systemNotice(let id, _): id
        }
    }
}
