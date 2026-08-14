# P111 report — settings and adapters that did not exist

Builder: fable. Brief: `docs/linux-rewrite/tasks/P111-settings-and-adapters-that-do-not-exist.md`.
Five rows censused ABSENT in P105, all five built. Per row: what was built, the tests, the
captures, and the conjuncts that were **not** exercised. Cross-cutting findings for `pireview`
at the end. I did not touch `INVENTORY-LEDGER.md`.

Evidence conventions shared by every capture below: wayland lane via `Scripts/wayland-drive.sh`
with `TILLER_WL_LABEL=fable`; credential isolation via
`TILLER_CREDENTIALS=/tmp/p111-fable-cred/credentials.json`; settings DB `/tmp/fable.sqlite`
(label-derived, persists across app boots — that persistence is what makes the relaunch proofs
possible). Store and DB contents were fingerprinted between runs with `ls -l` + `cat` and
python's `sqlite3` (no sqlite3 CLI on this box). Frames and drive logs are committed under
`reference/linux-progress/p111-*`.

---

## F-SET-12 — OpenCode Go cookie + workspace override in settings

**Commit:** `0211121`. **Status: BUILT**, with the judgement the brief demanded made first.

### The judgement (credential facility)

`account.rs:152-163` claimed no credential store exists on this platform. Options were: build a
real Linux facility, argue N/A-platform, or build only the portable half. I built the real
facility: `tiller_usage::CredentialStore`, a 0600 plaintext JSON file under the XDG data dir
(`TILLER_CREDENTIALS` overrides for tests and drives; temp-file + rename writes; a malformed
file reads as absent and heals on the next `set`). Secret Service was considered and rejected
on two grounds: unattended test runs must not write into the user's login keyring, and a
keyring-backed proof is not hermetic — you cannot fingerprint it from a test without the DE.
The security posture matches `~/.codex/auth.json`, which this same crate already treats as
credential ground truth in plaintext. The hard rule — no cookie field that silently drops its
value on quit — is discharged by the relaunch capture, not by argument.

### Built

- `CredentialStore` (new `credentials.rs`, 332 lines incl. unit tests).
- `OpenCodeGoUsageFetcher::fetch(workspace_id_override)` — the settings override skips
  `/_server` workspace discovery; cookie comes from the store on Linux (Keychain stays the
  macOS arm). The Swift normalize asymmetry is preserved: a bare token is normalized to
  `auth=<token>` for OpenCode Go (and only for it — see F-SET-13).
- Account state = cookie presence in the store (`state_from_cookie_presence`), replacing the
  `NoLocalStore` verdict.
- Card controls: masked cookie field, Save inert while the trimmed input is empty, Clear,
  a visible store-failure label that **keeps** the typed value, workspace-override field +
  Clear, both captions ported.
- Persistence: `usage.opencodeGo.workspaceIdOverride` (free text, no clamp) through
  `AppSettings` → `SettingsSnapshot` → `UsageBarPrefs` → status-bar fetch loop.

### Tests

GPUI tests in `tiller_ui` for save/clear/store-failure (the failure test blocks the store path
with a file where the parent dir must be, and asserts the typed value survives plus the exact
`Failed to update the credential store — …` prefix); `credentials.rs` unit tests (0600, heal,
rename); account-state fixture test seeding a real store file. Suites at commit time:
tiller_ui 256, tiller_usage, tiller_persistence 37, tiller — all green.

### Captures (committed in `0211121`)

`p111-set12-card-initial`, `-cookie-typed-masked`, `-cookie-saved-signed-in`,
`-override-typed`, `-relaunch-still-signed-in`, `-cleared` — typed and clicked through the
real virtual pointer/keyboard. Store file verified 0600 with the exact JSON between runs;
DB rows (`usage.opencode*`) fingerprinted. The relaunch frame is a fresh app process reading
only disk state.

### Not exercised

- Success path against real opencode.ai (a fake cookie can only earn the error state).
- The macOS Keychain arm (platform-gated out on this box).
- Concurrent writers to the store (single-process design; temp+rename gives atomicity per
  write, nothing more is claimed).

---

## F-SET-13 — Ollama Cloud provider card, cookie, bar segment, refresh

**Commit:** `7ae343d`. **Status: BUILT.** Same judgement as F-SET-12 (same store; the cookie
rows stand or fall together), plus one parser judgement below that `pireview` should read.

### The parser judgement (flag for pireview)

`ollama.rs` now contains **two parsers on purpose**:

- `parse_ollama_cloud_usage` — the pre-existing JSON parser (`usage_percent`/`usagePercent`
  as f64). F-CORE-USG-03's PASSED verdict stands on this function (see DEAD-MODULES.md), so it
  is **byte-identical, untouched**, still exported.
- `extract_ollama_cloud_usage` — new, and what `fetch()` actually uses. The Swift
  `OllamaCloudUsageParser` regexes `usagePercent\s*:\s*(\d+)` over page **text**, and that
  regex cannot match a quoted JSON key (the quote sits where it demands whitespace-then-colon).
  So the live path is a Swift-parity byte-scan for the first *unquoted* `usagePercent: <int>`
  in the fetched `ollama.com/settings` page, clamped at 100, session window only, labeled
  `"usage"`.

If the ledger ever re-adjudicates F-CORE-USG-03, the JSON parser is its evidence; the page
extractor is the product path. They must not be merged without re-arguing that row.

### Built

- 4th provider card `Ollama Cloud`: status from cookie presence, visibility toggle, refresh
  stepper + Refresh now, masked cookie field with inert-Save/Clear/caption/error label —
  same contract as F-SET-12's controls.
- Deliberate omissions, both Swift parity: **no Accounts section** (the Swift card has none —
  `provider_login_command` returns `Option` and is `None` here; `launch_account_login`
  no-ops via let-else), **no workspace override** (OpenCode-Go-only concept).
- The cookie is sent as a **raw** `Cookie:` header — the Swift fetcher does not normalize a
  bare token for Ollama the way the OpenCode Go path does. Asymmetry ported deliberately and
  documented in the module doc.
- `OllamaCloudUsageFetcher::fetch()`: no cookie → `Unavailable(LoggedOut)`; GET
  `https://ollama.com/settings`, 12s timeout; non-2xx → `Error`; `TimedOut` → `TimedOut`;
  transport failure → eprintln `[ollama-cloud-usage]` + `Error`; extractor miss → `Error`;
  hit → `Success`. Best-effort degradation by design, matching the Swift provider.
- Status bar: `ollama_visible` pref (default **false**), `Ollama Cloud` segment. There is no
  Ollama brand asset in the vendored icon set, so the segment uses `Icon::Globe` and the card
  uses a `"☁"` glyph, both marked as declared stand-ins in comments (vendoring a brand SVG
  means license diligence plus `assets/` edits outside my file set — left for a design pass).
- Persistence: `usage.ollamaVisible` through the same pipeline; sqlite row ordering asserted
  in `session.rs`.
- Control socket: `surface.settings.open` reply now includes `ollamaShowInBar` —
  `SettingsReport` gained the flat field. This gap was **caught during the captures**: the
  first probe's ctl reply listed `opencodeShowInBar` but no ollama key. The dump in
  `p111-set13-flow.log` shows the fixed reply.

### Tests

- `tiller_usage` 36+3: extractor tests (first-unquoted-match, clamp-above-100) plus the
  original JSON-parser test untouched; account-state test seeds a real store fixture and walks
  SignedOut → SignedIn → SignedOut on cookie set/delete.
- `tiller_ui` 263: three new GPUI tests — save (store written 0600, input drained, signed in,
  visibility auto-on, snapshot carries `ollama_show_in_bar`), clear (store emptied, signed
  out, visibility off), store-failure (typed value kept, exact error prefix, nothing
  persisted). Also: inert-Save-writes-nothing is asserted before the type step.
- `tiller_persistence` 37 (relaunch integration test now asserts `ollama_show_in_bar`),
  `tiller` 140 (round-trip test renamed `…all_eighteen_fields_explicitly`; both fixtures and
  both direction asserts extended; `session.rs` expects the `usage.ollamaVisible` row).

### GPUI finding (refines F-SET-12's below-the-fold note)

The three new GPUI tests initially failed with the field never receiving input: in a
`VisualTestContext`, `debug_bounds` **sees** controls painted beyond the window bounds, but
`simulate_click` beyond those bounds lands on nothing — silently. F-SET-12's tests passed
only because card 3 happened to end inside the default test window; card 4 does not. Fix:
`cx.simulate_resize(gpui::size(px(1100.0), px(3200.0)))` right after context creation
(`VisualTestContext::simulate_resize`, zed `test_context.rs:883`) — the test-lane twin of the
wayland lane's tall-resolution override. Any future card 5 test must do the same or it will
assert against a hit-test void while `debug_bounds` swears the control exists.

### Captures (committed with this report; drive transcripts in `p111-set13-{flow,relaunch}.log`)

Store started `{}` (F-SET-12's cleared end-state), DB had no `usage.ollamaVisible` row.
Resolutions `W1=1460 H1=2250 / W2=1459 H2=2249` (4 cards ≈ 2250px tall).

1. `p111-set13-card-initial` — 4th card present: Not signed in, toggle off, Save inert.
2. `p111-set13-cookie-typed-masked` — clicked the field, typed `sessp111ollama` through the
   virtual keyboard: 14 mask dots, never plaintext; Save enabled; still Not signed in.
3. `p111-set13-cookie-saved-signed-in` — after Save: green Signed in, toggle auto-ON, input
   drained, no error label. Store: `{"ollama-cloud-cookie":"sessp111ollama"}` at 0600.
   DB: `usage.ollamaVisible = 'true'`.
4. `p111-set13-bar-after-save` — Escape to the workspace: the bar now shows
   `Ollama Cloud logged out` beside the Claude/Codex segments. "Logged out" is honest: the
   segment reports the **last completed fetch** (startup, pre-save, empty store); the card's
   "Signed in" is credential presence at render. Two different truths, both correct.
5. `p111-set13-relaunch-still-signed-in` — **fresh app process**: card comes up Signed in,
   toggle ON, purely from disk. The hard rule ("no field that silently drops the value on
   quit") is discharged here.
6. `p111-set13-bar-after-refresh` — clicked Refresh now on the card, Escape: segment reads
   `Ollama Cloud error`. The refresh consumed the **stored** cookie, hit ollama.com live
   (no `[ollama-cloud-usage]` transport line in the app log, so a real HTTP response came
   back), found no unquoted `usagePercent` in what a fake session earns, and landed on
   `Unavailable(Error)`. Note the segment's two Unavailable reasons render distinctly:
   `logged out` without a cookie vs `error` with an unusable one.
7. `p111-set13-cleared` — Clear: Not signed in, toggle off. Store back to `{}`,
   DB `usage.ollamaVisible = 'false'`.

### Not exercised

- The **success path end-to-end**: a real percent in the bar requires a genuine ollama.com
  session cookie, which cannot be fabricated. The extractor's success behavior is unit-tested;
  the segment's Success rendering shares the code path of the Claude/Codex segments visible
  in the same frames.
- The macOS keychain arm of `ollama_cloud_local_cookie` (platform-gated).
- `HttpError::TimedOut` mapping (12s; not driven live).
- `fetch()`'s transport itself has no unit test (it shells out to curl); the pure extractor
  is what's unit-tested, and the live lane exercised the transport in its error shape only.

---

## F-SET-17 — agent-registry error state, surface and retry

**Commits:** `2dcbc1d` (code), `ec53f40` (captures). **Status: BUILT.**

### The judgement

Same shape as the cookie rows: the error state could not exist by construction —
`discover_availability()` returned a bare `Vec`, so a probe failure (EACCES on a PATH
directory, PATH unset) collapsed into "Not found on PATH", a **false absence claim**. "The
fixed adapter list can't fail" and "probing whether each CLI exists can't fail" are different
claims; only the first is true. So the row is a real gap, not an N/A.

### Built

`DiscoveryError`; `find_executable_in_path_checked` with **found-wins-over-error** semantics
(an unreadable dir only surfaces as an error when the binary is found nowhere else — otherwise
the absence claim would be a guess); `try_discover_availability[_in]` failing the sweep whole,
like the Swift registry's single banner; legacy infallible signatures kept byte-compatible for
`tab_bar.rs`/`main.rs` by delegating through the checked path. Settings routes construction
and the Agents screen's ↻ Refresh through `apply_agent_discovery`: success replaces rows and
clears the error; failure keeps the rows from the **last successful sweep** on screen under a
warning banner (`settings-agents-registry-error`) — mirroring AcpAgentCenter's keep-last-good.
The existing Refresh button is the retry; no new chrome invented.

### Tests

`tiller_agents` 52 green (checked-path units incl. the found-wins-over-error case);
`tiller_ui` 256 green (banner render, retry clears).

### Captures (committed in `ec53f40`)

One app lifetime, `PATH=/tmp/p111-locked:/usr/bin:/bin` with the first entry `chmod 000`:
`p111-set17-registry-error` (banner, no false absence rows) → `chmod 755` → real
virtual-pointer click on ↻ → `p111-set17-registry-recovered-by-retry` (banner gone, five
truthful "Not found on PATH" rows — that PATH really has no agents).
`p111-set17-agents-full-path` is a separate clean-PATH launch resolving the machine's real
install set. Locale note: the underlying io error renders in the system locale (Italian on
this box) inside the banner — cosmetic, and honest about what the OS returned.

### Not exercised

- The PATH-unset variant (only the EACCES-directory variant was driven live; the unit tests
  cover the unset shape).
- Visual parity with the Swift banner (different chrome; the *behavioral* parity —
  single banner, keep-last-good, refresh-as-retry — is what was ported).

---

## F-AGENT-OPENCODE-03 — opencode noninteractive summarizer command

**Commit:** `01c1739`. **Status: BUILT.**

`AgentAdapter::summarizer_command` added with an honest `None` default; the OpenCode
implementation generates `opencode run --pure '<prompt>'`, shell-quoted with the same
discipline the Swift original uses (checked against `ShellQuote.swift` before assuming
quoting was free). This row is a command *generator*, so the argv unit test is genuinely the
right proof — and beyond it, the generated argv was **executed live on this box**: exit 0,
only the model answer on stdout. No capture (nothing visual); the live run is recorded in the
commit message. Not exercised: nothing material.

## F-AGENT-OMP-03 — oh-my-pi noninteractive summarizer command

**Commit:** `56977f2`. **Status: BUILT (generator); live-run half unexercisable.**

Generates `oh-my-pi --print --no-tools '<prompt>'`. One documented deviation: the Swift
original spells the executable `omp`, but the distribution ships no such alias — the adapter
follows the crate's executable-name discipline and spawns `oh-my-pi`. The live-run half of
the clause stays unexercisable: `oh-my-pi@0.2.0` ships un-transpiled TypeScript in
`bin/oh-my-pi.js` and dies at parse before ever reading argv — re-measured on 2026-08-14, not
assumed from memory. Argv unit test green. When upstream ships a transpiled build, the live
half should be re-run before the ledger calls the row fully closed.

---

## Cross-cutting findings for pireview

1. **F-CORE-AUTH-03 / B-62.** F-SET-12 replaced the `NoLocalStore` account verdict with
   cookie-presence over a real store. Any ledger row whose PASSED stands on "no local store
   exists on Linux" (F-CORE-AUTH-03's shape, B-62's verdict) is superseded and needs
   re-adjudication against `credentials.rs`. The `NoLocalStore` enum variant is retained,
   now unconstructed, with a doc stating it is the honest answer for a future provider —
   not dead code to strip.
2. **Two parsers in `ollama.rs`** — see the F-SET-13 parser judgement. DEAD-MODULES.md's
   claim about the JSON parser being the adjudicated evidence still holds; the page extractor
   is the live path. Do not let a cleanup pass merge them.
3. **Settings-keys doc looseness (pre-existing).** The Linux rewrite's `usage.<provider>Visible`
   family (`usage.ollamaVisible` added here) follows F-SET-10/11's established pattern. The
   settings_keys doc claims keys are "verbatim" from Swift's `TillerCore/AppSettings.swift` —
   but that file has no `usage.*` keys at all (the Swift usage keys are @AppStorage keys in
   the usage feature's own files). The claim never covered this family; noting, not fixing —
   the doc is not mine to edit.
4. **GPUI hit-test trap** (F-SET-13 section above): `debug_bounds` sees beyond the test
   window; clicks there land on nothing, silently. `simulate_resize` before interacting with
   below-the-fold controls. This retroactively explains why F-SET-12's tests were only
   *accidentally* sound.
5. **Sweep flake, honestly recorded.** The first full-workspace sweep showed `tiller_ui`
   `262 passed; 1 failed` — and the failing test's **name was lost by my own grep filter**
   on the sweep output. An immediate standalone `cargo test -p tiller_ui` rerun: 263 green.
   A second, unfiltered full-workspace sweep: green across every crate, `tiller_ui` 263/263.
   I checked the obvious mechanism — no `set_var`/`remove_var` anywhere in `tiller_ui`
   settings tests or `tiller_usage` (store paths are constructor-injected, pid-suffixed) —
   so cross-test env races are ruled out. The failure is unreproduced in two full runs and
   its identity is unrecovered. Recorded as an open observation, not explained away; if it
   recurs, capture the unfiltered output first (`FLAKY_TEST_FINDINGS.md` exists for this).
6. **Drive-lane one-off.** One flow invocation produced zero script output (no frames, no
   FAIL line, store untouched) and succeeded identically on retry under `tee`. Unreproduced;
   both drive transcripts are committed so the successful runs are replayable verbatim.
7. **Out-of-set edits, all schema-forced.** My file set was `settings.rs`, `status_bar.rs`,
   `tiller_agents/`, `tiller_usage/`. The rows additionally forced: `tiller_persistence`
   (settings keys + relaunch test), `tiller/src/main.rs` (snapshot↔AppSettings mappings,
   control dump pairs, round-trip tests), `tiller/src/session.rs` (row expectations). Every
   main.rs/session.rs hunk in the F-SET-13 commit was verified mine before staging (codex11's
   chat work had already been committed separately; no foreign hunks were swept in).
   `chat.rs`/`composer.rs` and sidebar/terminal/changes files were never touched.
