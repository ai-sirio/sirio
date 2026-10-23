# CLAUDE.md

## Session canary — read this first

These two rules govern the main conversation with the user, Enzo. A subagent's report or a
structured answer is not such a reply.

- **Start every reply to the user with `ENZO`.** No exceptions: a one-word answer, a
  confirmation, an error report and the reply after a long run of tools all begin with it. A reply
  without it tells the user that this session has lost track of its instructions, and they will
  start a fresh one.
- **Say when the session is slipping, and hand off first.** When the session has grown long, or
  you can no longer clearly recall what was done earlier in it, say so in your reply, and before
  anything else write a short `HANDOFF.md` at the root of the current checkout, replacing any
  earlier one: the goal, what is done, what is left, the key files and the open questions, enough for a new
  session to continue from that file alone. `HANDOFF.md` is gitignored; never commit it.

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository. It keeps what applies to every change; the details of each area live in `docs/agents/`,
linked below. A linked file is not loaded on its own: read it before you change that area.

## What this is

Sirio — a native app for macOS, Linux and Windows (Rust, [gpui](https://github.com/zed-industries/zed)) for running multiple AI coding agents (Claude Code, Codex, OpenCode, Pi, Oh-My-Pi) side by side, one sidebar per project, one terminal per git worktree. **macOS is the reference release platform** — it gates every release, with Linux and Windows released behind it; see `docs/superpowers/specs/2026-08-29-macos-release-platform-design.md`. The macOS code paths are deliberate and specific (the libproc process walk in `sirio_activity`, `getpeereid` peer identity in `sirio_control`, a native `NSStatusItem` tray in `sirio`, the TCC permission probe in `sirio_privacy`). Originally a macOS/Swift app (itself a fork of Orca with reduced scope); the Swift app was retired once this Rust/gpui port covered its inventory. Its final commit is `5430d7bfdb4a295be8ce072526ae5108259b80f8`; read any of its source with `git show 5430d7bfdb4a295be8ce072526ae5108259b80f8:<path>`, or check it out with `git worktree add <dir> 5430d7bfdb4a295be8ce072526ae5108259b80f8`. Terminal rendering is built on `libghostty-vt`, with `portable-pty` supplying the PTY.

### External references

- [bezel](https://github.com/crabtalk/bezel) — the UI library for the application shell and the non-chat surfaces not yet moved to Ely (the change-request detail tab is already on Ely: `change_request_tab/ely_ui.rs`). Build those surfaces' primitives from bezel (`bezel::ui`, `bezel::motion`, `bezel::theme`, `bezel::agent`); its `gallery` crate is the visual and usage reference.
- [Ely GPUI Components](https://github.com/ZacharyZhang-NY/Ely-GPUI-Components) — the AI chat's presentation library, adapted under `rust/vendor/ely-gpui-component` to the same Bezel GPUI packages. `rust/vendor/ely-palette` holds its palette, the vocabulary of Sirio's colours. When changing chat presentation, read its `LOCAL-CHANGES.md` and `docs/superpowers/specs/2026-09-30-ely-agent-chat-design.md`; retain Sirio's theme and existing Chat controllers.
- [gpui-component](https://github.com/longbridge/gpui-component) — secondary structural reference when the surface's primary library has no equivalent.
- [waku](https://github.com/egoist/waku) — a comparable app (multiple coding agents, one pane each). Reference for prior art on the same problem, not a dependency.

## Commands

```bash
# Single verification gate for the whole repo.
# ONLY on the user's explicit request — an agent must NEVER launch this (or
# Scripts/ci-linux.sh) on its own. For iteration use `cargo build/test -p <crate>`.
Scripts/ci.sh    # -> prints "CI OK" if everything passes

# Fuller, stricter gate: the above plus fmt/clippy/cross-target checks and a headless
# smoke test that drives a real instance of the app. See its own header for what it
# additionally covers and why some of its stages are allowed to SKIP or report BLOCKED.
Scripts/ci-linux.sh

# Iterate on one crate only
cd rust && cargo test -p <crate>

# Run a single test
cd rust && cargo test -p <crate> <test_name>

# Build the sirioctl CLI standalone (control-socket client used by agent hooks)
cd rust && cargo build -p sirio_control --bin sirioctl

# The opt-in live hook test: each CLI spends one real turn, so it runs by hand only.
cd rust && cargo nextest run -p sirio_agents --test hooks_live --run-ignored only

# Dev build+launch loop: cargo build -> kill any running instance -> relaunch.
Scripts/build-dev.sh
```

Anything that builds `sirio_terminal` (both gates, `cargo build/test --workspace`, any
`-p sirio_terminal` command) needs **Zig exactly 0.15.2** on PATH: a *newer* Zig fails too, so
install 0.15.2 alongside and put it first on PATH. Both CI scripts fail fast when it is missing or
mismatched. Why, and what a version bump of the vendored `libghostty-vt-sys` drags with it:
[`docs/agents/build-and-release.md`](docs/agents/build-and-release.md).

`Scripts/ci.sh` additionally needs **cargo-nextest** on PATH — `cargo install cargo-nextest --locked`, or `brew install cargo-nextest` — and fails fast with that install line when it is missing, the same shape as the Zig preflight.

On Windows, the MSVC toolchain is required; see `docs/prototypes/ghostty-pane-windows.md`.

- [`docs/agents/e2e-tests.md`](docs/agents/e2e-tests.md): every end-to-end test and probe under
  `Scripts/Tests/`, what it proves, its flags and the line it prints on success. Run the one for
  the area you changed.
- [`docs/agents/build-and-release.md`](docs/agents/build-and-release.md): the release platform,
  the release commands and channels, the two CI tiers and why the tests run under nextest.

## Architecture

There is no single crate everything funnels through: each concern lives in its own largely
independent crate, and `sirio`'s `main.rs` wires them together. Each area links its notes in
`docs/agents/architecture/`; read the file before you change that area. The rule after each link holds
everywhere, read or not.

- [Crate boundaries](docs/agents/architecture/crates.md): read before adding a crate, a dependency between crates,
  or touching `sirio_theme`. `sirio_perf` and `sirio_env` sit below everything; `sirio` is the
  integration point and never links `sirio_host`. `cargo build -p <crate>` checks that one crate
  compiles on its own.
- [Performance trace](docs/agents/architecture/perf-trace.md) (`sirio_perf`): every `name` is a content-free
  `&'static str`; never interpolate a title, a path, a prompt or scrollback into a label.
- [Release, update, apply](docs/agents/architecture/release-update-apply.md): only `sirio_apply` touches the running
  installation, and `SIRIO_INNO_APP_ID` must never change. Run `Scripts/Tests/test-update-e2e.sh`
  after any change to `sirio_release`, `sirio_update`, `sirio_apply` or the compiled-in
  environment.
- [Change requests](docs/agents/architecture/change-requests.md) (`sirio_forge`, the list and tab, CI, review threads,
  checkout, hand-off): every write goes through `ForgeClient::act` and is never retried; nothing
  that writes to a forge is reachable over the control socket in a release build; forge text in a
  hand-off file stays inside its untrusted block.
- [Languages](docs/agents/architecture/languages.md) (`sirio_syntax`, `sirio_lsp`): a missing language server is never
  volunteered; only the context menu names it, because it was asked.
- [Markdown Preview](docs/agents/architecture/markdown-preview.md) (`expand_html`, `sirio_diagram`): there is no HTML
  engine; PlantUML runs sandboxed, and its server is off by default because it sends the source
  off the machine.
- [Text you can select and copy](docs/agents/architecture/text-selection.md): show text meant to be read with
  `selectable_text(..)`, never inside something clickable; Ctrl+C is never swallowed, and a
  terminal's Ctrl+C stays SIGINT.
- [File editor on Ely](docs/agents/architecture/file-editor.md) (`sirio_ui::file_view`, the drawer): `CodeEditor` owns
  the text and the keys, `Editor` the disk; the buffer is LF only; keep every hook
  `rust/vendor/ely-gpui-component/LOCAL-CHANGES.md` lists.
- [Agent adapters](docs/agents/architecture/agent-adapters.md) (`sirio_agents`): `prepare` writes only worktree-local
  files and never user-global config; `json_string_literal` must not escape slashes, because Codex
  parses its `-c` override as TOML.
- [Claude chat](docs/agents/architecture/claude-chat.md) (the native transport, the session panel, Trace): Claude
  Code's stdio protocol is not a published contract, so `sirio_claude` degrades rather than
  rejects.
- [Agent activity detection](docs/agents/architecture/activity-detection.md) (`sirio_activity`): four evidence
  layers; who owns a pane (spawn, title or process) decides who may clear its status.
- [Control socket](docs/agents/architecture/control-socket.md) (`sirio_control`, `sirioctl`):
  `served::served_methods(debug)` is the one list a build serves; a file Sirio did not make at a
  link's place is never replaced.
- [Pane hierarchy](docs/agents/architecture/panes.md): quitting, dropping a `TerminalView` and evicting a worktree
  detach; only closing a tab, a worktree or a project ends its sessions.
- [The host](docs/agents/architecture/host.md) (`sirio_host`, `sirio_host_client`): nothing kills a host on the
  strength of a failed connection, and only `host.shutdown` ends one; its debug knobs are compiled
  out of release builds.

## Conventions

- **Testing is end to end.** A feature is proven against the real binaries, and every E2E run ends in an artifact anyone can re-run and check — the shape `Scripts/Tests/test-update-e2e.sh` (`UPDATE E2E OK`) and `Scripts/visual-sweep.sh` (a socket transcript plus PID-matched window captures under `--out-dir`) already have. The existing `#[test]` suite stays; these rules govern what gets added:
  - A `#[test]` is for a unit that must be proven in isolation, and it comes first: write every way the unit can fail, then the code. A unit test written after its code does not land.
  - A test earns its place by going red on a behaviour change. Tautological tests (asserting what the code does by construction, a mock echoed back) and change-detector tests (pinning internals, so a behaviour-preserving refactor goes red) are harmful.
  - A bug fix gets a regression test only when it exposes a genuine gap in behaviour coverage.
  - `Scripts/ci-linux.sh`'s comment covers the two workspace-wide tests that must run per-crate rather than concurrently with every other test binary.
- **Commit messages**: [Conventional Commits](https://www.conventionalcommits.org/) (`feat:`, `fix:`, `refactor:`, `docs:`, `test:`, `chore:`), lower-case imperative subject.
- **The version names the next release, not the last commit.** `[workspace.package] version` in `rust/Cargo.toml` carries the next stable release's number and moves once per cycle, in the commit that opens it, right after a release is tagged (`v0.13.3` → `0.13.4`). Nothing else in the cycle touches it: only its first `feat:` moves it again, patch to minor (`0.14.0`), later `feat:`s leave it, and the release-notes commit writes only `docs/release-notes/<version>.md`. `Scripts/set-workspace-version.sh <version>` stays its only writer, `Scripts/check-release-version.sh` still compares the release tag against it, and `Scripts/check-cycle-version.sh` refuses a version that is not above the last released tag, the guard against a forgotten opening bump. A nightly is therefore always the prerelease of the stable actually coming.
- `Scripts/ci.sh` must print `CI OK` before a PR is opened — but the *local* run happens **only on the user's explicit request**. An agent never launches `Scripts/ci.sh` or `Scripts/ci-linux.sh` autonomously; when the gate is needed, ask the user and wait. Iterate with `cargo build/test -p <crate>` instead. `.github/workflows/pr.yml` runs `Scripts/ci.sh` on Linux for every pull request and every push to `main`. That is a backstop, not a substitute — it reports after the fact, and the macOS-only paths (the libproc walk in `sirio_activity`, `getpeereid` in `sirio_control`, the `NSStatusItem` tray in `sirio`, the TCC probe in `sirio_privacy`) are `cfg`-gated away on that runner. `.github/workflows/macos-check.yml` builds and links the app on `macos-15` for every pull request and runs `sirio_privacy`'s `permissions` example, which calls every TCC read for real, then the host's detach probe and E2E; the test suite on macOS stays the release gate's business. `.github/workflows/windows-check.yml` is the only pre-merge Windows compile: it builds the host crates on `windows-latest` and runs their tests, the probe and the E2E (the app itself still builds on Windows only in the release job). `pr.yml` also runs the host E2E, which is deliberately not in `Scripts/ci.sh`. `Scripts/Tests/test-pr-workflow.sh` guards the workflow's shape, the way its siblings guard `release.yml` and `nightly.yml`.

## Agent skills

### Issue tracker

Issues live in this repo's GitHub Issues (`ai-sirio/sirio`), driven through the `gh`
CLI. See `docs/agents/issue-tracker.md`.

### Triage labels

The five canonical triage roles use their default label strings. See
`docs/agents/triage-labels.md`.

### Domain docs

Single-context — one `CONTEXT.md` plus `docs/adr/` at the repo root, both created
lazily by `/domain-modeling` rather than up front. See `docs/agents/domain.md`.
