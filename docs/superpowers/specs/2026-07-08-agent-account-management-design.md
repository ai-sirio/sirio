# Claude/Codex agent account management

## Problem

`AIProvidersSettingsView`'s Claude Code and Codex sections show only a
single "Status: Active/Logged out" reading of whatever CLI login happens to
be active on the machine. There's no way to add a second account and
switch between them — unlike Orca, which has a full "Accounts" list per
provider (Add Account, per-account Re-authenticate/Remove, one Active at a
time, plus an always-present "System default").

## Scope

Real, functionally isolated multi-account switching for Claude Code and
Codex — not a cosmetic account list. Each added account gets its own
isolated CLI credential store; switching Active changes which credentials
new agent terminals launch with. OpenCode Go and Ollama Cloud are
out of scope (separate spec, already shipped — cookie-based, no CLI login
concept).

## Feasibility (verified)

- `claude` CLI supports `CLAUDE_CONFIG_DIR` to relocate `~/.claude`
  (confirmed via `claude auth --help`: `auth login` / `auth logout` /
  `auth status` subcommands exist, non-interactive `status` usable for
  identity capture).
- `codex` CLI supports `CODEX_HOME` to relocate `~/.codex` (confirmed via
  `codex login --help`: `login` opens browser OAuth, `login status`
  subcommand exists for non-interactive identity capture; Orca itself
  already sets `CODEX_HOME` for its own runtime, per `env`).
- Both `login`/`auth login` commands open the system browser themselves
  for OAuth — Helm does not need to embed a browser or webview.

## Design

### 1. Persistence

New GRDB table `AgentAccountRecord`, shared by both providers (one
`provider` discriminant column rather than two near-identical tables):

| column | type | notes |
|---|---|---|
| `id` | String (UUID) | primary key |
| `provider` | String | `"claude"` \| `"codex"` |
| `configDirPath` | String | `~/Library/Application Support/Helm/agent-accounts/<provider>/<id>/` |
| `label` | String | email, captured post-auth |
| `orgName` | String? | org/team name, if the CLI reports one |
| `createdAt` | Date | |
| `lastAuthenticatedAt` | Date | updated on add and on every successful re-authenticate |

New schema migration version; table starts empty. Existing users see just
"System default" until they explicitly add an account — zero behavior
change on upgrade.

Active-account selection persists as two `AppStorage` strings:
`usage.claude.activeAccountId` / `usage.codex.activeAccountId`. Empty/unset
means "System default" (no env override — today's behavior, unchanged).
Global to the whole app (confirmed): switching Active is not per-worktree
or per-tab.

### 2. Isolation strategy

Config-dir override only (not a full synthetic `$HOME`): each account's
directory holds just that provider's relocated config
(`CLAUDE_CONFIG_DIR` for Claude, `CODEX_HOME` for Codex). Everything else
in the spawned shell's environment — real `$HOME`, PATH, dotfiles — stays
untouched, so agents keep seeing the user's normal project/shell context;
only the CLI's own login state is isolated per account. Directory is
created lazily on "Add Account" and deleted on "Remove".

### 3. Auth flows

**Add Account:**
1. Generate a new `id`, create its config dir.
2. Spawn a hidden background process — `claude auth login` (env
   `CLAUDE_CONFIG_DIR=<dir>`) or `codex login` (env `CODEX_HOME=<dir>`).
   The command itself opens the system browser for OAuth; Helm shows no
   embedded terminal.
3. Settings shows "Waiting for browser login…" + Cancel. Cancel kills the
   process and deletes the half-created dir — no orphaned row.
4. Wait for the process to exit (soft safety-net timeout ~5 minutes;
   Cancel is the primary way out — OAuth flows can be slow and shouldn't
   be force-timed-out aggressively).
5. On success: run identity capture non-interactively in the same env —
   `claude auth status` / `codex login status` — parse email/org from
   stdout.
6. Insert the `AgentAccountRecord` row. The new account is **not**
   auto-activated — user picks "Active" explicitly (matches the
   reference: adding an account doesn't silently steal Active from
   System default).
7. On failure (non-zero exit): show an inline error, delete the
   half-created dir, insert nothing.

**Re-authenticate:** same spawn+wait+identity-capture flow, reusing the
account's *existing* `configDirPath` (refreshes an expired/revoked
session). Updates `lastAuthenticatedAt` and `label`/`orgName` on success;
leaves the existing row untouched on failure.

**Remove:** kill any in-flight auth process for that account, delete its
config dir, delete its row. If the removed account was Active, Active
resets to "System default" (never left pointing at a deleted id).

### 4. Agent-launch wiring

`PtyRuntime.spawnIfNeeded` (spawns `/bin/zsh -l -c "claude"` /
`"codex ..."`, currently inheriting `ProcessInfo.processInfo.environment`
unmodified): before spawn, look up the provider's `activeAccountId`. If
set and the account still exists, inject `CLAUDE_CONFIG_DIR` /
`CODEX_HOME` = that account's `configDirPath` into the child's env; if
unset or stale (account was removed), no override — today's behavior.
Read once at spawn time — switching Active does not retroactively affect
already-running panes, only panes opened afterward.

### 5. Settings UI

Added *into* the existing `Claude Code` / `Codex` sections in
`AIProvidersSettingsView` — the current Status / Last read / "Show in
usage bar" toggle / refresh controls are unchanged; a new sub-block is
appended below, matching the reference layout:

- "Accounts" header + "Showing accounts for this device. New accounts are
  added there." + "Add Account" button.
- Row list: "System default" always first, no Re-authenticate/Remove (it
  isn't a stored account, just "no override"). Then each
  `AgentAccountRecord`: label + org subtitle, "This device" badge
  (always — accounts aren't synced across machines), "Active" badge on
  whichever is currently selected, Re-authenticate / Remove actions.
- Clicking a row sets it Active immediately (no confirmation — cheap,
  reversible).

## Out of scope

- OpenCode Go / Ollama Cloud (cookie-based, no CLI login — separate,
  already-shipped spec).
- Per-worktree or per-tab account selection (explicitly rejected — global
  only).
- Syncing accounts across devices/machines (each device manages its own
  list, per the "This device" badge).
