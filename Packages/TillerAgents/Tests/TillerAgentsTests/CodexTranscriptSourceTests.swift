import Testing
import Foundation
@testable import TillerAgents

@Test func findsRolloutFileByIdAcrossDateSubdirectories() throws {
    let tmp = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    let sessionsDir = tmp.appendingPathComponent(".codex/sessions/2026/07/20")
    try FileManager.default.createDirectory(at: sessionsDir, withIntermediateDirectories: true)
    let id = "abcd1234-0000-0000-0000-000000000000"
    let jsonl = """
    {"type":"message","role":"user","content":[{"type":"text","text":"add rate limiting"}]}
    {"type":"message","role":"assistant","content":[{"type":"text","text":"Added a token bucket limiter"}]}
    """
    try jsonl.write(
        to: sessionsDir.appendingPathComponent("rollout-2026-07-20T12-00-00-\(id).jsonl"),
        atomically: true, encoding: .utf8)
    defer { try? FileManager.default.removeItem(at: tmp) }

    let source = CodexTranscriptSource(sessionRef: id, homeDirectory: tmp)
    let text = source.recentText()

    #expect(text?.contains("add rate limiting") == true)
    #expect(text?.contains("Added a token bucket limiter") == true)
}

@Test func returnsNilWhenNoMatchingRollout() {
    let source = CodexTranscriptSource(sessionRef: "missing", homeDirectory: FileManager.default.temporaryDirectory)
    #expect(source.recentText() == nil)
}
