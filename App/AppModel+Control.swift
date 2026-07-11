import Foundation
import AppKit
import TillerCore
import TillerControl
import TillerTerminal

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
        "workspace.list", "workspace.create", "workspace.select",
        "workspace.current", "workspace.close",
        "surface.list", "pane.surfaces", "surface.focus", "surface.split",
        "surface.send_text", "surface.send_key",
    ]

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
            let branch = request.params["branch"] ?? Self.generatedBranchName()
            let before = Set((worktrees[project.id] ?? []).map(\.id))
            await addWorktree(project: project, branch: branch)
            guard let created = (worktrees[project.id] ?? []).first(where: { !before.contains($0.id) }) else {
                return .failure(id: request.id, error: lastError ?? "workspace.create failed")
            }
            return .success(id: request.id, result: [
                "id": created.id.uuidString, "branch": created.branch, "path": created.path,
            ])

        case "surface.list", "pane.surfaces":
            guard let worktree = selectedWorktree else {
                return .failure(id: request.id, error: "no workspace selected")
            }
            let allTabs = tabs[worktree.id] ?? []
            let scope = request.method == "pane.surfaces"
                ? allTabs.filter { $0.id == activeTabId[worktree.id] }
                : allTabs
            let rows = ControlListing.paneRows(
                tabs: scope, activeTabId: activeTabId[worktree.id],
                agentIdForPane: { self.agentActivity.paneAgents[$0] },
                titleForPane: { self.paneTitles[$0] }
            )
            return .success(id: request.id, result: ["surfaces": ControlRows.encode(rows)])

        case "surface.focus":
            guard let paneId = request.params["surface"].flatMap(UUID.init(uuidString:)),
                  let tuple = tabContaining(paneId: paneId) else {
                return .failure(id: request.id, error: "unknown surface")
            }
            selectedWorktree = tuple.worktree
            activateTab(tuple.tab.id, in: tuple.worktree.id)
            NSApp.activate(ignoringOtherApps: true)
            NSApp.windows.first?.makeKeyAndOrderFront(nil)
            return .success(id: request.id)

        case "surface.split":
            let direction = request.params["direction"] ?? ""
            // ponytail: the split tree has no insert-before slot, so
            // left==right and up==down; revisit if position ever matters.
            let axis: SplitAxis
            switch direction {
            case "left", "right": axis = .horizontal
            case "up", "down": axis = .vertical
            default:
                return .failure(id: request.id, error: "invalid direction \(direction) (left|right|up|down)")
            }
            guard let worktree = selectedWorktree,
                  let target = activePaneId(in: worktree) else {
                return .failure(id: request.id, error: "no active pane to split")
            }
            split(paneId: target, axis: axis)
            return .success(id: request.id)

        case "surface.send_text":
            guard let text = request.params["text"] else {
                return .failure(id: request.id, error: "missing text")
            }
            guard let paneId = resolveTargetPane(request.params["surface"]) else {
                return .failure(id: request.id, error: "no target surface")
            }
            let wrote = await PaneRegistry.shared.write(paneId: paneId, data: Data(text.utf8))
            return wrote ? .success(id: request.id)
                         : .failure(id: request.id, error: "unknown surface")

        case "surface.send_key":
            guard let key = request.params["key"].flatMap(TerminalKey.init(rawValue:)) else {
                let names = TerminalKey.allCases.map(\.rawValue).joined(separator: "|")
                return .failure(id: request.id, error: "invalid key (\(names))")
            }
            guard let paneId = resolveTargetPane(request.params["surface"]) else {
                return .failure(id: request.id, error: "no target surface")
            }
            let wrote = await PaneRegistry.shared.write(paneId: paneId, data: key.bytes)
            return wrote ? .success(id: request.id)
                         : .failure(id: request.id, error: "unknown surface")

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

    /// Target pane for send/send-key: explicit surface param wins (the CLI
    /// already substituted TILLER_PANE_ID when run inside a pane), else the
    /// active pane of the selected worktree.
    func resolveTargetPane(_ explicit: String?) -> UUID? {
        if let explicit {
            return UUID(uuidString: explicit)
        }
        guard let worktree = selectedWorktree else { return nil }
        return activePaneId(in: worktree)
    }
}
