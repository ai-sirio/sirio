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


@Test func workspaceBuilders() {
    #expect(TillerctlRequestBuilder.workspaceList().method == "workspace.list")
    let create = TillerctlRequestBuilder.workspaceCreate(project: "P1", branch: nil)
    #expect(create.method == "workspace.create")
    #expect(create.params == ["project": "P1"])   // nil branch omitted
    let createBr = TillerctlRequestBuilder.workspaceCreate(project: "P1", branch: "fix")
    #expect(createBr.params == ["project": "P1", "branch": "fix"])
    #expect(TillerctlRequestBuilder.workspaceSelect(workspace: "W")
        .params == ["workspace": "W"])
    #expect(TillerctlRequestBuilder.workspaceCurrent().method == "workspace.current")
    #expect(TillerctlRequestBuilder.workspaceClose(workspace: "W")
        .method == "workspace.close")
}

@Test func surfaceBuilders() {
    #expect(TillerctlRequestBuilder.surfaceList().method == "surface.list")
    #expect(TillerctlRequestBuilder.paneSurfaces().method == "pane.surfaces")
    #expect(TillerctlRequestBuilder.surfaceFocus(surface: "S")
        .params == ["surface": "S"])
    #expect(TillerctlRequestBuilder.surfaceSplit(direction: "right")
        .params == ["direction": "right"])
    let send = TillerctlRequestBuilder.surfaceSendText(text: "ls\n", surface: nil)
    #expect(send.method == "surface.send_text")
    #expect(send.params == ["text": "ls\n"])      // nil surface omitted
    let sendTo = TillerctlRequestBuilder.surfaceSendText(text: "x", surface: "S")
    #expect(sendTo.params == ["text": "x", "surface": "S"])
    let key = TillerctlRequestBuilder.surfaceSendKey(key: "enter", surface: nil)
    #expect(key.method == "surface.send_key")
    #expect(key.params == ["key": "enter"])
}

@Test func notificationAndSystemBuilders() {
    let n = TillerctlRequestBuilder.notificationCreate(
        title: "T", subtitle: nil, body: "B")
    #expect(n.method == "notification.create")
    #expect(n.params == ["title": "T", "body": "B"])   // nil subtitle omitted
    #expect(TillerctlRequestBuilder.notificationList().method == "notification.list")
    #expect(TillerctlRequestBuilder.notificationClear().method == "notification.clear")
    #expect(TillerctlRequestBuilder.systemPing().method == "system.ping")
    #expect(TillerctlRequestBuilder.systemCapabilities().method == "system.capabilities")
    let id = TillerctlRequestBuilder.systemIdentify(worktree: "W", pane: nil)
    #expect(id.method == "system.identify")
    #expect(id.params == ["worktree": "W"])
    #expect(TillerctlRequestBuilder.sessionRestore().method == "session.restore")
}
