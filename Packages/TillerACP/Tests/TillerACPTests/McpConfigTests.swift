import Testing
import Foundation
@testable import TillerACP

@Suite struct McpConfigTests {
    private func writeConfig(_ json: String) throws -> String {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: root,
                                                withIntermediateDirectories: true)
        try json.data(using: .utf8)!
            .write(to: root.appendingPathComponent(".mcp.json"))
        return root.path
    }

    @Test func missingFileYieldsEmptyList() throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: root,
                                                withIntermediateDirectories: true)
        #expect(try McpConfig.load(worktreeRoot: root.path) == [])
    }

    @Test func parsesStdioServerWithEnv() throws {
        let root = try writeConfig(#"""
        {"mcpServers": {"tokensave": {"command": "npx",
          "args": ["-y", "tokensave-mcp"], "env": {"KEY": "V"}}}}
        """#)
        let specs = try McpConfig.load(worktreeRoot: root)
        #expect(specs == [McpServerSpec(
            name: "tokensave", command: "npx", args: ["-y", "tokensave-mcp"],
            env: [.init(name: "KEY", value: "V")])])
    }

    @Test func parsesHttpServerWithHeaders() throws {
        let root = try writeConfig(#"""
        {"mcpServers": {"docs": {"type": "http",
          "url": "https://example.com/mcp",
          "headers": {"Authorization": "Bearer x"}}}}
        """#)
        let specs = try McpConfig.load(worktreeRoot: root)
        #expect(specs == [McpServerSpec(
            type: "http", name: "docs", url: "https://example.com/mcp",
            headers: [.init(name: "Authorization", value: "Bearer x")])])
    }

    @Test func serversAreSortedByNameForDeterminism() throws {
        let root = try writeConfig(#"""
        {"mcpServers": {"zeta": {"command": "z"}, "alfa": {"command": "a"}}}
        """#)
        #expect(try McpConfig.load(worktreeRoot: root).map(\.name)
                == ["alfa", "zeta"])
    }

    @Test func malformedJSONThrows() throws {
        let root = try writeConfig("{not json")
        #expect(throws: (any Error).self) {
            try McpConfig.load(worktreeRoot: root)
        }
    }

    @Test func stdioEntryWithoutCommandThrows() throws {
        let root = try writeConfig(#"{"mcpServers": {"broken": {"args": ["x"]}}}"#)
        #expect(throws: (any Error).self) {
            try McpConfig.load(worktreeRoot: root)
        }
    }

    /// Il wire format ACP: env/headers come array {name,value}, niente
    /// chiavi null per i campi assenti (Codable sintetizzato usa
    /// encodeIfPresent — il test lo fissa contro regressioni).
    @Test func specEncodesToACPWireFormat() throws {
        let spec = McpServerSpec(name: "s", command: "cmd",
                                 env: [.init(name: "K", value: "V")])
        let data = try JSONEncoder().encode(spec)
        let object = try JSONSerialization.jsonObject(with: data) as? [String: Any]
        #expect(object?["name"] as? String == "s")
        #expect(object?["command"] as? String == "cmd")
        #expect((object?["env"] as? [[String: String]])?.first?["name"] == "K")
        #expect(object?["type"] == nil)
        #expect(object?["url"] == nil)
    }
}
