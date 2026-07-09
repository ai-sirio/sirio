# OpenCode Go / Ollama Cloud Cookie Section Polish Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add explanatory captions to the OpenCode Go and Ollama Cloud cookie
fields in Settings, and let OpenCode Go's auto-detected workspace ID be
manually overridden.

**Architecture:** Three modify-only changes to existing files — no new
files, no new persistence layer. `OpenCodeGoUsageFetcher.fetch()` gains an
optional `workspaceIdOverride` parameter that, when non-empty, bypasses the
`_server` auto-discovery HTTP call. `UsageStore.refreshOpencodeGo()` reads
the override from `UserDefaults` and threads it through.
`AIProvidersSettingsView` gets caption `Text` views and a new `TextField`
bound directly to `@AppStorage`.

**Tech Stack:** Swift, SwiftUI, `@AppStorage`/`UserDefaults` (no Keychain
changes — the workspace ID is not a secret).

## Global Constraints

- Ollama Cloud gets no workspace override field — it has no workspace
  concept (`OllamaCloudUsageFetcher` hits `ollama.com/settings` directly,
  no auto-discovery step exists to override).
- No new SPM test target — `OpenCodeGoUsageFetcher.swift` lives in the
  `Helm/App` Xcode target, which has no existing unit test target (only
  the `HelmCore` package's pure parsing logic is unit tested). Follow this
  existing pattern: verify the fetcher change by building and exercising it
  manually in the running app, not by inventing new network-mocking test
  infrastructure for one conditional branch.

---

### Task 1: Workspace ID override in `OpenCodeGoUsageFetcher`

**Files:**
- Modify: `Helm/App/OpenCodeGoUsageFetcher.swift:14-66` (the `fetch()` method)

**Interfaces:**
- Consumes: nothing new (same `KeychainCredentialStore`,
  `OpenCodeGoUsageParser`, `UsageFetchOutcome` as before).
- Produces: `OpenCodeGoUsageFetcher.fetch(workspaceIdOverride: String? = nil) async -> UsageFetchOutcome`
  — the default-`nil` keeps every existing call site (there are none yet
  outside `UsageStore`, changed in Task 2) source-compatible.

- [ ] **Step 1: Replace `fetch()` with the override-aware version**

Replace the entire body of `OpenCodeGoUsageFetcher` in
`Helm/App/OpenCodeGoUsageFetcher.swift` with:

```swift
import Foundation
import HelmCore

enum OpenCodeGoUsageFetcher {
    static let cookieKey = "opencode-go-cookie"
    private static let workspaceServerId = "def39973159c7f0483d8793a822b8dbb10d067e12c65455fcb4608459ba0234f"

    private static let session: URLSession = {
        let config = URLSessionConfiguration.default
        config.timeoutIntervalForRequest = 12
        return URLSession(configuration: config)
    }()

    static func fetch(workspaceIdOverride: String? = nil) async -> UsageFetchOutcome {
        guard
            let rawCookie = KeychainCredentialStore.get(key: cookieKey),
            !rawCookie.trimmingCharacters(in: .whitespaces).isEmpty
        else {
            return .unavailable(.loggedOut)
        }
        let cookie = OpenCodeGoUsageParser.normalizeCookie(rawCookie)

        let workspaceId: String
        if let override = workspaceIdOverride?.trimmingCharacters(in: .whitespaces), !override.isEmpty {
            workspaceId = override
        } else {
            guard let discovered = await discoverWorkspaceId(cookie: cookie) else {
                return .unavailable(.error)
            }
            workspaceId = discovered
        }

        guard let usageURL = URL(string: "https://opencode.ai/workspace/\(workspaceId)/go") else {
            return .unavailable(.error)
        }
        var usageRequest = URLRequest(url: usageURL)
        usageRequest.setValue(cookie, forHTTPHeaderField: "Cookie")

        do {
            let (usageData, usageResponse) = try await session.data(for: usageRequest)
            guard
                let usageHttp = usageResponse as? HTTPURLResponse,
                (200..<300).contains(usageHttp.statusCode)
            else {
                return .unavailable(.error)
            }
            guard let usage = OpenCodeGoUsageParser.extractUsage(from: String(decoding: usageData, as: UTF8.self)) else {
                return .unavailable(.error)
            }
            return .success(usage)
        } catch let error as URLError where error.code == .timedOut {
            return .timedOut
        } catch {
            return .unavailable(.error)
        }
    }

    /// Returns `nil` on any failure (network error, non-2xx, unparseable
    /// body) — callers translate that into `.unavailable(.error)`.
    private static func discoverWorkspaceId(cookie: String) async -> String? {
        guard let workspacesURL = URL(string: "https://opencode.ai/_server?id=\(workspaceServerId)") else {
            return nil
        }
        var workspacesRequest = URLRequest(url: workspacesURL)
        workspacesRequest.setValue(cookie, forHTTPHeaderField: "Cookie")
        workspacesRequest.setValue(workspaceServerId, forHTTPHeaderField: "X-Server-Id")

        guard
            let (workspacesData, workspacesResponse) = try? await session.data(for: workspacesRequest),
            let workspacesHttp = workspacesResponse as? HTTPURLResponse,
            (200..<300).contains(workspacesHttp.statusCode)
        else {
            return nil
        }
        return OpenCodeGoUsageParser.extractWorkspaceId(from: String(decoding: workspacesData, as: UTF8.self))
    }
}
```

This factors the old inline discovery request into a `discoverWorkspaceId`
helper (called only when no override is set) and keeps the timeout/error
mapping for the *usage* request identical to before. The discovery
helper collapses timeout vs. other errors into a single `nil` — the
outer `fetch()` already returns `.unavailable(.error)` for any discovery
failure today (there was no separate timeout path for the discovery call
in the original code), so behavior is unchanged.

- [ ] **Step 2: Build to verify it compiles**

Run: `xcodebuild -scheme Helm -configuration Debug -destination 'platform=macOS' build`
Expected: `** BUILD SUCCEEDED **`

- [ ] **Step 3: Commit**

```bash
git add Helm/App/OpenCodeGoUsageFetcher.swift
git commit -m "feat: support workspace ID override in OpenCode Go fetcher"
```

---

### Task 2: Thread the override through `UsageStore`

**Files:**
- Modify: `Helm/App/UsageStore.swift:90-98` (`refreshOpencodeGo()`)

**Interfaces:**
- Consumes: `OpenCodeGoUsageFetcher.fetch(workspaceIdOverride:)` from Task 1.
- Produces: no new public interface — `refreshOpencodeGo()`'s signature is
  unchanged, it just reads one more `UserDefaults` key internally.

- [ ] **Step 1: Read the override and pass it to `fetch()`**

In `Helm/App/UsageStore.swift`, replace:

```swift
    func refreshOpencodeGo() async {
        guard UserDefaults.standard.bool(forKey: "usage.opencodeGo.showInBar") else { return }
        guard !isFetchingOpencodeGo else { return }
        isFetchingOpencodeGo = true
        defer { isFetchingOpencodeGo = false }
        let outcome = await OpenCodeGoUsageFetcher.fetch()
        opencodeGo = UsageStateReducer.reduce(outcome: outcome, previous: opencodeGo)
        if case .loaded = opencodeGo { lastOpencodeGoUpdate = Date() }
    }
```

with:

```swift
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
```

This mirrors the existing `usage.opencodeGo.showInBar` /
`usage.refreshIntervalSeconds` pattern in this same file: read straight
from `UserDefaults.standard`, no new abstraction.

- [ ] **Step 2: Build to verify it compiles**

Run: `xcodebuild -scheme Helm -configuration Debug -destination 'platform=macOS' build`
Expected: `** BUILD SUCCEEDED **`

- [ ] **Step 3: Commit**

```bash
git add Helm/App/UsageStore.swift
git commit -m "feat: read OpenCode Go workspace override into refresh"
```

---

### Task 3: Settings UI — captions + override field

**Files:**
- Modify: `Helm/App/AIProvidersSettingsView.swift:13-17` (new `@AppStorage`)
- Modify: `Helm/App/AIProvidersSettingsView.swift:71-115` (OpenCode Go section)
- Modify: `Helm/App/AIProvidersSettingsView.swift:117-161` (Ollama Cloud section)
- Modify: `Helm/App/AIProvidersSettingsView.swift:162-170` (body modifiers, add `.onChange`)

**Interfaces:**
- Consumes: `store.refreshOpencodeGo()` (existing, from `UsageStore`,
  Task 2's change is transparent to callers).
- Produces: nothing consumed by later tasks — this is the final UI-facing
  task in this plan.

- [ ] **Step 1: Add the `@AppStorage` property**

In `Helm/App/AIProvidersSettingsView.swift`, directly below line 13-14
(`@State private var opencodeGoCookieInput = ""` /
`@State private var opencodeGoKeychainError = false`), add:

```swift
    @AppStorage("usage.opencodeGo.workspaceIdOverride") private var workspaceIdOverride = ""
```

So the top of the struct reads:

```swift
    @AppStorage("usage.codex.showInBar") private var showCodexInBar = true
    @AppStorage("usage.opencodeGo.showInBar") private var showOpencodeGoInBar = false
    @State private var opencodeGoCookieInput = ""
    @State private var opencodeGoKeychainError = false
    @AppStorage("usage.opencodeGo.workspaceIdOverride") private var workspaceIdOverride = ""
    @AppStorage("usage.ollamaCloud.showInBar") private var showOllamaCloudInBar = false
```

- [ ] **Step 2: Update the OpenCode Go section**

Replace the `Section("OpenCode Go") { ... }` block
(`Helm/App/AIProvidersSettingsView.swift:71-115`) with:

```swift
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
                Text("Paste either the raw token value (e.g. Fe26.2**...) or the full cookie header (e.g. auth=Fe26.2**...). Find it in your browser's DevTools → Network → any opencode.ai request → Cookie header. OpenCode Go auth is web-based and shared across Windows and WSL terminals.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
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
                Text("Find this in the URL after logging into opencode.ai (e.g. opencode.ai/workspace/wrk_.../go).")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                Button("Clear") {
                    workspaceIdOverride = ""
                }
                .disabled(workspaceIdOverride.isEmpty)
                Button("Refresh now") {
                    Task { await store.refreshOpencodeGo() }
                }
            }
```

- [ ] **Step 3: Update the Ollama Cloud section**

In the `Section("Ollama Cloud") { ... }` block
(`Helm/App/AIProvidersSettingsView.swift:117-161`), directly below
`SecureField("Session cookie", text: $ollamaCloudCookieInput)`, add:

```swift
                Text("Paste the raw token value or the full cookie header from ollama.com. Find it in your browser's DevTools → Network → any ollama.com request → Cookie header.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
```

So the section starts:

```swift
                SecureField("Session cookie", text: $ollamaCloudCookieInput)
                Text("Paste the raw token value or the full cookie header from ollama.com. Find it in your browser's DevTools → Network → any ollama.com request → Cookie header.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                if ollamaCloudKeychainError {
```

(the rest of the Ollama Cloud section — the error caption, Save/Clear
buttons, Refresh now button — is unchanged).

- [ ] **Step 4: Auto-refresh when the override changes**

In the `.onChange(of: showClaudeInBar)` block near the end of `body`
(`Helm/App/AIProvidersSettingsView.swift:168-170`), add a sibling
`.onChange` right after it:

```swift
        .onChange(of: showClaudeInBar) { _, isOn in
            if isOn { Task { await store.refresh() } }
        }
        .onChange(of: workspaceIdOverride) { _, _ in
            Task { await store.refreshOpencodeGo() }
        }
```

- [ ] **Step 5: Build**

Run: `xcodebuild -scheme Helm -configuration Debug -destination 'platform=macOS' build`
Expected: `** BUILD SUCCEEDED **`

- [ ] **Step 6: Manual verification in the running app**

```bash
pkill -f "Helm.app/Contents/MacOS/Helm"
open /Users/enzopiopalmisano/Library/Developer/Xcode/DerivedData/Helm-ejdbtdwcyoawkgencblugkcxdysm/Build/Products/Debug/Helm.app
```

Open Settings → AI Providers. Confirm:
- OpenCode Go section shows the new caption under the cookie field, plus
  a "Workspace ID override" field with its own caption and a "Clear"
  button below the Save/Clear row.
- Ollama Cloud section shows its new caption under the cookie field.
- Typing a workspace ID into the override field and tabbing away doesn't
  crash and doesn't require a "Save" button (value persists via
  `@AppStorage` immediately — quit and relaunch Helm, confirm the typed
  value is still there).
- Clearing the override field via its "Clear" button empties it and the
  button becomes disabled once empty.

- [ ] **Step 7: Commit**

```bash
git add Helm/App/AIProvidersSettingsView.swift
git commit -m "feat: add cookie instructions and workspace ID override to AI Providers settings"
```
