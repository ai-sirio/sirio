import Foundation

/// Decide se una transizione di stato di un agente merita una notifica
/// nativa macOS. Puramente funzionale: nessun side effect, testabile in isolamento.
public enum NotificationPolicy {

    /// - Parameters:
    ///   - old: stato precedente (nil = prima transizione nota per il pane).
    ///   - new: stato appena ricevuto via tillerctl notify o watchExit.
    ///   - appActive: `NSApp.isActive` al momento della transizione.
    ///   - paneVisible: true se il pane è nel worktree attualmente selezionato.
    /// - Returns: true se la transizione va notificata (app in background o pane
    ///   in worktree non selezionato, stato terminale, transizione reale).
    public static func shouldNotify(
        old: AgentStatus?,
        new: AgentStatus,
        appActive: Bool,
        paneVisible: Bool
    ) -> Bool {
        guard new != .running else { return false }             // start = rumore
        guard old != new else { return false }                  // nessuna transizione reale
        guard !(appActive && paneVisible) else { return false } // l'utente la sta guardando
        return true
    }
}
