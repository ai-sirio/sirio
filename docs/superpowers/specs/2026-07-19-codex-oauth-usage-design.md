---
title: Codex usage fetcher — direct OAuth instead of app-server subprocess
date: 2026-07-19
status: approved
---

# Codex usage fetcher — direct OAuth instead of app-server subprocess

## Background

`App/CodexUsageFetcher.swift` currently fetches Codex rate-limit usage by
spawning `codex -s read-only -a untrusted app-server` and speaking JSON-RPC
over stdio (`initialize` → `initialized` → `account/rateLimits/read`).

Root cause investigated 2026-07-19: on a machine where `codex login` used
`OPENAI_API_KEY` (env var or apikey auth mode) rather than "Sign in with
ChatGPT", `account/rateLimits/read` returns
`{"error":{"code":-32600,"message":"chatgpt authentication required to read
rate limits"}}`. The fetcher's error-message regex didn't recognize this
message, so it fell into the generic `.error` state instead of `.loggedOut`.
That specific regex bug is already fixed independently
(`App/CodexUsageFetcher.swift:95`, commit prior to this doc).

Separately, inspecting `steipete/codexbar` (an OSS macOS usage-bar app
covering dozens of providers) showed it dropped the CLI-subprocess approach
for Codex entirely in favor of reading OAuth tokens straight from
`~/.codex/auth.json` and calling the same HTTP endpoint the `codex` CLI
itself uses. This avoids all subprocess/PATH/pipe-lifecycle failure modes.
This spec adopts that approach for Tiller, scoped down to what Tiller
actually needs (no multi-provider abstraction, no CLI fallback, no API-key
auth mode).

## Decisions

- **OAuth only.** No support for `OPENAI_API_KEY` / apikey auth mode. If
  `auth.json` has no `tokens` object, treat as logged out.
- **Automatic token refresh.** Tokens older than 8 days are refreshed via
  `POST https://auth.openai.com/oauth/token` and persisted back to
  `auth.json` (merged, not overwritten — only `tokens` + `last_refresh`
  fields change).
- **Full replacement, no fallback.** The `codex app-server` subprocess path
  is deleted entirely, not kept as a fallback. Same account/network would
  fail both paths, so a fallback adds cost without adding resilience.
- **Lives in `TillerCore`,** not split between `App/` and a package. Mirrors
  `ClaudeUsageFetcher`'s pattern in `TillerTerminal`: the whole fetch flow,
  including the public entry point, is in one testable package with an
  injectable transport seam. `App/UsageStore.swift` needs zero changes
  (already `import TillerCore`; the new `CodexUsageFetcher` keeps the same
  type/method name and signature).

## Architecture / data flow

1. Load `CodexOAuthCredentials` from `$CODEX_HOME/auth.json` (or
   `~/.codex/auth.json` if `CODEX_HOME` unset) — same env-var precedence
   `App/AppModel.swift:1488` already uses elsewhere in the app.
2. If `tokens` missing → `.unavailable(.loggedOut)`.
3. If `needsRefresh` (no `last_refresh`, or >8 days old) and a
   `refresh_token` is present, POST to the refresh endpoint, update tokens,
   persist back to `auth.json`.
4. `GET https://chatgpt.com/backend-api/wham/usage` with
   `Authorization: Bearer <access_token>` and `ChatGPT-Account-Id:
   <account_id>`.
5. Decode the response's `rate_limit.primary_window` /
   `rate_limit.secondary_window` and map to `ProviderUsage` — same shape as
   today: primary → session window labeled `"5h"`, secondary → weekly window
   labeled `"wk"`, `used_percent` (Int) direct, `reset_at` (Unix seconds) →
   `Date`.

## Components

**Delete:**
- `App/CodexUsageFetcher.swift`
- `Packages/TillerCore/Sources/TillerCore/CodexRateLimitParser.swift`
- `Packages/TillerCore/Tests/TillerCoreTests/CodexRateLimitParserTests.swift`

**Create in `Packages/TillerCore/Sources/TillerCore/`:**

- `CodexOAuthCredentials.swift` — `struct CodexOAuthCredentials` (`accessToken`,
  `refreshToken`, `idToken`, `accountId`, `lastRefresh`) plus a loader/saver
  enum that reads/writes `auth.json`, respecting `CODEX_HOME`. `needsRefresh`
  computed property (8-day threshold). No apikey handling — apikey-only or
  missing-tokens `auth.json` surfaces as a distinct "missing tokens" error
  the fetcher maps to `.loggedOut`.

- `CodexTokenRefresher.swift` — `refresh(credentials:transport:)` POSTing to
  `https://auth.openai.com/oauth/token` with the fixed client ID
  (`app_EMoamEEZ73f0CkXaXp7hrann`), classifying 401 responses into
  expired/revoked/reused/unknown, plus network errors. Takes an injectable
  HTTP transport protocol so tests never hit the network.

- `CodexUsageFetcher.swift` — public `enum CodexUsageFetcher { static func
  fetch() async -> UsageFetchOutcome }`, same name/signature as the code
  being replaced. Orchestrates steps 1–5 above. Internally takes an
  injectable transport for the testable overload (mirrors
  `ClaudeUsageFetcher`'s injectable-PTY pattern in `TillerTerminal`).

No new file in `App/` — `UsageStore.swift:75`'s
`await CodexUsageFetcher.fetch()` call resolves to the new `TillerCore` type
unchanged.

## Error handling / state mapping

| Situation | `UsageFetchOutcome` |
|---|---|
| `auth.json` missing, or present without `tokens` (apikey-only or empty) | `.unavailable(.loggedOut)` |
| Refresh token expired / revoked / reused | `.unavailable(.loggedOut)` |
| 401/403 from the usage endpoint (including after a refresh attempt) | `.unavailable(.loggedOut)` |
| Network timeout | `.timedOut` |
| Other network or decode error | `.unavailable(.error)` |
| Success | `.success(ProviderUsage)` |

Persisting refreshed tokens: read the existing `auth.json` as a JSON object,
replace only the `tokens` and `last_refresh` keys, write back atomically.
Every other field in the file (config the `codex` CLI itself may rely on) is
preserved untouched.

## Testing

New tests in `Packages/TillerCore/Tests/TillerCoreTests/`:

- `CodexOAuthCredentialsTests.swift` — parses `auth.json` from an explicit
  path/`CODEX_HOME` override for test isolation; missing `tokens` → error;
  `needsRefresh` true/false around the 8-day boundary.
- `CodexTokenRefresherTests.swift` — successful refresh response mapping;
  401 responses classified as expired/revoked/reused; network error mapped
  to a refresh error.
- `CodexUsageFetcherTests.swift` — end-to-end via injected transport:
  success maps to the expected `ProviderUsage` windows; 401 → `.loggedOut`;
  timeout → `.timedOut`; missing `auth.json` → `.loggedOut`; a stale-token
  fetch triggers refresh-then-retry and succeeds.

No test hits the real network — all HTTP is behind the injectable transport
protocol, same approach `ClaudeUsageFetcher`'s tests already use for its PTY
seam.
