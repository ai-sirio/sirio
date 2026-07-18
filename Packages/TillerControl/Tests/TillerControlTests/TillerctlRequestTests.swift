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

@Test func canonicalPanelBuildersUseStableMethodsAndParameters() {
    let create = TillerctlRequestBuilder.panelCreate(worktree: "worktree", cmd: "codex")
    #expect(create.method == "panel.create")
    #expect(create.params == ["worktree": "worktree", "cmd": "codex"])

    let split = TillerctlRequestBuilder.panelSplit(
        from: "source", direction: "right", cmd: "pi")
    #expect(split.method == "panel.split")
    #expect(split.params == ["from": "source", "direction": "right", "cmd": "pi"])

    let list = TillerctlRequestBuilder.panelList(worktree: "/repo/worktree")
    #expect(list.method == "panel.list")
    #expect(list.params == ["worktree": "/repo/worktree"])

    let write = TillerctlRequestBuilder.panelWrite(id: "pane", input: "hello\r")
    #expect(write.method == "panel.write")
    #expect(write.params == ["id": "pane", "input": "hello\r"])

    let key = TillerctlRequestBuilder.panelKey(id: "pane", key: "enter")
    #expect(key.method == "panel.key")
    #expect(key.params == ["id": "pane", "key": "enter"])

    let read = TillerctlRequestBuilder.panelRead(id: "pane")
    #expect(read.method == "panel.read")
    #expect(read.params == ["id": "pane"])

    let wait = TillerctlRequestBuilder.panelWait(id: "pane", timeoutMs: 250)
    #expect(wait.method == "panel.wait")
    #expect(wait.params == ["id": "pane", "timeoutMs": "250"])

    let focus = TillerctlRequestBuilder.panelFocus(id: "pane")
    #expect(focus.method == "panel.focus")
    #expect(focus.params == ["id": "pane"])

    let close = TillerctlRequestBuilder.panelClose(id: "pane")
    #expect(close.method == "panel.close")
    #expect(close.params == ["id": "pane"])
}

@Test func optionalPanelParametersAreOmitted() {
    #expect(TillerctlRequestBuilder.panelCreate(worktree: "worktree", cmd: nil).params
        == ["worktree": "worktree"])
    #expect(TillerctlRequestBuilder.panelSplit(from: "pane", direction: "down", cmd: nil).params
        == ["from": "pane", "direction": "down"])
    #expect(TillerctlRequestBuilder.panelWait(id: "pane", timeoutMs: nil).params
        == ["id": "pane"])
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
