import Testing
@testable import TillerCore

@Test func sixKindsInOrcaOrder() {
    #expect(PermissionKind.allCases == [
        .notifications, .screenRecording, .accessibility,
        .fullDiskAccess, .automation, .localNetwork,
    ])
}

@Test func badgeLabels() {
    #expect(PermissionStatus.granted.badgeLabel == "GRANTED")
    #expect(PermissionStatus.denied.badgeLabel == "DENIED")
    #expect(PermissionStatus.notRequested.badgeLabel == "NOT REQUESTED")
    #expect(PermissionStatus.checkManually.badgeLabel == "CHECK MANUALLY")
}

@Test func actionLabels() {
    #expect(PermissionAction.request.label == "Request")
    #expect(PermissionAction.triggerPrompt.label == "Trigger Prompt")
    #expect(PermissionAction.openSettings.label == "Open Settings")
}

@Test func promptableKindsRequestOnlyWhenNotRequested() {
    for kind in [PermissionKind.notifications, .screenRecording, .accessibility] {
        #expect(PermissionPresentation.action(for: kind, status: .notRequested) == .request)
        #expect(PermissionPresentation.action(for: kind, status: .granted) == .openSettings)
        #expect(PermissionPresentation.action(for: kind, status: .denied) == .openSettings)
        #expect(PermissionPresentation.action(for: kind, status: .checkManually) == .openSettings)
    }
}

@Test func fullDiskAccessAlwaysOpensSettings() {
    for status in [PermissionStatus.granted, .denied, .notRequested, .checkManually] {
        #expect(PermissionPresentation.action(for: .fullDiskAccess, status: status) == .openSettings)
    }
}

@Test func automationTriggersUntilResolved() {
    #expect(PermissionPresentation.action(for: .automation, status: .notRequested) == .triggerPrompt)
    #expect(PermissionPresentation.action(for: .automation, status: .checkManually) == .triggerPrompt)
    // Dopo un esito definitivo il prompt Apple Events non riappare: solo System Settings.
    #expect(PermissionPresentation.action(for: .automation, status: .granted) == .openSettings)
    #expect(PermissionPresentation.action(for: .automation, status: .denied) == .openSettings)
}

@Test func localNetworkAlwaysTriggers() {
    for status in [PermissionStatus.granted, .denied, .notRequested, .checkManually] {
        #expect(PermissionPresentation.action(for: .localNetwork, status: status) == .triggerPrompt)
    }
}

@Test func everyKindHasDisplayCopy() {
    for kind in PermissionKind.allCases {
        #expect(!kind.title.isEmpty)
        #expect(!kind.symbol.isEmpty)
        #expect(!kind.detail.isEmpty)
    }
}
