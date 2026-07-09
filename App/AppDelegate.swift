import AppKit

/// Intercetta il quit graceful per flushare lo scrollback dei pane vivi
/// prima che il processo termini. `.terminateLater` è l'unico meccanismo
/// AppKit che fa attendere un lavoro async (actor snapshot + write GRDB).
final class AppDelegate: NSObject, NSApplicationDelegate {
    var model: AppModel?

    func applicationShouldTerminate(
        _ sender: NSApplication
    ) -> NSApplication.TerminateReply {
        guard let model else { return .terminateNow }
        Task { @MainActor in
            await model.flushLiveScrollback()
            NSApp.reply(toApplicationShouldTerminate: true)
        }
        return .terminateLater
    }
}
