import Foundation

/// Sorgente di transcript recente per l'auto-naming. Implementazioni:
/// `ChatTranscriptSource` (App, chat tab ACP), `FileTranscriptSource`
/// (TillerAgents, terminal tab con adapter a hook nativi).
///
/// Chiamato sempre in modo sincrono dal chiamante prima di passare il
/// risultato a un contesto concorrente — nessuna garanzia Sendable richiesta
/// sul conformante stesso (permette a `ChatTranscriptSource` di restare
/// @MainActor senza friction).
public protocol TranscriptSource {
    /// Testo piano delle ultime battute della conversazione, o nil quando
    /// non disponibile (file assente, sessione vuota, tab senza sessione).
    func recentText() -> String?
}
