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

    @Test func canonicalIdsMapBackToCatalogShortIds() {
        #expect(AgentIdMigration.catalogId("claude-acp") == "claude")
        #expect(AgentIdMigration.catalogId("codex-acp") == "codex")
        #expect(AgentIdMigration.catalogId("pi-acp") == "pi")
        #expect(AgentIdMigration.catalogId("opencode") == "opencode")
        #expect(AgentIdMigration.catalogId("omp") == "omp")
        #expect(AgentIdMigration.catalogId("claude") == "claude")
    }

    @Test func legacyIdsCanonicalize() {
        #expect(AgentIdMigration.canonical("claude") == "claude")
        #expect(AgentIdMigration.canonical("codex") == "codex")
        #expect(AgentIdMigration.canonical("pi") == "pi")
        #expect(AgentIdMigration.canonical("pi-acp") == "pi")
        #expect(AgentIdMigration.canonical("opencode") == "opencode")
        #expect(AgentIdMigration.canonical("claude-acp") == "claude")
        #expect(AgentIdMigration.canonical("codex-acp") == "codex")
        #expect(AgentIdMigration.canonical("omp") == "omp")
    }

    @Test func ompIsBuiltIn() throws {
        let spec = AgentLaunchSpec.resolved(id: "omp", installStore: try tempStore())
        #expect(spec == AgentLaunchSpec(executable: "/bin/zsh",
                                        arguments: ["-lc", "exec omp acp"]))
    }

    @Test func legacyPiACPDoesNotResolveThroughACPManifest() throws {
        let store = try tempStore()
        try store.write(InstalledAgentManifest(
            id: "pi-acp", version: "0.60.0",
            executable: "/x/pi-acp", arguments: ["--acp"],
            environment: ["K": "V"]))
        #expect(AgentLaunchSpec.resolved(id: "pi-acp", installStore: store) == nil)
    }

    @Test func manifestPathsWithSpacesAreQuoted() throws {
        let store = try tempStore()
        try store.write(InstalledAgentManifest(
            id: "pi-acp", version: "1.18.4",
            executable: "/Application Support/Tiller/acp-agents/pi-acp/pi-acp",
            arguments: ["acp"], environment: [:]))
        let spec = AgentLaunchSpec.resolved(id: "pi-acp", installStore: store)
        #expect(spec == nil)
    }

    @Test func nativeIdsDoNotResolveThroughACPManifests() throws {
        let store = try tempStore()
        for id in ["claude-acp", "codex-acp", "opencode", "pi", "pi-acp"] {
            try store.write(InstalledAgentManifest(
                id: id, version: "1.0.0", executable: "/x/\(id)",
                arguments: [], environment: [:]))
            #expect(AgentLaunchSpec.resolved(id: id, installStore: store) == nil)
        }
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
