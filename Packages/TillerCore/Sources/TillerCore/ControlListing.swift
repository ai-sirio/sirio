import Foundation

/// Pure row builders for the control socket's list responses. Rows are
/// `[String: String]` so they serialize into the protocol's string-map
/// results without a type bridge.
public enum ControlListing {
    public static func workspaceRows(
        projects: [Project],
        worktrees: [UUID: [Worktree]],
        selectedWorktreeId: UUID?
    ) -> [[String: String]] {
        projects
            .sorted { $0.name.localizedCaseInsensitiveCompare($1.name) == .orderedAscending }
            .flatMap { project in
                (worktrees[project.id] ?? [])
                    .sorted { $0.branch.localizedCaseInsensitiveCompare($1.branch) == .orderedAscending }
                    .map { wt in
                        [
                            "id": wt.id.uuidString,
                            "project": project.name,
                            "branch": wt.branch,
                            "path": wt.path,
                            "selected": wt.id == selectedWorktreeId ? "true" : "false",
                        ]
                    }
            }
    }

    /// Returns already-resolved panel rows unchanged. Resolution belongs to
    /// the caller because legacy panes and universal-engine terminals have
    /// different identity sources.
    public static func paneRows(_ rows: [[String: String]]) -> [[String: String]] {
        rows
    }
}
