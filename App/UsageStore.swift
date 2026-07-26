import Foundation
import TillerCore
import TillerTerminal

/// The slice of preferences `UsageStore` reads. Exists so tests can drive
/// the polling lifecycle from a fixed set of values: a `UserDefaults` suite
/// is not isolated — its search list still falls back to the app domain, so
/// a test using one would read the developer's real toggles and spawn real
/// fetchers.
protocol UsagePreferencesReading {
    func bool(forKey key: String) -> Bool
    func integer(forKey key: String) -> Int
    func string(forKey key: String) -> String?
}

extension UserDefaults: UsagePreferencesReading {}

/// Owns the latest usage state for all four tracked providers (Claude,
/// Codex, OpenCode Go, Ollama Cloud), a shared refresh timer, and
/// per-provider single-flight fetching. Risky mapping logic lives in
/// `UsageStateReducer` (TillerCore); this is plumbing around it.
@MainActor
@Observable
final class UsageStore {
    var claude: ProviderUsageState = .loading
    /// Timestamp of the last successful (.loaded) fetch, for the settings row.
    var lastClaudeUpdate: Date?

    var codex: ProviderUsageState = .loading
    var lastCodexUpdate: Date?
    private var isFetchingCodex = false

    var opencodeGo: ProviderUsageState = .loading
    var lastOpencodeGoUpdate: Date?
    private var isFetchingOpencodeGo = false

    var ollamaCloud: ProviderUsageState = .loading
    var lastOllamaCloudUpdate: Date?
    private var isFetchingOllamaCloud = false

    private var isFetching = false
    /// nonisolated(unsafe): accessed only on @MainActor; deinit needs
    /// access to cancel the timer.
    private nonisolated(unsafe) var timer: Task<Void, Never>?

    private let defaults: UsagePreferencesReading

    init(defaults: UsagePreferencesReading = UserDefaults.standard) {
        self.defaults = defaults
    }

    /// Refresh cadence, read from prefs each cycle and clamped so a corrupt
    /// value can never drive the timer.
    private var refreshInterval: Duration {
        let stored = defaults.integer(forKey: "usage.refreshIntervalSeconds")
        let seconds = stored == 0 ? AppSettings.defaultRefreshSeconds : AppSettings.clampRefresh(stored)
        return .seconds(seconds)
    }

    /// True while a refresh timer is armed. Nothing wakes up when false.
    var isPolling: Bool { timer != nil }

    /// How many times a timer has been armed. Lets tests prove the
    /// reconciliation is idempotent rather than merely non-crashing.
    private(set) var timerArmCount = 0

    /// Providers whose usage bar toggle is currently on.
    var enabledProviders: Set<UsageProvider> {
        Set(UsageProvider.allCases.filter { defaults.bool(forKey: $0.showInBarKey) })
    }

    /// Reconciles the timer with the set of providers that want polling:
    /// arms one timer for a non-empty set, cancels it for an empty one.
    /// Idempotent — calling it repeatedly with providers enabled keeps the
    /// timer that is already running rather than stacking another.
    func updatePolling(enabledProviders: Set<UsageProvider>) {
        guard !enabledProviders.isEmpty else {
            timer?.cancel()
            timer = nil
            return
        }
        guard timer == nil else { return }
        timerArmCount += 1
        Task { await refreshAll() }
        timer = Task { [weak self] in
            while !Task.isCancelled {
                guard let interval = self?.refreshInterval else { return }
                try? await Task.sleep(for: interval)
                await self?.refreshAll()
            }
        }
    }

    /// Reconciles the timer against the toggles currently stored in prefs.
    func updatePolling() {
        updatePolling(enabledProviders: enabledProviders)
    }

    /// Cancel and re-arm the timer so a changed interval applies immediately.
    func restartTimer() {
        timer?.cancel()
        timer = nil
        updatePolling()
    }

    func refresh() async {
        guard defaults.bool(forKey: UsageProvider.claude.showInBarKey) else { return }
        guard !isFetching else { return }
        isFetching = true
        defer { isFetching = false }
        let outcome = await ClaudeUsageFetcher.fetch()
        claude = UsageStateReducer.reduce(outcome: outcome, previous: claude)
        if case .loaded = claude { lastClaudeUpdate = Date() }
    }

    func refreshCodex() async {
        guard defaults.bool(forKey: UsageProvider.codex.showInBarKey) else { return }
        guard !isFetchingCodex else { return }
        isFetchingCodex = true
        defer { isFetchingCodex = false }
        let outcome = await CodexUsageFetcher.fetch()
        codex = UsageStateReducer.reduce(outcome: outcome, previous: codex)
        if case .loaded = codex { lastCodexUpdate = Date() }
    }

    /// Refreshes every provider currently enabled. Each provider's own
    /// `refreshX()` guards on its own `showInBar` pref and single-flight
    /// flag, so this simply fans them out concurrently.
    func refreshAll() async {
        async let claudeTask: Void = refresh()
        async let codexTask: Void = refreshCodex()
        async let opencodeGoTask: Void = refreshOpencodeGo()
        async let ollamaCloudTask: Void = refreshOllamaCloud()
        _ = await (claudeTask, codexTask, opencodeGoTask, ollamaCloudTask)
    }

    func refreshOpencodeGo() async {
        guard defaults.bool(forKey: UsageProvider.opencodeGo.showInBarKey) else { return }
        guard !isFetchingOpencodeGo else { return }
        isFetchingOpencodeGo = true
        defer { isFetchingOpencodeGo = false }
        let override = defaults.string(forKey: "usage.opencodeGo.workspaceIdOverride")
        let outcome = await OpenCodeGoUsageFetcher.fetch(workspaceIdOverride: override)
        opencodeGo = UsageStateReducer.reduce(outcome: outcome, previous: opencodeGo)
        if case .loaded = opencodeGo { lastOpencodeGoUpdate = Date() }
    }

    func refreshOllamaCloud() async {
        guard defaults.bool(forKey: UsageProvider.ollamaCloud.showInBarKey) else { return }
        guard !isFetchingOllamaCloud else { return }
        isFetchingOllamaCloud = true
        defer { isFetchingOllamaCloud = false }
        let outcome = await OllamaCloudUsageFetcher.fetch()
        ollamaCloud = UsageStateReducer.reduce(outcome: outcome, previous: ollamaCloud)
        if case .loaded = ollamaCloud { lastOllamaCloudUpdate = Date() }
    }

    deinit { timer?.cancel() }
}
