# Chat Interface via ACP — Design

Date: 2026-07-18
Status: approved (brainstorming session)

## Overview

Add a chat pane type to Tiller: a structured conversation UI that talks to coding
agents over the Agent Client Protocol (ACP, JSON-RPC 2.0 over stdio) instead of a
PTY. The agent declares its activity (messages, tool calls, plans, permission
requests) rather than Tiller inferring it from terminal evidence.

### Goals (v1)

- Generic ACP client proven against two independent implementations:
  - **Claude Code** via `npx -y @zed-industries/claude-code-acp@<pinned>`
  - **OpenCode** via its native ACP subcommand (user-installed binary)
- Chat panes participate in the existing worktree tab/split infrastructure,
  side by side with terminal panes.
- Inline permission flow (user approves/rejects edits and tool use).
- Tool call, plan, and subagent visibility in the transcript.
- Transcript persistence (GRDB) + real resume via `session/load` where supported.
- Composer with @-file mentions, slash commands, image paste, mode selector.

### Non-goals (v1, explicitly deferred)

- Codex / Pi / omp integrations (become config once the client is generic;
  omp inherits Pi's ACP — no separate adapter).
- OpenCode's proprietary HTTP/SSE server (possible richer upgrade later; v1 uses
  its native ACP for uniformity).
- Client-side auto-approve policy engine (ACP's `allow_always` options cover most
  of the value; adapter remembers).
- Session history browser (schema already supports it).
- ACP `terminal` capability (agent commands run in the adapter's internal runner).
- OAuth/auth flows inside Tiller (assume CLIs already authenticated).

## Architecture

```
┌─ App/Chat/ ──────────────────────────────┐
│ ChatPaneView, TranscriptView,            │
│ MessageComposer, PermissionCard,         │
│ ToolCallCard, ChatViewModel (@Observable)│
└──────────────┬───────────────────────────┘
               │
┌─ Packages/TillerACP ─────────────────────┐
│ ACPClient      JSON-RPC 2.0 over stdio   │
│ ACPTransport   child process (Process,   │
│                stdin/stdout pipes)       │
│ ACPSession     actor: initialize →       │
│                session/new|load → prompt │
│ TranscriptReducer  update stream → state │
│ AgentLaunchSpec    per-agent command     │
└──────┬───────────────────────┬───────────┘
       │                       │
  TillerCore            TillerPersistence
```

- New package `TillerACP`, depends only on `TillerCore` (and Foundation).
  Never imports TillerTerminal/TillerControl/TillerAgents — dependency direction
  preserved.
- `TranscriptReducer` is pure logic: reduces the `session/update` stream into
  `[TranscriptItem]` value types. Fully unit-testable with no UI and a mock
  transport.
- `TillerAgents.AgentCatalog` gains a `supportsACP` flag + launch command per
  agent; adding future agents is data, not code. `App/` wires catalog → launch
  spec (packages stay decoupled).
- One chat pane = one child process = one `ACPSession`. Pane close kills the
  process (same lifecycle as PTYs; confirm if a turn is active).
- DB writes happen async after items finalize — never in the render path.

## Protocol & Sessions

**Handshake** (`initialize`):
- `protocolVersion: 1`; client capabilities `fs.readTextFile: true`,
  `fs.writeTextFile: true`, `terminal: false`.
- `fs: true` is the linchpin of the permission flow: the adapter delegates file
  reads/writes to Tiller (`fs/read_text_file`, `fs/write_text_file`), so every
  edit passes through us with its diff — no sniffing.
- Auth: if the agent reports auth required, show an in-chat message directing the
  user to authenticate in a terminal (e.g. `claude /login`) with an
  open-terminal button. No auth flows in Tiller.

**Session lifecycle**:
- `session/new` with `cwd` = worktree path, `mcpServers: []`.
- **Resume**: if `agentCapabilities.loadSession` and a stored `acpSessionId`
  exists → show persisted transcript immediately (instant), then `session/load`;
  the agent's replay rebuilds live state and becomes source of truth. Load
  missing/failed → persisted transcript stays read-only above a fresh session,
  with a "new session" banner.
- **Updates handled**: `agent_message_chunk`, `agent_thought_chunk`,
  `tool_call` / `tool_call_update`, `plan`, `available_commands_update`,
  `current_mode_update`.
- **Modes**: composer mode selector driven by the agent's advertised modes
  (`session/set_mode`), never hardcoded.
- **Stop**: stop button → `session/cancel` → turn ends with
  `stopReason: cancelled`. Normal completion: `end_turn`.

**Process failure**: child crash/exit → session marked disconnected, transcript
intact, "Restart agent" button (new process, retry `session/load`). Adapter
stderr goes to logs, never into the transcript.

**Sidebar status**: ACP states map directly onto the existing activity model —
prompt in flight = running, pending `request_permission` = needs-input,
`end_turn` = idle. Acts as a new authoritative layer (like Layer A), reusing
existing badges and notifications.

## Persistence (TillerPersistence)

```
chat_session
  id UUID PK, worktreeId, agentId,
  acpSessionId TEXT?,          -- for session/load
  createdAt, lastActivityAt

chat_item
  id UUID PK, sessionId FK (cascade delete),
  ordinal INT,                 -- transcript order
  kind TEXT,                   -- userMessage|agentMessage|thought|toolCall|plan|permission
  payload BLOB (JSON),         -- Codable TranscriptItem
  createdAt
```

- **JSON payload, not typed columns**: `TranscriptItem` is an enum with
  heterogeneous payloads and the protocol evolves; queries only need
  session + ordinal.
- **Write finalized items only**: streaming chunks accumulate in memory; write
  when an item completes (message done, tool call terminal). Final flush on pane
  close/quit. No per-chunk writes.
- **Binding**: one active session per (worktree, agent). Reopening a chat pane
  for the same pair resumes the latest session. Explicit "New conversation"
  creates a new `chat_session` row; old rows remain (history browser = v2).
- **Cleanup**: worktree removal cascades sessions + items (same hook as existing
  worktree-state cleanup).

## Permissions & Tool Calls

**Tool call card** (one per `tool_call`):
- Icon per `kind` (read/edit/delete/search/execute/fetch/think/other), agent
  title, spinner → check/✗ status.
- Collapsible content: diff for edits, output for execute, content blocks
  otherwise.
- `locations` (path + line) clickable → open file in the existing RightPanel.

**Permission** (`session/request_permission`):
- Rendered **inline in the transcript**: the associated tool call card expands
  into request state (amber border) — diff/command prominent, buttons generated
  from the adapter-provided `options` (Allow once / Allow always / Reject …).
  Nothing hardcoded.
- Pane goes needs-input (existing sidebar badge + notification). Response sends
  the selected `optionId`. If the turn is cancelled while pending → `cancelled`
  outcome automatically.
- Composer disabled while a request is pending. ⏎ = primary option, ⎋ = reject.

**Full edit flow**: adapter requests permission with diff → allow → adapter
sends `fs/write_text_file` → Tiller writes to disk → existing RightPanel git
status/diff updates on its own. Files touch disk **only after** allow — a real
gate, not cosmetic. `fs/read_text_file` is silent (visible tool call, no gate).

**Subagents**: reported by adapters as regular tool calls (e.g. claude-code-acp's
Task tool) → card with "subagent" badge + name, final report as content. Inner
tool calls are not exposed by the protocol — honest v1 limit.

**Plan** (`plan` update): agent todo-list card pinned at the top of the current
turn, updated in place, per-entry status.

## UI

Chosen direction: **rich cards** (Zed/Cursor-like) — each tool call / plan /
subagent / permission is a distinct card in the transcript; user messages as
right-aligned bubbles; agent prose as markdown (reuse MarkdownUI). Thought
chunks rendered collapsed/dimmed.

**Header**: agent icon + name (reuse `AgentIcon`), connection/activity chip,
"+ New conversation" button.

**Composer**:
- Multiline field: ⏎ send, ⇧⏎ newline.
- `@` → autocomplete over worktree files (mention becomes a chip; sent as ACP
  `resource_link` / embedded context).
- `/` → slash command list from `available_commands_update`.
- Image paste → removable chip → multimodal prompt block (respect negotiated
  `promptCapabilities`).
- Mode selector (plan/default/acceptEdits/… from agent).
- **During a turn**: composer stays active and queues — sending while a turn is
  in flight shows the message as queued in the transcript and dispatches it when
  the turn ends. Exception: while a permission request is pending the composer is
  disabled (see Permissions). Stop button → `session/cancel`.

**Entry points**: the existing new-tab / split menus gain a "Chat" section —
one entry per `supportsACP` agent (v1: Claude Code, OpenCode). Chat panes join
recursive splits/tabs exactly like terminals.

## Testing (swift-testing, TDD)

- `TranscriptReducer`: core of the suite — synthetic `session/update` sequences
  → expected transcript (streaming chunks, tool call lifecycle, permission
  transitions, plan updates, `session/load` replay).
- `ACPClient`: in-memory mock transport — handshake, concurrent requests,
  server-initiated requests (fs/*, permission), JSON-RPC errors, EOF/crash.
- `AgentLaunchSpec`: generated commands (pinned npx, opencode).
- Persistence: GRDB transcript round-trip.
- No end-to-end tests against real adapters in CI (network/npx); manual smoke
  checklist instead.

## Edge Cases

- npx missing/failing → in-chat error message with instructions (install Node),
  never a crash.
- `fs/write_text_file` outside the worktree → rejected client-side (guardrail).
- App quit with active turn → best-effort `session/cancel` + transcript flush.
- Incompatible protocol version → readable error banner.

## Future Work

- Codex, Pi (+omp inherited) integrations as catalog entries.
- Session history browser over existing schema.
- Client-side auto-approve policies per worktree.
- ACP `terminal` capability → agent commands in Tiller PTYs.
- OpenCode HTTP/SSE client if richer events become necessary.
