# Native chat transports for Codex, Pi, OpenCode and Oh-My-Pi

**Date:** 2026-10-31
**Status:** programme designed, not implemented. Each agent gets its own spec
after its live probe (§9).
**Decisions:** [ADR 0001](../../adr/0001-chat-transport-native-first.md) (native
first, ACP as a first-class transport and the fallback) and
[ADR 0002](../../adr/0002-one-opencode-server-per-app.md) (one OpenCode server
per app). The terms *session panel* and *transport* are defined in `CONTEXT.md`.
**Touches:** `sirio_acp`, `sirio_agents`, `sirio_ui` (chat, session panel,
settings), `sirio` (resolution, picker, restore), a new `sirio_chat`, two new
leaf crates shared by every transport, `sirio_chat_vocab` and `sirio_chat_child`
(§2.2), and one leaf protocol crate per agent family: `sirio_pi`, `sirio_codex`,
`sirio_opencode`.

This is the programme spec. It holds what every delivery shares: the shape of
the code, the resolution rule, the parity contract, the panel map and the test
shape. Each agent's protocol details are settled in that agent's own spec,
written after its live probe, because a probe can overturn what a schema
promises. Facts below are tagged by how they were established:

- **[live]**: seen on the wire on 2026-10-31 with the version named.
- **[schema]** or **[docs]**: read from a generated schema, the agent's own
  documentation, or source embedded in its binary. Not yet seen on the wire.
- **[code]**: Sirio's own code at `d945ca2f`.

No probe on 2026-10-31 ran a model turn. Every claim about what a turn
streams is [schema] or [docs].

## §0 The problem this exists for

A chat tab for Claude Code drives `claude` over its own stdio protocol. A chat
tab for Codex, Pi, OpenCode or Oh-My-Pi (omp) drives an ACP bridge. That has
four costs.

1. **The session panel is Claude's alone.**
   - Connectors and the Context breakdown are gated on
     `ChatClient::supports_connectors`, which is `matches!(self, Self::Claude(_))`
     ([code] `sirio_acp/src/lib.rs:450`, used at
     `sirio_ui/src/chat/session_panel/mod.rs:375` and `:381`).
   - An ACP chat's Context shows a total and the row "This agent does not
     report what is in its context" ([code] `session_panel/context.rs:17`).
   - The Trace card needs a `TraceEvent`, and only the Claude native fold
     produces one ([code] `sirio_acp/src/claude/trace.rs`).
2. **The bridges cap what the panel could ever show** ([live] probe of
   2026-10-21, ADR 0001):
   - `codex-acp` never forwards a subagent's own tool calls;
   - `omp acp` sends no subagent metadata;
   - OpenCode's subagent tree exists only as a vendor `_meta` extension, which
     Sirio does not read ([code] no reader of `_meta` in `sirio_acp/src`).
3. **No ACP chat resumes its agent's session.**
   - ACP always sends `session/new` ([code] `sirio_acp/src/lib.rs:1801`).
   - Reopening a chat from history "always starts a fresh session beside the
     restored transcript" ([code] `sirio/src/main.rs:14672`).
   - Only a native Claude chat passes `--resume` ([code]
     `restored_chat_spec`, `main.rs:4367`).
4. **Codex and Pi have no chat without a registry package.**
   - Neither claims a builtin `acp` subcommand, so their chat resolves to
     `codex-acp` and `pi-acp` from the registry, and is refused when that
     package is not installed ([code] `sirio_registry/src/resolve.rs:84-85`,
     `main.rs:3934`).
   - Every one of these agents already serves a machine protocol from its own
     binary.

## §1 Decisions

The two ADRs record the decisions that are hard to reverse. Taken with them on
2026-10-31:

1. **Deliveries, each its own PR, in this order:**
   1. This spec, the ADRs and `CONTEXT.md` (`docs:`).
   2. The `sirio_chat` refactor (§2), with Claude's native transport moved
      into it (`refactor:`). No behaviour change.
   3. Pi.
   4. omp.
   5. Codex.
   6. OpenCode.

   Pi goes first because its contract is documented and it is the smallest. omp
   reuses Pi's base. Codex needs `codex login` first: the maintainer's login was
   expired on 2026-10-31. OpenCode goes last because its 2.x API calls itself
   experimental.
2. **Scope.** A native transport is done when its chat does at least
   everything its ACP chat does today (§4), feeds the session panel (§5), and
   resumes the agent's session on relaunch. Steering a running turn, forking or
   rewinding a session, and rate limits stay out, even where a protocol offers
   them.
3. **Resolution.** It is static, by a version floor, with a forced fallback
   and no runtime switch (§3).
4. **Missing data.** Where a protocol does not report something, the card or
   row is left out. Context is the exception (§5).
5. **Context without weights.** When an agent names what is loaded but not
   what it weighs, Context lists those items by origin, with no sizes and no
   estimates. The row "This agent does not report how much each takes"
   replaces the breakdown.
6. **Subagent lanes.** A subagent whose start and end are known, but not its
   own tool calls, gets a lane with a single bar and the counts the protocol
   gives (§6). No inner spans are invented.
7. **Pi.**
   - Its extensions' blocking dialogs (`select`, `confirm`, `input`,
     `editor`) go to the chat's question dock.
   - `notify` becomes a notice in the transcript.
   - `setStatus`, `setWidget` and `setTitle` are ignored.
   - The third-party npm extensions `pi-subagents` and `pi-mcp-adapter` are
     not read.
8. **omp.**
   - It runs in `--mode rpc-ui`, so its approvals reach the chat.
   - It honours the user's own `tools.approvalMode`.
   - It subscribes to its subagents at the `events` level.
9. **Full access.** A mode that runs tools without asking is confirmed with
   the same dialog as Claude's bypass when the user picks it in Sirio (§7).
10. **Crates.** Pi and omp share one protocol crate, `sirio_pi`, with an `omp`
    module.

## §2 Shape of the code

### §2.1 Where things live

```
sirio                      resolution per adapter, picker, restore
  └─ sirio_ui              chat, session panel: only ever sees ChatClient + the vocabulary
       └─ sirio_chat       ChatClient { Acp, Claude, Pi, Codex, OpenCode }, transport choice,
                           the native process owners (claude, pi, codex, opencode)
            ├─ sirio_acp   the ACP client and nothing else
            ├─ sirio_claude, sirio_pi, sirio_codex, sirio_opencode
            │              leaf protocol crates: types and pure functions, no process,
            │              no channels (the sirio_claude precedent)
            ├─ sirio_chat_child  leaf: supervising an agent's child process
            └─ sirio_chat_vocab  leaf: ChatEvent (was AcpEvent), ChatError (was AcpError),
                                 TraceEvent, ContextUsage, …
```

- `ChatClient` keeps an exhaustive `match` in every method ([code]
  `sirio_acp/src/lib.rs:225`). A new variant makes the compiler ask, method by
  method, what that transport does. Nothing stays Claude's by accident.
- `NativeChat` today is Claude-shaped (`program`, `min_version`, [code]
  `sirio_agents/src/lib.rs:116`). It becomes every adapter's claim.
- `AgentLaunchState.claude` and the hard-coded Claude test in
  `agent_launch_for` ([code] `main.rs:3973`) become one resolution per adapter.
- The same goes for the inline Claude clause in `chat_agents` ([code]
  `main.rs:18713`), and for resume in `restored_chat_spec`, which today matches
  only `LaunchSpec::Claude`.
- The UI downcasts to `ChatClient::Claude` in two places, for
  `last_user_message_id` and `resumed_session_refused` ([code]
  `sirio_ui/src/chat/mod.rs` near `:2974` and `:3505`). Both become
  `ChatClient` methods.
- The refactor delivery updates `docs/agents/architecture/crates.md` and
  `claude-chat.md` to match.

### §2.2 What every transport shares

`sirio_acp` cannot keep the events every transport emits. ADR 0001 makes it
"the ACP client and nothing else", and `sirio_chat` sits above it, so the
vocabulary cannot live in `sirio_chat` either. Everything below must go to a
crate below both:
- `AcpEvent` (17 variants, [code] `lib.rs:751`);
- `TraceEvent` ([code] `trace_event.rs`);
- `ContextUsage`, `SessionNotice`;
- `AcpError`, which the Claude worker raises too ([code] `claude/worker.rs:431`).

**Decided:** they go to a new leaf crate, **`sirio_chat_vocab`**. `AcpEvent`
becomes `ChatEvent` and `AcpError` becomes `ChatError`, in the same mechanical
commit, which is part of the refactor delivery. The context and MCP view types
that `sirio_acp` re-exports from `sirio_claude` today are re-exported by
`sirio_chat` instead, so the vocabulary depends on no protocol crate.

The Claude worker also shares the helpers that supervise an agent's child
process with the ACP client ([code] `sirio_acp/src/lib.rs` from `:2253`):
- the activity clock, the stderr tail and the MCP warnings read from it;
- reaping the process group, recording the exit status, and describing the
  exit and the timeout;
- detaching the worker thread.

Every native transport with a child process needs them as they are, and so
does the OpenCode server.

**Decided:** they go to a second leaf crate, **`sirio_chat_child`**, which
depends on `sirio_chat_vocab` for `ChatError` and contains nothing but that
supervision. `parse_permission_question` is a pure parser of a tool input, so
it goes to `sirio_chat_vocab`.

Today the crates that depend on `sirio_acp` are `sirio`, `sirio_ui`, and
`sirio_control` (only its integration test). Each takes the types from these
crates instead, and keeps `sirio_acp` only where it really drives ACP.

### §2.3 Protocol crates

Each protocol crate follows `sirio_claude`:
- it holds the wire as types and pure functions;
- it degrades rather than rejects: an unknown event or field is ignored, a
  malformed line is skipped, and a refused request costs that one feature, not
  the session;
- it is a leaf, so `sirio_agents` and `sirio_usage` can depend on it without
  inverting the graph.

## §3 Resolution and fallback

A chat resolves its transport once, when it starts:

1. If `SIRIO_<AGENT>_TRANSPORT=acp` is set, the chat uses ACP.
   `<AGENT>` is `CLAUDE`, `CODEX`, `PI`, `OPENCODE` or `OMP`.
2. If the agent's binary is not on PATH, the chat uses ACP.
3. If its version cannot be read, or it is below the floor, the chat uses ACP.
4. Otherwise the chat uses the native transport.

Settings → Agents shows the reason, as it does for Claude today
([code] `sirio/src/claude_transport.rs`). A native session that fails shows
its error in the chat. It never switches transport.

**Version floors** are the versions the delivery's probe saw. The candidates
today are:

| Agent | Binary | Native protocol | Floor candidate | Note |
|---|---|---|---|---|
| Pi | `pi` | `--mode rpc`, JSONL on stdio | 1.4.0 | [live] handshake |
| omp | `omp` | `--mode rpc-ui`, JSONL on stdio | 18.12.6 | [live] handshake in `--mode rpc` |
| Codex | `codex` | `app-server`, JSON-RPC lines on stdio | 0.159.2 | [live] handshake |
| OpenCode | `opencode` | `serve`, HTTP + SSE | 2.0.29 | [live] read-only GETs |

The version is read from the binary that **the chat's own PATH** resolves,
which is the one that will run. This matters for OpenCode on the maintainer's
machine. Two installs exist:
- `~/.opencode/bin/opencode` 2.0.29;
- mise's `opencode` 1.18.33, which is the one the running `opencode acp`
  processes use.

1.x serves a different API and stays on ACP.

## §4 The parity contract

**What a native transport must do.** These are the things an ACP chat does
today ([code] the ACP arms of `ChatClient` and `AcpClient`, `lib.rs:225-516`):

- **Prompts:** send a prompt with mentions and images, and cancel it.
- **Streaming:** stream assistant text and thoughts.
- **Tool calls:** start, update and complete, with text and diff content and
  the raw input and output.
- **Plans:** report the agent's plan.
- **Permissions:** answer a permission request with the agent's own options,
  or cancel it.
- **Selectors:** offer the mode, model and effort selectors and switch them
  mid-session.
- **Commands:** list the available slash commands.
- **Usage:** report used, window and cost, plus per-turn tokens where the
  agent reports them.
- **Lifecycle:** report the end of a turn, transport errors and timeouts.

**On top of that:**
- the panel data of §5;
- resume. With `resume_agent_sessions` on (the same setting as Claude's), a
  relaunch continues the agent's own session. When the agent refuses, the chat
  starts a fresh session rather than a dead tab, as Claude's worker does
  ([code] `sirio_acp/src/claude/worker.rs:306`).

**What each agent resumes with:**

| Agent | Resume | Basis |
|---|---|---|
| Pi | `switch_session {sessionPath}`, or `--session` at launch | [docs] |
| omp | `switch_session` or `open_session`, or `--resume` | [schema] |
| Codex | `thread/resume {threadId}` | [schema] |
| OpenCode | the session persists in the server's store; Sirio reattaches by id | [schema] |

**Model and effort mid-session use only stable methods.** For Codex that means
the `turn/start` overrides, which persist for later turns, and not the
experimental `thread/settings/update` [schema].

## §5 The session panel, per agent

Expected cards. Each is confirmed or cut by that agent's probe.

| Card | Pi | omp | Codex | OpenCode |
|---|---|---|---|---|
| Environment | yes | yes | yes | yes |
| Context total | `get_session_stats.contextUsage` [live] | `get_session_stats.contextUsage` [live] | `tokenUsage` with `modelContextWindow`, after the first turn [schema] | last step's tokens against `Model.limit` (`input` where given, else `context`) [schema] |
| Context items, no weights | skills, prompts and extension commands from `get_commands` [live] | `systemPrompt` sections, `dumpTools`, commands by source [live] | `instructionSources`, `skills/list`, MCP tools [schema; skills live] | `/api/skill`, MCP, agents, instruction entries [live, except instruction entries] |
| Connectors | no (Pi has no MCP of its own) | no (no MCP status over RPC) | `mcpServerStatus/list`, OAuth login [live list] | `/api/mcp`, connect/disconnect, integration OAuth [live list] |
| Trace | main lane only | main lane + subagent lanes from `subagent_event` | main lane + subagent lanes from `subAgentActivity` and child threads | main lane + child sessions via `parentID` |
| Change request | yes | yes | yes | yes |

**Context.** The item lists come from read-only calls that cost no turn. They
can be asked when the card unfolds, as Claude's `get_context_usage` is today.
OpenCode's `/api/skill` returns every skill's full text (675 KB on the
maintainer's machine, [live]), so it is asked once per chat and never polled.

**Cost.** Where an agent reports no USD cost, the cost is left out, and the
folded Trace line reads `Done · 14s`. Codex on a ChatGPT account is the
expected case.

## §6 The Trace for each agent

Each transport gets its own fold into `TraceEvent`, the role
`sirio_acp::claude::trace::TraceFold` plays for Claude.

- **Timing.**
  - Codex stamps items in milliseconds on the server (`startedAtMs`,
    `completedAtMs`, tool `durationMs`, [schema]), but only on live
    notifications. Replayed items carry `null` [live].
  - Pi and omp stamp messages, not events [docs], [schema].
  - OpenCode stamps parts and steps in epoch milliseconds [schema].
  - Where no stamp exists, the fold stamps the instant it read the line, as
    Claude's does.
- **Subagents.**
  - omp at `events` level forwards each child event, so a child's tool calls
    become spans in its lane [schema]. If the probe shows the traffic is too
    heavy, the subscription drops to `progress`, and the lane becomes one bar
    with the counts (`tokens`, `toolCount`, `cost`, `durationMs`).
  - Codex reports a subagent as `subAgentActivity` started and completed, with
    the child's `agentThreadId` [live, from a stored thread]. Whether the
    child's items stream on the parent's connection is unknown. If they do not,
    the lane is one bar.
  - OpenCode's children are sessions with `parentID` on the same stream
    [schema]. How a `task` tool call maps to its child session is not
    established. Until it is, a child lane is drawn from the child session
    alone, and the parent's task call stays a plain tool call.
  - Pi has no subagents of its own. It gets one lane.
- **Cap.** The 2 000-span cap and its rules apply unchanged.

## §7 Permissions and full access

| Agent | How it asks | What the chat does |
|---|---|---|
| Pi | It never asks: tools always run [docs]. Extensions may open blocking dialogs [live]. | The dialogs go to the question dock. There is no permission selector. |
| omp | `--mode rpc-ui` approvals. The wire form is not yet seen. | Permission requests, under the user's `tools.approvalMode` (default `yolo`) [schema]. |
| Codex | Six typed server requests: command, file change, permissions, user input, MCP elicitation, dynamic tool [schema]. | Permission requests and the question dock. |
| OpenCode | `permission.asked` → reply `once` / `always` / `reject`. Forms are separate [schema]. | Permission requests and the question dock. |

**Full-access modes** are Codex's `approvalPolicy: never` with
`dangerFullAccess`, omp's `yolo`, and OpenCode with every rule set to `allow`.
Each one goes through the bypass confirmation ([code]
`sirio_ui/src/chat/bypass.rs`) when the user picks it in Sirio's selector.
A session that starts that way because of the user's own configuration
starts without asking.

## §8 Tests

For every delivery:

1. **A fixture.** A Python stub of the agent's native protocol, injected by
   PATH under the agent's binary name, as the Claude stubs are
   (`Scripts/Tests/stubs/session-panel/claude` answers `--version` and execs
   `claude_fixture.py`).
2. **An end-to-end scenario.** It drives the real app with the stub, through
   the control socket (`surface.chat.read`, `sessionPanel`,
   `surface.trace.read`). It covers the chat, resume, a permission or dialog,
   and every card §5 promises for that agent. It ends in an artifact anyone
   can re-run, the shape `test-chat-session-panel-e2e.sh` already has.
3. **A conformance test** in `sirio_agents/tests/acp_conformance.rs`.
   - A free one: the handshake the native claim rests on. It SKIPs without the
     binary.
   - An opt-in one, `#[ignore]` and off the release gate: it spends one real
     turn and checks that the lines the transport and the fold read still
     arrive.
   - Probe models: Codex on `gpt-6-astra`, because a ChatGPT account refuses
     `gpt-6-luna`; OpenCode on `opencode/deepseek-v4.1-flash-free`; Pi and omp
     on `openai-codex/gpt-6-luna`. omp's own default is a Bedrock Opus, so
     every probe must name its model.

No unit test is written after the code it tests. A protocol crate's pure
functions get their failure modes written first.

## §9 The live probe that opens each delivery

Before its spec, each delivery records one probe with Sirio's own launch line,
in a throwaway directory. The probe must:
- run a turn with a tool call;
- run a turn with a permission or dialog;
- run a subagent, where the agent has one;
- interrupt a turn;
- resume a session.

What it settles:

| Agent | What the probe settles |
|---|---|
| Pi | `message_update` delta-only on the wire; `tool_execution_end.durationMs`; resume via `--session` in RPC; the extension dialogs a fresh `HOME` still raises |
| omp | the `rpc-ui` approval wire; `set_event_filter {messageUpdates: "delta"}`; `prompt_result` and `session_settled` order; `subagent_event` volume; whether protocol v2 is needed |
| Codex | the meaning of `tokenUsage.last` and `.total`; whether child-thread items stream to the parent's connection; `instructionSources` values; drift against the newer daemon (0.166 already adds fields) |
| OpenCode | the payloads of `session.*` and `message.*` events; whether `session.log` resumes from its cursor; whether a preset `OPENCODE_PASSWORD` keeps the server from printing it; whether a second server on the same `opencode.db` is safe beside the user's `opencode service`; how a `task` call maps to its child |

## §10 Out of scope

- Steering a running turn, forking or rewinding a session, rate limits, and
  Codex reviews.
- New panel cards (a Tools or Skills card for every agent, Claude included).
  These would be their own design.
- Reading third-party extensions' payloads (Pi's `pi-subagents`,
  `pi-mcp-adapter`) or vendor `_meta` over ACP.
- The detached host (`sirio_host`). If chats later move into the host, the
  shared OpenCode server moves with them, and that programme decides how.
- Issue #689 (OpenCode 2.x refuses the plugin Install Hooks writes). It
  concerns terminal panes, not chats.
