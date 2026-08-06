import Foundation
import TillerCore

/// Assembles the Activity list from live AppModel state across every mounted
/// worktree.
@MainActor
enum ActivityPanelModel {
    /// Iterates `openWorktreeIds`, not the selected worktree: only mounted
    /// worktrees have live PTYs, and all of them are in scope.
    static func rows(appModel: AppModel) -> [ActivityRow] {
        let inputs = appModel.openWorktreeIds.compactMap {
            input(worktreeId: $0, appModel: appModel)
        }
        return ActivityListBuilder.build(
            worktrees: inputs,
            agentStatus: appModel.agentActivity.agentStatus,
            paneAgents: appModel.agentActivity.paneAgents)
    }

    /// A worktree id with no worktree or no layout is skipped rather than
    /// treated as an error: both are ordinary transient states while a
    /// worktree is being closed.
    private static func input(
        worktreeId: UUID, appModel: AppModel
    ) -> ActivityListBuilder.WorktreeInput? {
        guard let worktree = appModel.worktree(byId: worktreeId),
              let layout = appModel.workspaceCoordinator.layouts[worktreeId]
        else { return nil }
        let tabs = layout.allTabs
        var livePaneIds: [WorkspaceTabID: UUID] = [:]
        for tab in tabs {
            guard case .terminal(let contentID) = tab.content else { continue }
            // Assigning nil removes the key, which is exactly what a terminal
            // tab with no mounted pane should leave behind.
            livePaneIds[tab.id] = appModel.workspaceCoordinator.liveControlPaneId(
                contentID: contentID, in: worktreeId)
        }
        return ActivityListBuilder.WorktreeInput(
            worktreeId: worktreeId, label: label(for: worktree, appModel: appModel),
            tabs: tabs, livePaneIds: livePaneIds)
    }

    /// "project/branch", degrading to the bare branch when the project row is
    /// gone — a worktree briefly outlives its project during removal, and a
    /// half-labelled row beats a crash.
    static func label(for worktree: Worktree, appModel: AppModel) -> String {
        guard let project = appModel.projects.first(where: { $0.id == worktree.projectId })
        else { return worktree.branch }
        let name = project.displayName.flatMap { $0.isEmpty ? nil : $0 } ?? project.name
        return "\(name)/\(worktree.branch)"
    }
}
