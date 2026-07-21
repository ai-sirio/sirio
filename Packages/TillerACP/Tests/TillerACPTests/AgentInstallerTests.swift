import Foundation
import Testing
@testable import TillerACP

/// Shell fake: records commands and simulates their filesystem effects.
final class FakeShell: ShellRunning, @unchecked Sendable {
    var commands: [String] = []
    var effect: (@Sendable (String, URL) throws -> Void)?
    var exitCode: Int32 = 0

    func run(_ command: String, cwd: URL) async throws -> ShellResult {
        commands.append(command)
        try effect?(command, cwd)
        return ShellResult(exitCode: exitCode, output: "fake")
    }
}

@Suite struct AgentInstallerTests {
    func tempStore() throws -> AgentInstallStore {
        let dir = FileManager.default.temporaryDirectory
            .appendingPathComponent("installer-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        return AgentInstallStore(rootDirectory: dir)
    }

    func npxAgent() -> RegistryAgent {
        RegistryAgent(id: "claude-acp", name: "Claude Agent", version: "0.60.0",
                      description: nil, icon: nil,
                      distribution: AgentDistribution(
                        npx: .init(package: "@agentclientprotocol/claude-agent-acp@0.60.0",
                                   args: nil, env: ["K": "V"])))
    }

    @Test func npxInstallWritesManifestPointingIntoBin() async throws {
        let store = try tempStore()
        let shell = FakeShell()
        shell.effect = { command, _ in
            // Simulate `npm install --prefix <staging> pkg` creating .bin/.
            guard command.hasPrefix("npm install --prefix ") else { return }
            let staging = URL(fileURLWithPath: String(
                command.dropFirst("npm install --prefix ".count)
                    .split(separator: " ", maxSplits: 1)[0]))
            let bin = staging.appendingPathComponent("node_modules/.bin")
            try FileManager.default.createDirectory(at: bin, withIntermediateDirectories: true)
            FileManager.default.createFile(
                atPath: bin.appendingPathComponent("claude-agent-acp").path, contents: Data())
        }
        let installer = AgentInstaller(store: store, shell: shell,
                                       download: { _, _ in Issue.record("no download for npx") })
        let manifest = try await installer.install(npxAgent(), platform: .darwinArm64)
        #expect(manifest.version == "0.60.0")
        #expect(manifest.executable
                == store.agentDirectory(id: "claude-acp")
                    .appendingPathComponent("node_modules/.bin/claude-agent-acp").path)
        #expect(manifest.environment == ["K": "V"])
        #expect(store.manifest(id: "claude-acp") == manifest)
        #expect(FileManager.default.fileExists(atPath: manifest.executable))
    }

    @Test func npmFailureThrowsAndLeavesNothingInstalled() async throws {
        let store = try tempStore()
        let shell = FakeShell()
        shell.exitCode = 1
        let installer = AgentInstaller(store: store, shell: shell, download: { _, _ in })
        await #expect(throws: (any Error).self) {
            _ = try await installer.install(npxAgent(), platform: .darwinArm64)
        }
        #expect(store.manifest(id: "claude-acp") == nil)
    }

    @Test func binaryInstallExtractsAndChmods() async throws {
        let store = try tempStore()
        let agent = RegistryAgent(
            id: "amp-acp", name: "Amp", version: "0.8.1", description: nil, icon: nil,
            distribution: AgentDistribution(binary: [
                "darwin-aarch64": .init(
                    archive: URL(string: "https://example.com/amp.tar.gz")!,
                    cmd: "./amp-acp")]))
        let shell = FakeShell()
        shell.effect = { command, _ in
            // Simulate `tar -xzf <archive> -C <staging>` producing the binary.
            guard command.hasPrefix("tar ") || command.hasPrefix("chmod ") else { return }
            if command.hasPrefix("tar ") {
                let staging = URL(fileURLWithPath:
                    String(command.split(separator: " ").last!))
                FileManager.default.createFile(
                    atPath: staging.appendingPathComponent("amp-acp").path, contents: Data())
            }
        }
        let installer = AgentInstaller(store: store, shell: shell,
                                       download: { _, destination in
            FileManager.default.createFile(atPath: destination.path, contents: Data())
        })
        let manifest = try await installer.install(agent, platform: .darwinArm64)
        #expect(manifest.executable
                == store.agentDirectory(id: "amp-acp").appendingPathComponent("amp-acp").path)
        #expect(shell.commands.contains { $0.hasPrefix("chmod +x ") })
    }

    @Test func updateReplacesPreviousInstallAtomically() async throws {
        let store = try tempStore()
        // Pre-existing install with a sentinel file that must disappear.
        let oldDir = store.agentDirectory(id: "claude-acp")
        try FileManager.default.createDirectory(at: oldDir, withIntermediateDirectories: true)
        let sentinel = oldDir.appendingPathComponent("old-version-file")
        FileManager.default.createFile(atPath: sentinel.path, contents: Data())
        let shell = FakeShell()
        shell.effect = { command, _ in
            guard command.hasPrefix("npm install --prefix ") else { return }
            let staging = URL(fileURLWithPath: String(
                command.dropFirst("npm install --prefix ".count)
                    .split(separator: " ", maxSplits: 1)[0]))
            let bin = staging.appendingPathComponent("node_modules/.bin")
            try FileManager.default.createDirectory(at: bin, withIntermediateDirectories: true)
            FileManager.default.createFile(
                atPath: bin.appendingPathComponent("claude-agent-acp").path, contents: Data())
        }
        let installer = AgentInstaller(store: store, shell: shell, download: { _, _ in })
        _ = try await installer.install(npxAgent(), platform: .darwinArm64)
        #expect(!FileManager.default.fileExists(atPath: sentinel.path))
        #expect(store.manifest(id: "claude-acp")?.version == "0.60.0")
    }

    @Test func uvxOnlyAgentThrowsUnsupported() async throws {
        let store = try tempStore()
        let agent = RegistryAgent(id: "fast-agent", name: "fast-agent", version: "1",
                                  description: nil, icon: nil,
                                  distribution: AgentDistribution(hasUvx: true))
        let installer = AgentInstaller(store: store, shell: FakeShell(), download: { _, _ in })
        await #expect(throws: AgentInstallError.unsupported) {
            _ = try await installer.install(agent, platform: .darwinArm64)
        }
    }
}
