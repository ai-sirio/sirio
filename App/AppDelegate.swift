import AppKit
import TillerCore

/// Intercetta il quit graceful per flushare lo scrollback dei pane vivi
/// prima che il processo termini. `.terminateLater` è l'unico meccanismo
/// AppKit che fa attendere un lavoro async (actor snapshot + write GRDB).
final class AppDelegate: NSObject, NSApplicationDelegate {
    var model: AppModel?

    /// Deadline oltre il quale si risponde comunque al quit, anche se il flush
    /// non è finito. Serve al relaunch "Esci e riapri" dei permessi TCC: il
    /// coordinatore di sistema abbandona il rilancio se la terminazione resta
    /// appesa, quindi un flush bloccato non deve poterlo trattenere. 1.5s
    /// copre un flush normale; un flush interrotto perde al più uno snapshot
    /// (GRDB scrive in transazione atomica, nessuna corruzione).
    private static let replyDeadline: Duration = .milliseconds(1500)

    func applicationShouldTerminate(
        _ sender: NSApplication
    ) -> NSApplication.TerminateReply {
        guard let model else { return .terminateNow }
        let gate = OnceGate()
        let reply = { NSApp.reply(toApplicationShouldTerminate: true) }
        Task { @MainActor in
            await model.flushLiveScrollback()
            gate.fireOnce(reply)
        }
        Task { @MainActor in
            try? await Task.sleep(for: Self.replyDeadline)
            gate.fireOnce(reply)
        }
        return .terminateLater
    }
}
