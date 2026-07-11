import Foundation
import AppKit
import TillerCore
import TillerControl

/// cmux-parity control methods. The legacy methods (panel.*, notify,
/// session.ref, worktree.set) stay in AppModel.handleControl; everything
/// added for CLI parity dispatches here.
extension AppModel {
    /// Methods answered by handleCmuxControl — the system.capabilities
    /// payload. Legacy methods are listed too: capabilities describes the
    /// whole socket, not just this file.
    static let cmuxMethods: [String] = [
        "panel.create", "panel.write", "panel.read", "panel.wait",
        "notify", "session.ref", "worktree.set",
        "system.ping", "system.capabilities", "system.identify",
    ]

    func handleCmuxControl(_ request: ControlRequest) async -> ControlResponse {
        switch request.method {
        case "system.ping":
            return .success(id: request.id, result: ["pong": "true"])

        case "system.capabilities":
            return .success(id: request.id, result: [
                "methods": ControlRows.encode(Self.cmuxMethods.map { ["method": $0] }),
                "socketEnabled": "true",   // we answered, therefore it's on
            ])

        case "system.identify":
            return identify(request)

        default:
            return .failure(id: request.id, error: "unknown method \(request.method)")
        }
    }

    /// Caller context: env-provided worktree/pane ids win (a process inside
    /// a Tiller pane identifies itself); otherwise fall back to the UI's
    /// selected worktree and its active pane.
    private func identify(_ request: ControlRequest) -> ControlResponse {
        let worktree = request.params["worktree"]
            .flatMap(UUID.init(uuidString:))
            .flatMap { id in worktrees.values.flatMap { $0 }.first { $0.id == id } }
            ?? selectedWorktree
        guard let worktree else {
            return .failure(id: request.id, error: "no worktree context (not inside a Tiller pane and nothing selected)")
        }
        let paneId = request.params["pane"].flatMap(UUID.init(uuidString:))
            ?? activePaneId(in: worktree)
        var result: [String: String] = [
            "workspaceId": worktree.id.uuidString,
            "branch": worktree.branch,
            "path": worktree.path,
            "project": projects.first { $0.id == worktree.projectId }?.name ?? "",
        ]
        if let paneId { result["surfaceId"] = paneId.uuidString }
        return .success(id: request.id, result: result)
    }

    /// "Active pane" of a worktree — first leaf of its active tab, the same
    /// heuristic splitCurrent() already uses.
    func activePaneId(in worktree: Worktree) -> UUID? {
        activeTab(for: worktree.id)?.leafIds.first
    }
}
