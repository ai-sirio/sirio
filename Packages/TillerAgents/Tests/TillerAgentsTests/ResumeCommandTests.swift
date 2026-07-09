import Testing
import Foundation
@testable import TillerAgents

@Test func claudeResumeCommand() {
    let cmd = ClaudeCodeAdapter().resumeCommand(
        worktreePath: "/w", paneId: UUID(), tillerctlPath: "/x", sessionRef: "abc-123")
    #expect(cmd == "claude --resume 'abc-123'")
}

@Test func codexResumeKeepsNotifyOverride() {
    let paneId = UUID()
    let cmd = CodexAdapter().resumeCommand(
        worktreePath: "/w", paneId: paneId,
        tillerctlPath: "/usr/local/bin/tillerctl",
        sessionRef: "11111111-2222-3333-4444-555555555555")
    #expect(cmd == "codex -c 'notify=[\"\\/usr\\/local\\/bin\\/tillerctl\",\"notify\",\"--session\",\"\(paneId.uuidString)\",\"--status\",\"needs-input\"]' resume '11111111-2222-3333-4444-555555555555'")
}

@Test func openCodeResumeCommand() {
    let cmd = OpenCodeAdapter().resumeCommand(
        worktreePath: "/w", paneId: UUID(), tillerctlPath: "/x", sessionRef: "ses_42")
    #expect(cmd == "opencode --session 'ses_42'")
}

@Test func piResumeCommand() {
    let cmd = PiAdapter().resumeCommand(
        worktreePath: "/w", paneId: UUID(), tillerctlPath: "/x",
        sessionRef: "/home/u/.pi/sessions/s1.jsonl")
    #expect(cmd == "pi --session '/home/u/.pi/sessions/s1.jsonl'")
}

@Test func ompResumeLoadsHookAndSession() {
    let cmd = OhMyPiAdapter().resumeCommand(
        worktreePath: "/tmp/wt", paneId: UUID(), tillerctlPath: "/x", sessionRef: "s1")
    #expect(cmd == "omp --hook '/tmp/wt/.tiller/omp-hook.ts' --resume='s1'")
}
