import Foundation

/// Riordino manuale drag&drop di una lista identificata da UUID: tab,
/// progetti e worktree condividono questa sola implementazione, così la
/// correzione dell'indice slittato non può divergere fra le tre.
public enum ManualOrder {
    /// Rimuove `id` e lo reinserisce immediatamente prima di `targetId`
    /// (`nil` = in coda). Id o target sconosciuti → lista invariata.
    public static func moving<T: Identifiable>(
        _ list: [T], id: T.ID, before targetId: T.ID?
    ) -> [T] {
        guard id != targetId,
              let sourceIndex = list.firstIndex(where: { $0.id == id })
        else { return list }
        if let targetId, !list.contains(where: { $0.id == targetId }) { return list }
        var result = list
        let item = result.remove(at: sourceIndex)
        // Il target va ricercato dopo la rimozione: se la sorgente stava
        // prima del target, l'indice del target è slittato di uno.
        if let targetId, let targetIndex = result.firstIndex(where: { $0.id == targetId }) {
            result.insert(item, at: targetIndex)
        } else {
            result.append(item)
        }
        return result
    }
}
