import SwiftUI
import Foundation
import TillerCore

/// Slim bottom bar showing usage for each enabled provider (Claude, Codex,
/// OpenCode Go, Ollama Cloud), e.g. `Claude 26% 5h · 53% wk · 66% Fable`.
struct UsageBarView: View {
    let store: UsageStore
    let worktree: Worktree?
    @AppStorage("usage.codex.showInBar") private var showCodexInBar = true
    @AppStorage("usage.opencodeGo.showInBar") private var showOpencodeGoInBar = false
    @AppStorage("usage.ollamaCloud.showInBar") private var showOllamaCloudInBar = false

    var body: some View {
        HStack(spacing: 12) {
            Button {
                Task { await store.refreshAll() }
            } label: {
                Image(systemName: "arrow.clockwise")
                    .imageScale(.small)
                    .rotationEffect(.degrees(isLoading ? 360 : 0))
                    .animation(isLoading ? .linear(duration: 1).repeatForever(autoreverses: false)
                                         : .default, value: isLoading)
            }
            .buttonStyle(.plain)
            .help("Refresh usage")
            ClaudeUsageSegment(state: store.claude)
            if showCodexInBar {
                ProviderUsageSegment(
                    displayName: "Codex", agentId: "codex", state: store.codex,
                    loadedTooltip: "Codex subscription usage (5h · weekly)",
                    unavailableTooltip: codexTooltip)
            }
            if showOpencodeGoInBar {
                ProviderUsageSegment(
                    displayName: "OpenCode Go", agentId: "opencode", state: store.opencodeGo,
                    loadedTooltip: "OpenCode Go usage (5h · weekly · monthly)",
                    unavailableTooltip: opencodeGoTooltip)
            }
            if showOllamaCloudInBar {
                ProviderUsageSegment(
                    displayName: "Ollama Cloud", agentId: "ollama", state: store.ollamaCloud,
                    loadedTooltip: "Ollama Cloud usage",
                    unavailableTooltip: ollamaCloudTooltip)
            }
            Spacer()
            if let worktree {
                WorktreeContextSegment(worktree: worktree)
            }
        }
        .font(.system(size: 10))
        .padding(.horizontal, 12)
        .padding(.vertical, 2)
        .frame(maxWidth: .infinity)
        .background(SidebarMaterialContainer())
    }

    private var isLoading: Bool {
        if case .loading = store.claude { return true }
        if case .loading = store.codex { return true }
        if case .loading = store.opencodeGo { return true }
        if case .loading = store.ollamaCloud { return true }
        return false
    }

    private func codexTooltip(_ reason: UsageReason) -> String {
        switch reason {
        case .notInstalled: return "Codex CLI not found on PATH."
        case .loggedOut: return "Not logged in — run `codex login`."
        case .timedOut: return "Timed out reading Codex usage."
        case .error: return "Failed to load Codex usage."
        }
    }

    private func opencodeGoTooltip(_ reason: UsageReason) -> String {
        switch reason {
        case .notInstalled: return "Not configured."
        case .loggedOut: return "No session cookie configured — add one in Settings."
        case .timedOut: return "Timed out reading OpenCode Go usage."
        case .error: return "Failed to load OpenCode Go usage."
        }
    }

    private func ollamaCloudTooltip(_ reason: UsageReason) -> String {
        switch reason {
        case .notInstalled: return "Not configured."
        case .loggedOut: return "No session cookie configured — add one in Settings."
        case .timedOut: return "Timed out reading Ollama Cloud usage."
        case .error: return "Failed to load Ollama Cloud usage."
        }
    }
}

private struct ClaudeUsageSegment: View {
    let state: ProviderUsageState

    var body: some View {
        HStack(spacing: 6) {
            AgentIcon(agentId: "claude", size: 10)
            Text(text)
        }
        .foregroundStyle(isDimmed ? AnyShapeStyle(.secondary) : AnyShapeStyle(.primary))
        .help(tooltip)
    }

    private var text: String {
        switch state {
        case .loading:
            return "Claude …"
        case .loaded(let u), .stale(let u):
            let parts = [
                u.session.map { "\($0.usedPercent)% 5h" },
                u.weekly.map { "\($0.usedPercent)% wk" },
                u.fableWeekly.map { "\($0.usedPercent)% Fable" }
            ].compactMap { $0 }
            return parts.isEmpty ? "Claude —" : "Claude " + parts.joined(separator: " · ")
        case .unavailable:
            return "Claude —"
        }
    }

    private var isDimmed: Bool {
        switch state {
        case .loaded: return false
        case .loading, .stale, .unavailable: return true
        }
    }

    private var tooltip: String {
        switch state {
        case .loading: return "Loading Claude usage…"
        case .loaded, .stale: return "Claude Code subscription usage (5h · weekly · Fable)"
        case .unavailable(.notInstalled): return "Claude CLI not found on PATH."
        case .unavailable(.loggedOut): return "Not logged in — run `claude` and sign in."
        case .unavailable(.timedOut): return "Timed out reading Claude usage."
        case .unavailable(.error): return "Failed to load Claude usage."
        }
    }
}

/// Reusable bar segment for any provider whose `ProviderUsage` only needs
/// session/weekly/monthly (no Fable-style extra window — that stays
/// Claude-specific in `ClaudeUsageSegment`).
private struct ProviderUsageSegment: View {
    let displayName: String
    let agentId: String
    let state: ProviderUsageState
    let loadedTooltip: String
    let unavailableTooltip: (UsageReason) -> String

    var body: some View {
        HStack(spacing: 6) {
            AgentIcon(agentId: agentId, size: 10)
            Text(text)
        }
        .foregroundStyle(isDimmed ? AnyShapeStyle(.secondary) : AnyShapeStyle(.primary))
        .help(tooltip)
    }

    private var text: String {
        switch state {
        case .loading:
            return "\(displayName) …"
        case .loaded(let u), .stale(let u):
            // Why: the window's own label (not a hardcoded "5h"/"wk"/"mo")
            // so a provider whose session window isn't 5 hours — e.g.
            // Ollama Cloud's speculative single "usage" window — isn't
            // mislabeled.
            let parts = [
                u.session.map { "\($0.usedPercent)% \($0.label)" },
                u.weekly.map { "\($0.usedPercent)% \($0.label)" },
                u.monthly.map { "\($0.usedPercent)% \($0.label)" }
            ].compactMap { $0 }
            return parts.isEmpty ? "\(displayName) —" : "\(displayName) " + parts.joined(separator: " · ")
        case .unavailable:
            return "\(displayName) —"
        }
    }

    private var isDimmed: Bool {
        switch state {
        case .loaded: return false
        case .loading, .stale, .unavailable: return true
        }
    }

    private var tooltip: String {
        switch state {
        case .loading: return "Loading \(displayName) usage…"
        case .loaded, .stale: return loadedTooltip
        case .unavailable(let reason): return unavailableTooltip(reason)
        }
    }
}

/// Branch + abbreviated path of the selected worktree — shown only when a
/// worktree is selected, so the bar degrades to just provider usage
/// otherwise (same as today when `openWorktreeIds` is empty).
private struct WorktreeContextSegment: View {
    let worktree: Worktree

    var body: some View {
        Text("\(worktree.branch) · \(abbreviatedPath)")
            .foregroundStyle(AppTheme.meta)
    }

    private var abbreviatedPath: String {
        (worktree.path as NSString).abbreviatingWithTildeInPath
    }
}
