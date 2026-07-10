import Foundation
import GhosttyTerminal

/// Instrada i cmd+click sui link del terminale. TerminalViewState (final,
/// del package GhosttyTerminal) è il delegate della surface ma non
/// conforma TerminalSurfaceOpenURLDelegate: la conformance retroattiva
/// qui sotto smista al pane registrato via ObjectIdentifier dello state.
@MainActor
enum TerminalOpenURLRouter {
    private static var handlers: [ObjectIdentifier: (String) -> Void] = [:]

    static func register(_ state: TerminalViewState, handler: @escaping (String) -> Void) {
        handlers[ObjectIdentifier(state)] = handler
    }

    static func unregister(_ state: TerminalViewState) {
        handlers[ObjectIdentifier(state)] = nil
    }

    static func route(from state: TerminalViewState, url: String) {
        handlers[ObjectIdentifier(state)]?(url)
    }
}

// ponytail: conformance retroattiva a un protocollo di un altro package —
// se un futuro libghostty-spm aggiunge questa conformance il compilatore
// segnala il conflitto e questo file va rimosso a favore dell’upstream.
extension TerminalViewState: @retroactive TerminalSurfaceOpenURLDelegate {
    public func terminalDidRequestOpenURL(_ url: String, kind: TerminalOpenURLKind) {
        TerminalOpenURLRouter.route(from: self, url: url)
    }
}
