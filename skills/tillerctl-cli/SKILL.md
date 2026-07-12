---
name: tillerctl-cli
description: Use when running inside a Tiller pane and you need to orchestrate other panes/worktrees — list/create/split panes, send text or keys to another terminal, check your own worktree/pane context, or post notifications back to Tiller.
---

# Using tillerctl

`tillerctl` is a CLI that talks to a running Tiller.app over a local unix
socket. It lets an agent running inside one Tiller pane control other panes
and worktrees — split a new pane, send text or keystrokes to it, check what
it's doing, and get notified when things finish.

It only works when Tiller is running and its control socket is enabled
(default: on). If a command fails with "Tiller control socket is disabled
or Tiller is not running", stop — there's nothing to orchestrate.

## Self-identification

Every pane Tiller spawns has `$TILLER_WORKTREE_ID` and `$TILLER_PANE_ID` set
in its environment automatically. Use `identify` to resolve them to
human-readable info without parsing the env vars yourself:

```bash
tillerctl identify
# project<TAB>branch<TAB>path<TAB>workspaceId<TAB>surfaceId
```

Run this first when you need to know "where am I" before acting on another
pane (e.g. to avoid sending text to yourself).

## Command reference

### Workspaces (worktrees)

```bash
tillerctl list-workspaces                          # id / project / branch / path / selected
tillerctl current-workspace                         # the one selected in the sidebar
tillerctl new-workspace --project <uuid-or-name> [--branch <name>]
tillerctl select-workspace --workspace <uuid-or-path>
tillerctl close-workspace --workspace <uuid-or-path> # unmounts terminals; worktree stays in sidebar
```

### Panes (surfaces)

```bash
tillerctl list-panels                                # panes of the current worktree
tillerctl list-pane-surfaces                          # panes of the active tab only
tillerctl new-split <left|right|up|down>              # split the active pane
tillerctl focus-panel --panel <uuid>                  # bring a pane to the foreground
```

### Sending input

```bash
tillerctl send [--surface <uuid>] "some text"         # types text into a pane, no Enter
tillerctl send-key [--surface <uuid>] enter            # submits/confirms in that pane
```

If `--surface` is omitted, the target is your own pane (`$TILLER_PANE_ID`)
if set, else whatever pane is currently active in the UI. Always pass
`--surface` explicitly when acting on a pane other than your own.

**Gotcha — always use `send-key … enter` to submit, never rely on a
trailing newline in `send`.** Interactive terminal UIs (coding agents
included) run their input box in raw terminal mode, which only recognizes a
literal carriage return (`\r`, `0x0D`) as "Enter". A plain newline
(`\n`, `0x0A`) — which is what canonical-mode shells auto-translate `\r`
into, but raw-mode TUIs do not — will sit in the input box unsubmitted.
`send-key enter` sends the correct `\r` byte; `send` does not append
anything. Sequence for "type a task and run it":

```bash
tillerctl send --surface <uuid> "do the thing"
tillerctl send-key --surface <uuid> enter
```

### Notifications

```bash
tillerctl notify --title "Done" --body "Task finished"   # user-visible macOS notification
tillerctl list-notifications                               # date / title / subtitle / body
tillerctl clear-notifications
```

### Session restore

```bash
tillerctl restore-session   # re-applies the layout Tiller loaded at launch
```

## When to use this vs. not

Use `tillerctl` when you're **orchestrating something outside your own
pane**: spawning a helper agent in a new split, sending it a task,
polling/reading its output, or notifying the user when a long job finishes
elsewhere. Don't use it to talk to your own terminal — your normal
stdin/stdout already does that, and `tillerctl send-key` targeting yourself
adds nothing.
