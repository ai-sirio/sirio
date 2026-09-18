# Claude Code chat over Claude Code's own protocol

**Date:** 2026-09-18
**Status:** designed, not implemented
**Touches:** `sirio_acp`, `sirio_agents`, `sirio_persistence`, `sirio_usage`,
`sirio_ui` (chat, settings), `sirio` (resolution, restore), and one new leaf
crate, `sirio_claude`.

## §0 The problem this exists for

A Claude Code chat tab does not talk to Claude Code. It talks to an ACP
wrapper — the registry row `claude-acp`, today
`@agentclientprotocol/claude-agent-acp` — which `sirio_registry` installs with
`npm` into `~/.local/share/sirio/agents/claude-acp/<version>/`. That wrapper
is fourteen thousand lines of JavaScript on top of `@anthropic-ai/claude-agent-sdk`,
and the SDK ships its **own** Claude Code as a platform package
(`@anthropic-ai/claude-agent-sdk-linux-x64`, a 215 MB binary). Measured on
the maintainer's machine on 2026-09-18:

| | Native `claude` on PATH | ACP wrapper (`node` + bundled `claude`) |
|---|---|---|
| Claude Code version | 2.1.273 | 2.1.257 |
| Install on disk | already there | 260 MB, plus Node on PATH |
| Cold-start budget in `sirio_acp` | — | 120 s (`npx` may download first) |
| Handshake to a usable session | 0.73 s | 1.47 s (`initialize` 0.28 s, `session/new` 1.47 s) |

So a chat tab and a terminal pane run two different Claude Codes, sixteen
patch releases apart, with one of them needing Node to exist. Everything the
wrapper does — and it does a lot — it does by spawning that binary with
`--output-format stream-json --input-format stream-json` and exchanging
newline-delimited JSON on its stdio. Nothing in that exchange needs
JavaScript.

A spike the same day verified that the `claude` already on PATH answers the
same protocol directly. Sent on stdin, with no API call and no session file
written:

```
{"type":"control_request","request_id":"probe-1","request":{"subtype":"initialize","hooks":{}}}
```

and back, in 0.73 s, a `control_response` carrying 53 slash commands, 5
models, 5 agents, the account, the output styles and the permission mode;
then `set_permission_mode`, `set_model` and `interrupt` each answered with
`success`, the mode change echoed as a `system/status` line. That is the
whole feasibility question answered.

What is *not* answered by any document is stability. The stream-json input
format and the `-p` flags are in the CLI reference, but the control channel
is the SDK's internal contract, versioned with the SDK and not published as a
public API (anthropics/claude-code#24612 is the open request to document it).
This design takes that risk with eyes open, and §3 and §8 are the parts that
pay for it: a version floor, tolerant parsing, an automatic fallback to the
wrapper that still exists, and a conformance test that says the day the claim
stops being true.

The terminal pane is untouched by all of this. It runs `claude` interactively
and always did.

## §1 The protocol, as verified

The SDK's own spawn line (read out of `sdk.mjs` 0.3.257, then run by hand):

```
claude -p --output-format stream-json --input-format stream-json --verbose
       --permission-prompt-tool stdio --include-partial-messages
```

Everything else is a JSON object per line.

**Outbound from the CLI** (`type`, then `subtype` where it matters):
`system/init` (once per turn, ahead of the turn: `session_id`, `model`,
`claude_code_version`, `tools`, `mcp_servers[{name,status}]`,
`slash_commands`, `terminal_slash_commands`, `permissionMode`,
`apiKeySource`), `system/status` (`permissionMode` after a change,
compaction outcome), `system/compact_boundary`, `system/commands_changed`,
`assistant` (a Messages-API message: `content` blocks `text`, `thinking`,
`tool_use`; `parent_tool_use_id`; `uuid`), `user` (a `tool_result` block plus
`tool_use_result`, the tool's structured output), `stream_event`
(`content_block_start`/`content_block_delta` with `text_delta`,
`thinking_delta`, `input_json_delta`/`content_block_stop`), `tool_progress`,
`task_notification`, `auth_status`, `rate_limit_event`, `keep_alive`,
`result` (`subtype` `success` or `error_*`; `is_error`; `result` text;
`total_cost_usd`; `modelUsage` keyed by model with `inputTokens`,
`outputTokens`, `cacheReadInputTokens`, `cacheCreationInputTokens`,
`contextWindow`, `costUSD`; `permission_denials`; `stop_reason`), and
`control_request` for the one request the CLI makes of the client:
`can_use_tool` (`tool_name`, `input`, `tool_use_id`, `permission_suggestions`,
`decision_reason`, `blocked_path`).

**Inbound to the CLI:** `user` messages (`{"type":"user","uuid":…,"message":
{"role":"user","content":[text | image]}}`), `control_response`
(`{"type":"control_response","response":{"subtype":"success","request_id":…,
"response":…}}`), and `control_request` with these subtypes, all present in
SDK 0.3.257: `initialize`, `interrupt`, `set_permission_mode`, `set_model`,
`apply_flag_settings`, `get_context_usage`, `get_usage`, `rewind_files`.
The SDK knows more (`hook_callback`, `mcp_message`, `mcp_set_servers`,
`read_file`, `generate_session_title`, `set_cwd`, `claude_authenticate`, …);
this design uses exactly the eight above and §10 lists what it leaves out.

The `initialize` response is the catalogue: `commands[{name, description,
argumentHint}]`, `models[{value, displayName, description}]`, `agents`,
`output_style`, `available_output_styles`, `current_permission_mode`,
`account{email?, organization?, subscriptionType?, tokenSource?,
apiProvider?}`, `hooks_applied`.

## §2 Shape: one leaf crate, one module, one enum

```
sirio_claude   (new leaf — serde, serde_json, nothing local)
    ^
sirio_acp      (gains the `claude` module: the stateful client, ChatClient, LaunchSpec)
sirio_usage    (gains a one-shot `get_usage` fetcher through sirio_claude)
    ^
sirio_ui, sirio (unchanged in shape; the twelve `AcpClient` call sites become ChatClient)
```

**`sirio_claude`** is the protocol with no process attached: serde types for
every message and control request in §1, each tolerant of fields it does not
know (unknown `type`/`subtype` decode to an `Other` variant, never an
error); the argv and environment builder; `ClaudeVersion` and its floor;
the tool table (§4); and the pure parsers for the initialize response,
`get_usage`, `get_context_usage` and `rewind_files`. Every test in it is a
JSON line in and a struct out.

**`sirio_acp::claude`** is `ClaudeClient`: the same worker shape as
`run_connection` — one thread named `sirio-claude`, a command channel in, an
unbounded event channel out, the stderr tail, the activity clock and the four
bounded waits — emitting **the same `AcpEvent`s** the ACP client emits. The
event vocabulary is the contract the chat surface is written against
(forty-seven match arms in `sirio_ui::chat`, one in `sirio`), and it does not
change. Renaming it to something transport-neutral is a chore for another
day, deliberately not this one.

Next to it, in the same crate:

```rust
pub enum LaunchSpec {
    Acp(AgentCommand),
    Claude(ClaudeLaunch),
}
pub struct ClaudeLaunch {
    pub program: PathBuf,            // resolved `claude`
    pub resume: Option<String>,      // a Claude session id to `--resume`
}
pub enum ChatClient {
    Acp(AcpClient),
    Claude(ClaudeClient),
}
```

`ChatClient` carries the methods the chat surface and `ChatSession` call
today — `session_id` (now `Option<String>`: the ACP arm knows it at launch,
the native arm only once the first turn's `system/init` names it),
`model_catalog`, `mode_catalog`, `mcp_warnings`, `prompt_content`,
`set_model`, `set_config_option`, `set_mode`, `cancel`, `respond_permission`,
`cancel_permission`, `shutdown` — plus two new ones, `supports_rewind` and
`rewind_files`, which the `Acp` arm answers with `false` and `Unsupported`. `ChatSessionConfig::command` becomes
`launch: LaunchSpec`; `Chat::launch_with_command*` take a `LaunchSpec`. An
enum, not a trait object: there are two transports, both known, and an
exhaustive match is the point.

`sirio_usage` depending on `sirio_claude` is leaf-to-leaf; the crate graph in
`CLAUDE.md` gains one line and one arrow and no crate moves level.

## §3 Resolution and fallback

`AgentAdapter` gains a claim in the exact shape of `builtin_acp`:

```rust
fn native_chat(&self) -> Option<NativeChat> { None }
pub struct NativeChat { pub program: &'static str, pub min_version: ClaudeVersion }
```

Only `ClaudeCodeAdapter` answers, with `"claude"` and **2.1.257** — the
Claude Code the official wrapper's SDK bundles and therefore certifies the
protocol against. The claim is static; §9's conformance test keeps it honest
the way `oh_my_pi_is_only_claimed_once_it_answers` keeps omp's.

`AgentLaunchState` resolves Claude in this order, and records why:

1. `SIRIO_CLAUDE_TRANSPORT=acp` in the environment → skip to 4. A diagnostic
   valve in the spirit of `SIRIO_ACP_PROGRAM`, not a setting, not in the UI.
2. `native_chat()` is `Some` and `claude` resolves on PATH
   (`sirio_agents::availability()` already computes this).
3. `claude --version` parses and is ≥ the floor. Read once per resolution on
   a background thread with a bounded wait, the way `acp_conformance` drives
   a handshake; the result is cached in `AgentLaunchState` beside `sources`.
   → `LaunchSpec::Claude`.
4. Otherwise the registry path exactly as today: `registry_id("claude") →
   "claude-acp"`, installed or installable, with its Install button.

There is no manual transport selector. The Settings → Agents row for Claude
Code shows what was resolved and why: `Native · claude 2.1.273`, or
`claude 2.1.200 is older than 2.1.257 — using the ACP wrapper`, or `claude
is not on PATH — using the ACP wrapper`, each with the wrapper's own
Installed badge or Install control beside it. A fallback is never silent.

The chat tab persists `agent_id` as it does now; the transport is not
persisted per tab and is resolved again on every open.

## §4 The session

**Launch.** `ClaudeClient::launch(ClaudeLaunch, cwd)` runs the §1 line, with
`--resume <id>` when `resume` is set, and sets one environment variable,
`CLAUDE_CODE_ENABLE_SDK_FILE_CHECKPOINTING=true` (§6). `CLAUDE_CODE_ENTRYPOINT`
is left unset; the spike ran without it. User, project and local settings are
the CLI's own to load, exactly as the wrapper lets it: the hooks, MCP servers,
plugins and skills a chat sees are the ones the pane sees.
`discover_mcp_servers` is not consulted on this path — `claude` reads
`.mcp.json` itself — and stays for ACP.

**Handshake.** The first line out is `control_request initialize` with
`hooks: {}`. Its response yields, before any prompt:

- `AcpEvent::ModelCatalog { config_id: "model", options, selected_id }`, the
  selection `default` until the first `system/init` names the real model;
- `AcpEvent::AvailableCommands`, minus `terminal_slash_commands` and minus the
  set the wrapper also hides because they are the terminal's business:
  `clear`, `cost`, `keybindings-help`, `login`, `logout`, `output-style:new`,
  `release-notes`, `todos`;
- `AcpEvent::Effort { option_id: "effort", choices: low, medium, high, default }`;
- the mode catalogue: `default`, `acceptEdits`, `plan`, `auto`, current from
  `current_permission_mode`. `bypassPermissions` is not offered: it needs
  `--allow-dangerously-skip-permissions`, which Sirio does not pass.

The startup wait is 30 s, not 120: nothing has to be downloaded first.

**Authentication, settled before the first prompt.** Verified against an
empty `CLAUDE_CONFIG_DIR`: a logged-out CLI answers `initialize` with
`account: {tokenSource: "none", apiProvider: "firstParty"}`, and a prompt then
ends in `result` with `is_error: true` and `Not logged in · Please run
/login`, exit status 1. The client reads `tokenSource == "none"` with a
first-party provider at the handshake and raises the existing
`AcpError::AuthRequired` (F-CHAT-02), so the card with the CLI login guidance
appears before the user types. One trap, recorded here so nobody trips on it:
`system/init.apiKeySource` is `"none"` for a **logged-in** OAuth session too;
it says where an API key came from, not whether anyone is logged in.

**Outbound messages → events.** With partial messages on, streamed text
arrives twice — as deltas and again inside the final `assistant` message —
so `assistant` contributes only its `tool_use` blocks.

| Line | Event |
|---|---|
| `stream_event` `text_delta` | `AgentMessageChunk` |
| `stream_event` `thinking_delta` | `ThoughtChunk` |
| `assistant` with a `tool_use` block | `ToolCallStarted` — title, kind, locations, content from the tool table below; `raw_input` the block's input; status `InProgress` |
| `control_request can_use_tool` | `ToolCallUpdated { status: Pending }` for that `tool_use_id`, then the `PermissionRequest` (below); an allow answer emits `ToolCallUpdated { status: InProgress }`, a deny lets the CLI's own errored `tool_result` close the call. Should the request precede its `assistant` block, the client emits `ToolCallStarted` from the request's `tool_name` and `input` and the later block does not repeat it |
| `user` with `tool_result` | `ToolCallCompleted` — `Failed` when `is_error`, else `Completed`; content from `tool_use_result` (diffs below); `raw_output` the result |
| `assistant` `TodoWrite` / `TaskCreate` / `TaskUpdate` input | additionally `PlanUpdate` (`todos[]`, or the running task list, as the wrapper's `planEntries`/`taskStateToPlanEntries`) |
| `system/status` | updates the live mode cell, then `OtherSessionUpdate { kind: "CurrentModeUpdate(<mode>)" }` — the same signal the ACP arm emits |
| `system/init` | records `session_id`, `claude_code_version`, the real model; re-emits `ModelCatalog` if the selection moved; each `mcp_servers[].status == "failed"` becomes an MCP warning (F-CHAT-33) |
| `system/commands_changed` | `AvailableCommands` |
| `tool_progress`, `task_notification`, `compact_boundary`, `auth_status`, `rate_limit_event`, `keep_alive`, anything unknown | `OtherSessionUpdate { kind }` |
| `result` | `TokenUsageBreakdown` summed over `modelUsage`, then `ContextUsage` (§7), then `TurnEnded { stop_reason: subtype }` |
| stdout EOF or process exit | `TransportError`, with the stderr tail and `describe_exit` |

Messages with `parent_tool_use_id` set belong to a subagent. Without
`--forward-subagent-text` the CLI does not forward their text, which is what
the chat tab sees today through a wrapper whose subagent routing Sirio never
opted into. Parity, on purpose (§10).

**The tool table** (`sirio_claude::tools`, ported from the wrapper's
`tools.js` and pinned by one test per row). Kinds are the `ToolKind` names
the surface already keys icons and verbs on.

| Tool | Kind | Title | Locations / content |
|---|---|---|---|
| `Task`, `Agent` | Think | `description` or `Task` | `prompt` as text |
| `Bash` | Execute | `command` | `description` as text |
| `Read` | Read | path, with `(offset - end)` when limited | path, line `offset` |
| `Edit`, `NotebookEdit` | Edit | path | path; diff `old_string → new_string` at start |
| `Write` | Edit | path | path; diff `None → content` at start |
| `Glob`, `Grep` | Search | `pattern` | `path` when given |
| `WebFetch`, `WebSearch` | Fetch | `url` / `query` | — |
| `TodoWrite`, `Task{Create,Update,List,Get}` | Think | `Update TODOs: …` | plus `PlanUpdate` |
| `ExitPlanMode` | SwitchMode | `Exit plan mode` | `plan` as text |
| `Skill` | Other | skill name | — |
| `mcp__<server>__<tool>` | Other | `<server>: <tool>` | — |
| anything else | Other | tool name | — |

**Diffs** (F-CHAT-31/32). At `ToolCallStarted` an `Edit` already carries
`old_string`/`new_string` and a `Write` its `content`, so a `ToolCallDiff`
exists from the first frame. At `ToolCallCompleted` the `tool_use_result` for
`Edit`/`Write` carries `filePath`, `originalFile` (whole file before, or
`null` for a new one) and `structuredPatch` (hunks); the diff is replaced by
`originalFile` as `old_text` and the hunks applied to it as `new_text`. An
empty patch with a null original — a `Write` whose previous content was too
large to diff, which the SDK documents — keeps the start-frame diff.

**Permissions.** `control_request can_use_tool` → `AcpEvent::PermissionRequest`
with the tool's title from the table and up to three options:

| Option | ACP kind | Response |
|---|---|---|
| Allow | `allow_once` | `{behavior: "allow", updatedInput: input}` |
| Always allow | `allow_always` — only when `permission_suggestions` is non-empty | the same, plus `updatedPermissions: permission_suggestions` |
| Reject | `reject_once` | `{behavior: "deny", message: "The user rejected this action."}` |

`cancel_permission`, and the 5-minute expiry, answer `deny`. The reply is a
`control_response` carrying the `PermissionResult` under the request's
`request_id`; the wait runs off the reader thread exactly as the ACP arm's
`connection.spawn` does, so a card left open never blocks the stream.

Two tools ride the same request and are shaped before the event goes out:

- **`AskUserQuestion`** arrives as `can_use_tool` for that tool name; its
  `input.questions` folds through the existing `parse_permission_question`
  into `PermissionQuestion` (header, prompt, text input). The answer is
  `allow` with `updatedInput = input ∪ {answers}`, `answers` keyed by each
  question's text — a typed answer wins over a picked option — the shape the
  wrapper's `applyAskElicitationResponse` produces and the tool reads back.
- **`ExitPlanMode`** offers `Approve, auto-accept edits` (→ allow, then
  `set_permission_mode acceptEdits`), `Approve, ask for each edit` (→ allow,
  then `set_permission_mode default`) and `Keep planning` (→ deny). The
  `system/status` echo is what moves the mode pill, as for any mode change.

**Commands from the surface.**

| `ChatClient` call | Line to the CLI |
|---|---|
| `prompt_content(text, mentions, images)` | `user` with a fresh `uuid` (kept for §6); `text` block, then one `image` block per attachment (`source: {type: "base64", media_type, data}`); each mention becomes `[@name](file://absolute-path)` in the text, the wrapper's own rendering |
| `set_model(_, value)` | `set_model { model: value }` (`"default"` resets) |
| `set_mode(id)` | `set_permission_mode { mode: id }` |
| `set_config_option("effort", v)` | `apply_flag_settings { settings: { effortLevel: v } }`, `null` for default |
| `cancel()` | `interrupt`; the turn still ends with its own `result` |
| `shutdown()` | close stdin (EOF ends a stream-json session), wait 5 s, then terminate and reap within 2 s |

The prompt idle window stays 10 minutes, restarted on every line the CLI
writes. Slash commands need nothing new: text that starts with `/` goes out
as text, and the CLI runs the ones it accepts in `-p` mode.

## §5 Resume

The client learns the Claude session id from the first line that carries one
(the `system/init` of the first turn, or a `system/status` if a mode was set
first), stores it in a live cell behind `ChatClient::session_id()`, and
signals it with `OtherSessionUpdate { kind: "SessionIdentified" }` — the
re-read pattern `CurrentModeUpdate` already uses, so `AcpEvent` does not
widen. The surface re-reads the id and emits a new `ChatEvent::SessionIdentified`
to the shell that owns the database, which saves it at once — a hook write,
like `save_session_ref`, never batched with the tab-strip save.

Persistence: one migration, `ALTER TABLE tab ADD COLUMN agent_session_id
TEXT;`, and `TabRecord.agent_session_id: Option<String>`, `None` for every
tab that is not a native Claude chat. `ChatSessionSummary` does not grow — the
history browser has no use for the id.

On restore, `restored_chat_spec` produces `ClaudeLaunch { resume: Some(id) }`
when the row has an id, the resolved transport is native, and
`session.resumeAgentSessions` is on — the same F-SET-04 setting that decides
whether a pane resumes its own session. The visible transcript keeps coming
from persistence; the CLI replays nothing. The re-paste path
(`transcript_for_resume`) stays for ACP agents and is not used on a native
resume.

A refused resume — the session file is gone, or the CLI rejects the id — is
never a dead tab. The client treats an exit or an error response before the
`initialize` answer on a `--resume` launch as a refusal, relaunches without
`--resume`, and clears the stored id; the tab opens fresh with its history on
screen. The refusal's exact wire shape is captured into the fixtures in the
first implementation task rather than guessed here.

## §6 Rewind

Every user message leaves with a `uuid` Sirio generated. The surface keeps
it on that turn's entry in memory, for the life of the connection; it is not
persisted. Checkpoints across a restart-and-resume are §10's business.

The affordance lives on the turn rail's `TurnPreview` — the card that appears
over a tick — as *Restore files to this message*, shown only when
`ChatClient::supports_rewind()` is true, no turn is streaming, and the entry
has a uuid from the current connection. The flow:

1. `rewind_files { user_message_id, dry_run: true }` → a confirmation card
   listing `filesChanged` with `insertions`/`deletions`, or, when `canRewind`
   is false, the CLI's `error` sentence and no button.
2. Confirm → `rewind_files { user_message_id }` → a system entry in the
   transcript naming the files restored (and `skippedLinks` when non-zero),
   or the error.

It is a request/response pair on the command channel, answered through a
oneshot the surface awaits on the background executor — not a new
`AcpEvent`, so the exhaustively matched enum does not widen. It restores
files, not the conversation, and the label says *files*.

## §7 Usage, structured

Two consumers, one protocol.

**The chat's context meter.** After each `result` the client sends
`get_context_usage { detail: "summary" }` — the variant that answers from the
last response and local estimates, with no token-count API calls — and
emits `ContextUsage { used: totalTokens, size: rawMaxTokens, cost:
Some(total_cost_usd, "USD"), input_tokens, output_tokens, cached_read_tokens }`
with the token fields summed from `modelUsage`. If the request errors — a CLI
that predates it — the client falls back to what the wrapper computes:
`modelUsage[model].contextWindow` for `size` and the last `assistant`
message's `usage` (input + cache read + cache creation + output) for `used`.
Either way the meter never shows a number nobody computed: no answer means
`None`, as F-CHAT-18 already requires.

**The status bar.** `sirio_usage::claude` gains `NativeUsageFetcher`, which
replaces the hidden PTY that drives `/usage` and scrapes the panel:

1. spawn the §1 line plus `--no-session-persistence --setting-sources ""`
   (verified to write nothing and fire nothing);
2. `initialize`, then `get_usage`; close stdin; 15 s total, against the
   scraper's 25;
3. map `rate_limits.five_hour` → the `5h` session window, `seven_day` → `wk`,
   the `model_scoped` entry whose `display_name` is `Fable` → `fable_weekly`,
   `resets_at` parsed from ISO 8601; no monthly window, Claude has none.

Outcomes: `rate_limits_available: false` → `Unavailable(ApiKey)`;
`account.tokenSource == "none"` → `Unavailable(LoggedOut)`; `claude` not on
PATH → `Unavailable(NotInstalled)`; a timeout keeps the last good value
visible as stale, the crate's standing contract. The PTY scraper is kept only
as the path for a `claude` below the §3 floor, and is marked for removal the
day that floor is the floor everywhere.

## §8 Failure and drift

- **Bounded waits**, with the existing `TimeoutOperation` names: startup
  30 s, prompt 10 min idle, permission 5 min, shutdown 5 s, reap 2 s.
- **A dead process** is a `TransportError` naming the exit status or signal
  and the last stderr lines; open permissions are answered `deny`, as
  `cancel_permissions` does today.
- **A malformed line** — not JSON, or no `type` — is logged and skipped. It
  never ends the session and never hangs it; the ACP arm has the same test
  (`malformed_frame_is_rejected_without_hanging_the_session`) and so will
  this one.
- **A newer CLI** has no upper bound. Unknown fields are ignored; unknown
  `type`/`subtype` become `OtherSessionUpdate`; a `control_response` with
  `subtype: "error"` to a request the CLI does not know degrades that one
  feature — rewind reports unsupported, the meter falls back to the estimate
  — and never the session. `claude_code_version` from `system/init` is
  recorded and shows in the death report and the Settings row, so a
  regression on a version is a sentence, not a mystery.
- **A `result` with `is_error`** whose text is the login error takes the
  `AuthRequired` path; any other error text is shown as an agent message so
  the user reads it, and the turn ends with `stop_reason` = the subtype.
- **Perf**: a `sirio_perf::span` at the top of the reader loop and an
  `event` where an `AcpEvent` is sent, with static names (`claude.line`,
  `claude.event`) — never a title, path or prompt in a label. The chat's
  existing `acp_redraw_trace_names_notifications_without_recording_content`
  test gets a native sibling.
- **Valves**: `SIRIO_CLAUDE_PROGRAM` points the launch at a fixture
  (integration tests only, like `SIRIO_ACP_PROGRAM`);
  `SIRIO_CLAUDE_TRANSPORT=acp` forces the fallback (§3).

## §9 Tests

1. **`sirio_claude`**, all pure. Fixture files under `tests/fixtures/` are
   real lines captured from the installed CLI with the spike's probes —
   the `initialize` response, `system/status`, `system/init`, `assistant`
   with text and `tool_use`, `user` with `tool_use_result`, `result`,
   `can_use_tool`, `get_usage`, `get_context_usage`, `rewind_files` —
   one test per kind; the argv builder; `ClaudeVersion` parse and floor;
   one test per row of the tool table; the diff reconstruction from
   `originalFile` + `structuredPatch`; and a tolerance test that feeds every
   fixture with extra keys and an unknown subtype and asserts nothing errors.
2. **`sirio_acp`**: `tests/fixtures/claude_fixture.py`, a deterministic fake
   `claude` speaking stream-json with the same mode switch as
   `acp_fixture.py` — normal turn, tool with permission, structured question,
   plan-mode exit, cancel, death mid-turn, logged-out, refused resume,
   rewind — and `claude_integration.rs` mirroring `acp_integration.rs`
   test for test: chunks, permission round trip, an open permission does not
   freeze the session, cancel gives a terminal outcome, shutdown reaps,
   malformed line, death names stderr. `chat_integration.rs` gains the same
   scenarios through `LaunchSpec::Claude`.
3. **`sirio_control`**: the chat control-socket tests run once per transport.
4. **`sirio_ui`**: `from_test_command` takes a `LaunchSpec`; the rewind
   affordance's three visibility conditions and both card outcomes; restore
   with an `agent_session_id`; the perf-name test's native sibling.
5. **`sirio`**: resolution — native with `claude` present and recent, ACP
   when absent, ACP when old, ACP under the valve — and the sentence the
   Agents row shows in each case; `restored_chat_spec` with and without
   `session.resumeAgentSessions`.
6. **`sirio_agents`**: `claude_answers_the_native_handshake_it_claims`, the
   shape of `opencode_answers_the_acp_handshake_it_claims` — SKIP when
   `claude` is not on PATH, otherwise drive `initialize` and assert the
   claim and the floor against the binary. Added to the `ci` and `pr`
   `default-filter` in `rust/.config/nextest.toml` beside its two siblings,
   for the reason written there: a gate must not depend on which binaries
   a runner carries.
7. **`sirio_persistence`**: the column, through the forward-migration test
   that plants rows and asserts they survive.
8. **`sirio_usage`**: the `get_usage` mapping from captured JSON, including
   `rate_limits_available: false` and a missing `model_scoped`; the fetcher
   against the python fake, as the other providers are tested against
   theirs.
9. **Live, ignored**: `real_claude_native.rs` beside `real_claude.rs` —
   real credentials, `SIRIO_CLAUDE_PROGRAM`, a nonce file written through a
   permission, then a rewind that removes it.

`Scripts/ci.sh` runs only on the maintainer's request, as `CLAUDE.md` says.

## §10 Deliberately absent

- **Subagent text** (`--forward-subagent-text`, `parent_tool_use_id`
  routing): the tab does not show it today, and showing it is a transcript
  design of its own.
- **Open a chat in a terminal** (`claude --resume <id>` in a pane) and the
  reverse: the id is now persisted, so this is a small follow-up, not part
  of this change.
- **In-process hooks and MCP** (`hook_callback`, `mcp_message`): would let
  the chat feed the activity model without `sirioctl` and expose Sirio's
  tools without a process. Separate spec.
- **Checkpoints after a restart**: rewind is offered only for messages of
  the live connection until a probe shows what `--resume` keeps.
- **`generate_session_title`** for auto-naming, replacing `claude -p`.
- **Removing the PTY scraper** once the §3 floor is universal.
- **Renaming `AcpEvent`** to a transport-neutral name.
- **A manual transport selector**: the fallback is automatic and explained;
  a setting would be a second way to reach a state the row already names.

## §11 Order of delivery

One spec, three shippable steps, each green on its own:

1. **Parity.** `sirio_claude`, `ClaudeClient`, `ChatClient`/`LaunchSpec`,
   resolution with the automatic fallback, the Settings row, and every
   §4 behaviour. A Claude chat tab stops needing Node.
2. **Resume.** The column, the id signal, `restored_chat_spec`, the refusal
   relaunch.
3. **Rewind and usage.** The rail affordance and cards; `get_context_usage`
   in the meter; `NativeUsageFetcher` in the status bar.

Nothing in 2 or 3 changes a decision made in 1; a release may land between
any two.
