<p align="center">
  <img src="assets/tiller-logo.png" width="120" alt="Tiller logo" />
</p>

<h1 align="center">Tiller</h1>

<p align="center">
  <strong>Steer every coding agent from one native Mac window.</strong>
</p>

<p align="center">
  A lightweight native macOS app for running Claude Code, Codex, OpenCode, Pi and Oh-My-Pi<br/>
  side by side — one sidebar per project, one terminal per worktree, one glance at who needs you.
</p>

<p align="center">
  <img src="https://img.shields.io/badge/macOS-15.0%2B-blue?style=flat-square" alt="macOS 15.0+" />
  <img src="https://img.shields.io/badge/license-MIT-lightgrey?style=flat-square" alt="MIT license" />
  <img src="https://img.shields.io/badge/Swift-6.0-orange?style=flat-square" alt="Swift 6.0" />
  <a href="https://github.com/e-palmisano/tiller"><img src="https://img.shields.io/github/stars/e-palmisano/tiller?style=flat-square&logo=github&label=stars&color=4c71f2" alt="GitHub stars" /></a>
  <a href="https://www.linkedin.com/in/enzo-palmisano-b16363147/"><img src="https://img.shields.io/badge/LinkedIn-Enzo_Palmisano-0077B5?style=flat-square&logo=linkedin" alt="LinkedIn" /></a>
</p>

---

## Features

- 🗂️ **Sidebar of projects & worktrees** — local git worktrees, one row per branch, sorted by urgency
- 🖥️ **Native terminal** — built on [libghostty](https://github.com/ghostty-org/ghostty), tabs and recursive splits
- 📝 **Markdown editor** — click a `.md` link in the terminal (or drag & drop / ⌘O) to open it in a tab: rendered preview + code mode, live reload while agents write
- 🤖 **5 agent adapters** — Claude Code, Codex, OpenCode, Pi, Oh-My-Pi, each with lifecycle hooks
- 🔌 **Control socket** — `tillerctl` CLI for scripted create/write/read/wait/notify against any pane
- 🔔 **Menu bar roster** — a live pulse on every active agent; spins while working, rings when one needs you; click to jump straight back into the right worktree, even with the window closed
- 🔕 **Native notifications** — Touch-free heads-up when an agent finishes or stalls
- 💾 **Session persistence** — agent sessions survive an app restart (GRDB-backed)
- 📊 **Provider usage tracking** — Claude / Codex / OpenCode / Ollama usage at a glance
- 🔐 **macOS permissions page** — one-shot onboarding + Settings section to grant the TCC permissions agents inherit (notifications, screen recording, accessibility, full disk access, automation, local network)

**Deliberately not doing:** diff viewer, embedded browser, remote SSH / mobile relay, scheduling — Tiller stays a focused terminal + agent hub, not an IDE.

---

## Install

Tiller doesn't ship prebuilt binaries yet — build it from source (see below). It's a small SPM monorepo, first build takes a couple of minutes.

---

## First Launch

Tiller asks for **Notifications permission** on first launch, so it can alert you when an agent finishes or needs input. Everything else runs with no special entitlements — no accessibility hooks, no telemetry, no network calls beyond what your agents themselves make.

---

## Keyboard Shortcuts

| Action | Shortcut |
|--------|----------|
| New tab in selected worktree | `⌘T` |
| Close active tab | `⌘W` |
| Settings | `⌘,` |
| Open markdown file in selected worktree | `⌘O` |
| Save markdown file | `⌘S` |

The menu bar icon is always one click away — it reflects the worst status across every active agent and opens straight into a full roster.

---

## Repository Layout

| Path | Role |
|------|------|
| `App/` | Thin app target: bootstrap, windows, menus, SwiftUI views |
| `Packages/TillerCore/` | Domain models, actors, pure logic (state machine, merge, filtering) |
| `Packages/TillerTerminal/` | PtyProcess, terminal panes, recursive splits, scrollback, libghostty |
| `Packages/TillerControl/` | ControlServer (unix socket) + `tillerctl` CLI |
| `Packages/TillerAgents/` | Adapters for the 5 supported agents, with lifecycle hooks |
| `Packages/TillerGit/` | Shell-out to git for local worktrees |
| `Packages/TillerPersistence/` | GRDB/SQLite schema, migrations, records |

---

## Building from Source

```bash
brew install xcodegen
xcodegen generate
open Tiller.xcodeproj
```

Build and run with `⌘R`. Requires macOS 15+ and Xcode 16+ with Swift 6.0.

Single verification gate for the whole repo:

```bash
Scripts/ci.sh    # → "CI OK" if everything passes
```

To iterate on a single package:

```bash
cd Packages/<Package> && swift test
```

---

## FAQ

**Why 5 agent adapters instead of one generic CLI wrapper?**
Each agent CLI (Claude Code, Codex, OpenCode, Pi, Oh-My-Pi) has its own lifecycle quirks — startup banners, resume flags, exit signals. A thin per-agent adapter behind a shared protocol keeps that mess contained instead of leaking into the terminal or sidebar code.

**Does closing the window stop my agents?**
No. Every agent session runs in its own PTY that keeps running after the window closes — macOS doesn't terminate an app just because its last window closed. The menu bar icon keeps tracking them and can bring you straight back. Only quitting the app (`⌘Q`) ends everything.

**What's `tillerctl` for?**
It's the CLI side of Tiller's control socket — create a pane, write to it, read its output, wait for a state, or send a notification, all scriptable from outside the app. It's also how agent lifecycle hooks talk back to Tiller.

---

## Contributing

Bug reports and pull requests are welcome.

### Before you start

- **For bug fixes and small improvements** — open a PR directly.
- **For new features or significant changes** — open an issue first to discuss the approach.

### Rules

1. **Tests first.** New logic needs `swift-testing` (`@Test` / `#expect`) coverage before implementation.
   ```bash
   cd Packages/<Package> && swift test
   ```

2. **Respect the package boundaries.** Dependencies flow one way: `TillerAgents` / `TillerGit` / `TillerTerminal` → `TillerCore` → `TillerPersistence`. `App/` is the only consumer that depends on everything; nothing underneath depends back up.

3. **Value types for models.** Domain types are structs; classes are reserved for things with real identity (windows, PTY processes) and must be actor-isolated or otherwise protected.

4. **Commit messages.** Follow [Conventional Commits](https://www.conventionalcommits.org/): `feat:`, `fix:`, `refactor:`, `docs:`, `test:`, `chore:`. Lower-case, imperative subject.

5. **Verification gate.** `Scripts/ci.sh` must print `CI OK` before you open a PR.

---

## License

MIT — see [LICENSE](LICENSE) for details.

---

## Acknowledgements

Tiller is a fork of [Orca](https://github.com/stability-ai/orca) with a deliberately reduced scope, designed and built with the help of AI pair programmers:

- **[Claude Code](https://claude.ai/code)** by Anthropic — architecture, implementation, and review throughout the project.
- **[OpenCode](https://opencode.ai/)** — parallel subagent execution for isolated, independently-verified feature branches.

> *A fork with its own name and its own terms.*
