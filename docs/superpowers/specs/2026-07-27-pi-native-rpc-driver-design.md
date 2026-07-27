# Pi native chat driver via `pi --mode rpc`

**Date:** 2026-07-27
**Status:** Approved (brainstorming session)
**Companion spec:** `2026-07-22-native-chat-transports-design.md` (this adds the 4th native driver, for the harness that spec left on ACP/terminal)

## Goal

Give Pi a native SwiftUI chat pane like Claude Code, Codex and OpenCode already have.
Today Pi only runs as a terminal-pane agent (`PiAdapter` → `pi` in a PTY, activity
detected via OSC title); it has no `AgentDriver`, so it cannot appear in "New Chat".

The Pi coding agent's embedding SDK (`createAgentSession`) is TypeScript/Node-only.
Its own documentation recommends **RPC mode** ("integrating from another language,
process isolation, language-agnostic client") for non-Node hosts — so Tiller embeds
Pi natively by driving `pi --mode rpc` (JSON-RPC over stdio) from a new Swift driver,
exactly the pattern established by the other three native drivers.

## Decisions (from brainstorming)

1. **`pi --mode rpc` subprocess, no JS runtime, no TS SDK sidecar.** The wire
   protocol is official and versioned with the CLI; a Bun/Node sidecar would add a
   second codebase, a build step and a distribution problem for zero user-visible
   gain. A compiled-SDK sidecar remains a future option for features RPC cannot
   reach (in-process custom tools); not needed for anything in this spec.
2. **Reuse the canonical pipeline.** The driver maps Pi's RPC events into the
   existing `ACPSessionEvent`/`SessionUpdate` stream. Timeline, transcript, tool
   cards and model picker stay unchanged. The only canonical/UI extension is a
   small free-text answer mode in `ChatQuestion` + `QuestionCardView`, required by
   Pi's `extension_ui_request(method: "input")`.
3. **Questions come from Pi's Extension UI sub-protocol** (`select` / `confirm` /
   `input` extension_ui_requests, e.g. from the `ask_user_question` tool) and are
   rendered with the existing question/permission cards. Pi intentionally has **no
   built-in permission popups** (its docs say so); tools execute directly, so there
   are no Claude-style Approve/Deny tool cards. Fire-and-forget UI methods
   (`notify`, `setStatus`, `setTitle`, `setWidget`, `set_editor_text`) are ignored
   in v1.
4. **Launch with `--approve`.** In RPC mode Pi shows no trust prompt; without a
   saved trust decision it *ignores* project resources (project AGENTS.md, `.pi/`
   skills/extensions/settings). Tiller worktrees are user-created and already
   trusted in the terminal flow, so the driver passes `--approve` to get parity
   with the terminal experience.
5. **Thinking level rides the existing "effort" slot** (`SessionConfigOption(id:
   "effort")`, same as Claude) — no new UI.
6. **Sessions persist by default; resume via stored session ref.** Transcript
   replay on resume comes from Tiller's own DB (`ChatSessionStore`), same as every
   other driver — Pi's `.jsonl` files are never parsed by Tiller.
7. **Out of scope (YAGNI):** session fork/clone/tree navigation, `export_html`,
   the client-initiated `bash` RPC command, MCP server forwarding (Pi reads its own
   `settings.json`), multiline `extension_ui_request(method: "editor")`, subagent
   cards (not built into Pi), `set_session_name`, queue-mode commands
   (`set_steering_mode` / `set_follow_up_mode`).

## Architecture

```
ChatController ──► AgentDriverFactory.makeDriver(agentId: "pi", …)
                        │
                   PiRPCDriver (actor, conforms to AgentDriver)
                        │  owns a ProcessTransport (already splits on \n only —
                        │  matches RPC strict-LF framing)
                        ▼
              pi --mode rpc --approve [--session <ref>]
              cwd = worktreePath
```

**New files** (`Packages/TillerACP/Sources/TillerACP/Drivers/`):

- `PiRPCDriver.swift` — the `AgentDriver` actor: `events`, `start/stop`, `connect`,
  `prompt`, `cancel`, `setModel`, `setConfigOption`, `answerPermission`,
  `supportsStructuredAnswers = true`.
- `PiWire.swift` — Codable wire types for commands, responses and events (mirrors
  `ClaudeWire.swift`'s role).

**Modified files:**

- `Packages/TillerACP/Sources/TillerACP/AgentDriverFactory.swift` — `"pi"` becomes a
  native id: `transportKind → .native`, `nativeBinary → "pi"`, new `case` in
  `makeNativeDriver` building `PiRPCDriver` over a `ProcessTransport`.
- `Packages/TillerACP/Sources/TillerACP/ChatQuestion.swift` — add canonical
  free-text answer metadata (placeholder/prefill) parsed from synthetic Pi input
  requests; ordinary permission and option questions remain unchanged.
- `App/Chat/QuestionCardView.swift` and `App/Chat/ChatController.swift` — render and
  submit free-text answers through the existing `PermissionOutcome.answered` path.
- `App/AcpAgentCenter.swift` — add `"pi"` to `nativeAgentIDs` so Pi appears in
  Settings (availability probe on the `pi` binary) and in the "New Chat" menu.

The terminal-side `PiAdapter` (PTY panes, OSC-title activity detection) is
untouched; chat and terminal stay independent surfaces.

## Wire mapping

### Streaming text

| Pi RPC event | Canonical emission |
| --- | --- |
| `message_update` · `text_delta` | `.update(.agentMessageChunk(.text(delta)))` |
| `message_update` · `thinking_delta` | `.update(.agentThoughtChunk(.text(delta)))` |

### Tool calls (correlated by `toolCallId`)

| Pi RPC event | Canonical emission |
| --- | --- |
| `tool_execution_start` (`toolName`, `args`) | `.update(.toolCall)` status `.inProgress`; `kind`: `read→read`, `edit`/`write→edit`, `bash→execute`, `grep`/`find`/`ls→search`, else `other`; `locations` from path-like args |
| `tool_execution_update` (`partialResult`, cumulative) | `.update(.toolCallUpdate)` — replace textual content (no delta math needed) |
| `tool_execution_end` (`result`, `isError`) | `.update(.toolCallUpdate)` status `.completed`/`.failed`; for `edit`, the retained `args.edits[]` supplies `.diff(path:oldText:newText:)` entries; for `write`, retained `args.content` supplies `.diff(path:oldText:nil,newText:)` (native rendering in `EditSummaryCardView`); `result.details.diff` remains a textual fallback |

### Turn lifecycle

| Pi RPC | Effect |
| --- | --- |
| `prompt` response `success: true` | prompt accepted; events stream asynchronously |
| `prompt` response `success: false` | driver's `prompt()` throws (error surfaced in chat) |
| `agent_end` | updates low-level run state only; never closes the canonical turn because retry, compaction or queued continuation may follow |
| `agent_settled` | `.turnEnded(.endTurn)`; resolves the pending `prompt()` continuation only when Pi guarantees no automatic continuation remains |
| `cancel()` → `abort` command | `.turnEnded(.cancelled)` |
| `compaction_start/end`, `auto_retry_start/end` | informational thought chunks ("⟳ Compacting context…", "↻ Retrying…") |
| `extension_error` | error thought chunk + stderr log |
| process exit / stream close | `.disconnected` |

Prompts sent **while streaming** use `streamingBehavior: "steer"` because Pi
rejects a plain prompt in that state. Slash-prefixed extension commands are the
exception: they remain `prompt` commands without `streamingBehavior` so Pi can run
them immediately (the dedicated `steer` command rejects extension commands).

### Questions (Extension UI → existing cards)

| Pi request | Canonical emission | `answerPermission` reply |
| --- | --- | --- |
| `select` (title, options, timeout?) | `.permissionRequested` with one option per choice | `extension_ui_response { id, value: <chosen option> }` or `{ cancelled: true }` |
| `confirm` (title, message) | `.permissionRequested` with Yes/No options | `{ id, confirmed: true | false }` |
| `input` (title, placeholder) | question card with canonical text field | `{ id, value: <text> }` |

`supportsStructuredAnswers = true`. A `turnEnded` arriving before an answer expires
the card (existing reducer behaviour). Pi auto-resolves timed-out requests
server-side; the client tracks no timeouts.

### Models & thinking

- On `connect`: `get_state` (current model + thinking level) and
  `get_available_models` → `SessionHandle.models = SessionModelState(currentModelId:,
  availableModels:)` with `ModelInfo(modelId: "provider/id", …)`.
- `setModel("provider/id")` splits on the first `/` and sends
  `{ "type": "set_model", "provider": "provider", "modelId": "id" }`.
- `get_available_thinking_levels` → `SessionConfigOption(id: "effort", choices:
  off/minimal/low/medium/high[/xhigh/max])` in `SessionHandle.configOptions`;
  `setConfigOption(id: "effort", value:)` → `set_thinking_level`.

### Sessions: persist + resume

- Launch persists by default (no `--no-session`); Pi writes under
  `~/.pi/agent/sessions/<slug-cwd>/`.
- After connect, `get_state` yields the session ref → `SessionHandle.sessionId`;
  `ChatSessionStore` stores it in `acpSessionId` as for other agents.
- Resume: `connect(cwd:, resumeSessionId:)` launches `pi --mode rpc --session <ref>`
  (`--session <path|id>` accepts a session file path or partial UUID) and returns
  `didResume: true`; the visible transcript reloads from Tiller's DB
  (`saveTranscript`/`loadTranscript`). The `--session` + `--mode rpc` combination
  is verified empirically in the first implementation task; fallback is the
  `switch_session` RPC command right after start.

### Slash commands

- After connect, `get_commands` → `.update(.availableCommandsUpdate(...))` (same
  slot where the Claude driver emits its init commands).

## Security

- `--approve` scopes trust to the worktree Tiller itself created; no global Pi
  config is touched, and `DefaultResourceLoader` global paths stay read-only to Pi.
- No secrets in argv: the driver never passes API keys; Pi resolves auth itself
  (`auth.json` / environment). stderr goes to the existing NSLog stderr logger,
  never into the protocol stream.

## Testing (swift-testing, TDD)

- **Fake RPC process / scripted transport**, same pattern as the existing driver
  tests: no real `pi` binary in unit tests.
- **Fixture-based mapping tests** (highest value): recorded JSONL (text turn,
  thinking turn, tool call with updates + diff result, select/confirm/input
  requests, compaction, abort, dirty exit) → expected `ACPSessionEvent` sequence.
- **Command correlation:** out-of-order `response` ids matched to the right pending
  command; `prompt` `success:false` surfaces as an error.
- **Settlement ordering:** `agent_end { willRetry: true }` does not finish the
  canonical turn; only the later `agent_settled` resolves `prompt()` and emits
  `.turnEnded`, after all streamed updates.
- **Question roundtrip:** `extension_ui_request` in → canonical permission event
  out → `answerPermission` writes the correct `extension_ui_response`.
- **Session args:** resume builds `--session <ref>`; fresh chats do not.
- Gate: `Scripts/ci.sh` → `CI OK`.

Manual integration smoke (real CLI): one tool-using turn with live tool cards,
thinking stream, an `ask_user_question` roundtrip (select + input), model switch
from the picker, effort change, mid-turn steer, cancel, kill `pi` → clean
`disconnected`, close/reopen tab → resume from stored session ref.

## Done criteria

- `Scripts/ci.sh` prints CI OK.
- Pi appears in Settings and "New Chat"; chats run through `PiRPCDriver`.
- Streaming text/thinking, tool cards (incl. edit diffs), question cards, model +
  effort pickers, mid-turn steer, cancel, and session resume all work.
- Terminal-pane Pi (`PiAdapter`) behaviour unchanged.
