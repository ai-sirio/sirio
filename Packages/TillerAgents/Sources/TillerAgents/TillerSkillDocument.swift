/// The agent-facing skill document teaching agents inside a Tiller pane to
/// drive tillerctl. Written to `<worktree>/.claude/skills/tiller/SKILL.md` by
/// `ClaudeCodeAdapter.prepare` on every agent pane setup (machine-managed,
/// always overwritten — versioned with the app, like the hook arms in
/// settings.local.json).
public enum TillerSkillDocument {
    public static let markdown = #"""
---
name: tiller
description: "Control Tiller from inside it. Dispatch worker panels, run commands, wait for exit, read output, report your status, and leave worktree progress comments — via tillerctl talking to the running Tiller app over a local unix socket. Use when running inside a Tiller pane (TILLER_ENV=1)."
---

<!-- Machine-managed by Tiller (ClaudeCodeAdapter). Overwritten on agent pane
     setup — do not hand-edit. -->

# tiller — agent skill

Before using this skill, check that `TILLER_ENV=1`. If it is not set to `1`,
say you are not running inside a Tiller-managed pane and stop. Do not control
Tiller from outside it.

You are running inside Tiller, a native macOS terminal for orchestrating AI
agents. Tiller gives you projects, worktrees, and panels — each panel is a real
terminal. The `tillerctl` binary talks to the running Tiller app over a unix
socket (path in `$TILLER_SOCKET`; tillerctl finds it automatically).

Your identity:

- `$TILLER_PANE_ID` — your panel's UUID. Use it for `notify` and `session-ref`.
- `$TILLER_WORKTREE_ID` — your worktree's UUID. Use it for `panel create`.

This means you can:

- dispatch a worker panel, run a command in it, wait for it to exit, read its output
- report your own status so Tiller's sidebar reflects reality
- leave short progress comments on your worktree

## commands

Dispatch a panel in your worktree (prints the new panelId; opens a visible
tab and focuses your worktree in the app):

```bash
PANEL=$(tillerctl panel create --worktree "$TILLER_WORKTREE_ID" --cmd "swift test")
```

Without `--cmd` the panel starts a plain shell.

Write input to a panel (`--enter` appends newline):

```bash
tillerctl panel write --id "$PANEL" --input "ls" --enter
```

Wait for a panel's process to exit (exit code passes through as tillerctl's
exit code; omit `--timeout-ms` to wait indefinitely):

```bash
tillerctl panel wait --id "$PANEL" --timeout-ms 600000
```

Read a panel's scrollback snapshot (raw text on stdout):

```bash
tillerctl panel read --id "$PANEL"
```

Report your own status (`running` | `needs-input` | `done` | `error`):

```bash
tillerctl notify --session "$TILLER_PANE_ID" --status done
```

Leave a short progress comment on your worktree (shows in the sidebar;
accepts worktree UUID or absolute path):

```bash
tillerctl worktree set --worktree "$TILLER_WORKTREE_ID" --comment "tests green; writing docs"
```

## recipes

### run a build in a worker panel and inspect the result

```bash
PANEL=$(tillerctl panel create --worktree "$TILLER_WORKTREE_ID" --cmd "Tiller/Scripts/ci.sh")
tillerctl panel wait --id "$PANEL" --timeout-ms 900000 && echo BUILD_OK || echo BUILD_FAILED
tillerctl panel read --id "$PANEL" | tail -40
```

### checkpoint your progress

Update the worktree comment at meaningful state changes (repro, fix,
validation, blocker):

```bash
tillerctl worktree set --worktree "$TILLER_WORKTREE_ID" --comment "root cause found; writing failing test"
```

## notes

- `panel create` prints the panelId as plain text. All other commands print
  nothing on success and fail with a nonzero exit code and message on stderr.
- `panel create` opens a visible tab and activates the Tiller app — expect a
  focus change. Dispatch is not headless.
- `panel read` returns the scrollback snapshot as rendered — for TUI-heavy
  output prefer `tail`.
- `panel wait` maps the child's exit code onto tillerctl's own exit code; a
  timeout or unknown panel exits nonzero with an error message.
- There is no `panel list` or `panel close` yet: track the panelIds you
  create; panels close when their process exits and the user closes the tab.
- Your lifecycle hooks already report `running`/`needs-input`/`done`
  automatically; explicit `notify` is for statuses the hooks cannot see
  (e.g. flagging `error` on an unrecoverable failure).
"""#
}
