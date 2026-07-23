import Foundation
import Testing
@testable import TillerACP

@Suite struct AgentDriverFactoryTests {
    private func tempStore() throws -> AgentInstallStore {
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("agent-driver-factory-\(UUID().uuidString)")
        try FileManager.default.createDirectory(
            at: directory, withIntermediateDirectories: true)
        return AgentInstallStore(rootDirectory: directory)
    }

    private func makeDriver(
        agentId: String,
        store: AgentInstallStore,
        pathAvailable: Bool = true
    ) -> (any AgentDriver)? {
        AgentDriverFactory.makeDriver(
            agentId: agentId, worktreePath: "/tmp",
            installStore: store, permissionMode: .ask,
            model: nil, effort: nil, resumeSessionId: nil,
            pathProbe: { _ in pathAvailable })
    }

    @Test func transportKindRoutesNativeAndACPIds() {
        #expect(AgentDriverFactory.transportKind(for: "claude-acp") == .native)
        #expect(AgentDriverFactory.transportKind(for: "codex-acp") == .native)
        #expect(AgentDriverFactory.transportKind(for: "opencode") == .native)
        #expect(AgentDriverFactory.transportKind(for: "omp") == .acp)
        #expect(AgentDriverFactory.transportKind(for: "pi-acp") == .acp)
        #expect(AgentDriverFactory.transportKind(for: "gemini") == .acp)
    }

    @Test func nativeBinaryUsesCanonicalNativeNames() {
        #expect(AgentDriverFactory.nativeBinary(for: "claude-acp") == "claude")
        #expect(AgentDriverFactory.nativeBinary(for: "codex") == "codex")
        #expect(AgentDriverFactory.nativeBinary(for: "opencode") == "opencode")
    }

    @Test func claudeFactoryReturnsNativeDriverWhenCLIIsAvailable() throws {
        let driver = makeDriver(agentId: "claude-acp", store: try tempStore())
        #expect(driver is ClaudeStreamJSONDriver)
    }

    @Test func ompFactoryReturnsACPSessionWithInstalledManifest() throws {
        let store = try tempStore()
        try store.write(InstalledAgentManifest(
            id: "omp", version: "1.0.0", executable: "/usr/local/bin/omp",
            arguments: ["acp"], environment: [:]))

        let driver = makeDriver(agentId: "omp", store: store)
        #expect(driver is ACPSession)
    }

    @Test func unknownUninstalledAgentReturnsNil() throws {
        let driver = makeDriver(
            agentId: "unknown-agent", store: try tempStore(), pathAvailable: false)
        #expect(driver == nil)
    }

    @Test func nativeCLIUnavailableReturnsNilWithoutUsingInstallManifest() throws {
        let store = try tempStore()
        try store.write(InstalledAgentManifest(
            id: "claude-acp", version: "1.0.0", executable: "/tmp/claude",
            arguments: [], environment: [:]))

        let driver = makeDriver(
            agentId: "claude-acp", store: store, pathAvailable: false)
        #expect(driver == nil)
    }
}
