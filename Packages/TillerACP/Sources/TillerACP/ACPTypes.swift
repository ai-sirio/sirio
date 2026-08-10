import Foundation

// MARK: - initialize

public struct FileSystemCapability: Sendable, Equatable, Codable {
    public var readTextFile: Bool
    public var writeTextFile: Bool
    public init(readTextFile: Bool, writeTextFile: Bool) {
        self.readTextFile = readTextFile
        self.writeTextFile = writeTextFile
    }
}

public struct ClientCapabilities: Sendable, Equatable, Codable {
    public var fs: FileSystemCapability
    public var terminal: Bool
    /// Extension capabilities (claude-agent-acp reads
    /// `_meta.terminal_output` to stream Bash output into tool calls).
    public var meta: JSONValue?

    enum CodingKeys: String, CodingKey {
        case fs, terminal, meta = "_meta"
    }

    public init(fs: FileSystemCapability, terminal: Bool,
                meta: JSONValue? = nil) {
        self.fs = fs
        self.terminal = terminal
        self.meta = meta
    }
}

public struct PromptCapabilities: Sendable, Equatable, Codable {
    public var image: Bool
    public var audio: Bool
    public var embeddedContext: Bool

    public init(image: Bool = false, audio: Bool = false, embeddedContext: Bool = false) {
        self.image = image
        self.audio = audio
        self.embeddedContext = embeddedContext
    }

    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        image = try container.decodeIfPresent(Bool.self, forKey: .image) ?? false
        audio = try container.decodeIfPresent(Bool.self, forKey: .audio) ?? false
        embeddedContext = try container.decodeIfPresent(Bool.self, forKey: .embeddedContext) ?? false
    }
}

public struct AgentCapabilities: Sendable, Equatable, Codable {
    public var loadSession: Bool
    public var promptCapabilities: PromptCapabilities

    public init(loadSession: Bool = false, promptCapabilities: PromptCapabilities = .init()) {
        self.loadSession = loadSession
        self.promptCapabilities = promptCapabilities
    }

    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        loadSession = try container.decodeIfPresent(Bool.self, forKey: .loadSession) ?? false
        promptCapabilities = try container.decodeIfPresent(
            PromptCapabilities.self, forKey: .promptCapabilities) ?? .init()
    }
}

public struct AuthMethod: Sendable, Equatable, Codable {
    public var id: String
    public var name: String
    public var description: String?
}

public struct InitializeParams: Sendable, Equatable, Codable {
    public var protocolVersion: Int
    public var clientCapabilities: ClientCapabilities
    public init(protocolVersion: Int, clientCapabilities: ClientCapabilities) {
        self.protocolVersion = protocolVersion
        self.clientCapabilities = clientCapabilities
    }
}

public struct InitializeResult: Sendable, Equatable, Codable {
    public var protocolVersion: Int
    public var agentCapabilities: AgentCapabilities
    public var authMethods: [AuthMethod]

    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        protocolVersion = try container.decode(Int.self, forKey: .protocolVersion)
        agentCapabilities = try container.decodeIfPresent(
            AgentCapabilities.self, forKey: .agentCapabilities) ?? .init()
        authMethods = try container.decodeIfPresent([AuthMethod].self, forKey: .authMethods) ?? []
    }
}

// MARK: - sessions

public struct NewSessionParams: Sendable, Equatable, Codable {
    public var cwd: String
    public var mcpServers: [McpServerSpec]
    public init(cwd: String, mcpServers: [McpServerSpec] = []) {
        self.cwd = cwd
        self.mcpServers = mcpServers
    }
}

public struct SessionMode: Sendable, Equatable, Codable {
    public var id: String
    public var name: String
    public init(id: String, name: String) {
        self.id = id
        self.name = name
    }
}

public struct SessionModeState: Sendable, Equatable, Codable {
    public var currentModeId: String
    public var availableModes: [SessionMode]
    public init(currentModeId: String, availableModes: [SessionMode]) {
        self.currentModeId = currentModeId
        self.availableModes = availableModes
    }
}

public struct ModelInfo: Sendable, Equatable, Codable {
    public var modelId: String
    public var name: String
    public var description: String?
    /// Effort levels this model accepts, as reported by the agent. `nil`
    /// means the model reports no effort support and no picker is offered.
    public var supportedEffortLevels: [String]?

    public init(modelId: String, name: String, description: String? = nil,
                supportedEffortLevels: [String]? = nil) {
        self.modelId = modelId
        self.name = name
        self.description = description
        self.supportedEffortLevels = supportedEffortLevels
    }
}

public struct SessionModelState: Sendable, Equatable, Codable {
    public var currentModelId: String
    public var availableModels: [ModelInfo]
    public init(currentModelId: String, availableModels: [ModelInfo]) {
        self.currentModelId = currentModelId
        self.availableModels = availableModels
    }

    /// OpenCode doesn't send the standard `models` field; its `session/new`
    /// carries a `configOptions` select with id "model" instead. Map it to the
    /// standard shape so the rest of the app sees one model API.
    public init?(configOptions: [SessionConfigOption]) {
        guard let option = configOptions.first(where: { $0.id == "model" }),
              let current = option.currentValue,
              let choices = option.options, !choices.isEmpty else { return nil }
        self.init(currentModelId: current,
                  availableModels: choices.map {
                      ModelInfo(modelId: $0.value, name: $0.name)
                  })
    }
}

/// One entry of OpenCode's non-standard `configOptions` (model/effort/mode).
public struct SessionConfigOption: Sendable, Equatable, Codable {
    public struct Choice: Sendable, Equatable, Codable {
        public var value: String
        public var name: String
        public init(value: String, name: String) {
            self.value = value
            self.name = name
        }
    }
    public var id: String
    public var name: String?
    public var currentValue: String?
    public var options: [Choice]?
}

/// `session/set_config_option` (OpenCode extension); the result echoes the
/// full updated `configOptions` list.
public struct SetConfigOptionParams: Sendable, Equatable, Codable {
    public var sessionId: String
    public var configId: String
    public var value: String
    public init(sessionId: String, configId: String, value: String) {
        self.sessionId = sessionId
        self.configId = configId
        self.value = value
    }
}

public struct SetConfigOptionResult: Sendable, Equatable, Codable {
    public var configOptions: [SessionConfigOption]?
}

public struct NewSessionResult: Sendable, Equatable, Codable {
    public var sessionId: String
    public var modes: SessionModeState?
    public var models: SessionModelState?
    public var configOptions: [SessionConfigOption]?

    /// Standard `models` when present, else derived from OpenCode's
    /// `configOptions`; nil when the agent offers no model choice.
    public var resolvedModels: SessionModelState? {
        models ?? configOptions.flatMap(SessionModelState.init(configOptions:))
    }
}

public struct LoadSessionParams: Sendable, Equatable, Codable {
    public var sessionId: String
    public var cwd: String
    public var mcpServers: [McpServerSpec]
    public init(sessionId: String, cwd: String,
                mcpServers: [McpServerSpec] = []) {
        self.sessionId = sessionId
        self.cwd = cwd
        self.mcpServers = mcpServers
    }
}

/// `session/load` result: same optional modes/models payload as `session/new`.
public struct LoadSessionResult: Sendable, Equatable, Codable {
    /// claude-agent-acp may register the resumed conversation under a new id
    /// (SDK resume can fork); when present it must replace the requested one,
    /// or subsequent `session/prompt` calls hit "Session not found".
    public var sessionId: String?
    public var modes: SessionModeState?
    public var models: SessionModelState?
    public var configOptions: [SessionConfigOption]?

    public var resolvedModels: SessionModelState? {
        models ?? configOptions.flatMap(SessionModelState.init(configOptions:))
    }
}

// MARK: - prompting

public struct PromptParams: Sendable, Equatable, Codable {
    public var sessionId: String
    public var prompt: [ContentBlock]
    public init(sessionId: String, prompt: [ContentBlock]) {
        self.sessionId = sessionId
        self.prompt = prompt
    }
}

public enum StopReason: String, Sendable, Equatable, Codable {
    case endTurn = "end_turn"
    case maxTokens = "max_tokens"
    case maxTurnRequests = "max_turn_requests"
    case refusal
    case cancelled
}

public struct PromptResult: Sendable, Equatable, Codable {
    public var stopReason: StopReason
}

public struct CancelParams: Sendable, Equatable, Codable {
    public var sessionId: String
    public init(sessionId: String) { self.sessionId = sessionId }
}

public struct SetModeParams: Sendable, Equatable, Codable {
    public var sessionId: String
    public var modeId: String
    public init(sessionId: String, modeId: String) {
        self.sessionId = sessionId
        self.modeId = modeId
    }
}

/// `session/set_model` — accepted by both claude-code-acp and opencode
/// (verified empirically; both answer `{}`).
public struct SetModelParams: Sendable, Equatable, Codable {
    public var sessionId: String
    public var modelId: String
    public init(sessionId: String, modelId: String) {
        self.sessionId = sessionId
        self.modelId = modelId
    }
}
