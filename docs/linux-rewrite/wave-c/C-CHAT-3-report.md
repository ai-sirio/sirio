# C-CHAT-3 report

Link 3 of 3 of the `C-CHAT` chain. Re-read every owned file from disk before editing (per the
brief) rather than trusting the brief's quoted line numbers.

## `F-CHAT-33` — ledger line 182 — **fixed**

`AcpClient::mcp_warnings()` (`tiller_acp/src/lib.rs`) was fully built and tested but had zero
callers in `tiller_ui/src/chat.rs` — the "code-absent, not a coverage gap" verdict on record.

Added:
- `Chat::mcp_warnings_shown: usize` — a cursor over `mcp_warnings()`'s cumulative (non-draining)
  list, so a warning is surfaced exactly once rather than re-posted every turn.
- `Chat::surface_mcp_warnings(&mut self)` — appends any new warnings as `Entry::Error` cards
  (non-retryable) to the transcript.
- `ErrorKind::McpWarning` — a new, non-retryable error kind (the session stays live; this is
  informational, not a transport failure).
- Wired into `AcpEvent::TurnEnded`, called right before the turn footer is pushed, so a
  misconfigured MCP server the agent silently tolerated is still visible to the user even
  though the turn itself "succeeded".

**howToExercise:** drive a chat turn against an agent whose stderr emits an MCP-configuration
warning (`looks_like_mcp_warning` matches lines containing "mcp" plus a failure word — e.g.
"MCP server 'foo' failed to start"). After the turn ends, the transcript should show a new
error-styled card with that line's text, appearing once (not repeated on the next turn unless a
new warning line arrives). `tiller_acp`'s own `mcp_warnings_stderr_line_is_observed_end_to_end`
test proves the underlying plumbing; this row wires the UI consumer that was missing.

Files: `rust/crates/tiller_ui/src/chat.rs`. Commit `2803ff1`.

## `F-SET-14` — ledger line 302 — **fixed**

Evidence on record: Cancel's `kill -TERM <pid>` on the recorded pid reproducibly left the spawned
login process alive, even under manual `kill -TERM`.

Root cause found live in this sandbox: `x-terminal-emulator` (`cosmic-term` here) runs the `-e`
command in a *new PTY session* — the login command's pgid differs from the terminal launcher's
own pgid (confirmed: `getpgid(child) != getpgid(launcher)` via `/proc/<pid>/stat`). A single
`kill <launcher_pid>`, or even a process-group kill on a group set at spawn time, never reaches
it — the child detaches into its own session as soon as it starts, independent of any group the
parent was launched into.

Fix: replaced the single-pid kill with a live `/proc` descendant walk
(`descendant_pids`, mirroring `tiller_terminal`'s battle-tested
`descendant_process_groups`/`terminate_process_group` shape but built pure-`std` — no `libc`
dependency, since `Cargo.toml` isn't an owned file this slice) — `terminate_login_process_group`
now signals the launcher **and every current descendant found by walking
`/proc/<pid>/task/<tid>/children`** — SIGTERM first, SIGKILL after a 200ms grace period for
anything that ignored it. Run off the render thread via `cx.background_spawn` so the grace-period
sleep never blocks the UI.

Verified live in this sandbox (not just unit-tested): spawned `x-terminal-emulator -e sleep N`,
confirmed the login-analog child's pgid differs from the launcher's, confirmed a plain
`kill -TERM <launcher_pid>` leaves the child alive, then confirmed the same `/proc` walk this fix
uses discovers and kills it.

Added test `descendant_pids_finds_a_child_detached_into_its_own_session` — reproduces the same
detachment shape with `sh -c 'setsid sleep 60 & wait'` (no dependency on any terminal emulator
being installed) and asserts `terminate_login_process_group` actually kills the detached
grandchild.

**howToExercise:** `TILLER_WL_LABEL=<x> Scripts/wayland-drive.sh <out> 'ctl surface.settings.select category=aiProviders; click <Claude Add Account>; wait 1s; click <Cancel>'`, then from a shell check `ps -eo pid,ppid,cmd | grep -i codex-login` (or whatever the spawned login command is) shortly after — it should be gone within ~1s of Cancel, not still running.

Files: `rust/crates/tiller_ui/src/settings.rs`. Commit `e7d8ffa`.

## `F-CORE-AUTH-01` — ledger line 408 — **not-attempted (already-correct per prior triage)**

Re-confirmed the code path: `AgentAccountIdentity::parse_claude_json` (`account.rs:52`) is
genuinely called from `settings.rs:548`'s real `claude auth status` shell-out, not a mock, and
has direct unit coverage for both populated and empty-field JSON
(`parses_account_identity_without_inventing_logged_in_state`). There is no code defect here —
`docs/linux-rewrite/triage/T1-core-plan.md`'s own conclusion (`files: none — exercise only`)
still holds: what's missing is a live OAuth completion (finishing `claude auth login`'s browser
consent) so `claude`'s own CLI writes real JSON for `settings.rs:548` to parse, not a code
change. I did not attempt to force a real OAuth login through — that's a live-drive exercise
task for the critic, not a build task; forcing it here would just be re-confirming what three
prior passes already reconfirmed unchanged.

**howToExercise:** through `wayland-drive.sh`, click a provider's "Add Account", let
`x-terminal-emulator` open, and manually complete the browser OAuth consent for a real Claude
account (cannot be scripted from the drive lane — it's an external browser flow). After the CLI
process exits successfully, confirm the AI Providers card now shows "Signed in
<email>" — sourced from `parse_claude_json` on real stdout, not a placeholder.

No files changed.

## `F-PERSIST-DB-06` — ledger line 509 — **blocked**

Confirmed independently, same evidence as three prior sweeps: `account_identity` (migration v13,
`rust/crates/tiller_persistence/src/migrations.rs`) is live schema but has **zero** read/write
callers anywhere in `tiller_persistence/src/db.rs` — no `account_identity` function exists there
at all (`grep account_identity rust/crates/tiller_persistence/src/db.rs` returns nothing). The
identity `discover_claude_identity`/`discover_codex_identity` produce
(`tiller_ui/src/settings.rs:539-565`) is display-only; nothing persists it and nothing reads it
back on restore.

Closing this properly needs edits to **three files this slice does not own**:
1. `rust/crates/tiller_persistence/src/db.rs` — add
   `pub fn account_identity(&self, provider: &str) -> Result<Option<(String, i64)>, PersistenceError>`
   (`SELECT identity, detected_at FROM account_identity WHERE provider = ?1`) and
   `pub fn save_account_identity(&self, provider: &str, identity: &str) -> Result<(), PersistenceError>`
   (`INSERT ... ON CONFLICT(provider) DO UPDATE SET identity = excluded.identity, detected_at = excluded.detected_at`)
   — same upsert shape as the existing `save_session_ref` (`db.rs:643`).
2. `rust/crates/tiller/src/main.rs` — `Settings` currently has **no** database handle or path at
   all (unlike `Chat`, which receives one through `ChatPersistence` at construction). Wiring
   requires either a new `Settings::set_database_path(PathBuf)` setter (additive, called once
   after `Settings::with_snapshot(...)`) or threading a path through the constructor — either way
   a call in `main.rs` is unavoidable, the same "main.rs bottleneck" the row's own triage
   (`T9-misc-plan.md`) already names.
3. `rust/crates/tiller_ui/src/settings.rs` (owned) — once (1) and (2) land, `discover_claude_identity`/
   `discover_codex_identity`'s success path calls `save_account_identity`, and construction reads
   `account_identity(provider)` as a fallback display value when the live shell-out fails or the
   CLI binary is transiently unavailable (matching the triage plan's own "approach").

I did not implement (3) alone because it cannot compile or do anything observable without (1)
and (2) existing — adding calls to functions that don't exist fails the build, and stubbing fake
ones would misrepresent the row as fixed when the actual persistence never happens. Per the
house rule ("if the fix genuinely needs a file you do not own, do not edit it"), I left `db.rs`
and `main.rs` untouched and am handing off the precise patch above instead.

`wantedForeignFiles`: `rust/crates/tiller_persistence/src/db.rs`,
`rust/crates/tiller/src/main.rs`.

No files changed.

## `F-SET-15` — ledger line 303 — **not-attempted (recommend reclassify, not build)**

Re-confirmed live and by code inspection: this app has exactly one credential slot per provider
by design — the System-default row's "Active" badge is hardwired `true` (its own code comment:
"this app has no isolated accounts"), and "Add Account" re-runs the *same* provider CLI's login
rather than creating a second, independently-selectable account. There is no code path that
could ever produce a second Accounts row, so the row's VERIFY clause ("with multiple accounts,
select System default and another account, confirm the badge moves") is unanswerable as written
— the same shape as `F-SET-21`'s existing N/A-by-design ruling, per
`docs/linux-rewrite/triage/T3-set-plan.md`'s own analysis, which I independently reproduced by
reading the current file rather than trusting the cited line numbers.

Building "real" multi-account credential isolation to make the clause answerable would be a new
feature (triage sizes it **L**), not a fix to existing code, and risks being thrown away without
a design ruling on whether this row should instead be marked N/A-by-design. I did not build it
speculatively.

No files changed.

## `F-SET-20` — ledger line 308 — **blocked**

Re-confirmed unchanged, live and by code inspection: `Settings::translucency: bool` toggles
locally (`set_translucency` calls `changed()` like every sibling setter) but `SettingsSnapshot`
— the only payload that leaves the surface (`surface.settings.select`'s wire response,
`Settings::on_change`) — has no `translucency` field, so the value is structurally trapped no
matter what the toggle does.

The blocker is real and structural, not a missed one-file edit: `SettingsSnapshot` is
constructed with a full explicit field literal (no `..Default::default()`) in exactly one place
outside this crate — `rust/crates/tiller/src/main.rs:8290`
(`settings_snapshot_from_app_settings`) — which is **not owned by this slice**. Adding a field to
an owned struct is fine on its own, but Rust's struct-literal syntax requires every field at each
construction site; since that site lives in an unowned file, adding the field here would make
`cargo build -p tiller` (the house rule's required gate) fail to compile in a file I'm not
permitted to edit. I did not add the field for that reason — doing so and leaving `main.rs`
broken would be strictly worse than reporting this honestly.

Complete patch for whoever owns `main.rs` next:
1. `tiller_ui/src/settings.rs` (owned, not yet touched): add `pub translucency: bool` to
   `SettingsSnapshot`, `false` in its `Default` impl, and route `self.translucency` into it
   wherever `Settings` builds the snapshot it hands to `on_change`.
2. `rust/crates/tiller/src/main.rs`: add one field to the `SettingsSnapshot { ... }` literal in
   `settings_snapshot_from_app_settings` (from a new `AppSettings.translucency`, once that
   exists) and one field to `app_settings_from_snapshot`'s reverse mapping.
3. `rust/crates/tiller_persistence/src/model.rs` + `db.rs` (not owned): add `translucency` to
   `AppSettings`, a `settings_keys::TRANSLUCENCY` constant, and load/save it in `settings()`/
   `save_settings()` — the existing boolean-setting pattern (see `control_socket_enabled`)
   applies directly, no schema/migration change needed (settings live in a generic key-value
   table).

`wantedForeignFiles`: `rust/crates/tiller/src/main.rs`,
`rust/crates/tiller_persistence/src/model.rs`, `rust/crates/tiller_persistence/src/db.rs`.

No files changed.

## Summary

| Row | Verdict I'm reporting | Files touched |
|---|---|---|
| F-CHAT-33 | fixed | `tiller_ui/src/chat.rs` |
| F-SET-14 | fixed | `tiller_ui/src/settings.rs` |
| F-CORE-AUTH-01 | not-attempted (already-correct, exercise-only) | none |
| F-PERSIST-DB-06 | blocked (needs foreign `db.rs` + `main.rs`) | none |
| F-SET-15 | not-attempted (recommend reclassify, not build) | none |
| F-SET-20 | blocked (needs foreign `main.rs` + persistence model/db) | none |

`cargo build -p tiller` is green after every commit in this report.
