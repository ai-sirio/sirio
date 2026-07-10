import Testing
@testable import TillerCore

/// Probe finto: stati scriptati per kind, registra le perform ricevute.
private actor ProbeLog {
    var performed: [(PermissionAction, PermissionKind)] = []
    var statuses: [PermissionKind: PermissionStatus]
    init(statuses: [PermissionKind: PermissionStatus]) { self.statuses = statuses }
    func record(_ action: PermissionAction, _ kind: PermissionKind) {
        performed.append((action, kind))
    }
    func set(_ kind: PermissionKind, _ status: PermissionStatus) { statuses[kind] = status }
    func status(_ kind: PermissionKind) -> PermissionStatus { statuses[kind] ?? .checkManually }
}

private struct MockProbe: PermissionProbe {
    let log: ProbeLog
    func status(for kind: PermissionKind) async -> PermissionStatus {
        await log.status(kind)
    }
    func perform(_ action: PermissionAction, for kind: PermissionKind) async {
        await log.record(action, kind)
    }
}

@Test @MainActor func unknownStatusDefaultsToCheckManually() {
    let model = PermissionsModel(probe: MockProbe(log: ProbeLog(statuses: [:])))
    #expect(model.status(for: .notifications) == .checkManually)
}

@Test @MainActor func refreshLoadsEveryKind() async {
    let log = ProbeLog(statuses: [
        .notifications: .granted, .screenRecording: .denied,
        .accessibility: .notRequested, .fullDiskAccess: .checkManually,
        .automation: .notRequested, .localNetwork: .checkManually,
    ])
    let model = PermissionsModel(probe: MockProbe(log: log))
    await model.refresh()
    #expect(model.status(for: .notifications) == .granted)
    #expect(model.status(for: .screenRecording) == .denied)
    #expect(model.status(for: .accessibility) == .notRequested)
    #expect(model.status(for: .fullDiskAccess) == .checkManually)
    #expect(model.status(for: .automation) == .notRequested)
    #expect(model.status(for: .localNetwork) == .checkManually)
}

@Test @MainActor func actionDelegatesToPresentation() async {
    let log = ProbeLog(statuses: [.notifications: .notRequested])
    let model = PermissionsModel(probe: MockProbe(log: log))
    await model.refresh()
    #expect(model.action(for: .notifications) == .request)
    #expect(model.action(for: .fullDiskAccess) == .openSettings)
}

@Test @MainActor func performActionInvokesProbeThenRefreshesThatKind() async {
    let log = ProbeLog(statuses: [.screenRecording: .notRequested])
    let model = PermissionsModel(probe: MockProbe(log: log))
    await model.refresh()
    // L'utente clicka Request; il sistema (mock) ora risponde granted.
    await log.set(.screenRecording, .granted)
    await model.performAction(for: .screenRecording)
    let performed = await log.performed
    #expect(performed.count == 1)
    #expect(performed[0] == (.request, .screenRecording))
    #expect(model.status(for: .screenRecording) == .granted)
}
