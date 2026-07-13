import Foundation

/// Decides which mounted worktrees (`AppModel.openWorktreeIds`) to unmount
/// once their count exceeds an opt-in cap. Unmounting terminates a
/// worktree's PTYs, so eviction only ever considers worktrees that are not
/// selected, have no running or input-waiting agent, and have no unsaved
/// work — evicted oldest-opened first since `openWorktreeIds` carries no
/// separate recency signal.
public enum WorktreeMountPolicy {
    public static func idsToEvict(
        openWorktreeIds: [UUID],
        selectedWorktreeId: UUID?,
        cap: Int,
        status: (UUID) -> AgentStatus?,
        hasUnsavedWork: (UUID) -> Bool
    ) -> [UUID] {
        guard cap > 0, openWorktreeIds.count > cap else { return [] }

        var mountedCount = openWorktreeIds.count
        var evicted: [UUID] = []
        for id in openWorktreeIds {
            guard mountedCount > cap else { break }
            guard id != selectedWorktreeId else { continue }
            switch status(id) {
            case .running, .needsInput: continue
            case .done, .error, nil: break
            }
            guard !hasUnsavedWork(id) else { continue }
            evicted.append(id)
            mountedCount -= 1
        }
        return evicted
    }
}
