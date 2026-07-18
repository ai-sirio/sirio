import Testing
import Foundation
@testable import TillerAgents

@Test func codexPrepareOnlyProvisionsWorktreeSkill() throws {
    let dir = NSTemporaryDirectory() + "tiller-agents-\(UUID().uuidString.prefix(8))"
    try FileManager.default.createDirectory(atPath: dir, withIntermediateDirectories: true)
    defer { try? FileManager.default.removeItem(atPath: dir) }
    try CodexAdapter().prepare(worktreePath: dir, paneId: UUID(), tillerctlPath: "/x/tillerctl", skillMarkdown: try repositorySkill())
    #expect(try FileManager.default.contentsOfDirectory(atPath: dir) == [".agents"])
}

@Test func codexCommandCarriesNotifyOverride() {
    let paneId = UUID()
    let cmd = CodexAdapter().command(worktreePath: "/w", paneId: paneId, tillerctlPath: "/usr/local/bin/tillerctl")
    // Slashes must stay unescaped: "\/" is not a valid TOML escape, and Codex's
    // "-c key=value" override parser rejects the whole value (falls back to a
    // raw string, then fails "expected a sequence") if they are.
    #expect(cmd == "codex -c 'notify=[\"/usr/local/bin/tillerctl\",\"notify\",\"--session\",\"\(paneId.uuidString)\",\"--status\",\"needs-input\"]'")
}

@Test func codexHasNativeHooks() {
    #expect(CodexAdapter().hasNativeHooks == true)
}
