import Foundation
import AppKit
import TillerCore
import TillerControl
import TillerTerminal

/// cmux-parity control methods. The core methods (panel.*, notify,
/// session.ref, worktree.set) stay in AppModel.handleControl; everything
/// added for CLI parity dispatches here.
extension AppModel {
    /// Complete system.capabilities payload for the control socket.
    static let cmuxMethods: [String] = [
        "panel.create", "panel.split", "panel.list",
        "panel.write", "panel.key", "panel.read",
        "panel.wait", "panel.focus", "panel.close",
        "notify", "session.ref", "worktree.set",
        "system.ping", "system.capabilities", "system.identify",
        "workspace.list", "workspace.create", "workspace.select",
        "workspace.current", "workspace.close",
        "notification.create", "notification.list", "notification.clear",
        "session.restore",
    ]

    /// Re-apply the launch snapshot: remount worktree hosts and add back
    /// tabs the user closed since launch. Existing state is left untouched
    /// (idempotent). Re-added agent panes get their resume command back;
    /// scrollback reattaches via loadScrollback on remount (records are
    /// never deleted on close). Returns re-added tabs + remounted worktrees.
    func restoreLaunchSnapshot() -> Int {
        guard let snapshot = launchSnapshot else { return 0 }
        var restored = 0
        // Worktrees first: a restored tab is invisible while its terminal
        // host is unmounted.
        for worktreeId in snapshot.openWorktreeIds
        where worktree(byId: worktreeId) != nil && !openWorktreeIds.contains(worktreeId) {
            openWorktreeIds.append(worktreeId)
            restored += 1
        }
        for (worktreeId, snapshotTabs) in snapshot.tabs {
            guard worktree(byId: worktreeId) != nil else { continue }
            let currentIds = Set((tabs[worktreeId] ?? []).map(\.id))
            var added = 0
            for tab in snapshotTabs where !currentIds.contains(tab.id) {
                tabs[worktreeId, default: []].append(tab)
                added += 1
                for paneId in tab.leafIds {
                    if let command = snapshot.paneCommands[paneId] {
                        paneCommands[paneId] = command
                        watchExit(paneId: paneId)
                    }
                }
            }
            if added > 0 { persistTabs(for: worktreeId) }
            restored += added
        }
        return restored
    }

    /// Resolve a worktree from a UUID string or absolute path — same dual
    /// selector the legacy worktree.set method accepts.
    func resolveWorktree(_ selector: String) -> Worktree? {
        if let uuid = UUID(uuidString: selector) {
            return worktrees.values.flatMap { $0 }.first { $0.id == uuid }
        }
        return worktrees.values.flatMap { $0 }.first { $0.path == selector }
    }

    /// Default branch name for workspace.create when --branch is omitted.
    static func generatedBranchName(now: Date = Date()) -> String {
        let fmt = DateFormatter()
        fmt.dateFormat = "yyyyMMdd-HHmmss"
        fmt.locale = Locale(identifier: "en_US_POSIX")
        return "wt-\(fmt.string(from: now))"
    }

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

        case "workspace.list":
            return .success(id: request.id, result: [
                "workspaces": ControlRows.encode(ControlListing.workspaceRows(
                    projects: projects, worktrees: worktrees,
                    selectedWorktreeId: selectedWorktree?.id))
            ])

        case "workspace.current":
            guard let wt = selectedWorktree else {
                return .failure(id: request.id, error: "no workspace selected")
            }
            return .success(id: request.id, result: [
                "id": wt.id.uuidString, "branch": wt.branch, "path": wt.path,
                "project": projects.first { $0.id == wt.projectId }?.name ?? "",
            ])

        case "workspace.select":
            guard let selector = request.params["workspace"] else {
                return .failure(id: request.id, error: "missing workspace")
            }
            guard let wt = resolveWorktree(selector) else {
                return .failure(id: request.id, error: "unknown workspace \(selector)")
            }
            selectedWorktree = wt
            NSApp.activate(ignoringOtherApps: false)
            return .success(id: request.id, result: ["id": wt.id.uuidString])

        case "workspace.close":
            guard let selector = request.params["workspace"] else {
                return .failure(id: request.id, error: "missing workspace")
            }
            guard let wt = resolveWorktree(selector) else {
                return .failure(id: request.id, error: "unknown workspace \(selector)")
            }
            // Unmount the terminal host: PTYs terminate via onDisappear.
            // The worktree itself stays in the sidebar.
            openWorktreeIds.removeAll { $0 == wt.id }
            if selectedWorktree?.id == wt.id { selectedWorktree = nil }
            return .success(id: request.id)

        case "workspace.create":
            guard let projectSelector = request.params["project"] else {
                return .failure(id: request.id, error: "missing project")
            }
            guard let project = projects.first(where: {
                $0.id.uuidString == projectSelector || $0.name == projectSelector
            }) else {
                return .failure(id: request.id, error: "unknown project \(projectSelector)")
            }
            guard isGitProject(project) else {
                return .failure(id: request.id, error: "project is not a git repository")
            }
            let branch = request.params["branch"] ?? Self.generatedBranchName()
            let before = Set((worktrees[project.id] ?? []).map(\.id))
            await addWorktree(project: project, branch: branch)
            guard let created = (worktrees[project.id] ?? []).first(where: { !before.contains($0.id) }) else {
                return .failure(id: request.id, error: lastError ?? "workspace.create failed")
            }
            return .success(id: request.id, result: [
                "id": created.id.uuidString, "branch": created.branch, "path": created.path,
            ])

        case "notification.create":
            guard let title = request.params["title"], let body = request.params["body"] else {
                return .failure(id: request.id, error: "missing title/body")
            }
            postUserNotification(title: title, subtitle: request.params["subtitle"], body: body)
            return .success(id: request.id)

        case "notification.list":
            let rows = await deliveredNotificationRows()
            return .success(id: request.id, result: ["notifications": ControlRows.encode(rows)])

        case "notification.clear":
            clearDeliveredNotifications()
            return .success(id: request.id)

        case "session.restore":
            let restored = restoreLaunchSnapshot()
            return .success(id: request.id, result: ["restored": String(restored)])

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

    private func activePaneId(in worktree: Worktree) -> UUID? {
        activeTab(for: worktree.id)?.leafIds.first
    }

    /// "Active pane" of a worktree — first leaf of its active tab, the same
    /// heuristic splitCurrent() already uses.
}
