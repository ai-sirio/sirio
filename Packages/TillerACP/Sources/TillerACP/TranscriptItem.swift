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

    public var id: String { toolCallId }

    public init(toolCallId: String, title: String, kind: ToolKind,
                status: ToolCallStatus, content: [ToolCallContent] = [],
                locations: [ToolCallLocation] = [], permission: PermissionState? = nil) {
        self.toolCallId = toolCallId
        self.title = title
        self.kind = kind
        self.status = status
        self.content = content
        self.locations = locations
        self.permission = permission
    }

    init(_ call: ToolCall) {
        self.init(toolCallId: call.toolCallId, title: call.title, kind: call.kind,
                  status: call.status, content: call.content, locations: call.locations)
    }

    /// Merges the non-nil fields of a partial update.
    mutating func merge(_ update: ToolCallUpdate) {
        if let title = update.title { self.title = title }
        if let kind = update.kind { self.kind = kind }
        if let status = update.status { self.status = status }
        if let content = update.content { self.content = content }
        if let locations = update.locations { self.locations = locations }
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

    public var id: String {
        switch self {
        case .userMessage(let id, _): id
        case .agentMessage(let id, _, _): id
        case .thought(let id, _): id
        case .toolCall(let item): item.id
        case .plan(let id, _): id
        case .turnDivider(let id, _): id
        }
    }
}
