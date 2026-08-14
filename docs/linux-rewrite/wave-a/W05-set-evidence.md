# W05-set evidence — Wayland lane (label `wavea-W05-set`)

HEAD under test: `4073297`. Captures under `reference/linux-progress/wavea-W05-set/`.

## `F-SET-12` — ledger line 300, currently **FAILED — absent**

**Approach taken:** re-drove live per triage (same-day build 7ae343d landed after the recorded
"no cookie UI or state" evidence). Opened Settings → AI Providers over the socket
(`surface.settings.open` + `surface.settings.select section=ai-providers`), forced the nested
output tall (1715x2400) to bring the OpenCode Go card into frame without a scroll primitive, then
drove the real gesture: clicked the "Session cookie" field at its actual on-screen coordinates,
typed a token through the persistent virtual keyboard, clicked Save, and forced a repaint.

**Observed, in order:**
- `04-tall-typed2.png` — click landed on the wrong y (script's `OUTPUT_W`/`OUTPUT_H` were stale
  from the `shot()` default 1715x972 while the output had been manually resized to 1715x2400 via
  raw `swaymsg`, so the pointer command's coordinate normalisation used the wrong denominator).
  Field stayed empty — a genuine miss, not a defect; corrected by setting `OUTPUT_W=1715
  OUTPUT_H=2400` in the action block before the next click.
- `05-tall-typed3.png` — corrected click at (650,1005) landed on the field: focus ring lit orange
  and masked dots (`••••••••••••••`) appeared as `TESTCOOKIE123` was typed — proves the field is a
  real, focusable, keystroke-accepting SecureField, not decoration.
- `06-after-save.png` — clicked Save (1220,1005). Three things changed in the same live frame,
  none of which the app would show on its own: the cookie field cleared back to its placeholder
  (`save_opencode_cookie`'s documented success path), OpenCode Go's Status flipped from grey "Not
  signed in" to green "Signed in", and its "Show in usage bar" toggle flipped ON by itself. This is
  a real write through `CredentialStore` reaching `LocalAccountState` (account.rs), driven through
  the same click→type→click path a user would use, not a socket shortcut.

**Claim:** exercised-working. Full loop closed: field renders, accepts real keystrokes, Save
reaches a real credential store, and the account-status/usage-bar-toggle UI reads the change back
live.

**Captures:** `reference/linux-progress/wavea-W05-set/02-tall-before.png`,
`04-tall-typed2.png` (the coordinate-bug frame, kept as a negative control),
`05-tall-typed3.png`, `06-after-save.png`.

---

## `F-SET-13` — ledger line 301, currently **FAILED — absent**

**Approach taken:** same commit (7ae343d) built the Ollama Cloud provider card, cookie UI, bar
segment and fetcher per triage; re-drove live with the same corrected click→type→click gesture
used for F-SET-12 (coordinates recomputed from a fresh crop of the Ollama Cloud card since its
"Session cookie" field sits ~250px further down the page than OpenCode Go's).

**Observed:**
- `08-ollama-before.png` — Ollama Cloud card renders in full: Status "Not signed in", Show in
  usage bar OFF, Refresh interval, Session cookie field with placeholder, Save/Clear, caption
  text naming ollama.com's DevTools → Network → Cookie header — same pattern as OpenCode Go, a
  real card not a stub.
- `11-ollama-typed2.png` — click on the field at its real coordinates (650,1510) focused it
  (orange ring) and typing `OLLAMATESTCOOKIE` produced masked dots.
- `12-ollama-after-save2.png` — clicked Save (1220,1510): field cleared back to placeholder, and
  Status flipped from "Not signed in" to green "Signed in" (`12-status-crop.png`) — a real write
  through the same `CredentialStore`/`OllamaCloudUsageFetcher::COOKIE_KEY` path, driven by the
  same real click+type+click gesture a user would use.

**Claim:** exercised-working. Same shape and same real end-to-end result as F-SET-12.

**Captures:** `reference/linux-progress/wavea-W05-set/08-ollama-before.png`,
`09-ollama-typed.png` (first, mis-targeted attempt — kept as a negative control showing the field
still empty when the click misses),`11-ollama-typed2.png`, `12-ollama-after-save2.png`.

---

## `F-SET-14` — ledger line 302, currently **FAILED — defective**

**Approach taken:** triage's premise ("dead control claim is false") checked against source: read
`settings.rs`'s `manage_account_handler` (line 2007-2015) and `launch_account_login` (line 1244).
It really does fall back to a self-contained handler when no host callback is wired:
`if let Some(handler) = host_manage_account.as_ref() { handler(provider_id) } else {
settings.launch_account_login(provider, cx) }`, and `launch_account_login` spawns
`x-terminal-emulator -e <program> <args>` with a real `provider_login_command(provider)`. Grep
confirms `main.rs` never calls `.on_manage_account(...)` (only caller of the *builder method* is
the test at settings.rs:4946), so production always takes the fallback branch, not a stub.

**Live drive (this session, not reused from P120-report.md):** opened Settings → AI Providers,
clicked the real "Add Account" button on the Claude Code card at its live coordinates
(`13-before-click.png` / `14-after-add-account-click.png`).
- The captured frame shows no visible change — no dialog, no spinner, no error text, cursor just
  sitting on an unchanged button. Read alone this looks exactly like the old "dead control"
  verdict.
- `ps aux` at that moment told the other half of the story: `x-terminal-emulator -e claude auth
  login` was a real running child process (pid 4058172) — the click really did reach
  `launch_account_login`, which really did spawn a genuine OAuth-login subprocess with the
  correct provider command. Killed the process afterward to avoid leaving a stray terminal.

**Claim:** exercised-working for the fallback-login path — reconfirms the ledger's current,
already-corrected reasoning (self-contained `launch_account_login` fires, no host wiring needed)
independently, from a fresh click rather than reused evidence. The frame alone is not proof of
anything here (a null screenshot result is not evidence per the lane's own rule) — the process
list is what closes it. The remaining named gap (no in-app waiting/cancel/retry affordance while
the spawned terminal runs) is still true: nothing in either capture renders such a control, and
none exists in `settings.rs` for this path.

**Captures:** `reference/linux-progress/wavea-W05-set/13-before-click.png`,
`14-after-add-account-click.png` (screenshots only prove the click landed on the right pixel;
the discriminating evidence is the live process list, recorded in this entry since a process
list isn't a screenshot).

---

## `F-SET-15` — ledger line 303, currently **half-proven**

**Approach taken:** triage says this row shares F-SET-14's premise but the underlying claim (no
multi-account model) is independently true by explicit design. Re-checked live rather than assume.

**Observed:** same `14-after-add-account-click.png` frame used for F-SET-14 also answers this row:
every provider card (Claude, Codex, OpenCode Go) shows exactly one "System default / This device /
Active" row under Accounts, both before and after the Add Account click and its real spawned
login subprocess — the click does not add a second row, cannot (the login command means "reuse
your CLI's session on this device," not "create a new isolated slot"), and there is nowhere in the
UI to reach a second account without a source edit. Grep reconfirms `on_manage_account`
(settings.rs:799/967) has exactly one call site workspace-wide and it is the test at
settings.rs:4946.

**Claim:** exercised-working for the "single account slot, Active hardwired" half (freshly
reconfirmed, not reused) — the half this row's clause can actually resolve. The other half (can a
user reach a *second* account) remains could-not-reach without a source edit, structurally, same
as the existing ledger note; nothing new closes it and nothing regressed it.

**Captures:** `reference/linux-progress/wavea-W05-set/14-after-add-account-click.png`.

---

## `F-SET-17` — ledger line 305, currently **FAILED — absent**

**Approach taken:** triage says built (commit 2dcbc1d, `try_discover_availability()` fallible +
registry-error banner + Retry via the existing Refresh button) and already live-photographed
(commit ec53f40). This session re-drove it independently rather than reuse that photograph, using
a fresh negative control: a *second* Tiller instance on the same Wayland lane label family
(`wavea-W05-set-patherr`, its own nested sway + socket + DB, `TILLER_SOCKET=/tmp/wavea-W05-set-
patherr.sock`), launched with `PATH` fully unset in the app's own environment (not the driving
shell's — the compositor/grim/python3 calls used the normal PATH throughout).

**Observed:**
- `panel`/`surface.settings` state read back over the fresh instance's socket:
  `"providers":"[]"` — zero rows, not stale ones.
- `16-patherr-agents.png` — the Agents screen renders the real banner text from
  `registry_error_message`: "⚠ Could not load the agent registry: PATH is not set in the
  environment", with an empty rows list and the Refresh button still present (the same button
  serves as Retry, per source: `refresh_agent_availability` calls
  `apply_agent_discovery(try_discover_availability())`).

Positive control for the same code path was already captured this session for F-SET-11/14/15's
work: the main `wavea-W05-set` instance's own AI Providers screen shows 5 real, non-empty provider
rows with genuine account state (`13-before-click.png`), so an empty/error state is not this
lane's default — it took the deliberate PATH removal to produce it.

**Claim:** exercised-working. The registry-error banner and its zero-false-row behavior are real
and reachable live, independently reproduced from a fresh drive (not reused evidence). Did not
re-drive the Retry-clears-the-banner half (already proven in ec53f40's own capture) to keep this
row's time-box tight; nothing here contradicts it.

**Captures:** `reference/linux-progress/wavea-W05-set/16-patherr-agents.png`.

---

## `F-SET-11` — ledger line 299, currently **half-proven**

**Approach taken:** triage's approach is to drive the remaining 4 of 5 `UsageReason` clause states
(`NotInstalled`, `LoggedOut`, `TimedOut`, `Error`) beyond the already-proven `Loaded`/valid state.
Investigated the real fetch path (`claude.rs`) rather than guess: `ClaudeUsageFetcher::fetch`
spawns a **login shell** (`login_shell()` → `$SHELL` or `/bin/bash`, run as `-lc claude`), and
`NotInstalled` fires either when `Pty::spawn` itself fails, or when the PTY output later matches
`classify_failure`'s "command not found" text. Codex/OpenCode's fetchers follow the same PTY
pattern (`codex.rs`, 15s/25s real timeouts per the manifest).

**Why the remaining 4 states were not driven this pass:** every route to them touches *shared,
destructive* state on a machine 10 sibling agents are actively driving from this same session:
- `NotInstalled` needs `claude`/`codex` to be genuinely unreachable from a **login shell**, not
  just this process's own `PATH` — `bash -lc` re-sources `~/.bashrc`/`~/.bash_profile`, which on
  this machine re-exports `PATH` (nvm, `~/.local/bin`) independent of the parent environment (this
  is exactly why F-SET-17's plain `PATH`-unset trick, which does work for the Agents-screen
  registry, does not transfer here: that path checks `std::env::var_os("PATH")` directly in-process,
  this one goes through a fresh login shell). Reproducing it live means either editing the user's
  real shell dotfiles or `chmod`-ing the real `claude`/`codex` binaries unreachable — both break
  every other agent's terminal sessions on this shared machine for the rest of the run.
  Attempted the safer edge (unset `PATH` for the app process only, matching F-SET-17's technique)
  and rejected it after tracing `login_shell()`'s actual behavior — did not spend the time driving
  it live only to file a false negative.
- `LoggedOut` needs real Claude/Codex credentials removed or expired — the same credentials this
  session's own `13-before-click.png` shows signed in as `e.palmisano@reply.it`, and that every
  sibling agent's own Claude Code/Codex CLI session depends on for its own work.
- `TimedOut` needs the real fetch host blackholed for up to the fetcher's real 25 s timeout — no
  sandboxed route to do that scoped to one process without a namespace/firewall change that would
  also stall sibling agents' live CLI traffic.
- `Error` was already flagged by the ledger's own prior pass as needing a stub server — still true,
  still out of scope for a no-source-edit exercise pass.

**Claim:** could-not-reach for the 4 remaining states — every route available on this lane means
either mutating shared credentials/PATH/network state that ten concurrently-running sibling agents
depend on, or a source/infra change outside this pass's scope. The existing `Loaded`/valid-state
proof on record (Claude 49%/77%, Codex 100%) stands unchanged; nothing here contradicts it or the
row's current half-proven verdict.

**Captures:** none new for this row — no drive was performed that produced discriminating
evidence beyond what is already on record.

---
