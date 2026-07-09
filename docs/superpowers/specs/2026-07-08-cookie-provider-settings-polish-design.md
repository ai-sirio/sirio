# OpenCode Go / Ollama Cloud cookie section polish

## Problem

`AIProvidersSettingsView`'s OpenCode Go and Ollama Cloud sections show a bare
`SecureField` for the session cookie with no explanation of where to find it.
OpenCode Go also has no way to override the auto-detected workspace ID, which
matters when auto-detection breaks or resolves the wrong workspace for an
account that belongs to more than one.

## Scope

Two additions to `Helm/App/AIProvidersSettingsView.swift`, plus one small
change to `Helm/App/OpenCodeGoUsageFetcher.swift`. No new persistence layer,
no new files, no changes to Claude/Codex sections (covered by a separate
spec: account management).

## Design

### 1. Explanatory caption under each cookie field

Below the existing `SecureField("Session cookie", ...)` in both sections, add
a `Text` in `.caption` font / secondary color:

- **OpenCode Go**: "Paste either the raw token value (e.g. `Fe26.2**...`) or
  the full cookie header (e.g. `auth=Fe26.2**...`). Find it in your
  browser's DevTools → Network → any opencode.ai request → Cookie header.
  OpenCode Go auth is web-based and shared across Windows and WSL
  terminals."
- **Ollama Cloud**: same structure, adapted to ollama.com and its cookie
  name (`OllamaCloudUsageFetcher.cookieKey`).

No behavior change — purely descriptive text matching the existing
`opencodeGoKeychainError` caption style already in the file.

### 2. Workspace ID override (OpenCode Go only)

Ollama Cloud has no workspace concept (`OllamaCloudUsageFetcher` does no
auto-discovery), so this field is OpenCode Go-only, matching the reference
screenshot.

- New `@AppStorage("usage.opencodeGo.workspaceIdOverride") private var
  workspaceIdOverride = ""` in `AIProvidersSettingsView`.
- `TextField("Workspace ID override", text: $workspaceIdOverride)` + a
  "Clear" button (mirrors the existing cookie field's Save/Clear pattern),
  placed directly under the cookie field/caption.
- Caption: "Find this in the URL after logging into opencode.ai (e.g.
  opencode.ai/workspace/wrk_.../go)."

### 3. Fetcher change

`OpenCodeGoUsageFetcher.fetch()` currently always calls the `_server`
endpoint to auto-discover the workspace ID before hitting
`/workspace/<id>/go`. Change: if the override `AppStorage` value is
non-empty, skip the `_server` discovery call entirely and build the usage
URL directly from the override value. Auto-discovery remains the default
when the override is empty (status quo, unchanged).

Since `OpenCodeGoUsageFetcher` is a `static` enum with no stored state
today, the override value needs to be passed in as a parameter to `fetch()`
(read from `@AppStorage` at the call site in `UsageStore`) rather than
reading `UserDefaults` directly inside `HelmCore` — keeps the fetcher
UI-framework-agnostic, consistent with how the cookie is already passed via
`KeychainCredentialStore` rather than baked into the fetcher.

## Testing

- `OpenCodeGoUsageFetcherTests` (new or extended): when override is set,
  verify the `_server` discovery endpoint is never called and the usage URL
  uses the override value directly.
- Manual: set an override, confirm usage bar/settings status still loads;
  clear it, confirm auto-discovery still works (regression check on
  existing behavior).

## Out of scope

- Claude/Codex account management (separate spec, larger — new persistence,
  isolated auth flow, env injection into agent terminals).
- Any change to the usage bar itself (`UsageBarView.swift`) — already
  handled in prior work this session.
