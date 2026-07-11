# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

Tiller — a native macOS app (SwiftUI, Swift 6, macOS 15+) for running multiple AI coding agents (Claude Code, Codex, OpenCode, Pi, Oh-My-Pi) side by side, one sidebar per project, one terminal per git worktree. Fork of Orca with reduced scope. Terminal rendering is built on libghostty.

## Commands

```bash
# Regenerate Tiller.xcodeproj from project.yml (required after adding files/targets/settings)
xcodegen generate

# Open in Xcode and build/run with ⌘R (requires Xcode 16+)
open Tiller.xcodeproj

# Single verification gate for the whole repo — run before considering any task done
Scripts/ci.sh    # -> prints "CI OK" if everything passes

# Iterate on one package only
cd Packages/<Package> && swift test

# Run a single test
cd Packages/<Package> && swift test --filter <TestName>

# Build the tillerctl CLI standalone (control-socket client used by agent hooks)
swift build --package-path Packages/TillerControl --product tillerctl
```

`Tiller.xcodeproj`, `App/Info.plist`, `DerivedData/`, and `.build/` are all generated/gitignored — never hand-edit `Tiller.xcodeproj`; change `project.yml` and re-run `xcodegen generate`. `App/Info.plist` is generated inline by `project.yml`'s `info.properties`, not a checked-in file.

## Architecture

### Package boundaries (dependencies flow one way)

```
TillerPersistence (GRDB/SQLite, no local deps)
    ^
TillerCore (domain models, actors, pure state-machine logic)
    ^
    +-- TillerTerminal (PtyProcess, panes, recursive splits, libghostty)
    +-- TillerControl  (ControlServer over a unix socket + tillerctl CLI)
    +-- TillerAgents   (one adapter per supported agent CLI)

TillerGit (shell-out to git for worktrees — standalone leaf, no local deps)

App/ (thin SwiftUI app target — the only consumer that depends on everything above, plus MarkdownUI and Sparkle)
```

Nothing below `App/` depends back up. `TillerCore` never imports `TillerTerminal`/`TillerControl`/`TillerAgents` — those all depend on it, not the reverse. When adding logic, put anything that doesn't need SwiftUI/AppKit into the relevant package, not `App/`.

### Agent adapters (`TillerAgents`)

Every supported CLI implements `AgentAdapter` (`id`, `displayName`, `hasNativeHooks`, `prepare`, `command`, `resumeCommand`). `AgentCatalog.all` is the fixed list of 5 adapters. `prepare` writes only worktree-local hook config — **never** touches user-global config (`~/.claude/settings.json`, `~/.codex/config.toml`, etc.). Adapters that generate command-line overrides embedding JSON (Codex's `-c notify=[...]`, omp's hook file) share `jsonStringLiteral` in `ShellQuote.swift` — it must build a JSON string literal with `.withoutEscapingSlashes`, because Codex's `-c key=value` override is parsed as **TOML**, and `\/` (JSON's optional slash-escaping, which `JSONEncoder` applies by default) is not a valid TOML escape. Getting this wrong makes Codex fail silently at config load, before it ever reaches its TUI.

### Agent activity detection — layered evidence, not one signal (`TillerCore/AgentActivityModel.swift`)

Whether a pane shows as running/idle/needs-input is resolved from four independent evidence layers, weakest overridden by strongest as it arrives:

- **Layer A — `tillerctl notify` hooks.** Authoritative when present. Agents with `hasNativeHooks == true` call `tillerctl notify --session <paneId> --status <status>` themselves; a recent Layer-A push suppresses Layer B for a debounce window (`AgentSignalMerger.shouldApplyTitleSignal`).
- **Layer B — OSC terminal title.** `AgentTitleIdentity.identify(title:)` assigns an unregistered pane's agent identity from its title text; `AgentTitleStatus.detect` reads status from the same title using each CLI's own convention. Every CLI has its own title format, captured empirically, not guessed: Claude idles as `✳ …`, works as `. …` or a braille spinner; Pi titles `π - <cwd>`; its omp fork titles `π: <cwd>` (the colon is the only distinguishing mark); Codex 0.144+ also writes a braille "dots" spinner into its title while working — a bare spinner alone is therefore ambiguous between Claude and Codex and must **not** be used to assign identity (both are caught by Layer D instead).
- **Layer C — content signal.** `ScreenManifest` matches live pane scrollback content on output-settle; not debounced against Layer A, since a genuine content match is closer to ground truth than a stale title.
- **Layer D — foreground process.** `App/ForegroundProcessAgent.swift` walks the pane shell's direct child processes via libproc (`proc_listchildpids`/`proc_name`) and matches comm names against `AgentCatalog`. This is the only signal that catches agents with no usable title convention. Node/Bun-hosted CLIs (pi, omp) are invisible here and rely on Layer B instead.

Pane ownership determines who is allowed to clear a pane's status, and matters when changing this code: **spawn-owned** (Tiller launched it — cleared by watching process exit), **title-owned** (`titleOwnedPanes` — cleared only when the title stops matching that agent's conventions), **process-owned** (`processOwnedPanes`, Layer D — cleared only by `processGone`, never by an unrelated title change). Mixing these up reintroduces bugs where one layer's signal (or absence of one) wipes state that another layer is still relying on.

### Control socket (`TillerControl`)

`ControlServer` listens on a unix socket (`~/Library/Application Support/Tiller/control.sock`, overridable via `$TILLER_SOCKET`) and dispatches line-delimited JSON `ControlRequest`s (`panel.create`, `panel.write`, `panel.read`, `panel.wait`, `notify`, `session.ref`, `worktree.set`) — see `AppModel.handleControl`. This is both the `tillerctl` CLI's transport and how agent lifecycle hooks talk back to Tiller (Layer A above). `PaneRegistry` (an actor in `TillerTerminal`) is the shared, non-UI-thread source of truth for live panes that both the control handler and `ForegroundProcessAgent` read from. Beyond the original methods, cmux-parity groups exist: workspace.* (worktrees), surface.*/pane.surfaces (panes), notification.*, system.* (ping/capabilities/identify), session.restore — all dispatched in App/AppModel+Control.swift; the flat kebab tillerctl commands (list-workspaces, send, …) mirror cmux's CLI names. The socket can be disabled in Settings (controlSocket.enabled / TILLER_SOCKET_ENABLE), which also disables Layer-A hooks.

### Pane hierarchy

`AppModel` (`App/AppModel.swift`, `@MainActor @Observable`) holds `Project -> [Worktree]`, and each open `Worktree` owns its own tabs/panes. `openWorktreeIds` tracks which worktrees' terminal hosts stay mounted (PTYs alive) across sidebar selection changes — removing a worktree from that list unmounts its host, which fires `onDisappear` and terminates its panes. Closing the app window does **not** kill agent PTYs (macOS doesn't terminate an app on last-window-close); only quitting the app does.

## Conventions

- **Tests first**, using `swift-testing` (`@Test` / `#expect`), not XCTest.
- **Value types for models.** Domain types are structs; classes are reserved for real identity (windows, PTY processes) and must be actor-isolated or otherwise protected.
- **Commit messages**: [Conventional Commits](https://www.conventionalcommits.org/) (`feat:`, `fix:`, `refactor:`, `docs:`, `test:`, `chore:`), lower-case imperative subject.
- `Scripts/ci.sh` must print `CI OK` before a PR is opened.
