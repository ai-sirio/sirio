import Foundation
import TillerCore
import TillerWorkspace

enum WorkspaceRenderPath: Equatable {
    case legacyTerminal
    case workspace
}

struct WorkspaceMountPlan: Equatable {
    let mountedWorktreeIDs: [UUID]
    let selectedWorktreeID: UUID?

    init(openWorktreeIDs: [UUID], selectedWorktreeID: UUID?) {
        mountedWorktreeIDs = openWorktreeIDs
        self.selectedWorktreeID = selectedWorktreeID
    }

    static func detachedHostIDs(
        from allTabIDs: [WorkspaceTabID], mountedTabIDs: [WorkspaceTabID]
    ) -> [WorkspaceTabID] {
        let mounted = Set(mountedTabIDs)
        return allTabIDs.filter { !mounted.contains($0) }
    }
}

@MainActor
final class WorkspaceIntentRouter: WorkspaceIntentSink {
    private let coordinator: WorkspaceCoordinator
    private let worktree: Worktree

    init(coordinator: WorkspaceCoordinator, worktree: Worktree) {
        self.coordinator = coordinator
        self.worktree = worktree
    }

    func send(_ intent: WorkspaceIntent) {
        Task { @MainActor [coordinator, worktree] in
            await coordinator.handle(intent, in: worktree)
        }
    }
}
