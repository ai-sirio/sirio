# Contributing to Sirio

Bug reports and pull requests are welcome. This page covers how the repository is laid out, how to build it, and the rules a change has to follow before it is merged. Issues live in [GitHub Issues](https://github.com/ai-sirio/sirio/issues).

## Before you start

- **Bug fixes and small improvements** — open a PR directly.
- **New features or significant changes** — open an issue first to discuss the approach. Design specs live in `docs/superpowers/specs/` and architecture decisions in `docs/adr/`; read the one that covers the behaviour you are changing, and write one for anything that changes how the pieces fit together.

## Building from source

### Requirements

- A current stable Rust toolchain (2024 edition).
- **Zig exactly 0.15.2** on `PATH`. `sirio_terminal` builds [libghostty-vt](https://github.com/ghostty-org/ghostty) through `zig build`, and upstream pins that version — a *newer* Zig fails too, so install 0.15.2 alongside and put it first on `PATH`. Optional: point `GHOSTTY_SOURCE_DIR` at a local ghostty checkout to skip the build script's network clone.
- **Windows:** the MSVC toolchain (see `docs/prototypes/ghostty-pane-windows.md`).
- **Linux:** GTK 3 and WebKitGTK 4.1 development packages, for the embedded browser surface.

### Build and run

```bash
git clone https://github.com/ai-sirio/sirio
cd sirio/rust
cargo build -p sirio                       # the app
cargo build -p sirio_control --bin sirioctl # the control-socket CLI used by agent hooks
cargo run -p sirio
```

`Scripts/build-dev.sh` wraps the build → kill running instance → relaunch loop. Release packaging (`Scripts/build-app-bundle.sh`, `Scripts/build-dmg.sh`, `Scripts/build-appimage.sh`, `Scripts/build-inno.sh`) reads its identity from `Scripts/identity.sh`.

### Verify

```bash
cd rust && cargo test -p <crate>           # iterate on one crate
cd rust && cargo test -p <crate> <test>    # run a single test
Scripts/ci.sh                              # whole-repo gate → prints "CI OK"
Scripts/ci-linux.sh                        # stricter tier: fmt, clippy, cross-target, headless smoke test
```

`Scripts/ci.sh` needs [cargo-nextest](https://nexte.st) on PATH (`cargo install cargo-nextest --locked`, or `brew install cargo-nextest`) and says so if it is missing. It runs the tests through nextest rather than `cargo test` because cargo runs the workspace's 45 test binaries one after another, and because a test that owns its own process cannot be tripped by a neighbour sharing one. `rust/.config/nextest.toml` holds the profiles and the test groups.

## Repository layout

The Rust workspace is 17 crates under `rust/crates/`. Leaves have no local dependencies; `sirio` (the app) is the only crate that depends on everything, and nothing underneath depends back up.

| Crate | Role |
|-------|------|
| `sirio` | The app: window shell, tabs/panes, control-socket dispatch, tray, command palette |
| `sirio_ui` | Reusable UI surfaces (sidebar, tab bar, chat, changes, editor, browser, settings), built on [bezel](https://github.com/crabtalk/bezel) |
| `sirio_terminal` | Terminal panes on `libghostty-vt`, PTY handling via `portable-pty`, splits |
| `sirio_activity` | Layered agent-activity detection (hooks / title / content / process), no GPUI dependency |
| `sirio_control` | `ControlServer` (unix socket) + `PaneRegistry` + the `sirioctl` CLI |
| `sirio_agents` | Adapters for the 5 supported agents, with lifecycle hooks |
| `sirio_acp` | Agent Client Protocol transport for chat-hosted agents |
| `sirio_registry` | ACP agent registry: which agents exist, which are installed, how to install one |
| `sirio_git` | Shell-out to git for local worktrees |
| `sirio_persistence` | SQLite (`rusqlite`) schema, migrations, records |
| `sirio_project` | Domain model of projects, worktrees and tabs, discovered from disk |
| `sirio_theme` | Theme tokens derived from `bezel::theme` (see `docs/THEME-PROVENANCE.md`) |
| `sirio_markdown` | Markdown parsing/rendering for the editor and chat |
| `sirio_usage` | Provider usage tracking (Claude / Codex / OpenCode / Ollama) |
| `sirio_release` | Signed-artifact format: channel manifest, accepted Ed25519 keys, verification |
| `sirio_update` | Discovers, downloads and verifies an update, then stops at `VerifiedUpdate` |
| `sirio_apply` | The only crate that touches the running installation |

`CLAUDE.md`'s Architecture section has the dependency graph and the design notes each subsystem relies on.

## Rules

1. **Tests first.** New logic needs `#[test]` coverage before implementation.
2. **Respect the crate boundaries.** Run `cargo build -p <crate>` to check a crate compiles in isolation before assuming a change is layered correctly.
3. **Keep pure logic pure.** State machines, parsers, and merge/filter logic that don't need a window belong in a crate with no `gpui` dependency (`sirio_activity` is the pattern) — that is what keeps them unit-testable without spinning up a window.
4. **UI comes from bezel.** Every primitive in `sirio_ui` is built from bezel before anything is hand-rolled; a bezel bump is a visual change to review, not a dependency chore.
5. **Commit messages** follow [Conventional Commits](https://www.conventionalcommits.org/): `feat:`, `fix:`, `refactor:`, `docs:`, `test:`, `chore:`, lower-case imperative subject.
6. **Verification gate.** `Scripts/ci.sh` must print `CI OK` before you open a PR.

## FAQ

**Why 5 agent adapters instead of one generic CLI wrapper?**
Each agent CLI has its own lifecycle quirks — startup banners, resume flags, exit signals, title conventions. A thin per-agent adapter behind a shared `AgentAdapter` trait keeps that contained instead of leaking into the terminal or sidebar code.

**Does closing the window stop my agents?**
No. Every agent session runs in its own PTY, and closing the window only flushes session state. The tray icon keeps tracking them and can bring you straight back. Only quitting the app ends everything.

**What's `sirioctl` for?**
It's the CLI side of Sirio's control socket — create a pane, write to it, read its output, wait for a state, or send a notification, all scriptable from outside the app. It's also how agent lifecycle hooks report status back to Sirio. Reference: [sirioai.app/docs/sirioctl/overview](https://sirioai.app/docs/sirioctl/overview).

**Where did the Swift app go?**
Sirio started as a native macOS app (SwiftUI/Swift 6). It was retired once this Rust/gpui port covered its inventory; its final commit is `5430d7bfdb4a295be8ce072526ae5108259b80f8`. Read any of its source with `git show 5430d7bfdb4a295be8ce072526ae5108259b80f8:<path>`.
