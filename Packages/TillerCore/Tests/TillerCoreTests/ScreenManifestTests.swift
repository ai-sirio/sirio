import Testing
@testable import TillerCore

@Test func detectClaudePermissionPromptReturnsNeedsInput() {
    let text = "Some tool output\nDo you want to proceed?\n1. Yes\n2. No"
    #expect(ScreenManifest.detect(tailText: text, agentId: "claude") == .needsInput)
}

@Test func detectClaudeStreamingIndicatorReturnsRunning() {
    let text = "Thinking about the fix... (esc to interrupt)"
    #expect(ScreenManifest.detect(tailText: text, agentId: "claude") == .running)
}

@Test func detectClaudeReturnsNilForUnrelatedText() {
    #expect(ScreenManifest.detect(tailText: "$ ls\nREADME.md\n", agentId: "claude") == nil)
}

@Test func detectGenericAgentYesNoPromptReturnsNeedsInput() {
    #expect(ScreenManifest.detect(tailText: "Run this command? (y/n)", agentId: "codex") == .needsInput)
    #expect(ScreenManifest.detect(tailText: "Allow? [y/n]", agentId: "opencode") == .needsInput)
}

@Test func detectGenericAgentConfirmWordReturnsNeedsInput() {
    #expect(ScreenManifest.detect(tailText: "About to delete files. Proceed?", agentId: "pi") == .needsInput)
    #expect(ScreenManifest.detect(tailText: "Continue? y/n", agentId: "omp") == .needsInput)
}

@Test func detectReturnsNilForUnknownAgentId() {
    #expect(ScreenManifest.detect(tailText: "Do you want to proceed?", agentId: "unknown") == nil)
}

@Test func detectReturnsNilForEmptyText() {
    #expect(ScreenManifest.detect(tailText: "", agentId: "claude") == nil)
}
