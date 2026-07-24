import Testing
import TillerAgents

@testable import Tiller

@Suite struct SummarizerSelectionTests {
    @Test func selectedAgentComesFirstThenTabAgent() {
        let ids = SummarizerSelection
            .adapters(selectedId: "codex", tabAgentId: "claude-acp").map(\.id)
        #expect(ids == ["codex", "claude"])
    }

    @Test func sameAgentIsDeduplicated() {
        let ids = SummarizerSelection
            .adapters(selectedId: "claude", tabAgentId: "claude-acp").map(\.id)
        #expect(ids == ["claude"])
    }

    @Test func registryOnlyTabAgentHasNoFallback() {
        let ids = SummarizerSelection
            .adapters(selectedId: "claude", tabAgentId: "gemini").map(\.id)
        #expect(ids == ["claude"])
    }

    @Test func unknownSelectedIdFallsBackToClaudePrimary() {
        let ids = SummarizerSelection
            .adapters(selectedId: "nonexistent", tabAgentId: "codex-acp").map(\.id)
        #expect(ids == ["claude", "codex"])
    }

    @Test func canonicalPiIdResolvesAsFallback() {
        let ids = SummarizerSelection
            .adapters(selectedId: "opencode", tabAgentId: "pi-acp").map(\.id)
        #expect(ids == ["opencode", "pi"])
    }
}
