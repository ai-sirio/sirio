# P87 — the hook that never fires

**Owner: `codex12`** (`main.rs` is yours). Worktree
`/home/enzopalmisano/Scrivania/Progetti/tiller-linux`, branch `linux/gpui-waku`.

**Do not start this until P85 (the gate) is finished or reported blocked.** It is queued behind it
deliberately.

## The finding

Layer A — the `tillerctl notify` hook push — is the **authoritative** agent-activity signal. Every
weaker layer (OSC title, content match, foreground process) exists to cover for its absence. On this
machine it has never fired once.

Pass 17 measured the symptom live and it is already in the ledger under `F-CTRL-CLI-02`: every hook
of a Tiller-launched Claude Code fails with `/bin/sh: 1: tillerctl: not found` — SessionStart,
UserPromptSubmit and all three Stop hooks, reproduced across three independent launches (frames
`p17-aq1`, `aq2`, `as2`, `as4`).

The cause is four characters of string literal:

```
main.rs:4706   adapter.prepare(&worktree_path, &pane_id,   "tillerctl")
main.rs:4713   adapter.command(&worktree_path, &pane_id,   "tillerctl")
main.rs:5035   adapter.prepare(&worktree_path, &pane_name, "tillerctl")
main.rs:5046   adapter.command(&worktree_path, &pane_name, "tillerctl")
```

`tillerctl` is not on `PATH` on this machine — `which tillerctl` finds nothing, and the binary exists
only at `rust/target/debug/tillerctl`. So the generated hook config contains a bare command that
cannot resolve, and every hook dies in the shell before it reaches the socket.

## The correction worth reading before you plan

Pass 17 concluded Layer A was *"irrecuperabile in-product"*. **That conclusion is wrong**, and it is
worth knowing why so you do not inherit it:

- `AgentAdapter::prepare` / `command` / `resumeCommand` already take a **`tillerctl_path: &str`**
  parameter (`tiller_agents/src/lib.rs:141,146,155`). The seam exists; it is being handed a
  placeholder.
- `shell_quote.rs` already quotes that path correctly, with tests covering paths containing spaces,
  and `json_string_literal` handles the Codex `-c notify=[...]` TOML case.

So the adapters are right and only the caller is wrong. This is a small change with a large blast
radius: it revives the layer the whole `F-CORE-ACT` cluster leans on.

## What to build

`F-CTRL-CLI-02` describes the macOS design: install a `tillerctl` symlink into an app-owned bin
directory, **and use that path for generated agent hooks**. Port that, don't invent something new:

1. **Resolve the real binary.** `session.rs:154` already uses `std::env::current_exe()` to locate
   app files relative to the running binary — reuse that idiom to find `tillerctl` as a sibling of
   the running executable, and fall back to a `PATH` lookup for an installed build.
2. **Install it where the app owns the directory.** The row's `PLATFORM` note asks for "an XDG
   data/bin or PATH installation policy" on Linux; pick one XDG-correct app-owned location and say
   which in your report.
3. **Pass the resolved absolute path** at all four call sites above.
4. **Surface a failure instead of swallowing it.** If `tillerctl` cannot be resolved, `set_notice`
   it — this is exactly the case the P71 notices seam exists for, and that seam is yours. A silent
   fallback to a bare `tillerctl` reproduces today's bug while looking fixed.

**Do not** write outside the app's own directories: no `sudo`, no `/usr/local/bin`, no editing the
user's shell profile or `PATH`, and **never** touch user-global agent config (`~/.claude/settings.json`,
`~/.codex/config.toml`) — hook config stays worktree-local, which is a standing rule in `CLAUDE.md`.

## Done means

1. The four call sites pass a resolved absolute path, and the install location is named.
2. **Live**: launch a Claude Code agent from Tiller into a pane, drive it until a hook fires, and
   show the hook **succeeding** — no `not found` in the log. A green unit test does not close this
   row; the failure is a shell-resolution failure that only appears in a real launch.
3. **Then show the consequence**: with Layer A alive, the pane's status should move on the hook push
   rather than on a title guess. Capture that.
4. Report what `F-CTRL-CLI-02` should now read, but **do not edit the ledger** — only a critic moves
   a verdict.
5. `cargo fmt`, suite green on what you touched, and `git status --short | grep '??'` before you
   finish.
