import Foundation
import TillerCore
import TillerTerminal

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

    /// Refresh cadence, read from prefs each cycle and clamped so a corrupt
    /// value can never drive the timer.
    private var refreshInterval: Duration {
        let stored = UserDefaults.standard.integer(forKey: "usage.refreshIntervalSeconds")
        let seconds = stored == 0 ? AppSettings.defaultRefreshSeconds : AppSettings.clampRefresh(stored)
        return .seconds(seconds)
    }

    func start() {
        guard timer == nil else { return }
        Task { await refreshAll() }
        timer = Task { [weak self] in
            while !Task.isCancelled {
                guard let interval = self?.refreshInterval else { return }
                try? await Task.sleep(for: interval)
                await self?.refreshAll()
            }
        }
    }

    /// Cancel and re-arm the timer so a changed interval applies immediately.
    func restartTimer() {
        timer?.cancel()
        timer = nil
        start()
    }

    func refresh() async {
        guard UserDefaults.standard.bool(forKey: "usage.claude.showInBar") else { return }
        guard !isFetching else { return }
        isFetching = true
        defer { isFetching = false }
        let outcome = await ClaudeUsageFetcher.fetch()
        claude = UsageStateReducer.reduce(outcome: outcome, previous: claude)
        if case .loaded = claude { lastClaudeUpdate = Date() }
    }

    func refreshCodex() async {
        guard UserDefaults.standard.bool(forKey: "usage.codex.showInBar") else { return }
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
        guard UserDefaults.standard.bool(forKey: "usage.opencodeGo.showInBar") else { return }
        guard !isFetchingOpencodeGo else { return }
        isFetchingOpencodeGo = true
        defer { isFetchingOpencodeGo = false }
        let override = UserDefaults.standard.string(forKey: "usage.opencodeGo.workspaceIdOverride")
        let outcome = await OpenCodeGoUsageFetcher.fetch(workspaceIdOverride: override)
        opencodeGo = UsageStateReducer.reduce(outcome: outcome, previous: opencodeGo)
        if case .loaded = opencodeGo { lastOpencodeGoUpdate = Date() }
    }

    func refreshOllamaCloud() async {
        guard UserDefaults.standard.bool(forKey: "usage.ollamaCloud.showInBar") else { return }
        guard !isFetchingOllamaCloud else { return }
        isFetchingOllamaCloud = true
        defer { isFetchingOllamaCloud = false }
        let outcome = await OllamaCloudUsageFetcher.fetch()
        ollamaCloud = UsageStateReducer.reduce(outcome: outcome, previous: ollamaCloud)
        if case .loaded = ollamaCloud { lastOllamaCloudUpdate = Date() }
    }

    deinit { timer?.cancel() }
}
