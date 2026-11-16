# Claude protocol parity: what Claude Code 2.1.325 serves that a native chat ignores

Status: proposed, 2026-11-15. Eleven items (A1–A11), delivered one at a time.

## Intent

Sirio's native Claude chat speaks Claude Code's stdio protocol (stream-json plus the SDK control
protocol) and was certified against Claude Code 2.1.257. The current release is 2.1.325, with Agent
SDK 0.3.325. An audit on 2026-11-15 compared the two:

- Of the control requests the CLI sends to its host, Sirio answers only `can_use_tool`
  (`sirio_chat/src/claude/worker.rs:935`). Any other request goes unanswered.
- Several `system` messages and host→CLI verbs that the CLI already serves are ignored. Some exist
  at 2.1.257; others arrived later.

The goal is to make what the CLI already serves usable in a Sirio chat, so a user does not have to
drop to Claude Code's terminal for it. Enzo picked the eleven items below from the audit's
"already on the wire" bucket. They are delivered in order, A1 to A11, each in its own worktree and
stacked PR.

The audit's working notes are in `.superpowers/brainstorm/2026-11-15-claude-parity/` (gitignored):
the CLI changelog digest 2.1.258→2.1.325, the SDK type diff 0.3.257→0.3.325, and Sirio's coverage
map. The facts each item depends on are repeated here, so this document stands without them.

## Goals

- Each item works in a native Claude chat against Claude Code 2.1.325.
- Each item proves itself end to end against the fake `claude`. It leaves a re-runnable artifact.
  Where its claim is about the real CLI, a live conformance test against the real binary backs it.
- On a CLI between 2.1.257 and 2.1.324, each item degrades to today's behaviour. It never breaks the
  chat.

## Non-goals

- Claude Code's agent view and background sessions (`claude agents`, `claude --bg`), Code Mode, and
  anything cloud-hosted: Remote Control, `/schedule` in the cloud, teleport. These were buckets B
  and C of the audit, which Enzo did not pick.
- Raising `MIN_CLAUDE_VERSION` (2.1.257).
- The ACP fallback (`claude-acp`). It carries none of these features, and updating the installed
  wrapper (0.76.0, while the registry lists 0.107.0) is a separate change.
- The open native-chat bugs #696, #698 and #699. Items that touch the same code must not fix them
  in passing. They keep their own issues.
- Two gaps the audit found in the same surfaces stay out of scope, to be filed as issues:
  - AskUserQuestion reads only the first question (`sirio_chat/src/claude/permission.rs:119`).
  - Native plan approval probably does not attach to the Plan card (`sirio_ui/src/chat/mod.rs:3388`).

## Anti-goals: these would be failures even if the feature worked

- A chat on an older CLI that errors, hangs or switches transport because of a new request.
- MCP-server or hook text rendered as clickable or loadable content outside the existing allowlist
  (`sirio_ui::chat::file_links`).
- A title, path, prompt or server name interpolated into a `sirio_perf` label.
- A card that polls the CLI while nothing paints it.

## Constraints

- **Degrade, never reject** (ADR 0001; `docs/agents/architecture/claude-chat.md`):
  - An unknown `type` or `subtype` becomes `Other`, and unknown fields are ignored.
  - A host→CLI request that the CLI refuses ("Unknown control request subtype: …", already parsed
    by `ControlEnvelope`) retires that one feature for the session. The card or action it fed is
    hidden. No error is shown.
- **What counts as a refusal** (my call). The test is a pure function in `sirio_claude`. A
  `#[test]` proves it, with its failure list written first.
  - An error response whose text contains `Unknown control request subtype` or
    `Unsupported control request subtype` means the CLI does not know the verb. The
    2026-10-20 session-panel probe recorded the second wording. That feature is retired for the
    session.
  - Any other error (for example `Server not found: <name>`) is that one action's failure. It is
    shown beside the action, as the Connectors card already does, and the feature stays.
- **The floor stays at 2.1.257.** The table says, per verb or message, whether it exists at 2.1.257
  (checked in SDK 0.3.257's `sdk.d.ts`) or arrived later. Anything that arrived later needs the
  refusal path above or a `system/init.capabilities` flag.
- **Crate shape.** `sirio_claude` stays a leaf of serde types and pure functions. The process and
  its requests live in `sirio_chat::claude`. The UI never learns the transport: new data crosses
  as a `ChatEvent` (`sirio_chat_vocab`) or through a control handle shaped like `McpControl`,
  whose calls wait for the CLI and so run off the GPUI thread.
- **Readable by tests.** Every new piece of UI state is readable through `surface.chat.read`
  (`sessionPanel`, `composerRow` or a new field), so an E2E can assert on it.
- **Testing** (repo `CLAUDE.md`):
  - E2E first: a mode of `rust/crates/sirio_chat/tests/fixtures/claude_fixture.py`, a stub under
    `Scripts/Tests/stubs/<name>/claude`, and a scenario in a `Scripts/Tests/` script that prints its
    OK line.
  - Live claims go in `rust/crates/sirio_agents/tests/acp_conformance.rs`. Those tests are opt-in,
    SKIP without the binary, and stay off the release gate.
  - When a live claim needs a model turn, use the fake model API pattern of
    `sirio_term_session/tests/program_status_live.rs` and `tests/live/fake_messages.py`:
    `ANTHROPIC_BASE_URL`, a private `CLAUDE_CONFIG_DIR` and onboarding pre-seeded.
  - A `#[test]` only for a unit proven in isolation, with its failure list written first. No
    tautological or change-detector tests.
- **The gates.** Iterate with `cargo test -p <crate>`. `Scripts/ci.sh` runs before each PR. Enzo
  authorised that for this series (2026-11-15), and only that.

## Approaches considered

**Delivery** (chosen by Enzo):
- **One umbrella spec, one worktree and one stacked PR per item, A1→A11, one at a time.** Chosen.
  Each PR is reviewable on its own. A failing item blocks only what stacks on it.
- One branch for all eleven: unreviewable, and one stuck item blocks the rest.
- Parallel worktrees from `main`: the items share `worker.rs`, `events.rs` and `chat/mod.rs`, so
  they would conflict. Enzo also asked for one at a time.

**Old CLIs** (my call, from ADR 0001):
- **Keep the floor, degrade per feature.** Chosen.
- Raise the floor to 2.1.325: rejected. It sends every older CLI to the ACP wrapper, which carries
  none of these features and has no resume, to protect features that user may never touch.

**Plans:** written per item at its turn, not up front. Each item moves the code the next one plans
against.

## Groundwork, landing in A1 because A1 needs it first

- **A default answer to every inbound control request Sirio does not handle**: an error
  `control_response` naming the subtype. This is what the ADR's degrade rule implies. Today such a
  request waits for an answer that never comes.
  - It is safe for `request_user_dialog`: an error answer is discarded, and the CLI sends only the
    dialog kinds a host declared, which Sirio declares none of.
  - It is safe for `hook_callback` and `mcp_message`: Sirio registers no hooks (`hooks: {}`) and no
    SDK MCP servers.
- **`control_cancel_request`** (`{type, request_id}`): the CLI withdraws a request it sent. Sirio
  ignores it today. From A1 on, it closes the matching card, both elicitation and permission. It is
  in SDK 0.3.257.

## The items

Each item is laid out the same way:
- Wire: what the CLI serves, and whether it exists at 2.1.257 or is newer.
- Sirio: what Sirio does with it.
- Degrade: what happens on an older CLI.
- Proof: the tests that show it works.

### A1 · MCP elicitation

- **Wire.** The CLI sends `control_request` `elicitation` to the host. It exists at 2.1.257.
  - Fields: `mcp_server_name`, `message`, `mode: 'form' | 'url'`, `url?`, `elicitation_id?`,
    `requested_schema?` (a flat object of primitive properties, per MCP's elicitation spec), and
    `property_order?` (new). A permission-display title may also be present.
  - The host answers with an MCP `ElicitResult`: `{action: 'accept' | 'decline' | 'cancel',
    content?}`.
  - URL mode ends with `system/elicitation_complete` `{mcp_server_name, elicitation_id}`.
  - The 2026-07-28 MCP protocol is the default since 2.1.292, and it uses URL mode for server
    sign-in.
- **Step 0, a live probe before any UI.** Run the real `claude` against the fake model API,
  scripted to call a tool of a local stdio MCP server that elicits, once in form mode and once in
  URL mode. Record:
  1. whether the CLI sends `elicitation` to a stream-json host as is, or only after the host
     declares something in `initialize`;
  2. what an unanswered request does (the audit read "stays pending until the server times out"
     from the types, not from a live run);
  3. the response shape the CLI accepts.
  The plan adapts to what the probe shows.
- **What the probe found** (2026-11-15, Claude Code 2.1.325, with `initialize` sent as Sirio sends
  it):
  1. The CLI sends `elicitation` without being asked.
  2. Unanswered, the turn hangs. The CLI writes `tool_progress` every 30 s.
  3. An error reply reaches the server as `cancel`, and the turn goes on.
  4. `{action, content}` reaches the server verbatim.
  5. URL completion arrives as `system/elicitation_complete`.
  6. On `interrupt`, the CLI sends `control_cancel_request` for the pending elicitation.
- **Sirio.** A card above the composer, in the place the permission and question cards use.
  - It names the server and shows `message` as plain, selectable text.
  - Form mode: the question dock walks the form one field at a time, in `property_order` (else
    schema order). It reuses the dock's keys: arrows, Enter, digits, Esc.
    - Supported kinds are string (with `format` hints shown, not enforced), number, integer,
      boolean and enum, with required fields marked.
    - It ends on a review of the answers, with Accept and Decline. Decline is also offered at
      every step, and Esc is Cancel.
  - URL mode: the URL as text, with Open and Decline. Open is offered only for `http`/`https`. It
    opens the URL in the system browser, as the Connectors card's sign-in does, and accepts.
    `elicitation_complete` marks the card completed.
  - A schema Sirio cannot render shows the message, plus Decline with a line saying why.
- **Waiting** (my call). An open elicitation is waiting on the user, the same as an open
  permission card.
  - It suspends the prompt watchdog (`open_permissions` in `worker.rs` counts it).
  - It expires on the same clock as a permission card, answering `cancel`, which is MCP's
    "dismissed without a choice".
  - A `control_cancel_request` for it closes the card.
  - Not replayed: the `pending_elicitation_requests` that the `initialize` answer may carry (newer
    than 2.1.257). It serves a client that joins a process already running. Sirio starts its own
    process and initializes it before the first prompt, so nothing can be pending yet. The live
    probe's `initialize` answer did not carry the field.
  - Stop (interrupt) stays available throughout.
  - #699's watchdog bug is still not fixed here.
- **Degrade.** None needed, because the request exists at 2.1.257. `property_order` absent → schema
  order.
- **Proof.**
  - A fixture mode that sends a form elicitation and a URL one, and records the answers. The E2E
    reads the card from `surface.chat.read` and answers it through the debug surface, then checks
    what the fixture received.
  - Live: `claude_sends_mcp_elicitation_to_its_host` in `acp_conformance.rs`, with a stub MCP server.

### A2 · Session state

- **Wire.** `system/session_state_changed` `{state: 'idle' | 'running' | 'requires_action'}`. It
  exists at 2.1.257.
  - The CLI emits it only when `CLAUDE_CODE_EMIT_SESSION_STATE_EVENTS` is set. That is read from
    the SDK bundle; confirm it by probe.
  - `idle` fires only after background agents finish. It is the CLI's own "turn over" signal.
- **Sirio.**
  - Set the variable in `sirio_claude::launch`, beside `CLAUDE_CODE_ENABLE_SDK_FILE_CHECKPOINTING`.
  - Fold the state into a `ChatEvent`.
  - Where the chat derives running, waiting-for-you or idle today, the CLI's state wins once one
    has arrived. That covers the sidebar's status dot, the Sessions tab and the Trace card's
    headline.
  - #699's watchdog is not touched.
- **Degrade.** No `session_state_changed` arrives → today's derivation, unchanged.
- **Proof.**
  - A fixture mode that emits the three states, including `running` after the main `result` while
    an agent runs. The E2E asserts the chat's status through the surface.
  - Live: the CLI emits the states under the variable.

### A3 · Background tasks: output and Stop

- **Wire.**
  - `stop_task` `{task_id}` exists at 2.1.257.
  - `get_task_output` `{task_id}` → `{output, total_bytes, truncated}` (the last 8 KiB of a shell or
    Monitor task) is new.
  - `task_*` and `background_tasks_changed` gained `run_id?` and `parent_task_id?`.
- **Sirio.**
  - Each running background task gets Stop, in the Trace (card and tab) and in the composer's
    background-tasks popover.
  - Shell and Monitor tasks also get an output tail. It is read off the GPUI thread, about once a
    second, only while a frame paints it.
  - A non-`local_agent` task stops being "launch only" in the Trace. Its lane stays open until a
    terminal `task_updated` or `task_notification`, as an agent's does. Update `claude-chat.md`,
    which describes the current rule.
- **Degrade.** `get_task_output` refused → no tail. Stop still works.
- **Proof.**
  - A fixture mode with a background shell task that writes, then stops on `stop_task`. The E2E
    checks the tail text, Stop, and the lane's end.
  - Live: `get_task_output` on a real background `Bash` scripted by the fake model.

### A4 · Rename reaches Claude Code

- **Wire.** `rename_session` `{title, source?: 'host', session_id?}`. The verb exists at 2.1.257.
  `source` and `session_id` are newer. The probe checks that an older CLI ignores unknown fields.
  If it does not, send them only on CLIs that report a version at or above where they appeared.
- **Sirio.** Renaming a native Claude chat's tab (`TabContextAction::Rename`) sends
  `rename_session`. The title then shows in `claude --resume` and `claude agents`. This is one way
  only: Sirio does not adopt titles the CLI picks.
- **Degrade.** Refused → the rename stays Sirio's own, as today.
- **Proof.**
  - The fixture records the request. The E2E renames the tab through the debug surface.
  - Live: after the rename, the session's title as the CLI lists it.

### A5 · Permission detail and the rules card

- **Wire.**
  - `can_use_tool` carries `title?`, `display_name?`, `description?`, `decision_reason?`,
    `decision_reason_type?`, `blocked_path?`, `mcp_server?: {name, source}` (new),
    `default_to_no?`, `suppress_always_allow_rule?` and `requires_user_interaction?`. Sirio drops
    all of these today.
  - `list_permission_rules` `{}` → `{state: {rules: [{behavior, source, rule, description?,
    editability, notInEffect?}], workspaceDirectories, originalCwd, managedOnly, errors?,
    withheld?}}` is new.
- **Sirio.**
  - The permission card shows the CLI's title or display name, the reason line, the blocked path
    and the MCP server.
  - `default_to_no` focuses Reject and drops the number shortcut.
  - `suppress_always_allow_rule` hides "Always allow".
  - A read-only Permissions card in the session panel lists the rules by behaviour, with source and
    editability.
- **Degrade.** Missing fields → today's card. `list_permission_rules` refused → no card.
- **Proof.**
  - A fixture `can_use_tool` that carries the fields. The E2E checks the card's text and options,
    then the rules card.
  - Live: `list_permission_rules` answers on a configured `CLAUDE_CONFIG_DIR`.

### A6 · Reload and connector toggle

- **Wire.** All of these exist at 2.1.257, except where marked.
  - `reload_plugins` `{refuse_if_busy?, hold_on_cache_impact?}`. Its fields are newer, and
    `refuse_if_busy` is honoured only with `reload_plugins_refuse_if_busy_v1`.
  - `reload_skills` `{}`.
  - `reload_output_styles` `{}`, which is new.
  - `mcp_toggle` `{serverName, enabled}`.
- **Sirio.**
  - The Connectors card gets an on/off switch per server.
  - One "Reload plugins and skills" action, sent only while no turn runs. After it, the chat
    refreshes the slash-command list (`commands_changed` is already handled) and `mcp_status`.
- **Reload requests** (my call).
  - Sirio sends `hold_on_cache_impact: true`. It also sends `refuse_if_busy: true`, but only when
    `system/init.capabilities` lists `reload_plugins_refuse_if_busy_v1`. A1's groundwork does not
    parse capabilities, so A6 adds that.
  - An answer with `held: true` shows what the reload would cost the prompt cache
    (`cache_impact`), with "Reload anyway". That button sends the request again without the hold.
  - A busy refusal reads as "try again when the turn ends".
- **Connector switch** (my call). The switch does exactly what the CLI's `mcp_toggle` does, the
  same verb the terminal's `/mcp` uses. A6's live probe records whether that is saved to the
  user's config. The switch's tooltip says which: for this session, or saved for this project.
  This follows ADR 0001: a native chat uses the user's own configuration.
- **Degrade.** Any verb refused → its control is hidden.
- **Proof.**
  - A fixture mode that answers each verb and changes its command list. The E2E toggles a server
    and reloads.
  - Live: `mcp_toggle` on a local stdio server.

### A7 · Errors and notices

- **Wire.**
  - Existing notices (Compacted, BackgroundTaskEnded, RateLimited) cover only part of what the CLI
    reports.
  - At 2.1.257: `system/api_retry` `{attempt, max_retries, retry_delay_ms, error_status, error}`
    (`no_response?` is newer), `system/notification` `{key, text, priority, timeout_ms?}`, and
    `system/model_refusal_fallback` `{original_model, fallback_model, direction, scope?, content}`.
  - New: `system/session_writes_lost` `{error_code, lost[]}`, and `startup_failure_reason` (23
    values) plus `invalid_config_file` on an error `result`.
- **Sirio.**
  - `api_retry` is a transient line ("Retrying 2 of 10 in 4 s: overloaded"), cleared by the next
    content.
  - `notification` is a transient line, kept for its `timeout_ms`, ordered by priority.
  - `model_refusal_fallback` is a notice naming both models.
  - `session_writes_lost` is a persistent warning that the transcript was not fully saved.
  - A startup failure replaces the generic transport error with a sentence per known reason. An
    unknown reason is shown verbatim.
  - All of this text is plain and selectable.
- **Degrade.** Absent → as today.
- **Proof.**
  - A fixture mode that emits each message. The E2E asserts the notices.
  - Live: a startup failure that can be provoked locally, such as an invalid settings file
    (`config_file_invalid`).

### A8 · Hooks card

- **Wire.** `get_hooks_listing` `{}` → `{events[], hooks[], policy, safeMode?, bareMode?,
  errors?}`. It is new. Each hook carries `event`, `matcher`, `sourceLabel`, `type`,
  `displayText`, `commandText` and `disabled?`.
- **Sirio.** A read-only Hooks card in the session panel, grouped by event. Each row shows its
  label, its source and whether it is disabled. The full command text unfolds.
- **Degrade.** Refused → no card.
- **Proof.**
  - The fixture answers a listing. The E2E reads the card.
  - Live: a settings hook in a private `CLAUDE_CONFIG_DIR`, listed back.

### A9 · Usage and limits

- **Wire.**
  - `rate_limit_event` carries a status, a reset time and utilization. Sirio parses all three and
    shows only the status. `overageEnabled?` and `limitScope?` are newer.
  - The assistant message that answers `/usage` carries `usage_report?: SDKUsageReport` (new,
    experimental): the session totals, plus the plan's limits (`kind`, `group`, `percent`,
    `resets_at`, `severity`, `is_active`) and extra usage.
- **Sirio.**
  - The rate-limit notice shows the reset time and the scope.
  - `/usage` in a chat renders a card from `usage_report`, which a claude.ai subscriber session
    sends. Any other session shows the CLI's own text.
- **Degrade.** Absent → as today.
- **Proof.**
  - Fixture rate-limit events with the fields, and a `/usage` answer with a report. The E2E
    asserts both.
  - Live (my call): a fake model API cannot produce a `usage_report`, so the live test runs
    against the user's own sign-in.
    - It is opt-in through `SIRIO_LIVE_CLAUDE_ACCOUNT=1`, SKIPs without it, and never runs in CI.
    - It sends only `/usage`, which spends no model turn. It asserts the answer's shape, never its
      numbers.
    - The audit could not confirm that a slash command sent as user text runs in stream-json
      mode, so the test answers that first. If it does not run, `/usage` gets no card and the item
      keeps only the rate-limit part.

### A10 · Subagent text: designed with Enzo when it comes up

- `--forward-subagent-text` makes the CLI stream subagents' text and thinking. Spec 2026-09-18 §10
  left it out on purpose: "showing it is a transcript design of its own".
- Before any plan, a short design conversation settles where the text goes (the Trace lane, a box
  in the transcript, or both) and how much of it.

### A11 · MCP Apps widgets: designed with Enzo when it comes up

- `mcp_read_resource` `{serverName, uri}`, for `ui://` URIs only, returns HTML from a third party.
  It is new and gated on `mcp_read_resource_v1`.
- Tools declare their UI in `mcp_status` (`_meta.ui.resourceUri`, gated on `mcp_tool_ui_meta_v1`).
- That content cannot be trusted, and MCP Apps (SEP-1865) lets a widget ask its host to call
  tools. A short design conversation settles both: how the widget is isolated, given that Sirio's
  web engine hosts pages in child windows, and what the widget may ask for, if anything.

## Delivery

- **Worktrees.** `~/.herdr/worktrees/sirio/claude-parity-NN-<slug>`, branch
  `feat/claude-parity-NN-<slug>`. A1 starts from `origin/main` at `453f9934`. Each later item
  starts from the branch before it.
- **PRs.** One per item. Its base is the previous item's branch (A1's is `main`). Conventional
  Commits. This spec lands in A1's PR.
- **Per item:**
  1. Plan.
  2. Code by the `coder` agent.
  3. Diff review by the orchestrating session.
  4. The `critic` agent's review, with every finding fixed or dismissed with a reason.
  5. The E2E artifact.
  6. `Scripts/ci.sh` → `CI OK`.
  7. PR.
- **Docs move with behaviour.** Each item updates `docs/agents/architecture/claude-chat.md` and
  `docs/agents/e2e-tests.md` where it changes them. Release notes are not written here.
- **Version.** `rust/Cargo.toml` is at 0.40.1. The cycle's first `feat:` moves it to 0.41.0 through
  `Scripts/set-workspace-version.sh`. At PR time, check whether a `feat:` has already landed on
  `main` this cycle. If none has, A1 carries the bump.
- **State between sessions.** `HANDOFF.md` in the active worktree (gitignored) holds a table: item,
  branch, PR, state, open questions.

## Left to the builder

- Card and row layout within Sirio's theme and the Ely chat components.
- `ChatEvent` variant names, fixture mode names, poll intervals.
- Each item's `surface.chat.read` field names.
