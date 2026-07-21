import Foundation

/// The official ACP agent registry document
/// (https://cdn.agentclientprotocol.com/registry/v1/latest/registry.json).
public struct ACPRegistry: Sendable, Equatable, Decodable {
    public var version: String
    public var agents: [RegistryAgent]
}

public struct RegistryAgent: Sendable, Equatable, Decodable, Identifiable {
    public var id: String
    public var name: String
    public var version: String
    public var description: String?
    public var icon: URL?
    public var distribution: AgentDistribution
}

/// The `distribution` object of a registry entry. `uvx` is parsed only as a
/// presence flag: it is unsupported in v1 and shown as such in Settings.
public struct AgentDistribution: Sendable, Equatable, Decodable {
    public struct Npx: Sendable, Equatable, Decodable {
        public var package: String
        public var args: [String]?
        public var env: [String: String]?
    }

    public struct BinaryPlatform: Sendable, Equatable, Decodable {
        public var archive: URL
        public var cmd: String
    }

    public var npx: Npx?
    public var binary: [String: BinaryPlatform]?
    public var hasUvx: Bool

    private enum CodingKeys: String, CodingKey { case npx, binary, uvx }

    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        npx = try container.decodeIfPresent(Npx.self, forKey: .npx)
        binary = try container.decodeIfPresent(
            [String: BinaryPlatform].self, forKey: .binary)
        hasUvx = container.contains(.uvx)
    }

    public init(npx: Npx? = nil, binary: [String: BinaryPlatform]? = nil,
                hasUvx: Bool = false) {
        self.npx = npx
        self.binary = binary
        self.hasUvx = hasUvx
    }
}
