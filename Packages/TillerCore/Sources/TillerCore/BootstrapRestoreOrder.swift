import Foundation

/// Splits the worktrees loaded at launch into the ones needed for the first
/// interactive paint and the ones that can be restored afterwards.
///
/// Only worktrees in `openWorktreeIds` actually mount a terminal host, so
/// restoring the rest before publishing the selection keeps the content area
/// empty for work nobody is waiting on. Ordering the selected worktree first
/// makes time-to-interactive depend on the panes the user left open rather
/// than on how many worktrees exist in total.
public enum BootstrapRestoreOrder {
    public static func partition(
        worktrees: [Worktree],
        openWorktreeIds: [UUID],
        selectedWorktreeId: UUID?
    ) -> (priority: [Worktree], deferred: [Worktree]) {
        let byId = Dictionary(
            worktrees.map { ($0.id, $0) },
            uniquingKeysWith: { first, _ in first }
        )

        var orderedIds: [UUID] = []
        var seen: Set<UUID> = []
        // A stored selection outside the open set never mounts, so it earns
        // no priority — openWorktreeIds is what drives mounting.
        if let selectedWorktreeId, openWorktreeIds.contains(selectedWorktreeId) {
            orderedIds.append(selectedWorktreeId)
            seen.insert(selectedWorktreeId)
        }
        for id in openWorktreeIds where !seen.contains(id) {
            seen.insert(id)
            orderedIds.append(id)
        }

        let priority = orderedIds.compactMap { byId[$0] }
        let priorityIds = Set(priority.map(\.id))
        return (priority, worktrees.filter { !priorityIds.contains($0.id) })
    }
}
