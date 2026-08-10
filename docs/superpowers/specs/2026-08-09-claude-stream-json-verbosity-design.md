# Claude stream-json: surface the protocol data the driver currently drops

**Date:** 2026-08-09
**Status:** Approved
**Package:** `TillerACP` (no SwiftUI dependency — all new types are data)

## Goal

`ClaudeStreamJSONDriver` speaks Claude Code's `stream-json` protocol but
consumes a small fraction of it. Four categories of data are lost: control
requests that are never answered, message types decoded as `.unknown`, fields
decoded and then discarded, and fields never decoded at all.

This spec covers wiring that data through to Tiller's canonical session event
stream, plus replacing the driver's simulated "effort" control with the real
one the CLI now exposes.

## Evidence method

Every claim below was verified against **Claude Code 2.1.226** on
2026-08-09, not against documentation. The official Agent SDK reference is
inaccurate on field names (it documents `system_type`; the wire uses
`subtype`) and was not used as a source.

Two evidence sources:

1. **Live stream capture** — a two-step session run with
   `--include-partial-messages --include-hook-events --forward-subagent-text
   --prompt-suggestions`, producing 92 NDJSON lines across 17 distinct
   message shapes.
2. **Binary string extraction** — the shipped Mach-O binary at
   `~/.local/share/claude/versions/2.1.226`, used to recover closed unions
   (control request subtypes, effort levels) that the capture did not
   exercise.

Where the two disagreed with prior assumptions, the assumptions lost. Three
were wrong: that Claude has no native effort concept, that a `success`
control response means a setting was applied, and that effort levels are a
fixed list.

## Current state

`Packages/TillerACP/Sources/TillerACP/Drivers/ClaudeStreamJSONDriver.swift`

| Line | Behaviour |
|---|---|
| `:76` | Launch command passes only `-p --input-format/--output-format stream-json --verbose --permission-mode --model --session-id`/`--resume` |
| `:186` | Comment asserts "Claude has no native effort concept" — no longer true on 2.1.226 |
| `:188` | Effort levels hardcoded to `low`/`medium`/`high` |
| `:248` | `didEmitCommands` latch fires once per session |
| `:285` | `guard request.subtype == "can_use_tool" else { return }` — exits without sending a `control_response` |
| `:376` | `get_context_usage` control request probed after every turn |
| `:416` | `modelState(from:)` reads `value`/`displayName`/`description`, discards the rest |
| `:524` | `effortPrefix` prepends prose to the user's prompt |

`ClaudeWire.swift:38` maps every unrecognised message type to `.unknown`.

## Inventory of unused protocol surface

### Control requests the CLI sends to Tiller (these require a response)

Union recovered from the binary: `"can_use_tool"`, `"request_user_dialog"`,
plus `hook_callback` and `mcp_message`.

Only `can_use_tool` is handled. The others hit the `guard` at `:285` and
return without emitting a `control_response`. The protocol correlates every
request to a response by `request_id`; a silent return leaves the CLI
waiting. This is a correctness defect, not a missing feature.

### Control requests Tiller can send

Used: `initialize`, `interrupt`, `set_permission_mode`, `set_model`,
`get_context_usage`.

Unused, from the recovered union: `apply_flag_settings`,
`set_max_thinking_tokens`, `rename_session`, `set_color`, `mcp_authenticate`,
`mcp_oauth_callback_url`, `mcp_reconnect`, `reload_plugins`, `side_question`,
`set_cwd`, `rewind_files`, `seed_read_state`, `read_file`.

Only `apply_flag_settings` is in scope here. The rest are recorded so the
next person does not have to re-derive the union.

### Message types decoded as `.unknown`

Top-level: `stream_event`, `rate_limit_event`, `prompt_suggestion`,
`task_progress`, `commands_changed`.

`system` subtypes: `status`, `thinking_tokens`, `compact_boundary`,
`notification`, `informational`, `model_fallback`, `model_refusal_fallback`,
`model_consent_fallback`, `model_refusal_no_fallback`, `permission_denied`,
`task_notification`, `worker_shutting_down`, `hook_started`, `hook_progress`,
`hook_response`.

### Fields decoded and discarded

- `ClaudeResult.usage` — decoded at `ClaudeWire.swift:306`, never read
- `ClaudeMessage.stopReason`, `stopDetails`
- `ClaudeUserMessage.toolUseResult` — the structured form of a tool result
- `ClaudeToolUse.caller`

### Fields never decoded

`init` message: `cwd`, `mcp_servers[]` with per-server `status`
(`connected`/`pending`/`needs-auth`), `permissionMode`, `apiKeySource`,
`claude_code_version`, `output_style`, `agents[]`, `skills[]`, `plugins[]`,
`capabilities[]`, `memory_paths`, `fast_mode_state`.

`initialize` control response: `account`, `available_output_styles`,
`current_permission_mode`, `output_style`, `pid`, `fast_mode_state`, and the
per-model capability fields described under Phase 2.

`result` message: `total_cost_usd`, `duration_ms`, `duration_api_ms`,
`ttft_ms`, `ttft_stream_ms`, `time_to_request_ms`, `num_turns`, `subtype`,
`terminal_reason`, `permission_denials`, `api_error_status`, `result`, and
`modelUsage[model]` — which carries `contextWindow` and `maxOutputTokens`.

## Scope

**In scope:** Phases 1–5 below.

**Out of scope, by explicit decision:**

- `--include-partial-messages` (token-level streaming). Deferred to its own
  piece of work. It is the only item that can regress behaviour that
  currently works, and this repo has already seen a UI freeze caused by
  per-token events from native drivers (coalescing fix `e3b64ab`). Bundling
  it with ~20 additive changes would make a regression untraceable.
- UI for the diagnostic channel (Phase 4). This spec wires the signals to the
  edge of `TillerACP` and stops. The panel gets its own brainstorming,
  because there is no stated use case yet for *when* a user would open it,
  and designing it now would be designing blind.

## Design

### Phase 1 — Correctness

No new UI. These are defects.

1. **Answer every control request.** Replace the `:285` guard with a switch
   that handles `can_use_tool` as today and sends an explicit
   `control_response` for `request_user_dialog`, `hook_callback`, and
   `mcp_message`. An explicit refusal is a valid response; silence is not.
   Unknown future subtypes must also get a response rather than falling
   through.

2. **Stop latching the command list.** Remove `didEmitCommands` (`:248`) and
   handle `commands_changed`. Today, after a `reload_plugins` or a skill
   install, Tiller's slash-command list is stale for the rest of the session.

3. **Read the real context window.** Take `modelUsage[model].contextWindow`
   from the `result` message. Keep the `get_context_usage` probe (`:376`) as
   a fallback for CLI versions that do not report it, but stop treating it as
   the primary source.

### Phase 2 — Real effort, derived per model

The `initialize` control response carries per-model capability data that
makes a hardcoded level list unnecessary:

```json
{"value":"default","resolvedModel":"claude-opus-5[1m]",
 "displayName":"Default (recommended)",
 "supportsEffort":true,
 "supportedEffortLevels":["low","medium","high","xhigh","max"],
 "supportsAdaptiveThinking":true,"supportsFastMode":true,"supportsAutoMode":true}
```

Measured variance across the five models the CLI reports: `default`,
`opus[1m]`, `claude-fable-5[1m]` and `sonnet` all support five levels;
**`haiku` reports `supportsEffort: null` and no levels at all.**

4. **Set effort natively.** `--effort <level>` at launch;
   `apply_flag_settings` with `{"settings":{"effort":"<level>"}}` for
   mid-session changes. Both verified working against 2.1.226.

5. **Derive the level list from the selected model**, from
   `supportedEffortLevels`. Where `supportsEffort` is falsy, show no effort
   control at all — a picker that does nothing on Haiku is worse than no
   picker.

6. **Validate client-side.** `apply_flag_settings` performs no validation: a
   request carrying `{"effort":"bogus_level"}` returns `subtype: "success"`
   exactly like a valid one. `success` means *received*, not *applied*. Tiller
   is the only place in the chain where a bad value can still be caught, so
   the level must be checked against the model's reported list before
   sending.

   (The launch-time `--effort` path does warn: its arg parser returns
   `{level, warning}` and writes the warning to **stderr** before coercing.
   `launchTransport` already accepts an `onStderrLine` callback at `:75`, so
   that warning is capturable.)

7. **Make `staticEffortOptions()` dynamic for Claude**, and rewrite the
   comment at `:186`. It documents the opposite of what will now be true, and
   left in place it will cause the same wrong conclusion to be drawn again.

8. **Delete `effortPrefix`** (`:524`) and its call site in `promptBlocks`.
   Selecting "High" currently prepends *"Reasoning effort for this and
   following turns: high."* to the user's prompt — a polite request to the
   model, not a setting.

`set_max_thinking_tokens` exists as a separate control request and is a
distinct axis from effort. Noted, not in scope.

### Phase 3 — Inline telemetry

Deliberately small. The selection criterion is whether a user would want the
signal interrupting the conversation.

9. **End-of-turn stats** from the `result` message: `total_cost_usd`,
   `duration_ms`, `ttft_ms`, `num_turns`, and token counts from `usage`.
   Needs one new `SessionUpdate` case and its transcript rendering.

10. **Inline notices**, reusing the existing
    `TranscriptItem.systemNotice` (`TranscriptItem.swift:146`) rather than
    introducing new item types: `compact_boundary` (context was compacted),
    `model_fallback` / `model_refusal_fallback` / `model_consent_fallback`
    (the responding model was not the requested one), `permission_denials`
    from the `result`, and `rate_limit_event` **only** when `status` is not
    `"allowed"`.

### Phase 4 — Diagnostic channel (wiring only)

11. **Decode and forward** `hook_started`/`hook_progress`/`hook_response`,
    `status`, `thinking_tokens`, `task_progress`, `informational`,
    `notification`, and the `init`/`initialize` metadata (MCP server names
    with status, CLI version, `output_style` + `available_output_styles`,
    `account`, `pid`).

    These reach the edge of `TillerACP` and are not rendered in the chat.

12. **Do not pass `--include-hook-events` by default.** The capture produced
    27 `hook_started` + 28 `hook_response` events for a two-step turn. It
    stays behind an explicit opt-in.

Note on classification: `thinking_tokens` lands here rather than in Phase 3
despite looking valuable, because the capture shows `thinking` content blocks
arriving **empty** (`"thinking":""`) with only `estimated_tokens` populated.
A token counter for reasoning text that cannot be read is telemetry, not
content. This also means Claude's thinking rows in the chat are structurally
empty today and cannot be filled — only counted.

### Phase 5 — Structured tool results

13. **Use `ClaudeUserMessage.toolUseResult`**, which is already decoded and
    discarded. It carries the structured form of each result where the
    rendered `content` carries only flattened text: `structuredPatch` for
    Edit, `{filePath, content, numLines, startLine, totalLines}` for Read,
    separated stdout/stderr for Bash, todo arrays for TodoWrite.

## Testing

`swift-testing` (`@Test`/`#expect`), decoder-level, per the repo convention.

**Fixtures come from the real capture, not hand-written JSON.** The captured
stream contains shapes neither author would have invented: an empty
`thinking` block, `supportsEffort: null`, `cache_creation` split across
ephemeral windows.

**Fixtures must be sanitized before they enter the repo.** The raw capture
contains `account.email`, `account.organization`, internal MCP server names
(including Unipol-internal ones), absolute paths containing the username, and
the full local plugin and skill inventory. Sanitization is a prerequisite of
committing them, not a follow-up.

Coverage required:

- Every message type in the Phase 4 list decodes without falling through to
  `.unknown`
- Every control request subtype produces a `control_response` (Phase 1.1)
- Effort options derive correctly from `supportedEffortLevels`, and resolve
  to *no options* for a model reporting `supportsEffort: null`
- An effort value outside the model's reported list is rejected before send
- `contextWindow` is read from `modelUsage` when present, falling back to the
  `get_context_usage` probe when absent

`Scripts/ci.sh` must print `CI OK`. Note the standing caveat recorded for
this repo: several suites are red on baseline as of 2026-08-06, so a diff is
exonerated by comparing two runs on the same tree, not against `HEAD`.

## Risks

- **`apply_flag_settings` silently accepts anything.** Mitigated by
  client-side validation (Phase 2.6), but there is no way to confirm from the
  CLI that a setting took effect. If a future change makes effort appear not
  to apply, this is the first place to look.
- **Recovered unions are version-pinned to 2.1.226.** They came from string
  extraction on a shipped binary, not from a published schema. A CLI upgrade
  can add subtypes. Phase 1.1's requirement that unknown subtypes still
  receive a response is what keeps that from becoming a hang.
- **`--effort` is a launch flag with no `set_effort` counterpart.** Mid-session
  changes depend entirely on `apply_flag_settings` continuing to accept the
  `effort` settings key.
