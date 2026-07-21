import Foundation
import Testing
@testable import TillerACP

@Suite struct AgentLaunchSpecTests {

    @Test func launchEnvironmentStripsNestedClaudeMarkers() {
        let env = AgentLaunchSpec.launchEnvironment(base: [
            "PATH": "/usr/bin",
            "CLAUDECODE": "1",
            "CLAUDE_CODE_ENTRYPOINT": "cli",
            "HOME": "/Users/x",
        ])
        #expect(env["CLAUDECODE"] == nil)
        #expect(env["CLAUDE_CODE_ENTRYPOINT"] == nil)
        #expect(env["PATH"] == "/usr/bin")
        #expect(env["HOME"] == "/Users/x")
    }
}

@Suite struct AgentLaunchResolutionTests {
    func tempStore() throws -> AgentInstallStore {
        let dir = FileManager.default.temporaryDirectory
            .appendingPathComponent("launch-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        return AgentInstallStore(rootDirectory: dir)
    }

    @Test func legacyIdsCanonicalize() {
        #expect(AgentIdMigration.canonical("claude") == "claude-acp")
        #expect(AgentIdMigration.canonical("codex") == "codex-acp")
        #expect(AgentIdMigration.canonical("pi") == "pi-acp")
        #expect(AgentIdMigration.canonical("opencode") == "opencode")
        #expect(AgentIdMigration.canonical("claude-acp") == "claude-acp")
        #expect(AgentIdMigration.canonical("omp") == "omp")
    }

    @Test func ompIsBuiltIn() throws {
        let spec = AgentLaunchSpec.resolved(id: "omp", installStore: try tempStore())
        #expect(spec == AgentLaunchSpec(executable: "/bin/zsh",
                                        arguments: ["-lc", "exec omp acp"]))
    }

    @Test func installedManifestResolvesThroughLoginShell() throws {
        let store = try tempStore()
        try store.write(InstalledAgentManifest(
            id: "claude-acp", version: "0.60.0",
            executable: "/x/claude-agent-acp", arguments: ["--acp"],
            environment: ["K": "V"]))
        let spec = AgentLaunchSpec.resolved(id: "claude", installStore: store)  // legacy id
        #expect(spec?.executable == "/bin/zsh")
        #expect(spec?.arguments == ["-lc", "exec '/x/claude-agent-acp' '--acp'"])
        #expect(spec?.environment == ["K": "V"])
    }

    @Test func manifestPathsWithSpacesAreQuoted() throws {
        let store = try tempStore()
        try store.write(InstalledAgentManifest(
            id: "opencode", version: "1.18.4",
            executable: "/Application Support/Tiller/acp-agents/opencode/opencode",
            arguments: ["acp"], environment: [:]))
        let spec = AgentLaunchSpec.resolved(id: "opencode", installStore: store)
        #expect(spec?.arguments ==
                ["-lc",
                 "exec '/Application Support/Tiller/acp-agents/opencode/opencode' 'acp'"])
    }

    @Test func notInstalledResolvesNil() throws {
        #expect(AgentLaunchSpec.resolved(id: "codex-acp",
                                         installStore: try tempStore()) == nil)
    }

    @Test func launchEnvironmentMergesManifestEnv() {
        let env = AgentLaunchSpec.launchEnvironment(
            base: ["PATH": "/usr/bin", "CLAUDECODE": "1"],
            extra: ["AUGMENT_DISABLE_AUTO_UPDATE": "1"])
        #expect(env["CLAUDECODE"] == nil)
        #expect(env["AUGMENT_DISABLE_AUTO_UPDATE"] == "1")
        #expect(env["PATH"] == "/usr/bin")
    }
}
