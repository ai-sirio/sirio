import Testing
@testable import TillerCore

@Test func usageProviderKeysMatchTheStoredPreferences() {
    // These exact keys are registered in TillerApp and read by @AppStorage
    // in the settings and usage bar views.
    #expect(UsageProvider.claude.showInBarKey == "usage.claude.showInBar")
    #expect(UsageProvider.codex.showInBarKey == "usage.codex.showInBar")
    #expect(UsageProvider.opencodeGo.showInBarKey == "usage.opencodeGo.showInBar")
    #expect(UsageProvider.ollamaCloud.showInBarKey == "usage.ollamaCloud.showInBar")
}

@Test func usageProviderCoversEveryPolledProvider() {
    #expect(UsageProvider.allCases.count == 4)
    #expect(Set(UsageProvider.allCases.map(\.showInBarKey)).count == 4)
}
