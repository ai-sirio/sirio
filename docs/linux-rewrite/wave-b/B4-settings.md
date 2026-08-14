# Wave B slice B4-settings — 7 rows to build

**settings surface and usage**

## Files you own this wave

- `rust/crates/tiller_ui/src/settings.rs`
- `rust/crates/tiller_usage/src/codex.rs`

No other agent will touch these. **You must not edit any file outside this list** —
every other source file belongs to a sibling and an edit there is silently lost or
silently overwrites theirs. If a fix genuinely needs a file you do not own, stop and
report it rather than making the change.

## Rows

### `F-SET-12` — ledger line 300, currently **half-proven**

- **Triage:** reclassify, size S
- **Files triage expects:** rust/crates/tiller_ui/src/settings.rs, rust/crates/tiller_usage/src/opencode_go.rs, rust/crates/tiller_usage/src/credentials.rs
- **Approach:** Evidence ('no cookie UI or state') is pass-10, predates commit 7ae343d which built a real, rendered, tested cookie field + Save/Clear + workspace-ID override wired to a real CredentialStore. Re-drive live rather than dispatch a build.
- **Shared cause:** same-day build landed after this row's evidence was recorded, ledger never refreshed
- **Evidence on record:** Live-drove: real click+type+click reached Save -> CredentialStore write, Not-signed-in -> Signed-in, Show-in-usage-bar auto-ON (04/05/06). Combined with on-record P111 captures (independently re-viewed): Clear empties the field and flips back to Not-signed-in; Workspace-ID override typed, survives a fresh-process relaunch, and clears. Refresh-now was never exercised for OpenCode Go specifically (F

### `F-SET-14` — ledger line 302, currently **half-proven**

- **Triage:** reclassify, size S
- **Files triage expects:** rust/crates/tiller_ui/src/settings.rs
- **Approach:** 'Dead control' claim is false: settings.rs's manage_account_handler falls back to a self-contained launch_account_login() needing no host wiring, independently proven live in P120-report.md (real x-terminal-emulator spawning a genuine OAuth PKCE URL). What remains is a smaller gap: no in-app waiting/cancel affordance during the spawned login.
- **Shared cause:** on_manage_account evidence trap shared with F-SET-15 -- counts only the external-caller field, misses the same-file self-contained fallback
- **Evidence on record:** Corrects a ledger error: the 'dead control, unset renders muted' contract quoted for this row (settings.rs ~723-726 today) actually documents a different field (on_install_skill, F-SET-09) -- on_manage_account's own doc (settings.rs:728-737) says plainly it invokes the installed provider CLI in an external terminal when unset. Live click on Add Account produced no visible frame change but ps aux s

### `F-SET-15` — ledger line 303, currently **half-proven**

- **Triage:** reclassify, size L
- **Files triage expects:** rust/crates/tiller_ui/src/settings.rs
- **Approach:** Shares F-SET-14's false on_manage_account premise, but the underlying claim (no multi-account model) is independently true by an explicit in-code design decision (single credential slot per provider, Active badge hardwired true). Clause may be structurally unanswerable as written, same shape as F-SET-21's N/A-platform precedent. Needs a build-vs-N/A ruling before dispatch.
- **Shared cause:** on_manage_account evidence trap shared with F-SET-14
- **Evidence on record:** Reconfirms unchanged: same click frame shows every provider card keeps exactly one Accounts row (System default/This device/Active) before and after Add Account's real spawned subprocess; grep reconfirms on_manage_account's only call site workspace-wide is the settings.rs:4946 test. Second-account route remains structurally unavailable without a source edit.

### `F-SET-16` — ledger line 304, currently **half-proven**

- **Triage:** both, size S
- **Files triage expects:** rust/crates/tiller_ui/src/settings.rs
- **Approach:** Search and Refresh are both genuinely wired and tested (refresh_agents_re_runs_agent_discovery). The row's byte-identical-capture evidence is likely a false negative from an unchanged environment. Genuinely missing: a last-refreshed timestamp render (clause's other conjunct). Add the field/render, then re-exercise Refresh against an environment that actually changes between clicks.
- **Evidence on record:** Search half proven working: no-match string filtered 5 rows to 0, clearing restored all 5 (real round trip). Refresh half proven absent: byte-identical capture before/after click, no spinner/reorder/timestamp text anywhere. shots/101,102,105.

### `F-SET-20` — ledger line 308, currently **FAILED — defective**

- **Triage:** build, size S
- **Files triage expects:** rust/crates/tiller_ui/src/settings.rs
- **Approach:** Verified accurate, not stale: set_translucency sets the field and notifies but never calls self.changed() (unlike every sibling setter), SettingsSnapshot has no translucency field, and zero code anywhere consumes a background-appearance flag. Either wire it for real (snapshot field + self.changed() + main.rs applies WindowBackgroundAppearance) or remove the dead control if out of scope -- needs the same kind of ruling flagged for F-SET-15.
- **Evidence on record:** pass 17: the translucency conjunct is a dead control, proven three ways — `set_translucency` (settings.rs:871-874) sets the field and `cx.notify()`s but never calls `self.changed()`, unlike EVERY sibling setter; `SettingsSnapshot` (settings.rs:338) has no translucency field, so the value cannot leave the surface; and zero code anywhere consumes the flag (no `background_appearance`/`WindowBackgroun

### `F-CORE-USG-05` — ledger line 403, currently **half-proven**

- **Triage:** both, size M
- **Files triage expects:** rust/crates/tiller_usage/src/codex.rs
- **Approach:** Load, real-401-refresh-fail-LoggedOut chain, and credential parsing already live-proven. Build: needs_refresh (codex.rs:52, REFRESH_AFTER=8 days) has zero callers — CodexUsageFetcher::fetch (codex.rs:315-350) only refreshes reactively after a live 401, never proactively on age; add a proactive check ahead of the first fetch attempt. Exercise: save_credentials' merge-not-overwrite logic (codex.rs:154-172) looks correct and is called on refresh success (codex.rs:339) but was never driven to a real refresh success this pass — force one and confirm the auth file's unrelated fields survive.
- **Shared cause:** Same codex.rs CodexUsageFetcher::fetch pipeline as F-CORE-USG-06 and F-CORE-USG-07.
- **Evidence on record:** half-proven: synthetic $CODEX_HOME creds prove load + real 401->refresh-fail->LoggedOut chain live against real endpoints. needs_refresh 8-day gate (still 0 non-test callers) and merge-save-on-success path not exercised.

### `F-CORE-USG-06` — ledger line 404, currently **half-proven**

- **Triage:** build, size M
- **Files triage expects:** rust/crates/tiller_usage/src/codex.rs, rust/crates/tiller_usage/src/model.rs, rust/crates/tiller_ui/src/status_bar.rs
- **Approach:** classify_token_refresh_failure (codex.rs:69-79) correctly classifies Reused/Revoked/Expired/Other and is called from production at codex.rs:253, but its caller (codex.rs:336-339, `Err(_) => return UsageFetchOutcome::Unavailable(UsageReason::LoggedOut)`) discards the classification entirely, and UsageReason (model.rs:64-74) has no variants to hold it anyway. Need new UsageReason variants (or a carried field), a real match at the discard site, and a status_bar.rs:283-286 render update (currently a plain 4-arm match).
- **Shared cause:** Same codex.rs pipeline as F-CORE-USG-05 and F-CORE-USG-07.
- **Evidence on record:** half-proven: real POST to auth.openai.com + real non-200 exercises classify_token_refresh_failure live, but its output is discarded by the caller — every variant maps to identical LoggedOut, so reused/revoked/expired has no observable signal currently.

