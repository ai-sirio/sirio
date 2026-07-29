import Foundation

/// Coppie (worktreeId, paneId) da snapshottare al quit: tutte le leaf di
/// tutte le tab di tutte le worktree. Pura e testabile; l'orchestrazione
/// I/O (snapshot + save) vive in AppModel.
///
/// `tabs` è un Dictionary: l'ordine tra worktree diverse non è definito.
public func scrollbackFlushTargets(
    tabs: [UUID: [LegacyWorkspaceTab]]
) -> [(worktreeId: UUID, paneId: UUID)] {
    var out: [(worktreeId: UUID, paneId: UUID)] = []
    for (worktreeId, tabList) in tabs {
        for tab in tabList {
            for paneId in tab.leafIds {
                out.append((worktreeId, paneId))
            }
        }
    }
    return out
}
