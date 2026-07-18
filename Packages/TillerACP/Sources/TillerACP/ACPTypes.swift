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
    public init(fs: FileSystemCapability, terminal: Bool) {
        self.fs = fs
        self.terminal = terminal
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
    public var mcpServers: [JSONValue]
    public init(cwd: String, mcpServers: [JSONValue] = []) {
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

public struct NewSessionResult: Sendable, Equatable, Codable {
    public var sessionId: String
    public var modes: SessionModeState?
}

public struct LoadSessionParams: Sendable, Equatable, Codable {
    public var sessionId: String
    public var cwd: String
    public var mcpServers: [JSONValue]
    public init(sessionId: String, cwd: String, mcpServers: [JSONValue] = []) {
        self.sessionId = sessionId
        self.cwd = cwd
        self.mcpServers = mcpServers
    }
}

/// `session/load` result: same optional modes payload as `session/new`.
public struct LoadSessionResult: Sendable, Equatable, Codable {
    public var modes: SessionModeState?
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
