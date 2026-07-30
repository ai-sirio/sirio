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

    /// Regression for a real hang, not a timing flake: `summarize()` used to
    /// read stdout via the blocking `readDataToEndOfFile()` and reap the
    /// child via the blocking `waitUntilExit()`, both inside an `async`
    /// function. Each in-flight call occupied one thread of Swift
    /// Concurrency's FIXED-SIZE cooperative pool for the whole subprocess
    /// lifetime. Enough concurrent calls exhausted the pool: later calls
    /// queued for a thread that never freed up, which is exactly what made
    /// `summarizeReturnsTrimmedStdout()` hang for 8+ minutes under the App
    /// test suite's parallel execution. 64 concurrent calls comfortably
    /// exceeds the pool size on any real machine; all must still complete
    /// well under the per-call timeout if the pool is never blocked.
    @Test func manyConcurrentSummarizeCallsDoNotExhaustTheCooperativePool() async {
        let start = ContinuousClock.now
        await withTaskGroup(of: String?.self) { group in
            for _ in 0..<64 {
                group.addTask {
                    await AutoNamer.summarize(
                        transcript: "user asked to fix login bug", worktreePath: "/tmp",
                        adapter: EchoAdapter(), timeout: 5)
                }
            }
            var results: [String?] = []
            for await result in group { results.append(result) }
            #expect(results.allSatisfy { $0 == "Fix login bug" })
        }
        #expect(ContinuousClock.now - start < .seconds(5))
    }
}
