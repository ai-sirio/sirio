import Foundation
import Testing
@testable import TillerACP

@Suite struct AgentRegistryModelsTests {
    static let fixture = Data("""
    {
      "version": "1.0.0",
      "agents": [
        {"id": "claude-acp", "name": "Claude Agent", "version": "0.60.0",
         "description": "ACP wrapper for Anthropic's Claude",
         "icon": "https://cdn.agentclientprotocol.com/registry/v1/latest/claude-acp.svg",
         "distribution": {"npx": {"package": "@agentclientprotocol/claude-agent-acp@0.60.0"}}},
        {"id": "auggie", "name": "Auggie CLI", "version": "0.33.0",
         "distribution": {"npx": {"package": "@augmentcode/auggie@0.33.0",
                                  "args": ["--acp"],
                                  "env": {"AUGMENT_DISABLE_AUTO_UPDATE": "1"}}}},
        {"id": "amp-acp", "name": "Amp", "version": "0.8.1",
         "distribution": {"binary": {
            "darwin-aarch64": {"archive": "https://example.com/amp-arm.tar.gz", "cmd": "./amp-acp"},
            "darwin-x86_64": {"archive": "https://example.com/amp-x64.tar.gz", "cmd": "./amp-acp"}}}},
        {"id": "opencode", "name": "OpenCode", "version": "1.18.4",
         "distribution": {"binary": {
            "darwin-aarch64": {"archive": "https://example.com/opencode.zip", "cmd": "./opencode",
                               "args": ["acp"], "env": {"OC_FLAG": "1"}}}}},
        {"id": "fast-agent", "name": "fast-agent", "version": "0.9.19",
         "distribution": {"uvx": {"package": "fast-agent-mcp@0.9.19"}}}
      ]
    }
    """.utf8)

    @Test func decodesRealRegistryShape() throws {
        let registry = try JSONDecoder().decode(ACPRegistry.self, from: Self.fixture)
        #expect(registry.agents.count == 5)
        let claude = registry.agents[0]
        #expect(claude.id == "claude-acp")
        #expect(claude.distribution.npx?.package == "@agentclientprotocol/claude-agent-acp@0.60.0")
        #expect(claude.distribution.npx?.args == nil)
        #expect(claude.icon != nil)
    }

    @Test func decodesNpxArgsAndEnv() throws {
        let registry = try JSONDecoder().decode(ACPRegistry.self, from: Self.fixture)
        let auggie = registry.agents[1]
        #expect(auggie.distribution.npx?.args == ["--acp"])
        #expect(auggie.distribution.npx?.env == ["AUGMENT_DISABLE_AUTO_UPDATE": "1"])
    }

    @Test func decodesBinaryPlatformMap() throws {
        let registry = try JSONDecoder().decode(ACPRegistry.self, from: Self.fixture)
        let amp = registry.agents[2]
        #expect(amp.distribution.binary?["darwin-aarch64"]?.cmd == "./amp-acp")
        #expect(amp.distribution.binary?["darwin-aarch64"]?.archive
                == URL(string: "https://example.com/amp-arm.tar.gz"))
    }

    @Test func decodesBinaryArgsAndEnv() throws {
        let registry = try JSONDecoder().decode(ACPRegistry.self, from: Self.fixture)
        let amp = registry.agents[2]
        #expect(amp.distribution.binary?["darwin-aarch64"]?.args == nil)
        let opencode = registry.agents[3]
        #expect(opencode.distribution.binary?["darwin-aarch64"]?.args == ["acp"])
        #expect(opencode.distribution.binary?["darwin-aarch64"]?.env == ["OC_FLAG": "1"])
    }

    @Test func uvxOnlyAgentIsFlagged() throws {
        let registry = try JSONDecoder().decode(ACPRegistry.self, from: Self.fixture)
        let fast = registry.agents[4]
        #expect(fast.distribution.hasUvx)
        #expect(fast.distribution.npx == nil)
        #expect(fast.distribution.binary == nil)
    }
}
