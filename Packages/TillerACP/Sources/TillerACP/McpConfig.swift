import Foundation

/// One MCP server in the ACP wire format `session/new`/`session/load`
/// expect: stdio entries carry command/args/env, http/sse entries carry
/// type/url/headers. Optional fields are omitted when nil (synthesized
/// Codable uses encodeIfPresent), which is what claude-agent-acp parses.
public struct McpServerSpec: Sendable, Equatable, Codable {
    public struct NamedValue: Sendable, Equatable, Codable {
        public var name: String
        public var value: String
        public init(name: String, value: String) {
            self.name = name
            self.value = value
        }
    }

    public var type: String?
    public var name: String
    public var command: String?
    public var args: [String]?
    public var env: [NamedValue]?
    public var url: String?
    public var headers: [NamedValue]?

    public init(type: String? = nil, name: String, command: String? = nil,
                args: [String]? = nil, env: [NamedValue]? = nil,
                url: String? = nil, headers: [NamedValue]? = nil) {
        self.type = type
        self.name = name
        self.command = command
        self.args = args
        self.env = env
        self.url = url
        self.headers = headers
    }
}

public enum McpConfigError: Error, Equatable {
    /// Entry missing the fields its type requires (stdio without command,
    /// http/sse without url).
    case invalidEntry(name: String)
}

/// Reads the project-level MCP configuration (`<worktree>/.mcp.json`,
/// Claude Code convention) into ACP wire-format specs. Missing file means
/// "no MCP servers"; a malformed file or entry throws so the caller can
/// surface a warning without blocking the session.
public enum McpConfig {
    private struct File: Decodable {
        var mcpServers: [String: Entry]?
    }
    private struct Entry: Decodable {
        var type: String?
        var command: String?
        var args: [String]?
        var env: [String: String]?
        var url: String?
        var headers: [String: String]?
    }

    public static func load(worktreeRoot: String) throws -> [McpServerSpec] {
        let path = (worktreeRoot as NSString).appendingPathComponent(".mcp.json")
        guard FileManager.default.fileExists(atPath: path) else { return [] }
        let data = try Data(contentsOf: URL(fileURLWithPath: path))
        let file = try JSONDecoder().decode(File.self, from: data)
        return try (file.mcpServers ?? [:])
            .sorted { $0.key < $1.key }
            .map { name, entry in try spec(name: name, entry: entry) }
    }

    private static func spec(name: String, entry: Entry) throws -> McpServerSpec {
        func namedValues(_ dict: [String: String]?) -> [McpServerSpec.NamedValue]? {
            guard let dict, !dict.isEmpty else { return nil }
            return dict.sorted { $0.key < $1.key }
                .map { .init(name: $0.key, value: $0.value) }
        }
        if entry.type == "http" || entry.type == "sse" {
            guard let url = entry.url else {
                throw McpConfigError.invalidEntry(name: name)
            }
            return McpServerSpec(type: entry.type, name: name, url: url,
                                 headers: namedValues(entry.headers))
        }
        guard let command = entry.command else {
            throw McpConfigError.invalidEntry(name: name)
        }
        return McpServerSpec(name: name, command: command, args: entry.args,
                             env: namedValues(entry.env))
    }
}
