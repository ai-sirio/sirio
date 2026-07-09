import Testing
@testable import TillerCore

@Test func claudeIdlePrefixIdentifiesClaude() {
    #expect(AgentTitleIdentity.identify(title: "✳ Fix login bug") == "claude")
    #expect(AgentTitleIdentity.identify(title: "✳") == "claude")
}

@Test func claudeWorkingPrefixIdentifiesClaude() {
    #expect(AgentTitleIdentity.identify(title: ". Fix login bug") == "claude")
}

@Test func bareBrailleSpinnerIdentifiesClaude() {
    #expect(AgentTitleIdentity.identify(title: "\u{280B} Fix login bug") == "claude")
}

@Test func codexNameTokenIdentifiesCodex() {
    #expect(AgentTitleIdentity.identify(title: "codex - thinking") == "codex")
}

@Test func opencodeNameTokenIdentifiesOpenCode() {
    #expect(AgentTitleIdentity.identify(title: "opencode running") == "opencode")
}

@Test func ompNameTokenIdentifiesOmp() {
    #expect(AgentTitleIdentity.identify(title: "omp working") == "omp")
}

@Test func piSpinnerWithNameIdentifiesPi() {
    #expect(AgentTitleIdentity.identify(title: "\u{280B} Pi") == "pi")
}

@Test func piNameWithoutSpinnerDoesNotIdentify() {
    // No spinner present — bare "pi" text alone is too ambiguous with a
    // branch/cwd name (e.g. "pi-notes") to safely claim identity.
    #expect(AgentTitleIdentity.identify(title: "Pi") == nil)
}

@Test func plainShellPromptDoesNotIdentify() {
    #expect(AgentTitleIdentity.identify(title: "zsh") == nil)
    #expect(AgentTitleIdentity.identify(title: "") == nil)
}

@Test func branchNameSubstringsDoNotFalselyIdentify() {
    #expect(AgentTitleIdentity.identify(title: "opencode-experiment") == nil)
    #expect(AgentTitleIdentity.identify(title: "~/codex-notes") == nil)
    #expect(AgentTitleIdentity.identify(title: "pi-notes") == nil)
}
