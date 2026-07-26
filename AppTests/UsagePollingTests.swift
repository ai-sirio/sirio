import Testing
import Foundation
import TillerCore
@testable import Tiller

/// Fixed preferences, so a test never reads the app domain and never lets the
/// immediate `refreshAll()` reach a real fetcher (they spawn shells).
private struct StubPreferences: UsagePreferencesReading {
    var bools: [String: Bool] = [:]
    var ints: [String: Int] = [:]

    init(enabled: Set<UsageProvider> = [], refreshSeconds: Int = 0) {
        for provider in UsageProvider.allCases {
            bools[provider.showInBarKey] = enabled.contains(provider)
        }
        ints["usage.refreshIntervalSeconds"] = refreshSeconds
    }

    func bool(forKey key: String) -> Bool { bools[key] ?? false }
    func integer(forKey key: String) -> Int { ints[key] ?? 0 }
    func string(forKey key: String) -> String? { nil }
}

@MainActor
struct UsagePollingTests {
    @Test func noEnabledProviderArmsNoTimer() {
        let store = UsageStore(defaults: StubPreferences())
        store.updatePolling(enabledProviders: [])
        #expect(store.isPolling == false)
        #expect(store.timerArmCount == 0)
    }

    @Test func firstEnabledProviderArmsExactlyOneTimer() {
        let store = UsageStore(defaults: StubPreferences())
        store.updatePolling(enabledProviders: [.claude])
        store.updatePolling(enabledProviders: [.claude, .codex])
        store.updatePolling(enabledProviders: [.claude, .codex, .ollamaCloud])
        #expect(store.isPolling)
        #expect(store.timerArmCount == 1)
        store.updatePolling(enabledProviders: [])
    }

    @Test func disablingTheLastProviderCancelsTheTimer() {
        let store = UsageStore(defaults: StubPreferences())
        store.updatePolling(enabledProviders: [.claude, .codex])
        #expect(store.isPolling)
        store.updatePolling(enabledProviders: [.codex])
        #expect(store.isPolling)
        store.updatePolling(enabledProviders: [])
        #expect(store.isPolling == false)
        #expect(store.timerArmCount == 1)
    }

    /// An interval change cancels and re-arms, so it must not leave two
    /// timers running.
    @Test func cancelThenReArmNeverStacksTimers() {
        let store = UsageStore(defaults: StubPreferences())
        store.updatePolling(enabledProviders: [.claude])
        store.updatePolling(enabledProviders: [])
        store.updatePolling(enabledProviders: [.claude])
        store.updatePolling(enabledProviders: [.claude])
        #expect(store.isPolling)
        #expect(store.timerArmCount == 2)
        store.updatePolling(enabledProviders: [])
        #expect(store.isPolling == false)
    }

    @Test func restartingWithNoProviderEnabledLeavesTheTimerOff() {
        let store = UsageStore(defaults: StubPreferences())
        store.restartTimer()
        #expect(store.isPolling == false)
        #expect(store.timerArmCount == 0)
    }

    /// Only reads the toggles — deliberately never arms, so no fetcher runs.
    @Test func enabledProvidersReflectsStoredToggles() {
        #expect(UsageStore(defaults: StubPreferences()).enabledProviders.isEmpty)
        let store = UsageStore(defaults: StubPreferences(enabled: [.codex, .ollamaCloud]))
        #expect(store.enabledProviders == [.codex, .ollamaCloud])
    }
}
