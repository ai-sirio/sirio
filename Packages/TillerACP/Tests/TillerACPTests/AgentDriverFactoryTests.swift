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
        #expect(AgentDriverFactory.transportKind(for: "pi") == .native)
        #expect(AgentDriverFactory.transportKind(for: "pi-acp") == .native)
        #expect(AgentDriverFactory.transportKind(for: "omp") == .acp)
        #expect(AgentDriverFactory.transportKind(for: "gemini") == .acp)
    }

    @Test func nativeBinaryUsesCanonicalNativeNames() {
        #expect(AgentDriverFactory.nativeBinary(for: "claude-acp") == "claude")
        #expect(AgentDriverFactory.nativeBinary(for: "codex") == "codex")
        #expect(AgentDriverFactory.nativeBinary(for: "opencode") == "opencode")
        #expect(AgentDriverFactory.nativeBinary(for: "pi") == "pi")
        #expect(AgentDriverFactory.nativeBinary(for: "pi-acp") == "pi")
    }

    @Test(arguments: ["pi", "pi-acp"])
    func piFactoryReturnsNativeDriverWhenCLIIsAvailable(agentId: String) throws {
        let driver = makeDriver(agentId: agentId, store: try tempStore())
        #expect(driver is PiRPCDriver)
    }

    @Test func piFactoryForwardsNativeConfigurationAndSuppressesLegacyResumeToken() async throws {
        let store = try tempStore()
        let legacy = try #require(makeConfiguredDriver(
            agentId: "pi-acp", store: store, resumeSessionId: "legacy-acp-token"))
        let native = try #require(makeConfiguredDriver(
            agentId: "pi", store: store, resumeSessionId: "native-session"))

        #expect(await legacy.piConfigurationForTesting() == (
            model: "anthropic/sonnet", effort: "high", resumeSessionId: nil))
        #expect(await native.piConfigurationForTesting() == (
            model: "anthropic/sonnet", effort: "high", resumeSessionId: "native-session"))
        let legacyCommand = (await legacy.piLaunchArgumentsForTesting()).joined(separator: " ")
        let nativeCommand = (await native.piLaunchArgumentsForTesting()).joined(separator: " ")
        #expect(legacyCommand.contains("--session") == false)
        #expect(nativeCommand.contains("--session") == true)
    }

    private func makeConfiguredDriver(
        agentId: String, store: AgentInstallStore, resumeSessionId: String?
    ) -> PiRPCDriver? {
        AgentDriverFactory.makeDriver(
            agentId: agentId, worktreePath: "/tmp", installStore: store,
            permissionMode: .ask, model: "anthropic/sonnet", effort: "high",
            resumeSessionId: resumeSessionId, pathProbe: { _ in true }) as? PiRPCDriver
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
