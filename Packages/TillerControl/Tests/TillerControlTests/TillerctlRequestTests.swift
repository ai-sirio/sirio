import Testing
@testable import TillerControl

@Test func panelCreateRequestShape() {
    let request = TillerctlRequestBuilder.panelCreate(worktree: "W-1", cmd: "claude")
    #expect(request.method == "panel.create")
    #expect(request.params == ["worktree": "W-1", "cmd": "claude"])
    #expect(!request.id.isEmpty)
}

@Test func panelCreateOmitsNilCmd() {
    let request = TillerctlRequestBuilder.panelCreate(worktree: "W-1", cmd: nil)
    #expect(request.params == ["worktree": "W-1"])
}

@Test func notifyRequestShape() {
    let request = TillerctlRequestBuilder.notify(session: "S-1", status: "done")
    #expect(request.method == "notify")
    #expect(request.params == ["session": "S-1", "status": "done"])
}

@Test func panelWaitCarriesTimeout() {
    let request = TillerctlRequestBuilder.panelWait(id: "P-1", timeoutMs: 5000)
    #expect(request.params == ["id": "P-1", "timeoutMs": "5000"])
}

@Test func worktreeSetRequest() throws {
    let request = TillerctlRequestBuilder.worktreeSet(worktree: "/tmp/wt", comment: "tests running")
    #expect(request.method == "worktree.set")
    #expect(request.params == ["worktree": "/tmp/wt", "comment": "tests running"])
}

@Test func notifyCarriesAgentSession() {
    let request = TillerctlRequestBuilder.notify(session: "S-1", status: "running", agentSession: "abc")
    #expect(request.params == ["session": "S-1", "status": "running", "agentSession": "abc"])
}

@Test func notifyOmitsNilAndEmptyAgentSession() {
    #expect(TillerctlRequestBuilder.notify(session: "S-1", status: "done").params
        == ["session": "S-1", "status": "done"])
    #expect(TillerctlRequestBuilder.notify(session: "S-1", status: "done", agentSession: "").params
        == ["session": "S-1", "status": "done"])
}

@Test func sessionRefRequestShape() {
    let request = TillerctlRequestBuilder.sessionRef(session: "S-1", ref: "ses_1")
    #expect(request.method == "session.ref")
    #expect(request.params == ["session": "S-1", "ref": "ses_1"])
}
