import Foundation
import Testing
@testable import TillerACP

@Suite struct AgentLaunchSpecTests {
    @Test func claudeCodeRunsPinnedAdapterThroughLoginShell() {
        let spec = AgentLaunchSpec.claudeCode()
        #expect(spec.executable == "/bin/zsh")
        #expect(spec.arguments.count == 2)
        #expect(spec.arguments[0] == "-lc")
        #expect(spec.arguments[1] ==
            "exec npx -y @agentclientprotocol/claude-agent-acp@\(AgentLaunchSpec.claudeCodeACPVersion)")
    }

    @Test func pinnedVersionLooksLikeSemver() {
        let parts = AgentLaunchSpec.claudeCodeACPVersion.split(separator: ".")
        #expect(parts.count == 3)
        #expect(parts.allSatisfy { Int($0) != nil })
    }

    @Test func openCodeRunsNativeACPThroughLoginShell() {
        let spec = AgentLaunchSpec.openCode()
        #expect(spec.executable == "/bin/zsh")
        #expect(spec.arguments == ["-lc", "exec opencode acp"])
    }

    @Test func forAgentMapsSupportedIdsAndRejectsOthers() {
        #expect(AgentLaunchSpec.forAgent(id: "claude") == .claudeCode())
        #expect(AgentLaunchSpec.forAgent(id: "opencode") == .openCode())
        #expect(AgentLaunchSpec.forAgent(id: "codex") == nil)
        #expect(AgentLaunchSpec.forAgent(id: "pi") == nil)
        #expect(AgentLaunchSpec.forAgent(id: "omp") == nil)
    }

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

    @Test func installedManifestResolves() throws {
        let store = try tempStore()
        try store.write(InstalledAgentManifest(
            id: "claude-acp", version: "0.60.0",
            executable: "/x/claude-agent-acp", arguments: ["--acp"],
            environment: ["K": "V"]))
        let spec = AgentLaunchSpec.resolved(id: "claude", installStore: store)  // legacy id
        #expect(spec?.executable == "/x/claude-agent-acp")
        #expect(spec?.arguments == ["--acp"])
        #expect(spec?.environment == ["K": "V"])
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
