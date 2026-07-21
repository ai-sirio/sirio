# Unified ACP Chat + Agent Registry — Design

Date: 2026-07-21
Status: Approved

## Goal

Merge Tiller's per-agent chats into agent-agnostic chats (JetBrains AI Assistant model): any chat can talk to any ACP-capable agent, the agent is picked — and switched — from a selector inside the chat itself. A new Settings section installs ACP agents from the official ACP registry.

## Background / current state

- Chat is already ACP-based, but each chat tab is bound to one agent at creation: `ChatController` takes an immutable `agentId`, and `AgentLaunchSpec.forAgent(id:)` hardcodes exactly two launchable agents (`claude` via the npx adapter, `opencode` via the user's binary). All other agents return `nil`.
- Sessions are persisted in `ChatSessionStore` (SQLite) keyed by `(worktreeId, agentId)`.
- Settings has General / Appearance / AI Providers tabs; no agent-install UI.

## Decisions made during brainstorming

1. **Context transfer on agent switch**: replay the persisted transcript into the new agent's first prompt (option A). No live "summary handoff" from the old agent (fails when the old process is dead — the common case). A warning is shown in chat that long conversations can exceed the new agent's context window.
2. **Agent list source**: the official ACP registry at `https://cdn.agentclientprotocol.com/registry/v1/latest/registry.json` (38 agents at time of writing) is the source of available agents — full list, not filtered. **omp** is not in the registry and remains a Tiller built-in entry (`omp acp`, native).
3. **Chat structure**: multiple chats per worktree remain allowed; none is bound to an agent at creation. Each chat has its own selector and the agent can be switched mid-conversation.
4. **In-chat selector contents**: installed agents only, plus an "Other agents…" item that opens the Settings section. (With 38 registry agents, listing everything in the selector is noise; discoverability lives in Settings.)
5. **Install semantics**: Tiller-local installs under `Application Support/Tiller/acp-agents/<id>/`, driven by the registry's `distribution` metadata. Never touches the user's global environment (no `npm install -g`).

## Architecture

All non-UI logic goes in `TillerACP` (per package-boundary rules); SwiftUI lives in `App/`.

### 1. Registry layer (`TillerACP`)

- `RegistryAgent`: `Decodable` model for a registry entry — `id`, `name`, `version`, `description`, `repository`, `website`, `authors`, `license`, `icon` (URL), `distribution`.
- `AgentDistribution`: enum-ish model of the `distribution` object:
  - `npx`: `package` (pinned `name@version`), optional `args`, optional `env`.
  - `binary`: per-platform map; only `darwin-aarch64` / `darwin-x86_64` are relevant. Each entry: `archive` URL + `cmd`.
  - `uvx`: parsed but **unsupported in v1** — the agent is shown in Settings as "not supported yet" and is not installable.
- `AgentRegistry` (actor): fetches the registry JSON, caches it on disk at `Application Support/Tiller/acp-registry.json`, TTL 24h, manual refresh from Settings. Offline → serve cached copy. No cache and fetch fails → Settings shows an error state with retry; chat selector still works from installed manifests.
- Built-in entries appended to the registry list:
  - **omp**: `id: "omp"`, launch `exec omp acp` through `zsh -lc` (user PATH resolution, same rationale as existing `AgentLaunchSpec`). "Install" for omp is a binary-presence check (`command -v omp`), with a short "install omp yourself" hint when missing.

### 2. Installer (`AgentInstaller`, actor in `TillerACP`)

- Install root: `Application Support/Tiller/acp-agents/<id>/`.
- **npx distribution**: `npm install --prefix <root>/<id> <package>@<version>` (npm resolved through `zsh -lc` for PATH). Launch command resolved from the resulting `node_modules/.bin/` entry; registry `args`/`env` applied at launch.
- **binary distribution**: pick the archive for the current platform (`darwin-aarch64` on Apple Silicon, `darwin-x86_64` on Intel), download, extract into the agent dir, `chmod +x`, launch the declared `cmd` relative to the extraction dir.
- After a successful install, write `<root>/<id>/installed.json` manifest: `{ id, version, launchExecutable, launchArguments, env }`. The manifest is the single source of truth for "installed" state and for launching.
- **Update**: registry `version` newer than manifest `version` → Settings shows "Update"; update = install new version, then atomically replace the manifest (install into a temp dir and swap, so a failed update never breaks the working install).
- Existing `AgentLaunchSpec.forAgent(id:)` hardcoding is replaced by resolution from installed manifests + built-ins. `launchEnvironment` (CLAUDECODE marker stripping) stays and applies to every launch.
- Errors (network, npm missing, archive corrupt, no matching platform) surface inline in the Settings row; installs are cancellable.

### 3. Unified chat

- `ChatController.agentId` becomes mutable selected-agent state. Chat tab creation no longer asks for an agent upfront: a new chat opens with the most recently used agent app-wide (falling back to the first installed one; if nothing is installed, the chat shows an empty state pointing to Settings).
- **Selector UI**: menu in the chat header listing installed agents (registry icon, cached on disk, monochrome-tinted to match the toolbar) + "Other agents…" → opens the Agents settings tab.
- **Switch flow** (mid-conversation):
  1. Tear down the current ACP session/process.
  2. Append a system-style transcript item: *"Agent changed: X → Y. The previous conversation will be resent to the new agent; long conversations may exceed its context window."* (UI strings in English per project rules.)
  3. Spawn the new agent, create a new ACP session.
  4. On the next user prompt, prepend the **handoff preamble** (below), then the user's message. Subsequent prompts carry no preamble.
- **Handoff serializer** (pure function, `TillerACP`): `[TranscriptItem]` → markdown block: `"Previous conversation with another agent (for context — continue from here):"` followed by user/agent messages verbatim and tool calls compressed to one line each (`- [tool] title → status`). Size cap ~100 KB: oldest items dropped first, with a leading `"(older messages truncated)"` note when trimmed.
- Switching to the same agent is a no-op. Switching while the agent is mid-turn cancels the turn first (existing cancel path), then switches.

### 4. Persistence (`ChatSessionStore`)

- Schema unchanged; **semantics** of `agentId` change from "owner agent" to "last used agent" — restored as the selector value when a chat is reopened.
- `latestSession(worktreeId:agentId:)` → `latestSession(worktreeId:)` (agent filter removed). Existing rows remain valid; no destructive migration.
- `acpSessionId` (used for same-agent resume) is cleared on agent switch — resume only ever targets the agent that created the ACP session.

### 5. Settings — "Agents" tab

- New Settings tab listing built-ins + registry agents with a search field (38+ rows).
- Row: icon, name, short description, registry version, and one state control: **Install** / **Installed vX** / **Update** / progress spinner (cancellable) / inline error with retry / **Not supported** (uvx) / **Not found** (omp missing binary, with hint).
- Header: registry refresh button + last-fetched timestamp; error banner with retry when the registry is unreachable and no cache exists.

### 6. Error handling

- Registry fetch failure → cached copy; none → explicit error state (never an empty silent list).
- Install/launch failures surface where they happen (Settings row / chat banner), with the underlying process output attached for diagnosis.
- Agent process dies mid-chat → existing chat error handling applies unchanged; switching agents is also the recovery path.

### 7. Testing (swift-testing, TDD)

- `RegistryAgent`/`AgentDistribution` decoding against a fixture of the real registry JSON (including `binary` per-platform maps, `env`, `uvx`).
- Platform resolution: aarch64 vs x86_64 selection, unsupported-platform error.
- `AgentInstaller`: manifest write/read round-trip, temp-dir-and-swap update atomicity, launch-command resolution — against a temp filesystem, no network in tests.
- Handoff serializer: formatting per item kind, tool-call compression, size cap and truncation note, empty transcript → no preamble.
- `ChatSessionStore`: `latestSession(worktreeId:)` without agent filter, `acpSessionId` cleared on switch, `agentId` round-trip as last-used agent.
- Selector state: installed-only filtering, empty-state when nothing installed.

## Out of scope (v1)

- `uvx` distributions (shown as unsupported).
- Background auto-update of agents.
- Uninstall (delete the agent dir manually; UI can come later).
- Summary-based context handoff (live summarization by the outgoing agent).
- Windows/Linux distribution entries (Tiller is macOS-only).
