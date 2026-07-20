import Testing
@testable import TillerAgents

@Test func claudeSummarizerCommandUsesPrintMode() {
    let cmd = ClaudeCodeAdapter().summarizerCommand(prompt: "summarize this")
    #expect(cmd == "claude -p 'summarize this'")
}

@Test func codexSummarizerCommandUsesExecOutputLastMessage() {
    let cmd = CodexAdapter().summarizerCommand(prompt: "summarize this")
    #expect(cmd == "codex exec --output-last-message /dev/stdout 'summarize this'")
}

@Test func openCodeSummarizerCommandUsesRunPure() {
    let cmd = OpenCodeAdapter().summarizerCommand(prompt: "summarize this")
    #expect(cmd == "opencode run --pure 'summarize this'")
}

@Test func piSummarizerCommandUsesPrintNoTools() {
    let cmd = PiAdapter().summarizerCommand(prompt: "summarize this")
    #expect(cmd == "pi --print --no-tools 'summarize this'")
}

@Test func ohMyPiSummarizerCommandUsesPrintNoTools() {
    let cmd = OhMyPiAdapter().summarizerCommand(prompt: "summarize this")
    #expect(cmd == "omp --print --no-tools 'summarize this'")
}

@Test func promptWithSingleQuoteIsShellSafe() {
    let cmd = ClaudeCodeAdapter().summarizerCommand(prompt: "it's a test")
    #expect(cmd == "claude -p 'it'\\''s a test'")
}
