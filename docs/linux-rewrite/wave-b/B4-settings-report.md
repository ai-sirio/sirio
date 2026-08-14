# B4-settings — build report

Slice: `settings.rs` + `codex.rs` (7 rows). Owned files touched:
`rust/crates/tiller_ui/src/settings.rs`, `rust/crates/tiller_usage/src/codex.rs`.

No verdicts stated here — that's the orchestrator's call on the ledger.

## F-SET-12 — OpenCode Go cookie UI (reclassify, S) — outcome: already-correct

No code change. Re-read the code the row's own approach pointed at, live rather than
dispatching a build, per the manifest's instruction:

- `opencode_cookie_section` (settings.rs) renders the masked cookie field, the workspace-ID
  override field, and Save/Clear buttons.
- `save_opencode_cookie` writes through `CredentialStore::set(OpenCodeGoUsageFetcher::COOKIE_KEY, ...)`;
  `clear_opencode_cookie` deletes it and flips `provider_accounts.opencode_go` back to
  `LocalAccountState`'s signed-out label.
- `opencode_workspace_id_override` is a real `SettingsSnapshot` field (persisted, not
  transient like the cookie), read on construction and edited via
  `opencode_workspace_override_edits_persist_and_clear`.
- The generic per-provider "Refresh now" button (`refresh-opencode-now`) applies to the
  OpenCode Go card the same as every other card — nothing OpenCode-Go-specific is missing
  there; it was simply never clicked in the pass that recorded this row's evidence.

This matches the row's own diagnosis: the "no cookie UI or state" evidence predates commit
7ae343d. Nothing here needed a fix.

**howToExercise:** Settings → AI Providers → OpenCode Go card. Type a cookie into the
session-cookie field, click Save → field clears, masks empty, status flips to "Signed in".
Click Clear → status flips back to "Not signed in". Type a `wrk_…` id into the workspace
override field, it survives navigating away and back. Click "Refresh now" on this card
specifically (not another card) and confirm the status re-reads from disk.

## F-SET-14 — Add Account: in-flight affordance (reclassify, S) — outcome: built

Confirmed the row's correction is accurate: `on_manage_account`'s doc (settings.rs, the
field right above it) already says plainly it falls back to `launch_account_login` — a
self-contained `x-terminal-emulator` spawn needing no host wiring — when the optional host
callback is unset. The "dead control" ledger evidence was quoting a different field's
comment (`on_install_skill`, F-SET-09).

Built the residual gap the row named: no in-app sign of a spawned login in progress.

- Added `account_login_pending: Option<ProviderKind>`, `account_login_pid: Option<u32>`,
  `account_login_canceled: bool` to `Settings`.
- `launch_account_login` now sets `account_login_pending` before the spawn, publishes the
  child's pid once known, and clears both when the terminal session ends.
- Added `cancel_account_login`: kills the terminal process by pid (closing its pty takes the
  foreground login command with it), sets `account_action_error` to "Sign-in canceled", and
  guards the login task's own completion handler (via `account_login_canceled`) so that
  handler doesn't overwrite the cancellation message with an exit-status one once `wait()`
  resolves.
- `render_provider_card` swaps "Add Account" for a "Signing in…" label + Cancel button while
  `account_login_pending` matches that card's provider.

**Known limitation, stated in the code:** if Cancel is clicked in the sub-millisecond window
before the spawn task has published a pid (a race no human click can realistically win,
since the Cancel button itself only exists once the pending state has already rendered),
there is nothing to kill yet and the terminal is left running; it disappears on its own once
that login process exits.

**Commit:** `5378e29`

**Tests:** `cargo test -p tiller_ui --lib settings::` → `41 passed; 0 failed`. New:
`add_account_in_flight_renders_signing_in_and_cancel` (drives `account_login_pending`
directly, since this sandbox may not have `x-terminal-emulator` installed, but exercises
`cancel_account_login` itself through a real click and checks the rendered message).
Pre-existing `add_account_renders_wired_by_default` and
`manage_account_click_reaches_the_wired_host_callback_with_the_providers_id` still pass
unchanged.

**howToExercise:** Settings → AI Providers → any card with an "Add Account" button (Claude,
Codex, or OpenCode Go) → click it. If `x-terminal-emulator` is installed, the card should
immediately show "Signing in…" and a Cancel button in place of "Add Account", and a spawned
terminal should appear on screen running the provider's login command (`claude auth login`,
`codex login`, or `opencode auth login`). Clicking Cancel should close/kill that terminal,
the card should show "Add Account" again, and an error line reading "Sign-in canceled"
should appear on the card.

## F-SET-15 — single credential slot per provider (reclassify, L) — outcome: not-built

No code change — this needs a ruling this builder is not positioned to make, exactly as the
manifest's own approach text flags ("needs a build-vs-N/A ruling before dispatch").

Reconfirmed the row's diagnosis is accurate:
- `grep -rn "on_manage_account" rust/ --include="*.rs"` finds exactly one call site
  workspace-wide: the test at settings.rs (the one `manage_account_click_reaches_...`
  exercises). No production caller anywhere wires a real multi-account host callback.
- `render_provider_card`'s "System default" `account_row` renders with `active` hardwired
  `true` (a comment above it explains why: this app has no isolated per-provider account
  store, so the current CLI login is definitionally the one agent terminals use).
- This is a real, explicit, in-code design decision, not an oversight — the same shape as
  F-SET-21's platform-N/A precedent the manifest cites. Making it multi-account for real
  would mean inventing a Tiller-owned credential store this app deliberately doesn't have,
  which is a different and much larger feature than "wire a dead control."

Left as-is pending the orchestrator's ruling on whether this row is a build target at all
or should close as N/A/by-design.

**howToExercise (to reconfirm the current, unbuilt state):** Settings → AI Providers → any
card. Before and after clicking "Add Account" and letting its spawned login finish, the card
shows exactly one row under Accounts ("System default", badged "This device" + "Active").
No second account row appears regardless of what the spawned CLI login does.

## F-SET-16 — Agents screen: Refresh timestamp (both, S) — outcome: built

Reconfirmed Search and Refresh were both already genuinely wired and tested before this row
(`refresh_agents_re_runs_agent_discovery` already covered Refresh; a search-narrows/clears
test already covered Search) — the row's own approach text says the byte-identical-capture
evidence against Refresh was likely a false negative from an unchanged environment. Built the
genuinely missing conjunct: a rendered last-refreshed timestamp.

- Added `agent_last_refreshed: Option<String>` to `Settings`, stamped at construction (the
  discovery sweep `with_snapshot` already runs counts as a refresh) and by
  `refresh_agent_availability` on every "↻ Refresh" click, via a new
  `format_refreshed_stamp()` (`chrono::Local::now().format("%H:%M:%S")`, the same clock
  `chat.rs`'s message timestamps already use).
- `render_agents` now renders `"Refreshed HH:MM:SS"` next to the "↻ Refresh" button whenever
  the stamp is set.

**Commit:** `5378e29`

**Tests:** New `refresh_agents_renders_a_last_refreshed_timestamp`: clears the
construction-time stamp directly, confirms no timestamp renders before the first click,
clicks "↻ Refresh", confirms the timestamp element renders and the backing field is
populated. `cargo test -p tiller_ui --lib settings::` → `41 passed; 0 failed`.

**howToExercise:** Settings → Agents. A "Refreshed HH:MM:SS" label should already be visible
next to the "↻ Refresh" button on first opening the screen. Click "↻ Refresh" and confirm the
label's time value advances (wait at least a second between checks, since the stamp has
second precision).

## F-SET-20 — Translucency toggle (build, S) — outcome: partially-built

Reconfirmed the row's defect exactly as described: `set_translucency` set the field and
`cx.notify()`d but never called `self.changed()`, unlike every sibling setter, and
`SettingsSnapshot` has no `translucency` field, so the value cannot leave the surface via
`on_change` even now.

**Built, within `settings.rs`:**
- `set_translucency` now calls `self.changed()`, matching every sibling setter.
- Documented the remaining gap directly on the `translucency` field, in place.

**Not built, and why:** giving `translucency` an emitted value requires adding a
`pub translucency: bool` field to `SettingsSnapshot`. `SettingsSnapshot` is constructed via
exhaustive struct literals (no `..Default::default()` spread) at two call sites in
`rust/crates/tiller/src/main.rs`
(`settings_snapshot_from_app_settings`/`app_settings_from_snapshot`, ~line 8009). That file
is owned exclusively by `B1-tabbar-zorder` this wave. Adding the field here without a
matching one-line addition there would break the `tiller` binary crate's compilation for a
file I'm not permitted to touch — so the field was not added.

**wantedForeignFiles:** `rust/crates/tiller/src/main.rs` — needs one line added inside
`settings_snapshot_from_app_settings`'s `SettingsSnapshot { ... }` literal, following the
exact precedent already there for `agent_colors` (a UI-only field with no `AppSettings`
backing yet):
```rust
// No AppSettings field yet for translucency (same as agent_colors above).
translucency: SettingsSnapshot::default().translucency,
```
`app_settings_from_snapshot` does not need a matching change unless/until translucency
becomes a persisted `AppSettings` field — right now it would stay UI-session-only, same as
`agent_colors` is today. Once that one line lands, applying the resulting
`SettingsSnapshot.translucency` to a real `WindowBackgroundAppearance` is a separate,
larger main.rs-side task (also not in this file), not required just to make the value leave
the surface.

**Tests:** existing `appearance_controls_drive_theme_translucency_and_font_size` still
passes unchanged (it only asserts the in-memory `translucency` flag flips, which was already
true before this row). `cargo test -p tiller_ui --lib settings::` → `41 passed; 0 failed`.

**howToExercise:** Not user-visibly different yet — this is the honest state. Settings →
Appearance → Translucency toggle still flips the switch's own visual state (unchanged
behavior), but nothing downstream (a persisted setting, a window's actual background
appearance) can react to it yet, because the toggle's value still never reaches
`SettingsSnapshot`. A verifier can confirm the gap is exactly as described by checking that
`SettingsSnapshot` (settings.rs) has no `translucency` field.

## F-CORE-USG-05 — Codex proactive token refresh (both, M) — outcome: built

**Build:** `CodexOAuthCredentials::needs_refresh` (8-day `REFRESH_AFTER` gate) had zero
non-test callers before this row — `CodexUsageFetcher::fetch` only ever refreshed reactively,
after a live 401. `fetch` now checks `needs_refresh` against the loaded credentials before
its first API call; on a token old enough, it calls `refresh_token` proactively and saves the
result on success. A failed *proactive* refresh is not fatal (logged, falls back to the
on-disk token) — the reactive 401 path below remains the real arbiter, so a network hiccup
during the proactive check can't turn a still-good session into `LoggedOut`.

**Exercise:** the row asked to force a real refresh success and confirm the auth file's
unrelated fields survive the merge — `refresh_token` was split into a thin wrapper over a new
`refresh_token_at(url, credentials)`, so a test can point it at a real local HTTP server
instead of `auth.openai.com`. Added a one-shot TCP fixture server (real socket, real `curl`
child process — the same transport production code uses) and drove a genuine 200 response
through `refresh_token_at` into `save_credentials_to`'s merge-not-overwrite logic, confirming
`id_token`, `auth_mode`, and a `custom_field` the fetcher never wrote all survive.

**Commit:** `ced2431`

**Tests:** `cargo test -p tiller_usage codex::` → `11 passed; 0 failed`. New:
`a_real_refresh_success_merges_into_the_auth_file_without_losing_unrelated_fields`,
`a_real_401_response_is_classified_through_refresh_token_at`.

**howToExercise:** This is a background fetch path, not a click. With real Codex credentials
at `$CODEX_HOME/auth.json` (or `~/.codex/auth.json`) whose `last_refresh` is stamped more
than 8 days in the past, trigger a usage fetch (status bar Codex segment, or "Refresh now" on
the Codex AI Provider card) and confirm — via the auth file's `last_refresh` timestamp
updating even though the access token had not yet been rejected — that the refresh happened
proactively rather than only after a 401.

## F-CORE-USG-06 — Discarded refresh-failure classification (build, M) — outcome: needs-foreign-file

`classify_token_refresh_failure` (Reused/Revoked/Expired/Other) is real and reachable — a new
test in this pass (`a_real_401_response_is_classified_through_refresh_token_at`) drives it
through a genuine non-200 HTTP response, not just a direct unit call. But its only production
caller (the reactive refresh-failure branch in `CodexUsageFetcher::fetch`) still discards it:
every case maps to the same `UsageReason::LoggedOut`, because `UsageReason`
(`tiller_usage::model`) has no variant to carry the classification, and the status bar's
render (`tiller_ui::status_bar`) has nothing to match on even if it did.

**Built, within `codex.rs`:** the discard site now logs the classification
(`eprintln!("[codex-usage] token refresh failed: {failure:?}")`) instead of silently dropping
it via `Err(_) => ...`, and the code comment there documents exactly what's still missing and
where.

**Not built — needs files this slice doesn't own:**
- `rust/crates/tiller_usage/src/model.rs` — `UsageReason` (lines ~64-74) needs new variants
  to carry the classification, e.g.:
  ```rust
  pub enum UsageReason {
      NotInstalled,
      LoggedOut,
      /// A refresh attempt failed with a reason worth distinguishing in the UI.
      LoggedOutRefreshFailed(TokenRefreshFailure), // or four separate variants
      TimedOut,
      Error,
  }
  ```
  (or four flat variants `LoggedOutReused`/`LoggedOutRevoked`/`LoggedOutExpired`/
  `LoggedOutOther` if this codebase prefers flat enums to a carried payload — either shape
  needs a decision by whoever owns model.rs this wave, and `TokenRefreshFailure` would need
  to move to (or be re-exported from) a place `model.rs` can reach it without a dependency
  cycle back into `codex.rs`).
- `rust/crates/tiller_ui/src/status_bar.rs` (~line 283-286) — the plain 4-arm match over
  `UsageReason` needs arms for the new variant(s), with distinct copy per failure reason
  (e.g. "Session expired — sign in again" vs. "Access revoked — sign in again").
- Once both land, `codex.rs`'s discard site (codex.rs:336-339, right where the new
  `eprintln!` is) changes from logging to
  `Err(failure) => return UsageFetchOutcome::Unavailable(UsageReason::LoggedOutRefreshFailed(failure))`
  (or the flat-variant equivalent).

**Commit:** `ced2431` (the codex.rs-side logging + doc; model.rs/status_bar.rs untouched).

**Tests:** `cargo test -p tiller_usage codex::` → `11 passed; 0 failed` (includes the new
`a_real_401_response_is_classified_through_refresh_token_at`, which proves the classification
is real and reachable — just not yet routed anywhere the UI can see).

**howToExercise (current, incomplete state):** force a Codex refresh failure (revoke the
refresh token, or point `$CODEX_HOME` at credentials with an already-invalidated
`refresh_token`) and trigger a usage fetch. Today the status bar's Codex segment shows the
same generic "logged out" state regardless of *why* the refresh failed; stderr will show a
`[codex-usage] token refresh failed: Revoked` (or `Reused`/`Expired`/`Other`) line, which is
as far as the classification reaches until model.rs/status_bar.rs are built out.

## Verification summary

```
cargo check -p tiller_usage         → clean (0 warnings in codex.rs)
cargo check -p tiller_ui            → clean (1 pre-existing, unrelated warning: browser.rs pump_task)
cargo test  -p tiller_usage codex:: → 11 passed; 0 failed
cargo test  -p tiller_ui --lib settings:: → 41 passed; 0 failed
```

Two transient compile breaks were hit and waited out during this slice — `tiller_acp` and
`tiller_ui/src/project_identity.rs` were both mid-edit by sibling agents at the moments
checked; neither file is owned by this slice and neither was touched here. Both resolved on
retry once the respective sibling committed.
