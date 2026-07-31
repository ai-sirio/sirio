import Foundation

/// Operazioni pure sull'ordine delle tab di un worktree: riordino drag&drop,
/// ciclo ⌃Tab e selezione ⌘n. Nessuno stato: AppModel applica il risultato.
public enum TabOrdering {
    /// Rimuove `id` e lo reinserisce immediatamente prima di `targetId`
    /// (`nil` = in coda). Id o target sconosciuti → lista invariata.
    public static func moving<T: Identifiable>(
        _ list: [T], id: T.ID, before targetId: T.ID?
    ) -> [T] {
        ManualOrder.moving(list, id: id, before: targetId)
    }

    /// Indice successivo/precedente con wrap-around; `current == nil` parte
    /// dal bordo. Lista vuota → nil.
    public static func cycledIndex(current: Int?, forward: Bool, count: Int) -> Int? {
        guard count > 0 else { return nil }
        guard let current else { return forward ? 0 : count - 1 }
        return forward ? (current + 1) % count : (current - 1 + count) % count
    }

    /// ⌘n → indice tab: 9 = ultima (stile browser), n entro il conteggio →
    /// n-1, altrimenti nil (no-op).
    public static func selectionIndex(number: Int, count: Int) -> Int? {
        guard count > 0 else { return nil }
        if number == 9 { return count - 1 }
        guard number >= 1, number <= count else { return nil }
        return number - 1
    }
}
