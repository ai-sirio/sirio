import Foundation
import Observation

/// Sonda un permesso e ne esegue l'azione. L'implementazione reale
/// (SystemPermissionProbe, nel target App) fa le chiamate TCC; nei test si
/// usa un mock. Tenere le chiamate di sistema dietro questo protocol è ciò
/// che rende PermissionsModel testabile in CI.
public protocol PermissionProbe: Sendable {
    func status(for kind: PermissionKind) async -> PermissionStatus
    func perform(_ action: PermissionAction, for kind: PermissionKind) async
}

/// Stato osservabile della pagina permessi. Nessun refresh automatico
/// all'avvio: chi presenta la vista chiama refresh() (e ri-chiama quando
/// l'app torna in foreground, per raccogliere le modifiche fatte in
/// System Settings).
@MainActor @Observable
public final class PermissionsModel {
    public private(set) var statuses: [PermissionKind: PermissionStatus] = [:]
    private let probe: any PermissionProbe

    public init(probe: any PermissionProbe) {
        self.probe = probe
    }

    public func status(for kind: PermissionKind) -> PermissionStatus {
        statuses[kind] ?? .checkManually
    }

    public func action(for kind: PermissionKind) -> PermissionAction {
        PermissionPresentation.action(for: kind, status: status(for: kind))
    }

    public func refresh() async {
        for kind in PermissionKind.allCases {
            statuses[kind] = await probe.status(for: kind)
        }
    }

    /// Esegue l'azione corrente della riga e rilegge lo stato di quel solo
    /// permesso (i prompt di sistema sono modali: al ritorno lo stato può
    /// essere cambiato).
    public func performAction(for kind: PermissionKind) async {
        await probe.perform(action(for: kind), for: kind)
        statuses[kind] = await probe.status(for: kind)
    }
}
