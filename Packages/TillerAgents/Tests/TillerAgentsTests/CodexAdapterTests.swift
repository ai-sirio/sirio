import Testing
import Foundation
@testable import TillerAgents

@Test func codexPrepareIsNoOpNeverTouchesFiles() throws {
    let dir = NSTemporaryDirectory() + "tiller-agents-\(UUID().uuidString.prefix(8))"
    try FileManager.default.createDirectory(atPath: dir, withIntermediateDirectories: true)
    defer { try? FileManager.default.removeItem(atPath: dir) }
    try CodexAdapter().prepare(worktreePath: dir, paneId: UUID(), tillerctlPath: "/x/tillerctl")
    #expect(try FileManager.default.contentsOfDirectory(atPath: dir).isEmpty)
}

@Test func codexCommandCarriesNotifyOverride() {
    let paneId = UUID()
    let cmd = CodexAdapter().command(worktreePath: "/w", paneId: paneId, tillerctlPath: "/usr/local/bin/tillerctl")
    #expect(cmd == "codex -c 'notify=[\"\\/usr\\/local\\/bin\\/tillerctl\",\"notify\",\"--session\",\"\(paneId.uuidString)\",\"--status\",\"needs-input\"]'")
}

@Test func codexHasNativeHooks() {
    #expect(CodexAdapter().hasNativeHooks == true)
}
