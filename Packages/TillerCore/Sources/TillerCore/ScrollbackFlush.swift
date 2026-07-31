import Foundation

/// Coppie (worktreeId, paneId) da snapshottare al quit: tutti i pane live
/// di tutte le worktree. Pura e testabile; l'orchestrazione I/O (snapshot +
/// save) vive in AppModel, che risolve i pane live per worktree prima di
/// chiamare questa funzione.
///
/// `paneIds` è un Dictionary: l'ordine tra worktree diverse non è definito.
public func scrollbackFlushTargets(
    paneIds: [UUID: [UUID]]
) -> [(worktreeId: UUID, paneId: UUID)] {
    var out: [(worktreeId: UUID, paneId: UUID)] = []
    for (worktreeId, ids) in paneIds {
        for paneId in ids {
            out.append((worktreeId, paneId))
        }
    }
    return out
}
