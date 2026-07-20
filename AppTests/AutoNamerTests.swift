import Foundation
import TillerAgents
import Testing
@testable import Tiller

private struct EchoAdapter: AgentAdapter {
    let id = "echo"
    let displayName = "Echo"
    var hasNativeHooks: Bool { false }
    func prepare(worktreePath: String, paneId: UUID, tillerctlPath: String) throws {}
    func command(worktreePath: String, paneId: UUID, tillerctlPath: String) -> String { "" }
    func resumeCommand(worktreePath: String, paneId: UUID, tillerctlPath: String, sessionRef: String) -> String? { nil }
    func summarizerCommand(prompt: String) -> String? { "echo 'Fix login bug'" }
}

private struct MissingBinaryAdapter: AgentAdapter {
    let id = "missing"
    let displayName = "Missing"
    var hasNativeHooks: Bool { false }
    func prepare(worktreePath: String, paneId: UUID, tillerctlPath: String) throws {}
    func command(worktreePath: String, paneId: UUID, tillerctlPath: String) -> String { "" }
    func resumeCommand(worktreePath: String, paneId: UUID, tillerctlPath: String, sessionRef: String) -> String? { nil }
    func summarizerCommand(prompt: String) -> String? { "definitely-not-a-real-binary-xyz" }
}

private struct UnsupportedAdapter: AgentAdapter {
    let id = "unsupported"
    let displayName = "Unsupported"
    var hasNativeHooks: Bool { false }
    func prepare(worktreePath: String, paneId: UUID, tillerctlPath: String) throws {}
    func command(worktreePath: String, paneId: UUID, tillerctlPath: String) -> String { "" }
    func resumeCommand(worktreePath: String, paneId: UUID, tillerctlPath: String, sessionRef: String) -> String? { nil }
}

@Suite("AutoNamerTests")
struct AutoNamerTests {
    @Test func summarizeReturnsTrimmedStdout() async {
        let result = await AutoNamer.summarize(
            transcript: "user asked to fix login bug", worktreePath: "/tmp",
            adapter: EchoAdapter())
        #expect(result == "Fix login bug")
    }

    @Test func summarizeReturnsNilWhenAdapterUnsupported() async {
        let result = await AutoNamer.summarize(
            transcript: "anything", worktreePath: "/tmp", adapter: UnsupportedAdapter())
        #expect(result == nil)
    }

    @Test func summarizeReturnsNilWhenBinaryMissing() async {
        let result = await AutoNamer.summarize(
            transcript: "anything", worktreePath: "/tmp", adapter: MissingBinaryAdapter(), timeout: 2)
        #expect(result == nil)
    }
}
