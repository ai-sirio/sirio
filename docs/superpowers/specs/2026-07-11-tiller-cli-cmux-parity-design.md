# Tiller CLI (cmux-parity) + manual session restore — Design

Date: 2026-07-11
Status: approved (brainstorming session)

## Goal

Bring Tiller's CLI/socket surface to ergonomic parity with cmux's CLI so
scripts and AI agents written against cmux conventions adapt with minimal
changes, and add a manual "restore previous launch" command. **Ergonomic
parity, not wire compatibility**: cmux command *names* on the CLI, Tiller's
own socket method names underneath.

Explicitly out of scope (deferred, each addable later without breaking this
design):

- Sidebar metadata commands (`set-status`, `set-progress`, `log`, …) — new
  sidebar UI, separate feature with its own brainstorming.
- Custom surface resume commands (`surface resume set --shell "tmux …"`) —
  security-sensitive trust system; native agent resume already covers the
  main case.
- Process-ancestry socket access control (cmux's "cmux processes only"
  mode) — only an on/off toggle for now.
- Tab-level CLI commands (`list-tabs`, `new-tab`, …) — CLI operates on the
  active tab.
- `--window` flag — Tiller has a single main window.

## Concept mapping

| cmux concept | Tiller concept |
|---|---|
| workspace | worktree (qualified by its project) |
| surface / panel | pane (leaf of a tab's split tree) |
| window | — (single window; `--window` not implemented) |
| — | tab (invisible to the CLI; commands target the active tab) |

## Architecture

No new transport. The existing `TillerControl` stack is extended:

- Same unix socket (`~/Library/Application Support/Tiller/control.sock`,
  `$TILLER_SOCKET` override) and framing: newline-delimited JSON
  `{id, method, params}` → `{id, ok, result, error}`. `params` stays
  `[String: String]`.
- New socket methods dispatched in `AppModel.handleControl`, delegating to
  existing AppModel functionality (`split`, `openTab`, sidebar selection,
  `notifier`, `PaneRegistry`).
- New flat kebab-case CLI subcommands added to the existing `tillerctl`
  binary alongside the current noun-verb commands. `panel`, `notify`
  (agent-status mode), `session-ref`, `worktree` are untouched — agent
  hooks keep working unchanged.
- Package placement respects existing boundaries: request builders, CLI
  parsing, and the symbolic-key → escape-sequence table live in
  `TillerControl` (pure, testable); UI/state dispatch lives in
  `App/AppModel`; the socket on/off flag lives in
  `TillerCore/AppSettings`.

## Command surface

| CLI (`tillerctl …`) | Socket method | Behavior |
|---|---|---|
| `list-workspaces [--json]` | `workspace.list` | All worktrees: id, project, branch, path, selected flag |
| `new-workspace --project <id> [--branch <b>]` | `workspace.create` | Creates a worktree via TillerGit |
| `select-workspace --workspace <id\|path>` | `workspace.select` | Selects the worktree in the sidebar (accepts UUID or absolute path) |
| `current-workspace [--json]` | `workspace.current` | Currently selected worktree |
| `close-workspace --workspace <id>` | `workspace.close` | Unmounts the worktree's terminal host (terminates its PTYs); worktree stays in the sidebar |
| `new-split <left\|right\|up\|down>` | `surface.split` | Splits the active pane of the active tab |
| `list-panels [--json]` | `surface.list` | All panes of the current worktree (all tabs): id, title, agent, focused flag |
| `list-pane-surfaces [--json]` | `pane.surfaces` | Panes of the active tab only |
| `focus-panel --panel <id>` | `surface.focus` | Focuses a pane (switching tab if needed) |
| `send [--surface <id>] "text"` | `surface.send_text` | Writes text to a pane; default target: active pane |
| `send-key [--surface <id>] <key>` | `surface.send_key` | Symbolic key press: enter, tab, escape, backspace, delete, up, down, left, right |
| `notify --title <T> [--subtitle <S>] --body <B>` | `notification.create` | User-visible macOS notification |
| `list-notifications [--json]` | `notification.list` | Delivered Tiller notifications via `UNUserNotificationCenter` |
| `clear-notifications` | `notification.clear` | `removeAllDeliveredNotifications` |
| `ping` | `system.ping` | `{"pong": "true"}` liveness check |
| `capabilities [--json]` | `system.capabilities` | Lists available socket methods + socket-enabled state |
| `identify [--json]` | `system.identify` | Current worktree/pane context of the caller |
| `restore-session` | `session.restore` | Re-applies the launch snapshot (see below) |

### `notify` dispatch

The existing `tillerctl notify --session <paneId> --status <s>` (agent
hooks, Layer A) and the new cmux-style `tillerctl notify --title --body`
share one command name. Dispatch is on flags: `--title` present → user
notification (`notification.create`); `--session` present → agent status
(existing `notify` method). Providing both, or neither, is a usage error.

### Symbolic keys

`send-key` maps key names to terminal escape sequences in a pure table in
`TillerControl` (e.g. `enter` → `\r`, `up` → `\u{1B}[A`). The socket method
carries the symbolic name; the app resolves it at write time so the mapping
can account for terminal modes later if needed.

## Active context and environment

Panes spawned by Tiller export to their shell:

- `TILLER_WORKSPACE_ID` — owning worktree UUID
- `TILLER_SURFACE_ID` — pane UUID
- `TILLER_SOCKET` — socket path
- `TILLER_ENV=1` (already present today)

Target resolution for `send` / `send-key` without `--surface`:

1. If the calling environment has `TILLER_SURFACE_ID`, the CLI passes it as
   the target (a process inside a Tiller pane talks to its own pane).
2. Otherwise the app resolves "active pane": the focused pane of the active
   tab of the currently selected worktree.

`identify` reports the caller's worktree/pane (from env when inside a pane,
else the UI-selected context).

## Manual session restore

Automatic restore at launch already exists (GRDB: `TerminalTabRecord`
layout, `PaneScrollbackRecord`, `AgentSessionRecord` + validation +
`resumeCommand`). This feature adds only the manual re-apply:

- At startup, after the automatic restore runs, AppModel keeps the loaded
  snapshot in memory (open worktrees, tabs, validated agent session refs).
- `session.restore` (CLI `restore-session`) and a new menu item
  History → "Restore Previous Launch" (⌘⇧O) re-apply that snapshot:
  reopen closed worktrees/tabs and re-launch agent resume commands for refs
  that were valid at launch. Already-open worktrees/tabs are left alone.
- No new persistent storage: the continuous GRDB persistence remains the
  single source; the launch snapshot is an in-memory copy.

## Socket enable/disable

- New setting in `TillerCore/AppSettings` + Settings UI toggle: "Enable
  control socket" (default ON).
- Launch-time env override: `TILLER_SOCKET_ENABLE=0/1` (accepts
  `true/false`, `on/off`).
- OFF → `ControlServer` is not started (or stopped when toggled live).
- Settings copy must warn: disabling the socket also disables agent
  lifecycle hooks (Layer A of activity detection) — badges degrade to
  title/content/process signals.
- Baseline defense unchanged: POSIX permissions on the socket file (owner
  only).

## Error handling

- Unknown method → `ControlResponse.failure` with `"unknown method: <m>"`.
- Missing/invalid param → failure naming the param.
- Socket disabled → CLI reports "Tiller control socket is disabled or
  Tiller is not running" on connection failure.
- `restore-session` with nothing to re-apply → success with
  `{"restored": "0"}` (idempotent).

## Testing

- `TillerControl` (swift-testing): request builders for every new method,
  CLI argument parsing (including `notify` flag dispatch and its
  usage-error cases), key → escape-sequence table.
- `handleControl` dispatch tests for new methods (App target tests),
  including active-pane resolution fallbacks.
- Pure logic (key table, target resolution rules) kept out of `App/` so it
  tests without UI.
- Gate: `Scripts/ci.sh` prints `CI OK`.

## Decisions log

1. **Parity level**: ergonomic (cmux CLI names, Tiller socket method
   names) — not literal socket compatibility.
2. **Mapping**: workspace = worktree, surface = pane; tabs hidden from CLI.
3. **Scope**: everything except sidebar metadata commands.
4. **Restore**: manual re-apply of launch snapshot only; no custom resume
   commands.
5. **Security**: on/off toggle only; no ancestry check.
6. **Binary**: single `tillerctl` binary, flat commands added beside the
   existing noun-verb ones.
