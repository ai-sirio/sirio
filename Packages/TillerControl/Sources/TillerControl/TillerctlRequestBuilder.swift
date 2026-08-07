import Foundation

public enum TillerctlRequestBuilder {
    /// Build a panel.create request.
    /// - Parameters:
    ///   - worktree: The worktree identifier.
    ///   - cmd: Optional initial command; omitted from params when nil.
    public static func panelCreate(worktree: String, cmd: String?) -> ControlRequest {
        var params = ["worktree": worktree]
        if let cmd { params["cmd"] = cmd }
        return ControlRequest(id: UUID().uuidString, method: "panel.create", params: params)
    }

    /// Build a panel.split request.
    public static func panelSplit(
        from: String, direction: String, cmd: String?
    ) -> ControlRequest {
        var params = ["from": from, "direction": direction]
        if let cmd { params["cmd"] = cmd }
        return request("panel.split", params)
    }

    /// Build a panel.list request.
    public static func panelList(worktree: String) -> ControlRequest {
        request("panel.list", ["worktree": worktree])
    }

    /// Build a panel.write request.
    /// - Parameters:
    ///   - id: The panel identifier.
    ///   - input: Text to send. Caller appends "\r" for an Enter keystroke.
    public static func panelWrite(id: String, input: String) -> ControlRequest {
        ControlRequest(id: UUID().uuidString, method: "panel.write", params: ["id": id, "input": input])
    }

    /// Build a panel.key request.
    public static func panelKey(id: String, key: String) -> ControlRequest {
        request("panel.key", ["id": id, "key": key])
    }

    /// Build a panel.read request.
    public static func panelRead(id: String) -> ControlRequest {
        ControlRequest(id: UUID().uuidString, method: "panel.read", params: ["id": id])
    }

    /// Build a panel.wait request.
    /// - Parameters:
    ///   - id: The panel identifier.
    ///   - timeoutMs: Optional server-side timeout; omitted from params when nil.
    public static func panelWait(id: String, timeoutMs: Int?) -> ControlRequest {
        var params = ["id": id]
        if let timeoutMs { params["timeoutMs"] = String(timeoutMs) }
        return ControlRequest(id: UUID().uuidString, method: "panel.wait", params: params)
    }

    /// Build a panel.focus request.
    public static func panelFocus(id: String) -> ControlRequest {
        request("panel.focus", ["id": id])
    }

    /// Build a panel.close request.
    public static func panelClose(id: String) -> ControlRequest {
        request("panel.close", ["id": id])
    }

    /// Build a notify request. `agentSession` is included only when non-empty.
    public static func notify(session: String, status: String, agentSession: String? = nil) -> ControlRequest {
        var params = ["session": session, "status": status]
        if let agentSession, !agentSession.isEmpty { params["agentSession"] = agentSession }
        return ControlRequest(id: UUID().uuidString, method: "notify", params: params)
    }

    /// Build a session.ref request: report an agent-native session reference
    /// for a pane without touching the status pipeline.
    public static func sessionRef(session: String, ref: String) -> ControlRequest {
        ControlRequest(id: UUID().uuidString, method: "session.ref",
                       params: ["session": session, "ref": ref])
    }

    /// Build a worktree.set request. `worktree` is a worktree UUID or its
    /// absolute path (agents typically pass "$PWD").
    public static func worktreeSet(worktree: String, comment: String) -> ControlRequest {
        ControlRequest(
            id: UUID().uuidString, method: "worktree.set",
            params: ["worktree": worktree, "comment": comment]
        )
    }


    // MARK: - cmux-parity methods

    private static func request(_ method: String, _ params: [String: String?] = [:]) -> ControlRequest {
        ControlRequest(id: UUID().uuidString, method: method,
                       params: params.compactMapValues { $0 })
    }

    public static func workspaceList() -> ControlRequest { request("workspace.list") }
    public static func workspaceCreate(project: String, branch: String?) -> ControlRequest {
        request("workspace.create", ["project": project, "branch": branch])
    }
    public static func workspaceSelect(workspace: String) -> ControlRequest {
        request("workspace.select", ["workspace": workspace])
    }
    public static func workspaceCurrent() -> ControlRequest { request("workspace.current") }
    public static func workspaceClose(workspace: String) -> ControlRequest {
        request("workspace.close", ["workspace": workspace])
    }


    public static func notificationCreate(title: String, subtitle: String?, body: String) -> ControlRequest {
        request("notification.create", ["title": title, "subtitle": subtitle, "body": body])
    }
    public static func notificationList() -> ControlRequest { request("notification.list") }
    public static func notificationClear() -> ControlRequest { request("notification.clear") }

    public static func systemPing() -> ControlRequest { request("system.ping") }
    public static func systemCapabilities() -> ControlRequest { request("system.capabilities") }
    public static func systemIdentify(worktree: String?, pane: String?) -> ControlRequest {
        request("system.identify", ["worktree": worktree, "pane": pane])
    }
    public static func sessionRestore() -> ControlRequest { request("session.restore") }

    public static func browserOpen(
        url: String, workspace: String?, window: String?, idFormat: String? = nil
    ) -> ControlRequest {
        request("browser.open", [
            "url": url, "workspace": workspace, "window": window, "id-format": idFormat
        ])
    }

    public static func browserNavigate(
        surface: String, action: String, workspace: String? = nil
    ) -> ControlRequest {
        request("browser.navigate", [
            "surface": surface, "action": action, "workspace": workspace
        ])
    }

    public static func browserGet(
        surface: String, what: String, selector: String?, workspace: String? = nil
    ) -> ControlRequest {
        request("browser.get", [
            "surface": surface, "what": what, "selector": selector, "workspace": workspace
        ])
    }

    public static func browserScreenshot(
        surface: String, path: String?, workspace: String? = nil
    ) -> ControlRequest {
        request("browser.screenshot", [
            "surface": surface, "path": path, "workspace": workspace
        ])
    }

    public static func browserSnapshot(surface: String, workspace: String? = nil)
        -> ControlRequest {
        request("browser.snapshot", ["surface": surface, "workspace": workspace])
    }

    public static func browserAct(
        surface: String, verb: String, ref: String?, selector: String?, value: String?,
        key: String?, generation: Int?, snapshotAfter: Bool, deltaX: Double?, deltaY: Double?,
        workspace: String? = nil
    ) -> ControlRequest {
        var params: [String: String?] = [
            "surface": surface, "verb": verb, "ref": ref, "selector": selector,
            "value": value, "key": key,
            "generation": generation.map { String($0) },
            "snapshotAfter": snapshotAfter ? "true" : nil,
            "deltaX": deltaX.map { String($0) }, "deltaY": deltaY.map { String($0) },
            "workspace": workspace,
        ]
        return request("browser.act", params)
    }

    public static func browserWait(
        surface: String, conditionKey: String, conditionValue: String, timeoutMs: Int,
        workspace: String? = nil
    ) -> ControlRequest {
        request("browser.wait", [
            "surface": surface, conditionKey: conditionValue,
            "timeoutMs": String(timeoutMs), "workspace": workspace,
        ])
    }

    public static func browserEval(surface: String, script: String, workspace: String? = nil)
        -> ControlRequest {
        request("browser.eval", ["surface": surface, "script": script, "workspace": workspace])
    }

    public static func browserConsole(surface: String, since: Double?, workspace: String? = nil)
        -> ControlRequest {
        request("browser.console", [
            "surface": surface, "since": since.map { String($0) }, "workspace": workspace,
        ])
    }
}
