import Foundation
import Testing
import TillerACP

@testable import Tiller

@Suite
@MainActor
struct AcpAgentCenterTests {
    @Test func nativeIdsAreBuiltInWhenTheirBinariesResolve() async throws {
        let store = try tempStore()
        defer { try? FileManager.default.removeItem(at: store.rootDirectory) }

        let center = makeCenter(
            store: store,
            registryJSON: """
            {"version":"test","agents":[
              {"id":"external-acp","name":"External ACP","version":"1.0.0",
               "description":"External agent","icon":null,
               "distribution":{"npx":{"package":"external-acp"}}}
            ]}
            """,
            pathProbe: { _ in true })

        await center.refresh(force: true)

        for id in ["claude-acp", "codex-acp", "opencode"] {
            #expect(center.statuses[id] == .builtin(available: true))
        }
        #expect(Set(center.rows.map(\.id)).isSuperset(
            of: ["claude-acp", "codex-acp", "opencode"]))
        #expect(Set(center.installedAgents.map(\.id)).isSuperset(
            of: ["claude-acp", "codex-acp", "opencode"]))
    }

    @Test func missingNativeBinaryShowsRequiresPathHintAndSkipsInstallState() async throws {
        let store = try tempStore()
        defer { try? FileManager.default.removeItem(at: store.rootDirectory) }
        try store.write(InstalledAgentManifest(
            id: "codex-acp", version: "0.1.0", executable: "/old/codex-acp",
            arguments: [], environment: [:]))

        let center = makeCenter(
            store: store,
            registryJSON: """
            {"version":"test","agents":[
              {"id":"codex-acp","name":"Codex","version":"9.9.9",
               "description":"Registry description","icon":null,
               "distribution":{"npx":{"package":"codex-acp"}}}
            ]}
            """,
            pathProbe: { _ in false })

        await center.refresh(force: true)

        #expect(center.statuses["codex-acp"] == .builtin(available: false))
        #expect(center.rows.first(where: { $0.id == "codex-acp" })?.description
                == "Requires codex on PATH")
        #expect(!center.installedAgents.contains { $0.id == "codex-acp" })
    }

    @Test func availableNativeBinaryHidesManifestUpdateState() async throws {
        let store = try tempStore()
        defer { try? FileManager.default.removeItem(at: store.rootDirectory) }
        try store.write(InstalledAgentManifest(
            id: "claude-acp", version: "0.1.0", executable: "/old/claude-acp",
            arguments: [], environment: [:]))

        let center = makeCenter(
            store: store,
            registryJSON: """
            {"version":"test","agents":[
              {"id":"claude-acp","name":"Claude Code","version":"9.9.9",
               "description":"Registry description","icon":null,
               "distribution":{"npx":{"package":"claude-code-acp"}}}
            ]}
            """,
            pathProbe: { $0 == "claude" })

        await center.refresh(force: true)

        #expect(center.statuses["claude-acp"] == .builtin(available: true))
        #expect(center.installedAgents.contains { $0.id == "claude-acp" })
    }

    private func tempStore() throws -> AgentInstallStore {
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("acp-center-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: directory,
                                                withIntermediateDirectories: true)
        return AgentInstallStore(rootDirectory: directory)
    }

    private func makeCenter(
        store: AgentInstallStore,
        registryJSON: String,
        pathProbe: @escaping @Sendable (String) -> Bool
    ) -> AcpAgentCenter {
        let cache = store.rootDirectory.appendingPathComponent("registry.json")
        let client = AgentRegistryClient(cacheURL: cache, fetch: {
            Data(registryJSON.utf8)
        })
        return AcpAgentCenter(
            installStore: store,
            registryClient: client,
            shell: FailingShell(),
            pathProbe: pathProbe)
    }
}

private struct FailingShell: ShellRunning {
    func run(_ command: String, cwd: URL) async throws -> ShellResult {
        ShellResult(exitCode: 1, output: "")
    }
}
