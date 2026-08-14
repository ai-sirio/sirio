# Wave A slice W05-set — 6 rows needing no source change

Triage says each of these needs only exercising, or that its verdict looks
wrong. **Change no code. Do not edit the ledger.**

## `F-SET-11` — ledger line 299, currently **half-proven**

- **Triage says:** exercise
- **Approach:** All 5 clause states already exist as real code (ProviderUsageState variants fed by real fetchers). Drive each: strip PATH for NotInstalled, expire/remove local creds for LoggedOut, blackhole the fetch host for TimedOut (real 15s/25s timeouts in codex.rs/claude.rs), and accept Error as the hardest to force cleanly without a stub server.
- **Evidence on record:** Live 01-baseline.png proves 'valid' condition: real bottom-bar percentages Claude 49% 5h/77% wk, Codex 100% 5h from this machine's actual accounts (ProviderUsageState::Loaded). Driver's 2nd claimed state rejected: OpenCode Go 'Not signed in' (02-06) is LocalAccountState (account.rs), a disk-credential check distinct from the clause's UsageReason usage-fetch mechanism (status_bar.rs), and OpenCode Go isn't a named pro

## `F-SET-12` — ledger line 300, currently **FAILED — absent**

- **Triage says:** reclassify
- **Approach:** Evidence ('no cookie UI or state') is pass-10, predates commit 7ae343d which built a real, rendered, tested cookie field + Save/Clear + workspace-ID override wired to a real CredentialStore. Re-drive live rather than dispatch a build.
- **Shared cause:** same-day build landed after this row's evidence was recorded, ledger never refreshed
- **Evidence on record:** no cookie UI or state

## `F-SET-13` — ledger line 301, currently **FAILED — absent**

- **Triage says:** reclassify
- **Approach:** Same commit (7ae343d) built a full Ollama Cloud provider card, cookie UI, bar segment and fetcher, all wired and tested. Re-drive live rather than dispatch a build.
- **Shared cause:** same-day build landed after this row's evidence was recorded, ledger never refreshed
- **Evidence on record:** no cookie UI or state

## `F-SET-14` — ledger line 302, currently **FAILED — defective**

- **Triage says:** reclassify
- **Approach:** 'Dead control' claim is false: settings.rs's manage_account_handler falls back to a self-contained launch_account_login() needing no host wiring, independently proven live in P120-report.md (real x-terminal-emulator spawning a genuine OAuth PKCE URL). What remains is a smaller gap: no in-app waiting/cancel affordance during the spawned login.
- **Shared cause:** on_manage_account evidence trap shared with F-SET-15 -- counts only the external-caller field, misses the same-file self-contained fallback
- **Evidence on record:** pass 10's "status-only provider cards" is stale — an "Add Account" button is now drawn per provider (settings.rs:1494, ids add-claude/codex/opencode-account). **But it is a dead control**: its handler `on_manage_account` (settings.rs:799) has exactly one caller in the whole workspace and it is a *test* (settings.rs:3188); main.rs never installs it, so by the field's own documented contract (settings.rs:641 "Unset, th

## `F-SET-15` — ledger line 303, currently **half-proven**

- **Triage says:** reclassify
- **Approach:** Shares F-SET-14's false on_manage_account premise, but the underlying claim (no multi-account model) is independently true by an explicit in-code design decision (single credential slot per provider, Active badge hardwired true). Clause may be structurally unanswerable as written, same shape as F-SET-21's N/A-platform precedent. Needs a build-vs-N/A ruling before dispatch.
- **Shared cause:** on_manage_account evidence trap shared with F-SET-14
- **Evidence on record:** Live 02-06 reconfirms single account row per provider (System default/This device/Active); grep confirms on_manage_account (settings.rs:799) has exactly one call site workspace-wide, a test at settings.rs:4946 -- main.rs never wires it. No live route to a second account without a source edit. Reconfirms existing verdict.

## `F-SET-17` — ledger line 305, currently **FAILED — absent**

- **Triage says:** reclassify
- **Approach:** Built (commit 2dcbc1d: try_discover_availability() now fallible, registry-error banner + Retry via the existing Refresh button, green test) and already live-photographed (commit ec53f40, Wayland lane: chmod-000 PATH fails and renders the banner with zero false rows; chmod-755 + real click on Refresh clears it and shows five truthful rows). Flag for immediate re-adjudication toward PASSED; no build remains.
- **Shared cause:** same-day build landed after this row's evidence was recorded, ledger never refreshed
- **Evidence on record:** no agent registry

