import Foundation

/// Esegue un'azione una sola volta, qualunque sia il primo chiamante a
/// invocarla. Usato nel quit path: la reply di terminazione va postata appena
/// il flush dello scrollback finisce OPPURE alla scadenza di un deadline di
/// sicurezza — mai due volte. Confinato al MainActor, così i due task che
/// corrono (flush e deadline) accedono al flag in modo serializzato.
@MainActor
public final class OnceGate {
    private var fired = false

    public init() {}

    public func fireOnce(_ body: () -> Void) {
        guard !fired else { return }
        fired = true
        body()
    }
}
