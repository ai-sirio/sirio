# Native chat transports for Claude Code, Codex, OpenCode (T3-style drivers)

**Date:** 2026-07-22
**Status:** Approved (brainstorming session)
**Companion spec:** `2026-07-22-t3-chat-ui-structure-design.md` (Spec 2, depends on this one)

## Goal

Replace the ACP adapters for Claude Code, Codex and OpenCode with direct wire-protocol
drivers written in Swift, following t3code's per-provider driver architecture
(`ProviderDriver` + canonical events). All other harnesses (omp, pi, gemini, …) keep
the existing ACP path unchanged.

Why: the native protocols expose features the ACP adapters do not translate —
permission modes, live model switching, effort, context-window usage — and remove the
adapter subprocess as a failure class (transportClosed, shebang/PATH issues, adapter
version skew).

## Decisions (from brainstorming)

1. **Direct wire protocols from Swift** — no Node sidecar, no Agent SDK. The SDKs are
   wrappers over these same wire protocols, so nothing is lost except pre-built types.
2. **Cutover + best-effort resume** — ACP adapters removed for the three harnesses;
   existing stored sessions are resumed through native resume mechanisms where the ids
   map, otherwise degrade to read-only history with a banner.
3. **Full T3 permission parity** — per-session permission mode selector plus per-tool
   approval prompts.
4. Multi-account / HOME isolation (a t3code feature) is **out of scope** (YAGNI).

## Architecture

```
TillerChat (evolved from TillerACP)
├── AgentDriver (protocol)            ← connect/prompt/cancel/setMode/setModel/events
├── Drivers/
│   ├── ClaudeStreamJSONDriver        ← claude --input/output-format stream-json
│   ├── CodexAppServerDriver          ← codex app-server (JSON-RPC over stdio)
│   ├── OpenCodeHTTPDriver            ← opencode serve (HTTP + SSE)
│   └── ACPDriver                     ← wraps existing ACPSession (all other agents)
└── Canonical events                  ← existing SessionUpdate/TranscriptItem, extended
```

- **No new canonical model.** The existing `SessionUpdate` → `TranscriptReducer`
  pipeline stays the single internal representation (the UI already consumes it).
  It is extended with: `permissionModeChanged`, `contextUsage`, rich `modelState`,
  effort. Native drivers map their wire events into it; `ACPDriver` becomes one of
  four drivers at zero cost to the remaining harnesses.
- `ChatController` selects a driver from the registry by agent id
  (`claude-acp` → ClaudeStreamJSONDriver, `codex-acp` → CodexAppServerDriver,
  `opencode` → OpenCodeHTTPDriver, everything else → ACPDriver) and consumes the
  canonical stream. This mirrors t3code's `contracts` + per-provider driver split.
- Launch continues through the login shell (`zsh -lc exec …`) as today, so PATH
  resolves like the user's terminal.

## Driver detail

### ClaudeStreamJSONDriver
- **Spawn:** `claude -p --input-format stream-json --output-format stream-json
  --model <m> --permission-mode <mode>` in the worktree cwd; `--resume <sessionId>`
  to resume.
- **Stream (stdout NDJSON):** `system/init` (model, tools, slash commands),
  `assistant`/`user` (content and tool-use deltas), `result` (usage, cost).
  Stdin carries user turns and control requests.
- **Permissions:** bidirectional control protocol — incoming `can_use_tool` → UI
  prompt → allow/deny reply; `set_permission_mode` backs the mode dropdown.
  Plan mode surfaces `ExitPlanMode` as a proposed-plan approval (rendered in Spec 2).
- **Models/effort:** `set_model` live; effort via prompt prefix (t3code's
  `applyClaudePromptEffortPrefix` trick — no native flag exists).
- **Context:** control request `get_context_usage` after each result, plus per-turn
  usage in `result` messages.
- **Legacy resume:** claude-code-acp's ACP session id IS the underlying Claude Code
  session id → `--resume` directly. Verify empirically in the first implementation task.

### CodexAppServerDriver
- **Spawn:** `codex app-server`, line-delimited JSON-RPC over stdio; `initialize`
  handshake.
- **Turns:** `newConversation` / `resumeConversation`, then `sendUserTurn` carrying
  `model`, `effort`, `approvalPolicy`, `sandboxPolicy`, cwd. Server events: agent
  deltas, exec begin/end, `token_count`.
- **Permissions:** `applyPatchApproval` / `execCommandApproval` requests → same UI
  prompt; per-session approval policy = mapped permission mode.
- **Legacy resume:** codex-acp builds on Codex rollout files; if the stored ACP id
  matches a rollout id → `resumeConversation`, otherwise history-only.

### OpenCodeHTTPDriver
- **Spawn:** `opencode serve --port 0` per worktree (shared-server option decided at
  plan time); HTTP client + SSE `/event` stream.
- **Turns:** `POST /session/:id/message` with parts; message parts, permission and
  question requests arrive over SSE.
- **Permissions:** permission SSE requests → UI prompt; permission rules derived from
  the session mode.
- **Models/effort:** full model list from `/config/providers`; effort and other
  tunables are session config options (already modelled as `configOptions`).
- **Legacy resume:** OpenCode ACP ids are the upstream `ses_…` ids → re-adopt directly.

### Common to all drivers
- Kill the process on session close; unexpected exit → `disconnected` with a readable
  error in chat.
- stderr logged to a per-session file for diagnosis (t3code's `EventNdjsonLogger`
  pattern).

## Permission model

Single Tiller enum, mapped per driver; the dropdown shows only modes the driver
supports. ACP agents get no dropdown (today's behaviour).

| Tiller mode | Claude | Codex | OpenCode |
|---|---|---|---|
| Ask (default) | `default` | `untrusted` | ask rules |
| Accept edits | `acceptEdits` | `on-request` | edits allowed, rest ask |
| Plan | `plan` | — (hidden) | — (hidden) |
| Full auto | `bypassPermissions` | `never` + workspace-write sandbox | all allowed |

Live switching: Claude via `set_permission_mode`; Codex applies on the next turn
(policy is per-turn); OpenCode via updated rules.

Per-tool prompts: one canonical `permissionRequested(tool, description, options)`
event (already exists for ACP); native drivers feed it from `can_use_tool` /
`execCommandApproval` / permission SSE. Existing prompt UI is reused in this spec and
redesigned in Spec 2.

## Persistence & migration (DB v13)

- New chat-session columns: `permissionMode`, `selectedModel`, `selectedEffort`,
  `transportKind` (`native` | `acp`).
- Existing sessions: `transportKind` derived from agent id (claude-acp / codex-acp /
  opencode → native, rest → acp); mode defaults to Ask.
- Best-effort resume: on first open post-migration the driver attempts resume with the
  stored id; on failure a "session resumed as history" banner appears, a fresh session
  starts underneath, and the old transcript stays intact.

## Security

- Codex Full auto is always paired with the `workspace-write` sandbox, never
  `danger-full-access`.
- No secrets in argv (environment only); stderr logs never dump the environment.

## Testing (swift-testing, TDD)

- **Per-driver, no real CLIs:** each driver tested against a fake process/server
  replaying the wire protocol (like `MockTransport`; t3code's `acp-mock-agent`
  pattern). Recorded fixtures committed: init, turn with tool call, permission
  request, cancel, dirty exit.
- **Canonical mapping:** fixture in → expected `SessionUpdate` sequence out, per
  driver (highest-value tests).
- **Permission matrix:** mode × driver → correct wire parameters.
- **Migration v13:** v12 fixture DB → v13, derived transportKind, sessions readable.
- **Resume:** old id accepted/rejected → banner path.

Manual integration smoke (real CLIs): one tool-using turn per driver, live mode
switch, model switch, moving context meter, mid-turn cancel, CLI kill → clean
disconnect, resume of a pre-migration session.

## Done criteria

- `Scripts/ci.sh` prints CI OK.
- The three harnesses chat through native drivers; all others through ACPDriver
  unchanged.
- No regressions vs ACP (text streaming, tool cards, basic permissions).
- New visible features: mode dropdown, context usage, live model switch, effort.
