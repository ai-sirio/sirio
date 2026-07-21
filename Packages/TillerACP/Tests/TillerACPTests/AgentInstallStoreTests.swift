import Foundation
import Testing
@testable import TillerACP

@Suite struct AgentInstallStoreTests {
    func tempStore() throws -> AgentInstallStore {
        let dir = FileManager.default.temporaryDirectory
            .appendingPathComponent("acp-agents-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        return AgentInstallStore(rootDirectory: dir)
    }

    @Test func npxDistributionResolvesRegardlessOfPlatform() {
        let agent = RegistryAgent(
            id: "x", name: "X", version: "1.0.0", description: nil, icon: nil,
            distribution: AgentDistribution(
                npx: .init(package: "pkg@1.0.0", args: ["--acp"], env: ["K": "V"])))
        #expect(agent.installMethod(platform: .darwinArm64)
                == .npx(package: "pkg@1.0.0", args: ["--acp"], env: ["K": "V"]))
    }

    @Test func binaryDistributionPicksCurrentPlatform() {
        let arm = URL(string: "https://example.com/arm.tar.gz")!
        let agent = RegistryAgent(
            id: "x", name: "X", version: "1.0.0", description: nil, icon: nil,
            distribution: AgentDistribution(binary: [
                "darwin-aarch64": .init(archive: arm, cmd: "./x")]))
        #expect(agent.installMethod(platform: .darwinArm64)
                == .binary(archive: arm, cmd: "./x", args: [], env: [:]))
        #expect(agent.installMethod(platform: .darwinX86) == nil)
    }

    @Test func uvxOnlyAgentHasNoInstallMethod() {
        let agent = RegistryAgent(
            id: "x", name: "X", version: "1.0.0", description: nil, icon: nil,
            distribution: AgentDistribution(hasUvx: true))
        #expect(agent.installMethod(platform: .darwinArm64) == nil)
    }

    @Test func manifestRoundTripsThroughStore() throws {
        let store = try tempStore()
        let manifest = InstalledAgentManifest(
            id: "claude-acp", version: "0.60.0",
            executable: "/tmp/bin/claude-agent-acp", arguments: [],
            environment: [:])
        try store.write(manifest)
        #expect(store.manifest(id: "claude-acp") == manifest)
        #expect(store.installedManifests() == [manifest])
    }

    @Test func missingManifestReadsAsNil() throws {
        let store = try tempStore()
        #expect(store.manifest(id: "nope") == nil)
        #expect(store.installedManifests().isEmpty)
    }

    @Test func statusResolution() {
        let installed = InstalledAgentManifest(
            id: "a", version: "1.0.0", executable: "/x", arguments: [], environment: [:])
        #expect(AgentInstallStatus.resolve(manifest: nil, latestVersion: "1.0.0",
                                           isSupported: false) == .unsupported)
        #expect(AgentInstallStatus.resolve(manifest: nil, latestVersion: "1.0.0",
                                           isSupported: true) == .notInstalled)
        #expect(AgentInstallStatus.resolve(manifest: installed, latestVersion: "1.0.0",
                                           isSupported: true) == .installed(version: "1.0.0"))
        #expect(AgentInstallStatus.resolve(manifest: installed, latestVersion: "2.0.0",
                                           isSupported: true)
                == .updateAvailable(installed: "1.0.0", latest: "2.0.0"))
    }
}
