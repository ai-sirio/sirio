import Testing
@testable import TillerCore

@Test func claudeIdlePrefixIsNeedsInput() {
    #expect(AgentTitleStatus.detect(title: "✳ Fix login bug", agentId: "claude") == .needsInput)
    #expect(AgentTitleStatus.detect(title: "✳", agentId: "claude") == .needsInput)
}

@Test func claudeWorkingPrefixIsRunning() {
    #expect(AgentTitleStatus.detect(title: ". Fix login bug", agentId: "claude") == .running)
}

@Test func claudeBrailleSpinnerIsRunning() {
    #expect(AgentTitleStatus.detect(title: "\u{280B} Fix login bug", agentId: "claude") == .running)
}

@Test func claudeUnrecognizedTitleIsNil() {
    #expect(AgentTitleStatus.detect(title: "zsh", agentId: "claude") == nil)
}

@Test func piBrailleSpinnerIsRunning() {
    #expect(AgentTitleStatus.detect(title: "\u{280B} Pi", agentId: "pi") == .running)
}

@Test func piWithoutSpinnerIsNeedsInput() {
    #expect(AgentTitleStatus.detect(title: "Pi", agentId: "pi") == .needsInput)
}

@Test func piUnrelatedTitleIsNil() {
    #expect(AgentTitleStatus.detect(title: "zsh", agentId: "pi") == nil)
}

@Test func genericWorkingKeywordIsRunning() {
    #expect(AgentTitleStatus.detect(title: "codex - thinking", agentId: "codex") == .running)
    #expect(AgentTitleStatus.detect(title: "opencode running", agentId: "opencode") == .running)
}

@Test func genericIdleKeywordIsNeedsInput() {
    #expect(AgentTitleStatus.detect(title: "codex ready", agentId: "codex") == .needsInput)
    #expect(AgentTitleStatus.detect(title: "opencode done", agentId: "opencode") == .needsInput)
}

@Test func genericWaitingKeywordIsNeedsInput() {
    #expect(AgentTitleStatus.detect(title: "codex - action required", agentId: "codex") == .needsInput)
}

@Test func genericWithoutAgentNameIsNil() {
    #expect(AgentTitleStatus.detect(title: "zsh", agentId: "codex") == nil)
}

@Test func genericAvoidsSubstringFalsePositives() {
    // "~/codex-ready" must not fire "ready" as a strong idle keyword —
    // it's a path fragment, not a status word with real boundaries either side.
    #expect(AgentTitleStatus.detect(title: "~/codex-ready", agentId: "codex") == nil)
    // "reworking" contains "working" as a substring but isn't the keyword.
    #expect(AgentTitleStatus.detect(title: "codex reworking diff", agentId: "codex") == nil)
}

@Test func emptyTitleIsNil() {
    #expect(AgentTitleStatus.detect(title: "", agentId: "claude") == nil)
}
