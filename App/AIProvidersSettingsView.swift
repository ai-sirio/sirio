import SwiftUI
import TillerCore
import TillerPersistence

/// AI provider accounts: status, bar visibility, and refresh for all four
/// tracked providers (Claude, Codex: zero-config; OpenCode Go, Ollama
/// Cloud: cookie configured via Keychain).
struct AIProvidersSettingsView: View {
    let store: UsageStore
    let accounts: AgentAccountStore?

    @AppStorage("usage.claude.showInBar") private var showClaudeInBar = true
    @AppStorage("usage.codex.showInBar") private var showCodexInBar = true
    @AppStorage("usage.opencodeGo.showInBar") private var showOpencodeGoInBar = false
    @State private var opencodeGoCookieInput = ""
    @State private var opencodeGoKeychainError = false
    @AppStorage("usage.opencodeGo.workspaceIdOverride") private var workspaceIdOverride = ""
    @AppStorage("usage.ollamaCloud.showInBar") private var showOllamaCloudInBar = false
    @State private var ollamaCloudCookieInput = ""
    @State private var ollamaCloudKeychainError = false
    @AppStorage("usage.refreshIntervalSeconds") private var intervalSeconds = 300

    var body: some View {
        Form {
            Section("Claude Code") {
                LabeledContent {
                    HStack(spacing: 6) {
                        Circle().fill(statusColor).frame(width: 8, height: 8)
                        Text(statusText)
                    }
                } label: {
                    HStack(spacing: 8) {
                        AgentIcon(agentId: "claude", size: 18)
                        Text("Status")
                    }
                }
                if let updated = store.lastClaudeUpdate {
                    LabeledContent("Last read", value: updated.formatted(date: .omitted, time: .shortened))
                }
                Toggle("Show in usage bar", isOn: $showClaudeInBar)
                Picker("Refresh interval", selection: $intervalSeconds) {
                    Text("1 min").tag(60)
                    Text("5 min").tag(300)
                    Text("15 min").tag(900)
                    Text("30 min").tag(1800)
                    Text("60 min").tag(3600)
                }
                Button("Refresh now") {
                    Task { await store.refresh() }
                }
                if let accounts {
                    AgentAccountsBlock(
                        title: "Accounts",
                        accounts: accounts.claudeAccounts,
                        activeId: accounts.activeClaudeAccountId,
                        authState: accounts.claudeAuthState,
                        onAdd: { Task { await accounts.addClaudeAccount() } },
                        onCancelAdd: { accounts.cancelClaudeAuth() },
                        onSelect: { accounts.activeClaudeAccountId = $0 },
                        onReAuthenticate: { account in Task { await accounts.reAuthenticateClaudeAccount(account) } },
                        onRemove: { accounts.removeClaudeAccount($0) }
                    )
                }
            }

            Section("Codex") {
                LabeledContent {
                    HStack(spacing: 6) {
                        Circle().fill(codexStatusColor).frame(width: 8, height: 8)
                        Text(codexStatusText)
                    }
                } label: {
                    HStack(spacing: 8) {
                        AgentIcon(agentId: "codex", size: 18)
                        Text("Status")
                    }
                }
                if let updated = store.lastCodexUpdate {
                    LabeledContent("Last read", value: updated.formatted(date: .omitted, time: .shortened))
                }
                Toggle("Show in usage bar", isOn: $showCodexInBar)
                Button("Refresh now") {
                    Task { await store.refreshCodex() }
                }
                if let accounts {
                    AgentAccountsBlock(
                        title: "Accounts",
                        accounts: accounts.codexAccounts,
                        activeId: accounts.activeCodexAccountId,
                        authState: accounts.codexAuthState,
                        onAdd: { Task { await accounts.addCodexAccount() } },
                        onCancelAdd: { accounts.cancelCodexAuth() },
                        onSelect: { accounts.activeCodexAccountId = $0 },
                        onReAuthenticate: { account in Task { await accounts.reAuthenticateCodexAccount(account) } },
                        onRemove: { accounts.removeCodexAccount($0) }
                    )
                }
            }

            Section("OpenCode Go") {
                LabeledContent {
                    HStack(spacing: 6) {
                        Circle().fill(opencodeGoStatusColor).frame(width: 8, height: 8)
                        Text(opencodeGoStatusText)
                    }
                } label: {
                    HStack(spacing: 8) {
                        AgentIcon(agentId: "opencode", size: 18)
                        Text("Status")
                    }
                }
                if let updated = store.lastOpencodeGoUpdate {
                    LabeledContent("Last read", value: updated.formatted(date: .omitted, time: .shortened))
                }
                SecureField("Session cookie", text: $opencodeGoCookieInput)
                    .textFieldStyle(.roundedBorder)
                Text("Paste either the raw token value (e.g. Fe26.2**...) or the full cookie header (e.g. auth=Fe26.2**...). Find it in your browser's DevTools → Network → any opencode.ai request → Cookie header. OpenCode Go auth is web-based and shared across Windows and WSL terminals.")
                    .font(.caption)
                    .foregroundStyle(AppTheme.subtitle)
                if opencodeGoKeychainError {
                    Text("Failed to update Keychain — check System Settings > Privacy & Security.")
                        .font(.caption)
                        .foregroundStyle(.red)
                }
                HStack {
                    Button("Save") {
                        let saved = KeychainCredentialStore.set(key: OpenCodeGoUsageFetcher.cookieKey, value: opencodeGoCookieInput)
                        opencodeGoCookieInput = ""
                        opencodeGoKeychainError = !saved
                        if saved {
                            showOpencodeGoInBar = true
                            Task { await store.refreshOpencodeGo() }
                        }
                    }
                    .disabled(opencodeGoCookieInput.trimmingCharacters(in: .whitespaces).isEmpty)
                    Button("Clear") {
                        let deleted = KeychainCredentialStore.delete(key: OpenCodeGoUsageFetcher.cookieKey)
                        opencodeGoKeychainError = !deleted
                        if deleted {
                            showOpencodeGoInBar = false
                            store.opencodeGo = .unavailable(.loggedOut)
                        }
                    }
                }
                TextField("Workspace ID override", text: $workspaceIdOverride)
                    .textFieldStyle(.roundedBorder)
                Text("Find this in the URL after logging into opencode.ai (e.g. opencode.ai/workspace/wrk_.../go).")
                    .font(.caption)
                    .foregroundStyle(AppTheme.subtitle)
                Button("Clear") {
                    workspaceIdOverride = ""
                }
                .disabled(workspaceIdOverride.isEmpty)
                Button("Refresh now") {
                    Task { await store.refreshOpencodeGo() }
                }
            }

            Section("Ollama Cloud") {
                LabeledContent {
                    HStack(spacing: 6) {
                        Circle().fill(ollamaCloudStatusColor).frame(width: 8, height: 8)
                        Text(ollamaCloudStatusText)
                    }
                } label: {
                    HStack(spacing: 8) {
                        AgentIcon(agentId: "ollama", size: 18)
                        Text("Status")
                    }
                }
                if let updated = store.lastOllamaCloudUpdate {
                    LabeledContent("Last read", value: updated.formatted(date: .omitted, time: .shortened))
                }
                SecureField("Session cookie", text: $ollamaCloudCookieInput)
                    .textFieldStyle(.roundedBorder)
                Text("Paste the raw token value or the full cookie header from ollama.com. Find it in your browser's DevTools → Network → any ollama.com request → Cookie header.")
                    .font(.caption)
                    .foregroundStyle(AppTheme.subtitle)
                if ollamaCloudKeychainError {
                    Text("Failed to update Keychain — check System Settings > Privacy & Security.")
                        .font(.caption)
                        .foregroundStyle(.red)
                }
                HStack {
                    Button("Save") {
                        let saved = KeychainCredentialStore.set(key: OllamaCloudUsageFetcher.cookieKey, value: ollamaCloudCookieInput)
                        ollamaCloudCookieInput = ""
                        ollamaCloudKeychainError = !saved
                        if saved {
                            showOllamaCloudInBar = true
                            Task { await store.refreshOllamaCloud() }
                        }
                    }
                    .disabled(ollamaCloudCookieInput.trimmingCharacters(in: .whitespaces).isEmpty)
                    Button("Clear") {
                        let deleted = KeychainCredentialStore.delete(key: OllamaCloudUsageFetcher.cookieKey)
                        ollamaCloudKeychainError = !deleted
                        if deleted {
                            showOllamaCloudInBar = false
                            store.ollamaCloud = .unavailable(.loggedOut)
                        }
                    }
                }
                Button("Refresh now") {
                    Task { await store.refreshOllamaCloud() }
                }
            }
        }
        .formStyle(.grouped)
        .scrollContentBackground(.hidden)
        .onChange(of: intervalSeconds) { _, _ in
            store.restartTimer()
        }
        .onChange(of: showClaudeInBar) { _, isOn in
            if isOn { Task { await store.refresh() } }
        }
        .onChange(of: workspaceIdOverride) { _, _ in
            Task { await store.refreshOpencodeGo() }
        }
    }

    private var statusText: String {
        switch store.claude {
        case .loading: "Reading…"
        case .loaded: "Active"
        case .stale: "Stale (last known)"
        case .unavailable(.notInstalled): "Not found on PATH"
        case .unavailable(.loggedOut): "Logged out"
        case .unavailable(.timedOut): "Timed out"
        case .unavailable(.error): "Error"
        }
    }

    private var statusColor: Color {
        switch store.claude {
        case .loaded: .green
        case .loading, .stale: .yellow
        case .unavailable: .red
        }
    }

    private var codexStatusText: String {
        switch store.codex {
        case .loading: "Reading…"
        case .loaded: "Active"
        case .stale: "Stale (last known)"
        case .unavailable(.notInstalled): "Not found on PATH"
        case .unavailable(.loggedOut): "Logged out"
        case .unavailable(.timedOut): "Timed out"
        case .unavailable(.error): "Error"
        }
    }

    private var codexStatusColor: Color {
        switch store.codex {
        case .loaded: .green
        case .loading, .stale: .yellow
        case .unavailable: .red
        }
    }

    private var opencodeGoStatusText: String {
        switch store.opencodeGo {
        case .loading: "Reading…"
        case .loaded: "Active"
        case .stale: "Stale (last known)"
        case .unavailable(.notInstalled): "Not configured"
        case .unavailable(.loggedOut): "No cookie configured"
        case .unavailable(.timedOut): "Timed out"
        case .unavailable(.error): "Error"
        }
    }

    private var opencodeGoStatusColor: Color {
        switch store.opencodeGo {
        case .loaded: .green
        case .loading, .stale: .yellow
        case .unavailable: .red
        }
    }

    private var ollamaCloudStatusText: String {
        switch store.ollamaCloud {
        case .loading: "Reading…"
        case .loaded: "Active"
        case .stale: "Stale (last known)"
        case .unavailable(.notInstalled): "Not configured"
        case .unavailable(.loggedOut): "No cookie configured"
        case .unavailable(.timedOut): "Timed out"
        case .unavailable(.error): "Error"
        }
    }

    private var ollamaCloudStatusColor: Color {
        switch store.ollamaCloud {
        case .loaded: .green
        case .loading, .stale: .yellow
        case .unavailable: .red
        }
    }
}

/// Reusable "Accounts" list for one provider: System default row (always
/// present, never removable) + one row per `AgentAccountRecord`. Shared by
/// the Claude Code and Codex sections above — same shape, different
/// callbacks per provider.
private struct AgentAccountsBlock: View {
    let title: String
    let accounts: [AgentAccountRecord]
    let activeId: String?
    let authState: AgentAccountStore.AccountAuthState
    let onAdd: () -> Void
    let onCancelAdd: () -> Void
    let onSelect: (String?) -> Void
    let onReAuthenticate: (AgentAccountRecord) -> Void
    let onRemove: (AgentAccountRecord) -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack {
                VStack(alignment: .leading, spacing: 2) {
                    Text(title).font(.headline)
                    Text("Showing accounts for this device. New accounts are added there.")
                        .font(.caption)
                        .foregroundStyle(AppTheme.subtitle)
                }
                Spacer()
                switch authState {
                case .idle:
                    Button("Add Account", action: onAdd)
                case .waitingForBrowser:
                    HStack(spacing: 6) {
                        ProgressView().controlSize(.small)
                        Text("Waiting for browser login…").font(.caption)
                        Button("Cancel", action: onCancelAdd)
                    }
                case .failed(let message):
                    HStack(spacing: 6) {
                        Text(message).font(.caption).foregroundStyle(.red)
                        Button("Add Account", action: onAdd)
                    }
                }
            }

            Group {
                row(label: "System default", subtitle: "Use your current CLI login on this device.",
                    isActive: activeId == nil, showActions: false,
                    onSelect: { onSelect(nil) }, onReAuthenticate: {}, onRemove: {})

                ForEach(accounts, id: \.id) { account in
                    row(label: account.label, subtitle: account.orgName,
                        isActive: activeId == account.id, showActions: true,
                        onSelect: { onSelect(account.id) },
                        onReAuthenticate: { onReAuthenticate(account) },
                        onRemove: { onRemove(account) })
                }
            }
        }
        .padding(.vertical, 4)
    }

    @ViewBuilder
    private func row(
        label: String, subtitle: String?, isActive: Bool, showActions: Bool,
        onSelect: @escaping () -> Void, onReAuthenticate: @escaping () -> Void, onRemove: @escaping () -> Void
    ) -> some View {
        HStack {
            VStack(alignment: .leading, spacing: 2) {
                HStack(spacing: 6) {
                    Text(label).fontWeight(.medium)
                    Text("This device").font(.caption2).padding(.horizontal, 6).padding(.vertical, 2)
                        .background(.quaternary, in: Capsule())
                    if isActive {
                        Text("Active").font(.caption2).padding(.horizontal, 6).padding(.vertical, 2)
                            .background(.tint, in: Capsule())
                    }
                }
                if let subtitle {
                    Text(subtitle).font(.caption).foregroundStyle(AppTheme.subtitle)
                }
            }
            Spacer()
            if showActions {
                Button("Re-authenticate", action: onReAuthenticate)
                Button("Remove", role: .destructive, action: onRemove)
            }
        }
        .contentShape(Rectangle())
        .onTapGesture(perform: onSelect)
    }
}
