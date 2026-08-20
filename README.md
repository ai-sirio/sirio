<p align="center">
  <img src="assets/tiller-logo.png" width="120" alt="Tiller logo" />
</p>

<h1 align="center">Tiller</h1>

<p align="center">
  <strong>Steer every coding agent from one native Linux window.</strong>
</p>

<p align="center">
  A lightweight native Linux app for running Claude Code, Codex, OpenCode, Pi and Oh-My-Pi<br/>
  side by side — one sidebar per project, one terminal per worktree, one glance at who needs you.
</p>

<p align="center">
  <img src="https://img.shields.io/badge/platform-Linux-blue?style=flat-square" alt="Linux" />
  <img src="https://img.shields.io/badge/license-MIT-lightgrey?style=flat-square" alt="MIT license" />
  <img src="https://img.shields.io/badge/Rust-2024%20edition-orange?style=flat-square" alt="Rust 2024 edition" />
  <a href="https://github.com/e-palmisano/tiller"><img src="https://img.shields.io/github/stars/e-palmisano/tiller?style=flat-square&logo=github&label=stars&color=4c71f2" alt="GitHub stars" /></a>
  <a href="https://www.linkedin.com/in/enzo-palmisano-b16363147/"><img src="https://img.shields.io/badge/LinkedIn-Enzo_Palmisano-0077B5?style=flat-square&logo=linkedin" alt="LinkedIn" /></a>
</p>

---

> Tiller started as a native macOS app (SwiftUI/Swift 6) and has been rewritten in Rust on
> [gpui](https://github.com/zed-industries/zed) for Linux. The Swift original is retired —
> see [`docs/linux-rewrite/README.md`](docs/linux-rewrite/README.md) for the exact commit and
> how to read its source from git history.

## Features

- 🗂️ **Sidebar of projects & worktrees** — local git worktrees, one row per branch, sorted by urgency
- 🖥️ **Native terminal** — built on [alacritty_terminal](https://github.com/alacritty/alacritty), tabs and recursive splits
- 📝 **Markdown editor** — click a `.md` link in the terminal (or drag & drop / `Ctrl+O`) to open it in a tab: rendered preview + code mode, live reload while agents write
- 🌐 **Embedded browser tab** — a native WebKitGTK surface composited alongside the terminal, for previewing a running dev server without leaving the window
- 📋 **Diff/changes viewer** — a git-status-aware Changes surface: stage, unstage, discard, and open a path-specific diff tab
- 🤖 **5 agent adapters** — Claude Code, Codex, OpenCode, Pi, Oh-My-Pi, each with lifecycle hooks
- 🔌 **Control socket** — `tillerctl` CLI for scripted create/write/read/wait/notify against any pane
- 🔔 **Tray roster** — a `StatusNotifierItem` tray icon with a live pulse on every active agent; click to jump straight back into the right worktree, even with the window closed
- 🔕 **Desktop notifications** — a heads-up when an agent finishes or stalls
- 💾 **Session persistence** — agent sessions survive an app restart (SQLite-backed)
- 📊 **Provider usage tracking** — Claude / Codex / OpenCode / Ollama usage at a glance

**Deliberately not doing:** remote SSH / mobile relay, scheduling — Tiller stays a focused terminal + agent hub, not an IDE.

---

## Supported Agents

<p>
  <a href="https://docs.anthropic.com/claude/docs/claude-code"><kbd><img src="https://www.google.com/s2/favicons?domain=docs.anthropic.com&sz=64" alt="Claude Code logo" width="16" valign="middle" /> Claude Code</kbd></a>&nbsp;
  <a href="https://github.com/openai/codex"><kbd><img src="https://www.google.com/s2/favicons?domain=openai.com&sz=64" alt="Codex logo" width="16" valign="middle" /> Codex</kbd></a>&nbsp;
  <a href="https://opencode.ai/docs/cli/"><kbd><img src="https://www.google.com/s2/favicons?domain=opencode.ai&sz=64" alt="OpenCode logo" width="16" valign="middle" /> OpenCode</kbd></a>&nbsp;
  <a href="https://pi.dev"><kbd><img src="https://www.google.com/s2/favicons?domain=pi.dev&sz=64" alt="Pi logo" width="16" valign="middle" /> Pi</kbd></a>&nbsp;
  <a href="https://omp.sh"><kbd><img src="https://www.google.com/s2/favicons?domain=omp.sh&sz=64" alt="Oh-My-Pi logo" width="16" valign="middle" /> Oh-My-Pi</kbd></a>
</p>

---

## Agent Orchestration

Tiller automatically provisions [`skills/tiller/SKILL.md`](skills/tiller/SKILL.md) inside every launched worktree for all five Tiller harnesses: Claude Code, Codex, OpenCode, Pi, and Oh-My-Pi. No manual install is needed in those worktrees.

For supported Skills CLI agents outside a launched Tiller worktree, install the public package with:

```bash
npx skills add e-palmisano/tiller --skill tiller -a claude-code,codex,opencode,pi -y
```

`tillerctl panel` returns panel UUIDs. Capture them and address every operation explicitly with `--id`; use `--from` only to identify the UUID of the panel being split.

```bash
WORKER=$(tillerctl panel create --cmd 'claude')
PEER=$(tillerctl panel split right --from "$TILLER_PANE_ID" --cmd 'codex')

tillerctl panel write --id "$WORKER" --input 'Implement the parser change' --enter
tillerctl panel wait --id "$WORKER"
tillerctl panel read --id "$WORKER"
tillerctl panel close --id "$WORKER"
```

The eleven panel subcommands are:

- `create` — create a panel and return its UUID.
- `split` — split an existing panel identified by `--from` and return the new UUID.
- `list` — list panel UUIDs and their state.
- `write` — write text to a panel UUID.
- `key` — send a key to a panel UUID.
- `read` — read a panel UUID's output.
- `state` — report a panel UUID's current state.
- `scrollback` — read a panel UUID's scrollback buffer.
- `wait` — wait for a panel UUID to exit.
- `focus` — focus a panel UUID.
- `close` — close a panel UUID.

---

## Install

Tiller doesn't ship prebuilt binaries yet — build it from source (see below). It's a Cargo workspace of 13 crates; first build takes a few minutes.

---

## First Launch

Tiller needs no special permission prompts on Linux — it runs with no elevated access, no telemetry, and no network calls beyond what your agents themselves make. Desktop notifications go through the freedesktop D-Bus notification service, and the tray roster needs a `StatusNotifierItem`-capable panel (KDE, GNOME/COSMIC and most other desktops via their SNI/AppIndicator bridge).

---

## Keyboard Shortcuts

| Action | Shortcut |
|--------|----------|
| New terminal tab | `Ctrl+T` |
| Close active tab | `Ctrl+W` |
| Open file | `Ctrl+O` |
| Save file | `Ctrl+S` |
| Toggle sidebar | `Ctrl+Shift+S` |
| Toggle right panel | `Ctrl+Shift+I` |
| New browser tab | `Ctrl+Shift+L` |
| Focus browser address bar | `Ctrl+L` |
| Restore previous launch | `Ctrl+Shift+O` |
| Settings | `Ctrl+,` |

The tray icon is always one click away — it reflects the worst status across every active agent and opens straight into a full roster.

---

## Repository Layout

| Path | Role |
|------|------|
| `rust/crates/tiller/` | The app: window shell, tabs/panes, control-socket dispatch, tray, command palette |
| `rust/crates/tiller_ui/` | Reusable UI surfaces (sidebar, tab bar, chat, changes, editor, browser, settings) |
| `rust/crates/tiller_terminal/` | Terminal panes backed by `alacritty_terminal`, PTY handling, splits |
| `rust/crates/tiller_activity/` | Layered agent-activity detection (hooks / title / content / process), no GPUI dependency |
| `rust/crates/tiller_control/` | `ControlServer` (unix socket) + `PaneRegistry` + `tillerctl` CLI |
| `rust/crates/tiller_agents/` | Adapters for the 5 supported agents, with lifecycle hooks |
| `rust/crates/tiller_acp/` | Agent Client Protocol transport for chat-hosted agents |
| `rust/crates/tiller_git/` | Shell-out to git for local worktrees |
| `rust/crates/tiller_persistence/` | SQLite (`rusqlite`) schema, migrations, records |
| `rust/crates/tiller_project/` | Workspace/project domain logic, update-check state machine |
| `rust/crates/tiller_theme/` | Color palette and theme tokens |
| `rust/crates/tiller_markdown/` | Markdown parsing/rendering for the editor and chat |
| `rust/crates/tiller_usage/` | Provider usage-tracking (Claude/Codex/OpenCode/Ollama) |

See `CLAUDE.md`'s Architecture section for the dependency graph between them.

---

## Building from Source

```bash
cd rust
cargo build --workspace
cargo run -p tiller
```

Requires a Rust toolchain (2024 edition) and, on Linux, GTK/WebKit development headers for the embedded browser surface (`gtk`, `webkit2gtk` — package names vary by distro).

Single verification gate for the whole repo:

```bash
Scripts/ci.sh    # → "CI OK" if everything passes
```

To iterate on a single crate:

```bash
cd rust && cargo test -p <crate>
```

---

## FAQ

**Why 5 agent adapters instead of one generic CLI wrapper?**
Each agent CLI (Claude Code, Codex, OpenCode, Pi, Oh-My-Pi) has its own lifecycle quirks — startup banners, resume flags, exit signals. A thin per-agent adapter behind a shared trait keeps that mess contained instead of leaking into the terminal or sidebar code.

**Does closing the window stop my agents?**
No. Every agent session runs in its own PTY, and closing the window only flushes session state — it does not tear panes down. The tray icon keeps tracking them and can bring you straight back. Only quitting the app ends everything.

**What's `tillerctl` for?**
It's the CLI side of Tiller's control socket — create a pane, write to it, read its output, wait for a state, or send a notification, all scriptable from outside the app. It's also how agent lifecycle hooks talk back to Tiller.

---

## Contributing

Bug reports and pull requests are welcome.

### Before you start

- **For bug fixes and small improvements** — open a PR directly.
- **For new features or significant changes** — open an issue first to discuss the approach.

### Rules

1. **Tests first.** New logic needs `#[test]` coverage before implementation.
   ```bash
   cd rust && cargo test -p <crate>
   ```

2. **Respect the crate boundaries.** See `CLAUDE.md`'s Architecture section for the current dependency graph between `rust/crates/*`. `tiller` (the app) is the only crate that depends on everything; nothing underneath depends back up.

3. **Keep pure logic pure.** State machines, parsers, and merge/filter logic that don't need a window belong in a crate with no `gpui` dependency (see `tiller_activity` for the pattern) — that is what keeps them unit-testable without spinning up a window.

4. **Commit messages.** Follow [Conventional Commits](https://www.conventionalcommits.org/): `feat:`, `fix:`, `refactor:`, `docs:`, `test:`, `chore:`. Lower-case, imperative subject.

5. **Verification gate.** `Scripts/ci.sh` must print `CI OK` before you open a PR.

---

## License

MIT — see [LICENSE](LICENSE) for details.

---

## Acknowledgements

Tiller is a fork of [Orca](https://github.com/stability-ai/orca) with a deliberately reduced scope, designed and built with the help of AI pair programmers:

- **[Claude Code](https://claude.ai/code)** by Anthropic — architecture, implementation, and review throughout the project, including the Rust/gpui Linux port.
- **[OpenCode](https://opencode.ai/)** — parallel subagent execution for isolated, independently-verified feature branches.

> *A fork with its own name and its own terms.*
