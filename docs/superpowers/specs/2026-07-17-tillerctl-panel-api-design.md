# Canonical `tillerctl panel` API and Tiller agent skill

**Date:** 2026-07-17  
**Status:** Approved  
**Branch:** `feat-tillerctl-panel-api`

## Problem

Tiller exposes two overlapping terminal-control interfaces:

- nested `tillerctl panel create|write|read|wait` commands backed by `panel.*` requests;
- flat cmux-parity commands such as `new-split`, `list-panels`, `send`, `send-key`, `focus-panel`, and `close-panel`, mostly backed by `surface.*` requests.

The overlap is incomplete and stateful. `panel create` returns the new pane UUID, while `new-split` acts on the selected pane and returns no target UUID. Flat commands often default to the active UI selection. An agent therefore cannot reliably create a terminal, retain its identity, and address it later without depending on visual state.

The repository also has two divergent skill documents: the public `skills/tillerctl-cli/SKILL.md` and a large embedded `TillerSkillDocument.markdown` string. Settings install the public skill only for a subset of supported harnesses, while adapter preparation does not provision the skill into every worktree. Pi and Oh-My-Pi are excluded from the current Settings installer.

## Goals

1. Provide one canonical, deterministic `tillerctl panel` namespace for terminal lifecycle operations.
2. Let an agent create either a new terminal tab or a split, optionally launching a command in the same request.
3. Return and retain a pane UUID for every create operation.
4. Remove implicit dependence on selected worktrees, tabs, or panes.
5. Provision the same Tiller-managed skill automatically for Claude Code, Codex, OpenCode, Pi, and Oh-My-Pi.
6. Keep one source-controlled skill document as the content authority.

## Non-goals

- Replacing workspace, notification, system, or session-restore commands.
- Changing the control socket transport or its line-delimited JSON envelope.
- Adding headless terminals: created terminals remain visible Tiller tabs or splits.
- Editing user-global harness configuration.
- Preserving deprecated pane/surface CLI aliases. This is an intentional clean cutover.

## Canonical CLI contract

All terminal lifecycle operations live under `tillerctl panel`:

```text
tillerctl panel create [--worktree <uuid-or-path>] [--cmd <shell-command>] [--json]
tillerctl panel split <left|right|up|down> [--from <pane-id>] [--cmd <shell-command>] [--json]
tillerctl panel list [--worktree <uuid-or-path>] [--json]
tillerctl panel write --id <pane-id> --input <text> [--enter]
tillerctl panel key --id <pane-id> <key>
tillerctl panel read --id <pane-id>
tillerctl panel wait --id <pane-id> [--timeout-ms <milliseconds>]
tillerctl panel focus --id <pane-id>
tillerctl panel close [--id <pane-id>]
```

### Target resolution

- `panel create` resolves `--worktree`, then `$TILLER_WORKTREE_ID`. Missing both is an error.
- `panel split` resolves `--from`, then `$TILLER_PANE_ID`. Missing both is an error.
- `panel list` resolves `--worktree`, then `$TILLER_WORKTREE_ID`. Missing both is an error.
- `panel close` resolves `--id`, then `$TILLER_PANE_ID`. Missing both is an error.
- `write`, `key`, `read`, `wait`, and `focus` require `--id` because they normally target a UUID returned by `create`, `split`, or `list`.
- No command falls back to the worktree, tab, or pane selected in Tiller's UI.

### Output

- `create` and `split` print only the created pane UUID by default. With `--json`, they print a stable object containing `id`.
- `list` prints the existing stable table columns (`id`, `tab`, `title`, `agent`, `active`) or the JSON array with `--json`.
- `read` preserves raw stdout behavior so callers can pipe or capture scrollback.
- `wait` exits with the child process's exit code. A timeout or unknown pane exits nonzero with a diagnostic on stderr.
- Mutation commands never print a success value before the server operation has committed.

### Launch semantics

`--cmd` is optional for both `create` and `split`:

- without `--cmd`, Tiller starts an interactive shell;
- with `--cmd`, the terminal launches the command as its initial shell command;
- the pane UUID is allocated by the server and registered before a successful response is returned.

This avoids the race inherent in “create, discover active pane, then send command.”

## Canonical control protocol

The CLI maps directly to these control methods:

| CLI | Method | Parameters | Result |
|---|---|---|---|
| `panel create` | `panel.create` | `worktree`, optional `cmd` | `id` |
| `panel split` | `panel.split` | `from`, `direction`, optional `cmd` | `id` |
| `panel list` | `panel.list` | `worktree` | encoded panel rows |
| `panel write` | `panel.write` | `id`, `input` | success |
| `panel key` | `panel.key` | `id`, `key` | success |
| `panel read` | `panel.read` | `id` | scrollback output |
| `panel wait` | `panel.wait` | `id`, optional `timeoutMs` | `exitCode` |
| `panel focus` | `panel.focus` | `id` | success |
| `panel close` | `panel.close` | `id` | success |

`--enter` remains a CLI convenience: it appends one newline to `panel.write` input. Named keys continue to use `TerminalKey` through `panel.key`.

The flat pane commands and their redundant `surface.*` request builders/dispatch branches are removed:

- `new-split`
- `list-panels`
- `list-pane-surfaces`
- `focus-panel`
- `send`
- `send-key`
- `close-panel`

Workspace, notification, system, and session-restore commands remain unchanged.

## App behavior and atomicity

### Create

1. Resolve the explicit worktree UUID or path.
2. Generate a pane UUID.
3. Record an optional initial command for that UUID.
4. Create a tab containing a leaf with that exact UUID.
5. Mount the target worktree host without changing the selected sidebar worktree or bringing Tiller to the foreground.
6. Wait for `PaneRegistry` registration.
7. Return the UUID only after registration succeeds.

If registration times out, remove the tab and any associated command state, then return failure. No orphan tab remains.

### Split

1. Resolve the source pane UUID and owning worktree/tab.
2. Validate the direction.
3. Generate a new pane UUID and record an optional initial command.
4. Mutate the split tree using that exact UUID.
5. Wait for `PaneRegistry` registration.
6. Return the UUID only after registration succeeds.

If registration times out, close the new leaf and remove command state, restoring the pre-request layout.

### Focus and close

Only `panel focus` changes sidebar/tab/pane selection and activates Tiller. Creation and management commands do not steal macOS focus.

`panel close` closes the addressed pane. Unknown UUIDs fail; they are never treated as already successful.

## Skill content and provisioning

### Single source of truth

The repository skill moves to:

```text
skills/tiller/SKILL.md
```

Its frontmatter is:

```yaml
---
name: tiller
description: Use when running inside a Tiller pane to create and manage terminal panels, dispatch worker agents, wait for completion, read output, report status, or leave worktree progress comments through tillerctl.
---
```

The embedded `TillerSkillDocument.markdown` copy is removed. `project.yml` adds the canonical Markdown file to the Tiller app's resources. The app loads that bundled resource and passes its contents into adapter preparation; package tests inject the same repository file as their fixture. There is no second hand-maintained content copy.

The skill teaches:

1. stop unless `$TILLER_ENV=1`;
2. run `tillerctl ping`, then `tillerctl identify --json`;
3. use `panel create --cmd` for a worker in a new tab;
4. use `panel split <direction> --cmd` for an adjacent worker;
5. capture every returned UUID and use it for `write`, `key`, `read`, `wait`, `focus`, and `close`;
6. read output and preserve the child exit code;
7. close temporary panes on success or failure;
8. use `tillerctl notify` only for statuses native hooks cannot report;
9. use `worktree set` for progress comments when appropriate;
10. never infer a target from Tiller's current visual selection.

### Worktree-local provisioning

Each `AgentAdapter.prepare` receives the bundled skill content from the app and provisions the marked, Tiller-managed file without touching global configuration:

| Harness | Worktree-local path |
|---|---|
| Claude Code | `.claude/skills/tiller/SKILL.md` |
| Codex | `.agents/skills/tiller/SKILL.md` |
| OpenCode | `.agents/skills/tiller/SKILL.md` |
| Pi | `.agents/skills/tiller/SKILL.md` |
| Oh-My-Pi | `.agents/skills/tiller/SKILL.md` |

A shared helper writes atomically and overwrites only content carrying Tiller's managed-file marker. Unmanaged user files at the target path cause preparation to fail with a clear error rather than being overwritten. Re-running preparation is idempotent.

The Settings action installs the renamed `tiller` skill from the repository. Local adapter provisioning remains authoritative for all five supported harnesses, including Pi and Oh-My-Pi.

## Migration surface

The implementation updates all callers in one cutover:

- `Tillerctl.swift`: canonical nested commands and removal of flat pane commands;
- `CmuxCommands.swift`: keep non-pane cmux-parity commands only;
- `TillerctlRequestBuilder.swift`: canonical `panel.*` builders;
- `AppModel.handleControl` and `AppModel.handleCmuxControl`: canonical dispatch and removal of redundant pane branches;
- capability reporting: advertise only supported methods;
- `README.md`: replace flat examples with `panel` recipes;
- public and embedded skill artifacts: consolidate under `skills/tiller/SKILL.md`;
- adapter preparation and skill installer configuration;
- all affected tests and test fixtures.

No aliases, compatibility shims, deprecated paths, or duplicate request methods remain.

## Verification

Implementation is test-first with Swift Testing (`@Test` and `#expect`). Required coverage:

1. request builders emit each canonical method and parameter shape;
2. CLI parsing covers explicit targets, environment defaults, `--cmd`, `--enter`, JSON output, invalid directions, and missing targets;
3. `create` and `split` return the server-assigned UUID and wait for registration;
4. registration timeout rolls back the created tab or split;
5. canonical commands never use `selectedWorktree` or `activePaneId` as an implicit target;
6. each adapter provisions an identical, discoverable skill at its harness path;
7. unmanaged skill files are preserved and reported as errors;
8. the public skill content and provisioned content are byte-identical;
9. removed flat commands and removed `surface.*` pane methods are absent from help and capabilities;
10. existing workspace, notification, system, and session-restore commands remain green.

Smoke verification runs the built CLI against a live Tiller control socket:

1. `panel create --cmd 'exit 0'`, capture UUID, `panel wait`, then close;
2. `panel split right --cmd 'printf ready; exit 0'`, capture UUID, read output, wait, then close;
3. confirm both operations work while a different worktree is selected;
4. confirm an invalid UUID and a missing target fail nonzero;
5. confirm the app does not come to the foreground until `panel focus` is invoked.

The final repository gate is `Scripts/ci.sh`, which must print `CI OK`.

## References

- Claude Code skills: <https://code.claude.com/docs/en/skills>
- OpenAI Codex skills: <https://developers.openai.com/codex/build-skills>
- OpenCode skills: <https://opencode.ai/docs/skills/>
- Pi coding agent skills: <https://github.com/badlogic/pi-mono/tree/main/packages/coding-agent>
- Oh-My-Pi repository and project-local agent-skill support: <https://github.com/can1357/oh-my-pi> and <https://github.com/can1357/oh-my-pi/issues/2401>
