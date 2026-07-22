import Foundation

/// A decoded line from Claude Code's `stream-json` protocol.
public enum ClaudeWireMessage: Decodable, Sendable, Equatable {
    case systemInit(ClaudeInit)
    case assistant(ClaudeAssistantMessage)
    case user(ClaudeUserMessage)
    case result(ClaudeResult)
    case controlRequest(id: String, ClaudeControlRequest)
    case controlResponse(id: String, JSONValue?)
    case unknown(String)

    public init(from decoder: Decoder) throws {
        let value = try JSONValue(from: decoder)
        let type = value["type"]?.stringValue

        switch type {
        case "system" where value["subtype"]?.stringValue == "init":
            self = .systemInit(try value.decoded(ClaudeInit.self))
        case "assistant":
            self = .assistant(try value.decoded(ClaudeAssistantMessage.self))
        case "user":
            self = .user(try value.decoded(ClaudeUserMessage.self))
        case "result":
            self = .result(try value.decoded(ClaudeResult.self))
        case "control_request":
            let id = value["request_id"]?.stringValue
                ?? value["id"]?.stringValue
                ?? ""
            let request = value["request"] ?? value
            self = .controlRequest(id: id, try request.decoded(ClaudeControlRequest.self))
        case "control_response":
            let id = value["request_id"]?.stringValue
                ?? value["id"]?.stringValue
                ?? value["response"]?["request_id"]?.stringValue
                ?? ""
            self = .controlResponse(id: id, value["response"])
        default:
            self = .unknown(type ?? "")
        }
    }
}

public struct ClaudeInit: Decodable, Sendable, Equatable {
    public let sessionId: String
    public let model: String
    public let tools: [String]
    public let slashCommands: [String]

    enum CodingKeys: String, CodingKey {
        case sessionId = "session_id"
        case model
        case tools
        case slashCommands = "slash_commands"
    }

    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        sessionId = try container.decodeIfPresent(String.self, forKey: .sessionId) ?? ""
        model = try container.decodeIfPresent(String.self, forKey: .model) ?? ""
        tools = try container.decodeIfPresent([String].self, forKey: .tools) ?? []
        slashCommands = try container.decodeIfPresent([String].self, forKey: .slashCommands) ?? []
    }
}

public struct ClaudeAssistantMessage: Decodable, Sendable, Equatable {
    public let requestId: String?
    public let sessionId: String
    public let message: ClaudeMessage
    public let parentToolUseId: String?
    public let uuid: String?
    public let timestamp: String?

    enum CodingKeys: String, CodingKey {
        case requestId = "request_id"
        case sessionId = "session_id"
        case message
        case parentToolUseId = "parent_tool_use_id"
        case uuid
        case timestamp
    }

    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        requestId = try container.decodeIfPresent(String.self, forKey: .requestId)
        sessionId = try container.decodeIfPresent(String.self, forKey: .sessionId) ?? ""
        message = try container.decodeIfPresent(ClaudeMessage.self, forKey: .message) ?? .empty
        parentToolUseId = try container.decodeIfPresent(String.self, forKey: .parentToolUseId)
        uuid = try container.decodeIfPresent(String.self, forKey: .uuid)
        timestamp = try container.decodeIfPresent(String.self, forKey: .timestamp)
    }
}

public struct ClaudeUserMessage: Decodable, Sendable, Equatable {
    public let requestId: String?
    public let sessionId: String
    public let message: ClaudeMessage
    public let parentToolUseId: String?
    public let uuid: String?
    public let timestamp: String?
    public let toolUseResult: JSONValue?

    enum CodingKeys: String, CodingKey {
        case requestId = "request_id"
        case sessionId = "session_id"
        case message
        case parentToolUseId = "parent_tool_use_id"
        case uuid
        case timestamp
        case toolUseResult = "tool_use_result"
    }

    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        requestId = try container.decodeIfPresent(String.self, forKey: .requestId)
        sessionId = try container.decodeIfPresent(String.self, forKey: .sessionId) ?? ""
        message = try container.decodeIfPresent(ClaudeMessage.self, forKey: .message) ?? .empty
        parentToolUseId = try container.decodeIfPresent(String.self, forKey: .parentToolUseId)
        uuid = try container.decodeIfPresent(String.self, forKey: .uuid)
        timestamp = try container.decodeIfPresent(String.self, forKey: .timestamp)
        toolUseResult = try container.decodeIfPresent(JSONValue.self, forKey: .toolUseResult)
    }
}

public struct ClaudeMessage: Decodable, Sendable, Equatable {
    public let model: String?
    public let id: String?
    public let type: String?
    public let role: String?
    public let content: [ClaudeContentBlock]
    public let stopReason: String?
    public let stopSequence: String?
    public let stopDetails: JSONValue?
    public let usage: ClaudeUsage?

    enum CodingKeys: String, CodingKey {
        case model
        case id
        case type
        case role
        case content
        case stopReason = "stop_reason"
        case stopSequence = "stop_sequence"
        case stopDetails = "stop_details"
        case usage
    }

    static let empty = ClaudeMessage(
        model: nil, id: nil, type: nil, role: nil, content: [],
        stopReason: nil, stopSequence: nil, stopDetails: nil, usage: nil)

    private init(model: String?, id: String?, type: String?, role: String?,
                 content: [ClaudeContentBlock], stopReason: String?,
                 stopSequence: String?, stopDetails: JSONValue?, usage: ClaudeUsage?) {
        self.model = model
        self.id = id
        self.type = type
        self.role = role
        self.content = content
        self.stopReason = stopReason
        self.stopSequence = stopSequence
        self.stopDetails = stopDetails
        self.usage = usage
    }

    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        model = try container.decodeIfPresent(String.self, forKey: .model)
        id = try container.decodeIfPresent(String.self, forKey: .id)
        type = try container.decodeIfPresent(String.self, forKey: .type)
        role = try container.decodeIfPresent(String.self, forKey: .role)
        content = try container.decodeIfPresent([ClaudeContentBlock].self, forKey: .content) ?? []
        stopReason = try container.decodeIfPresent(String.self, forKey: .stopReason)
        stopSequence = try container.decodeIfPresent(String.self, forKey: .stopSequence)
        stopDetails = try container.decodeIfPresent(JSONValue.self, forKey: .stopDetails)
        usage = try container.decodeIfPresent(ClaudeUsage.self, forKey: .usage)
    }
}

public enum ClaudeContentBlock: Decodable, Sendable, Equatable {
    case text(String)
    case thinking(String)
    case toolUse(ClaudeToolUse)
    case toolResult(ClaudeToolResult)
    case unknown(String)

    enum CodingKeys: String, CodingKey {
        case type
        case text
        case thinking
    }

    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        let type = try container.decodeIfPresent(String.self, forKey: .type) ?? ""
        switch type {
        case "text":
            self = .text(try container.decodeIfPresent(String.self, forKey: .text) ?? "")
        case "thinking":
            self = .thinking(try container.decodeIfPresent(String.self, forKey: .thinking) ?? "")
        case "tool_use":
            self = .toolUse(try ClaudeToolUse(from: decoder))
        case "tool_result":
            self = .toolResult(try ClaudeToolResult(from: decoder))
        default:
            self = .unknown(type)
        }
    }
}

public struct ClaudeToolUse: Decodable, Sendable, Equatable {
    public let id: String
    public let name: String
    public let input: JSONValue?
    public let caller: JSONValue?

    enum CodingKeys: String, CodingKey {
        case id
        case name
        case input
        case caller
    }

    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        id = try container.decodeIfPresent(String.self, forKey: .id) ?? ""
        name = try container.decodeIfPresent(String.self, forKey: .name) ?? ""
        input = try container.decodeIfPresent(JSONValue.self, forKey: .input)
        caller = try container.decodeIfPresent(JSONValue.self, forKey: .caller)
    }
}

public struct ClaudeToolResult: Decodable, Sendable, Equatable {
    public let toolUseId: String
    public let content: JSONValue?
    public let isError: Bool

    enum CodingKeys: String, CodingKey {
        case toolUseId = "tool_use_id"
        case content
        case isError = "is_error"
    }

    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        toolUseId = try container.decodeIfPresent(String.self, forKey: .toolUseId) ?? ""
        content = try container.decodeIfPresent(JSONValue.self, forKey: .content)
        isError = try container.decodeIfPresent(Bool.self, forKey: .isError) ?? false
    }
}

public struct ClaudeUsage: Decodable, Sendable, Equatable {
    public let inputTokens: Int?
    public let cacheCreationInputTokens: Int?
    public let cacheReadInputTokens: Int?
    public let outputTokens: Int?
    public let serverToolUse: JSONValue?
    public let serviceTier: String?
    public let cacheCreation: JSONValue?
    public let inferenceGeo: String?
    public let iterations: [JSONValue]?
    public let speed: String?

    enum CodingKeys: String, CodingKey {
        case inputTokens = "input_tokens"
        case cacheCreationInputTokens = "cache_creation_input_tokens"
        case cacheReadInputTokens = "cache_read_input_tokens"
        case outputTokens = "output_tokens"
        case serverToolUse = "server_tool_use"
        case serviceTier = "service_tier"
        case cacheCreation = "cache_creation"
        case inferenceGeo = "inference_geo"
        case iterations
        case speed
    }

    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        inputTokens = try container.decodeIfPresent(Int.self, forKey: .inputTokens)
        cacheCreationInputTokens = try container.decodeIfPresent(Int.self, forKey: .cacheCreationInputTokens)
        cacheReadInputTokens = try container.decodeIfPresent(Int.self, forKey: .cacheReadInputTokens)
        outputTokens = try container.decodeIfPresent(Int.self, forKey: .outputTokens)
        serverToolUse = try container.decodeIfPresent(JSONValue.self, forKey: .serverToolUse)
        serviceTier = try container.decodeIfPresent(String.self, forKey: .serviceTier)
        cacheCreation = try container.decodeIfPresent(JSONValue.self, forKey: .cacheCreation)
        inferenceGeo = try container.decodeIfPresent(String.self, forKey: .inferenceGeo)
        iterations = try container.decodeIfPresent([JSONValue].self, forKey: .iterations)
        speed = try container.decodeIfPresent(String.self, forKey: .speed)
    }
}

public struct ClaudeResult: Decodable, Sendable, Equatable {
    public let sessionId: String
    public let usage: ClaudeUsage?
    public let isError: Bool

    enum CodingKeys: String, CodingKey {
        case sessionId = "session_id"
        case usage
        case isError = "is_error"
    }

    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        sessionId = try container.decodeIfPresent(String.self, forKey: .sessionId) ?? ""
        usage = try container.decodeIfPresent(ClaudeUsage.self, forKey: .usage)
        isError = try container.decodeIfPresent(Bool.self, forKey: .isError) ?? false
    }
}

public struct ClaudeControlRequest: Decodable, Sendable, Equatable {
    public let subtype: String
    public let toolName: String?
    public let input: JSONValue?
    public let permissionSuggestions: JSONValue?

    enum CodingKeys: String, CodingKey {
        case subtype
        case toolName = "tool_name"
        case input
        case permissionSuggestions = "permission_suggestions"
    }

    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        subtype = try container.decodeIfPresent(String.self, forKey: .subtype) ?? ""
        toolName = try container.decodeIfPresent(String.self, forKey: .toolName)
        input = try container.decodeIfPresent(JSONValue.self, forKey: .input)
        permissionSuggestions = try container.decodeIfPresent(
            JSONValue.self, forKey: .permissionSuggestions)
    }
}
