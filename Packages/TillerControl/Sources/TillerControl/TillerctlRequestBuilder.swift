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

    /// Build a panel.write request.
    /// - Parameters:
    ///   - id: The panel identifier.
    ///   - input: Text to send. Caller appends "\n" for an Enter keystroke.
    public static func panelWrite(id: String, input: String) -> ControlRequest {
        ControlRequest(id: UUID().uuidString, method: "panel.write", params: ["id": id, "input": input])
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
}
