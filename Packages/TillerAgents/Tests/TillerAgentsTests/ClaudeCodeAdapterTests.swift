import Testing
import Foundation
@testable import TillerAgents

@Test func prepareWritesProjectLocalHooks() throws {
    let dir = NSTemporaryDirectory() + "tiller-agents-\(UUID().uuidString.prefix(8))"
    try FileManager.default.createDirectory(atPath: dir, withIntermediateDirectories: true)
    defer { try? FileManager.default.removeItem(atPath: dir) }
    let paneId = UUID()
    let adapter = ClaudeCodeAdapter()
    try adapter.prepare(worktreePath: dir, paneId: paneId, tillerctlPath: "/usr/local/bin/tillerctl")
    let data = try Data(contentsOf: URL(fileURLWithPath: dir + "/.claude/settings.local.json"))
    let json = try JSONSerialization.jsonObject(with: data) as? [String: Any]
    let hooks = json?["hooks"] as? [String: Any]
    #expect(hooks?.keys.sorted() == ["Notification", "SessionEnd", "SessionStart", "Stop", "UserPromptSubmit"])
    let stop = ((hooks?["Stop"] as? [[String: Any]])?.first?["hooks"] as? [[String: Any]])?.first
    #expect(stop?["command"] as? String ==
        "'/usr/local/bin/tillerctl' notify --session \(paneId.uuidString) --status needs-input --stdin-json")
    // SessionStart lands on an idle prompt awaiting the first user input, not a
    // running turn — it must report needs-input, otherwise a freshly started or
    // /compact-resumed session shows "running" while actually waiting.
    let sessionStart = ((hooks?["SessionStart"] as? [[String: Any]])?.first?["hooks"] as? [[String: Any]])?.first
    #expect(sessionStart?["command"] as? String ==
        "'/usr/local/bin/tillerctl' notify --session \(paneId.uuidString) --status needs-input --stdin-json")
}

@Test func prepareMergePreservesExistingKeys() throws {
    let dir = NSTemporaryDirectory() + "tiller-agents-\(UUID().uuidString.prefix(8))"
    try FileManager.default.createDirectory(atPath: dir + "/.claude", withIntermediateDirectories: true)
    defer { try? FileManager.default.removeItem(atPath: dir) }
    let existing = #"{"permissions":{"allow":["Bash"]},"hooks":{"PreToolUse":[]}}"#
    try existing.write(toFile: dir + "/.claude/settings.local.json", atomically: true, encoding: .utf8)
    try ClaudeCodeAdapter().prepare(worktreePath: dir, paneId: UUID(), tillerctlPath: "/x/tillerctl")
    let json = try JSONSerialization.jsonObject(
        with: Data(contentsOf: URL(fileURLWithPath: dir + "/.claude/settings.local.json"))
    ) as? [String: Any]
    #expect((json?["permissions"] as? [String: Any]) != nil)                 // preserved
    let hooks = json?["hooks"] as? [String: Any]
    #expect((hooks?["PreToolUse"] as? [Any]) != nil)                         // preserved
    #expect((hooks?["Stop"] as? [Any]) != nil)                               // added
}

@Test func claudeCommandIsPlainLaunch() {
    #expect(ClaudeCodeAdapter().command(worktreePath: "/w", paneId: UUID(), tillerctlPath: "/x") == "claude")
}

@Test func claudeCodeHasNativeHooks() {
    #expect(ClaudeCodeAdapter().hasNativeHooks == true)
}

@Test func prepareWritesTillerSkill() throws {
    let dir = NSTemporaryDirectory() + "tiller-agents-\(UUID().uuidString.prefix(8))"
    try FileManager.default.createDirectory(atPath: dir, withIntermediateDirectories: true)
    defer { try? FileManager.default.removeItem(atPath: dir) }
    try ClaudeCodeAdapter().prepare(worktreePath: dir, paneId: UUID(), tillerctlPath: "/x/tillerctl")
    let written = try String(contentsOfFile: dir + "/.claude/skills/tiller/SKILL.md", encoding: .utf8)
    #expect(written == TillerSkillDocument.markdown)
}

@Test func prepareOverwritesStaleSkill() throws {
    let dir = NSTemporaryDirectory() + "tiller-agents-\(UUID().uuidString.prefix(8))"
    try FileManager.default.createDirectory(atPath: dir + "/.claude/skills/tiller", withIntermediateDirectories: true)
    defer { try? FileManager.default.removeItem(atPath: dir) }
    try "stale".write(toFile: dir + "/.claude/skills/tiller/SKILL.md", atomically: true, encoding: .utf8)
    try ClaudeCodeAdapter().prepare(worktreePath: dir, paneId: UUID(), tillerctlPath: "/x/tillerctl")
    let written = try String(contentsOfFile: dir + "/.claude/skills/tiller/SKILL.md", encoding: .utf8)
    #expect(written == TillerSkillDocument.markdown)
}
