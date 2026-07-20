import Testing
import Foundation
@testable import TillerAgents

@Test func readsLastAssistantAndUserLinesFromJSONL() throws {
    let tmp = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    let worktreePath = "/Users/tester/project"
    // Claude Code escapa il path della working dir sostituendo "/" con "-".
    let escaped = worktreePath.replacingOccurrences(of: "/", with: "-")
    let projectDir = tmp.appendingPathComponent(".claude/projects/\(escaped)")
    try FileManager.default.createDirectory(at: projectDir, withIntermediateDirectories: true)
    let sessionRef = "abc-123"
    let jsonl = """
    {"type":"user","message":{"role":"user","content":"fix the login bug"}}
    {"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"Looking at auth.ts now"}]}}
    """
    try jsonl.write(to: projectDir.appendingPathComponent("\(sessionRef).jsonl"), atomically: true, encoding: .utf8)
    defer { try? FileManager.default.removeItem(at: tmp) }

    let source = ClaudeTranscriptSource(worktreePath: worktreePath, sessionRef: sessionRef, homeDirectory: tmp)
    let text = source.recentText()

    #expect(text?.contains("fix the login bug") == true)
    #expect(text?.contains("Looking at auth.ts now") == true)
}

@Test func returnsNilWhenTranscriptFileMissing() {
    let source = ClaudeTranscriptSource(worktreePath: "/nope", sessionRef: "missing", homeDirectory: FileManager.default.temporaryDirectory)
    #expect(source.recentText() == nil)
}
