import Testing
import Foundation
@testable import TillerTerminal

@Test func composeSetsTermPaneIdAndPreservesBase() {
    let paneId = UUID()
    let env = PtyEnvironment.compose(
        base: ["PATH": "/usr/bin", "TERM": "dumb"],
        term: "xterm-256color",
        extra: [:],
        paneId: paneId
    )
    #expect(env.contains("TERM=xterm-256color"))
    #expect(env.contains("PATH=/usr/bin"))
    #expect(env.contains("TILLER_PANE_ID=\(paneId.uuidString)"))
    #expect(!env.contains("TERM=dumb"))
}

@Test func extraEnvironmentOverridesBase() {
    let env = PtyEnvironment.compose(
        base: ["TILLER_ENV": "0"],
        term: "xterm-256color",
        extra: ["TILLER_ENV": "1", "TILLER_SOCKET": "/tmp/tiller.sock", "TILLER_WORKTREE_ID": "W1"],
        paneId: UUID()
    )
    #expect(env.contains("TILLER_ENV=1"))
    #expect(env.contains("TILLER_SOCKET=/tmp/tiller.sock"))
    #expect(env.contains("TILLER_WORKTREE_ID=W1"))
    #expect(!env.contains("TILLER_ENV=0"))
}

@Test func paneIdCannotBeSpoofedByExtra() {
    let paneId = UUID()
    let env = PtyEnvironment.compose(
        base: [:],
        term: "xterm-256color",
        extra: ["TILLER_PANE_ID": "spoofed"],
        paneId: paneId
    )
    #expect(env.contains("TILLER_PANE_ID=\(paneId.uuidString)"))
    #expect(!env.contains("TILLER_PANE_ID=spoofed"))
}
