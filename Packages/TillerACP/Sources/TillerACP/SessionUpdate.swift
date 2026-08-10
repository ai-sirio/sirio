import Foundation

public struct PlanEntry: Sendable, Equatable, Codable {
    public var content: String
    public var priority: String
    public var status: String
    public init(content: String, priority: String, status: String) {
        self.content = content
        self.priority = priority
        self.status = status
    }
}

public struct AvailableCommand: Sendable, Equatable, Codable {
    public var name: String
    public var description: String
    public init(name: String, description: String) {
        self.name = name
        self.description = description
    }
}

public struct ContextUsage: Sendable, Equatable, Codable {
    public var used: Int
    public var size: Int
    public init(used: Int, size: Int) {
        self.used = used
        self.size = size
    }
}

/// One `session/update` payload, discriminated by `sessionUpdate` on the wire.
public enum SessionUpdate: Sendable, Equatable {
    case userMessageChunk(ContentBlock)
    case agentMessageChunk(ContentBlock)
    case agentThoughtChunk(ContentBlock)
    case toolCall(ToolCall)
    case toolCallUpdate(ToolCallUpdate)
    case plan([PlanEntry])
    case availableCommandsUpdate([AvailableCommand])
    case currentModeUpdate(String)
    case usageUpdate(ContextUsage)
    /// Driver-synthesised inline notice; never sent by the ACP wire.
    case notice(String)
    case unknown(String)
}

extension SessionUpdate: Decodable {
    private enum CodingKeys: String, CodingKey {
        case sessionUpdate, content, entries, availableCommands, currentModeId, used, size
    }

    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        let discriminator = try container.decode(String.self, forKey: .sessionUpdate)
        switch discriminator {
        case "user_message_chunk":
            self = .userMessageChunk(try container.decode(ContentBlock.self, forKey: .content))
        case "agent_message_chunk":
            self = .agentMessageChunk(try container.decode(ContentBlock.self, forKey: .content))
        case "agent_thought_chunk":
            self = .agentThoughtChunk(try container.decode(ContentBlock.self, forKey: .content))
        case "tool_call":
            self = .toolCall(try ToolCall(from: decoder))
        case "tool_call_update":
            self = .toolCallUpdate(try ToolCallUpdate(from: decoder))
        case "plan":
            self = .plan(try container.decode([PlanEntry].self, forKey: .entries))
        case "available_commands_update":
            self = .availableCommandsUpdate(
                try container.decode([AvailableCommand].self, forKey: .availableCommands))
        case "current_mode_update":
            self = .currentModeUpdate(try container.decode(String.self, forKey: .currentModeId))
        case "usage_update":
            self = .usageUpdate(ContextUsage(
                used: try container.decode(Int.self, forKey: .used),
                size: try container.decode(Int.self, forKey: .size)))
        default:
            self = .unknown(discriminator)
        }
    }
}

/// Params of the `session/update` notification.
public struct SessionNotification: Sendable, Equatable, Decodable {
    public var sessionId: String
    public var update: SessionUpdate
}


public extension SessionUpdate {
    /// Locations trasportate dall'update; usate dal following per aprire il
    /// file toccato dall'agente nel right panel.
    var toolCallLocations: [ToolCallLocation] {
        switch self {
        case .toolCall(let call): call.locations
        case .toolCallUpdate(let update): update.locations ?? []
        default: []
        }
    }
}
