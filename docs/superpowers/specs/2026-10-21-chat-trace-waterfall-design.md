# The chat's trace waterfall, and session panel cards folded by default

**Date:** 2026-10-21
**Status:** design
**Mockups:** `2026-10-21-chat-trace-waterfall-mockups.html` (beside this file). The
mockup runs a simulated turn. Where it and this document differ, this document wins.

## §0 Intent

The user asked for two changes to the chat's session panel (#647,
`docs/superpowers/specs/2026-10-20-chat-session-panel-design.md`):

1. **Cards start folded.** The user opens the sections they need.
2. **A new section, a trace waterfall.** It is modelled on affectUI's Trace
   Waterfall (https://www.affectui.com/components/trace-waterfall, React) and
   built with GPUI and Ely.

The waterfall is for one moment. In the user's words, it shows *"come sta andando il flusso
degli agenti"*: how the agents' work is going, **while it runs**. Live monitoring
comes first. Reading a finished turn afterwards comes second.

Decided with the user, in order:

1. **Folded by default.** Environment stays open. That was read from the user's
   screenshot and confirmed in the play-back.
2. **Variant C.** The column shows one *lane* per agent: the main agent and each
   subagent, with what each is doing now. The full waterfall is a wide tab in the
   Secondary half.
3. **Claude only, native transport.** Codex, OpenCode, omp and Pi are out of this
   version. A live probe measured what each could give (§12), for a later version.
4. **The rest of the play-back was accepted as written** ("Nulla continua").
   §2–§7 restate it.

**Licence.** affectUI's Trace Waterfall is a paid ("Pro") component. Its source was
not read: the registry answers 401 without a key. Sirio's waterfall is its own
implementation, inspired by the public page's description and screenshots.
Nothing is copied from affectUI.

## §1 Goals, non-goals, anti-goals

**Goals**

- **Folded,** the Trace card says at a glance whether the turn is running,
  waiting for the user, failed or done, and for how long.
- **Unfolded,** it shows which agents are working right now and what each is
  doing. When one stops on a permission, the card shows it, and one click
  reaches the request.
- **The wide tab** shows where the turn's time went: every model call and every
  tool, nested under the agent that made it, on one time axis.
- **One click from a lane to its place in the transcript.**

**Non-goals**

- Codex, OpenCode, omp and Pi, and Claude over the `claude-acp` wrapper.
- Traces that survive a restart of Sirio. Traces of turns that ran before the
  chat tab existed, such as a resumed session's history.
- Cost per span. The CLI reports cost per session only, and estimating it from
  tokens × price is out.
- Controlling agents from the trace, such as stopping a subagent or retrying a
  tool.
- Export to OpenTelemetry, JSON or anything else.
- **Changing the transcript.** A subagent's inner calls are not in the
  transcript today, and they stay out. The transcript's subagent heuristic
  (`is_subagent_tool_call`, `chat/mod.rs`) is untouched.

**Anti-goals.** The work fails, even if it works, when any of these happens:

- **A number nobody measured is shown.** That covers:
  - a zero for "not reported";
  - a cost on a bar;
  - a span that never ended, drawn as if it had;
  - a token count taken from a placeholder.
- **The trace and the transcript disagree about a tool's status.** For example,
  the transcript shows it done while the trace shows it running, or the reverse.
- **Idle cost.** A frame is requested while nothing runs, or while neither the
  column nor the tab is painted. A finished turn draws nothing more.
- **A change in Claude's undocumented protocol breaks the chat or the panel**
  instead of degrading the trace.
- **Opening the column with its cards folded causes more forge traffic than
  today.**
- **Content in a `sirio_perf` label.**

## §2 Cards folded by default

- `SessionPanel::new` (`chat/session_panel/mod.rs`) seeds `collapsed` with every
  section key except `environment`: `context`, `connectors`, `trace` and
  `change_request`.
  - Every new chat tab and every relaunch starts this way.
  - Folds stay unpersisted, as today. Only the column's open state persists.
- **A folded card keeps its one-line summary,** for example Context's
  `11% · 110.5k / 1000.0k` and Connectors' `15 · 2 need sign-in`. The screenshot
  the user sent is the default view.
- **What happens when the column opens stays as it is:**
  - the Environment read;
  - Context's single `get_context_usage` request;
  - Connectors' `mcp_status`.

  They are local requests to the CLI, and they feed the folded summaries.
- **The Change request card needs a fix.**
  - *The problem (found while researching this design).* Today the card's forge
    read starts only when the card is unfolded (`start_session_change_request`
    returns early when collapsed). The card is drawn only once a read produced a
    row (`change_request::section` returns `None` with no rows). A card folded at
    start would therefore never appear and could never be opened. It is a
    deadlock, and it also breaks `surface.chat.panel toggle change_request`.
  - *The fix (designer's call).*
    1. When the column opens, Sirio decides whether this worktree has a forge,
       without sending any request to the forge. If it has one, the folded card
       is drawn: its title and no summary.
    2. The forge is asked only after the user unfolds the card.
    3. Folding stops the reads, as today.

    Opening the column therefore costs **less** forge traffic than today.
  - A folded card shows no summary even after a read. Nothing reads while it is
    folded, so the last answer could go stale (anti-goal).
- **Known casualties.** These tests unfold implicitly today and must unfold
  explicitly instead:
  - the E2E scenarios `connectors`, `change_request` and `rate_limit_shared`;
  - the `sirio_ui` tests that `settle()` on the Change request card;
  - `a_click_folds_a_node_and_opens_a_file` and
    `a_connectors_name_has_the_line_beside_its_action`.

  The fix is the setup, not the assertion.

## §3 What a trace is

**One trace per turn.** It starts when the user's line is written to the CLI. It
ends at the first of:
- that turn's `result`;
- a transport error;
- the CLI's death;
- the prompt timeout.

Each turn holds a tree of **spans**:

| Kind | What it is | Starts | Ends |
|---|---|---|---|
| agent | the main Claude Code thread of the turn (the root) | prompt sent | `result` |
| subagent | one `Agent` (formerly `Task`) tool call and the agent it runs | the `tool_use` block | its `tool_result` |
| llm | one model call (one Messages API message) | see below | see below |
| tool | one tool call | its `tool_use` block | its `tool_result` |

**Statuses:**
- *running*;
- *waiting* (on a permission);
- *done*;
- *failed* (`is_error`, or a turn `result` with `is_error`);
- *interrupted*: still open when the turn ended, because of a cancel, a crash or
  a missing `tool_result`. An interrupted span ends at the turn's end and is
  drawn as interrupted, never as done.

**Wait intervals.** A tool span may hold one or more. Each runs from the
permission request (`can_use_tool`) to the moment its answer is written.

**The turn's own state** must agree with the transcript's footer:

| What happened | Turn state | Why |
|---|---|---|
| Sirio sent an interrupt during the turn | **Interrupted**, whatever the `result` says | Measured: the CLI answers an interrupt with `subtype: error_during_execution`, `is_error: true`, `stop_reason: null`. Only Sirio knows it asked. |
| A transport error, the CLI's death, or the prompt timeout | **Failed**, with the reason the transcript shows ("Agent disconnected", …) | Every open span ends there as *interrupted*. A turn with no `result` must still end, or its frames would never stop. |
| A `result` with `is_error` and no interrupt | **Failed** | |
| Any other `result` | **Done** | |

### Where the data comes from

Measured live on `claude` 2.1.306 on 2026-10-21 (§12).

- **No line carries a timestamp.** Every span edge is an `Instant` stamped **when
  the worker reads the line from the CLI's stdout**, not when the chat receives
  the event. The GPUI thread can lag behind the pipe. Permission waits use the
  worker's existing `asked_at` and the moment the answer is written.
- **The CLI's own clocks win where they exist:**
  - the turn's duration is `result.duration_ms` (per turn);
  - a finished subagent's duration is the `Agent` result's `totalDurationMs`.
- **Main-thread model calls:**
  - `stream_event` `message_start` (`message.id`, `model`, input-side `usage`)
    → `message_stop`;
  - final tokens come from `message_delta`'s `usage`;
  - several `assistant` lines sharing one `message.id` are one call.
- **Subagent:**
  - The main thread's `Agent` `tool_use` (id *X*) is the span.
  - `system/task_started` names it: `tool_use_id` *X*, `task_id`,
    `description`, `subagent_type`, `spawn_depth`, `task_type`.
  - `system/task_progress` gives live totals: `usage.total_tokens`,
    `usage.tool_uses`, `usage.duration_ms`, `last_tool_name`, and a
    `description` such as "Reading README.md".
  - It ends with the `tool_result` for *X*, whose `tool_use_result` carries
    `status`, `agentType`, `resolvedModel`, `totalDurationMs`, `totalTokens`,
    `totalToolUseCount` and `usage`.
  - `system/task_updated` and `system/task_notification` confirm the end.
- **Inside a subagent:**
  - Lines carry `parent_tool_use_id` = *X*.
  - A subagent sends **no `stream_event`s**. Its `assistant` lines arrive whole,
    when each call has finished, each with `message.id`, `model` and `usage`.
  - A subagent's model call therefore runs from its previous edge to the
    arrival of its `assistant` line. The previous edge is `task_started` or its
    previous `tool_result`.
  - Its last call, the report, sends **no `assistant` line at all**. The gap
    from its last tool result to its end is drawn as that final call, with the
    tokens of the `Agent` result's `usage`.
- **Nesting:** an `Agent` `tool_use` inside a subagent, carrying a parent id,
  opens a subagent one level down (`spawn_depth` 2), and so on.
- **A subagent's permission:**
  - It arrives as an ordinary `can_use_tool` (measured). Its `tool_use_id` is
    the subagent's tool call, whose parented `assistant` line arrived 22 ms
    earlier, and its `agent_id` is the subagent's `task_id`.
  - The wait goes on that span, and the subagent's lane, then the root's,
    shows *waiting*.
  - A `can_use_tool` whose id matches no known span puts the wait on the root.
- **Cost:** `result.total_cost_usd` and `modelUsage` are **cumulative for the CLI
  process**. The capture read 0.01284 and then 0.01329 over two turns.
  - A turn's cost is this `result`'s total minus the previous `result`'s total
    in the same process.
  - The first turn after a `--resume` shows a cost only if a capture proves the
    counter restarts with the process. Otherwise it shows `—`.
  - **An interrupted turn shows `—`, and so does the turn after it.** In the
    capture, an interrupted turn left `total_cost_usd` unchanged, although its
    model call had begun. Whether the counter catches that spend up later is
    unknown.
  - A turn's tokens are the difference of the `modelUsage` sums.
    `result.usage` covers the main thread only.
- **Latent bug.** `ModelUsage.cost_usd` is never read. Its camelCase rename
  reads `costUsd`, while the wire says `costUSD`. Fix it only if a per-model
  cost is shown.

### How it reaches the chat

- **Today the fold drops every parented line** (`sirio_acp/src/claude/events.rs`
  in `apply`: `StreamEvent`, `Assistant` and `User` return early on
  `parent_tool_use_id`).
- **New:** the native fold *also* emits trace events, a new `AcpEvent` family,
  for every line, parented ones included. Each carries span id, parent id, kind,
  label, model, usage and the read `Instant`.
- **The transcript events** (`ToolCallStarted`, `ToolCallCompleted` and the rest)
  are emitted **exactly as today**: main thread only, with one exception.
  - *The bug.* Today a subagent's permission makes the fold open a top-level
    tool row (`start_from_permission`, `worker.rs`). That row never settles,
    because the call's parented `tool_result` is dropped. The transcript would
    then show *pending* forever while the trace says *failed* or *done*, which
    is an anti-goal.
  - *The fix.* The fold recognises the id from the parented `assistant` line
    and opens **no tool row** for it. The `Permission` entry, which the user
    must answer, stays as it is.
- **ACP clients never emit trace events,** so the chat stays transport-agnostic.
  A chat that has received none shows no Trace card.
- **The trace model is a pure structure,** built in the chat from those events,
  separate from the transcript's `Entry` list. It is per chat and in memory,
  and holds the last 20 turns.
- **Unplaceable data is shown, not dropped:**
  - a span whose parent is unknown hangs under the turn's root;
  - an unknown `system` subtype is ignored.

## §4 The Trace card (in the column)

- **Order:** Environment, Context, Connectors, **Trace**, Change request.
- **Shown only when the chat has a trace,** that is, from its first native turn
  onwards.
- **Folded title:** `Trace`, then the latest turn's state:

  | State | Title summary |
  |---|---|
  | live | `● Live · 34s`, a blue pulse |
  | waiting | `● Waiting for you · 46s`, a yellow pulse |
  | failed | `Failed · 1m 02s · $0.12` |
  | interrupted | `Interrupted · 12s` |
  | done | `Done · 1m 16s · $0.31` |

  The cost appears only when it is known (§3).
- **Unfolded:**
  1. A line with the status pill and the turn's prompt, first line only,
     truncated.
  2. Stats: Duration, Cost (`—` until known), Tokens (`—` until known), LLM
     calls, Tool calls, Errors.
  3. **Lanes.** One per agent: the root first, then each subagent in tree
     order, indented one step per depth.
     - The lane's dot says running, waiting, done or failed.
     - Its name is `Claude Code`, or the subagent type.
     - Its detail is the model, or the description.
     - On the right: the call count and the duration.
  4. **The "now" row**, under a running lane. It shows the first of these that
     holds: the permission wait; running subagents; the running tool; the model
     writing (`Thinking · Opus 5.5 · 4.1s`).
  5. **The lane's track.** LLM and tool segments on an axis shared by every
     lane, with round ticks and a "now" line.
  6. **Finished lanes shrink** to name, track and count.
  7. **`Open full trace ↗`**, which opens §5.
- **Clicks:**
  - a subagent lane scrolls the transcript to its `Agent` tool row;
  - the root lane scrolls to the turn's user message;
  - the waiting row scrolls to the permission request.

  The chat has no "reveal entry" function today. The turn rail's
  `list_state.scroll_to` is the precedent. Revealing an entry stops
  follow-tail.
- **After the turn,** the card keeps showing that turn until the next one
  starts.

## §5 The Trace tab (Secondary half)

- **New `TabKind::Trace`**, default pane Secondary, not movable, not in the
  sidebar.
  - It is **never written to the saved layout**: excluded in `saved_layout`,
    beside `tab_shows_attached_session`, and skipped on restore. Its data lives
    only in memory.
  - Its persisted name `trace` exists only for the exhaustive matches and their
    round-trip test.
- **One Trace tab per chat.** Opening it again focuses it.
  - It opens through a new `ChatEvent` handled in `bind_chat`, modelled on
    `add_ci_log_tab`.
  - **It closes with its chat.** It holds the chat's trace store and has no
    data of its own.
- **Turn navigation:**
  - `‹ Turn 3 of 3 ›`, and a **Follow the latest turn** checkbox, on by
    default;
  - going back turns following off;
  - only the last 20 turns can be reached.
- **Body: a tree on the left, a timeline on the right.** It follows affectUI's
  layout:
  - **Tree:** a chevron, a kind glyph, the name and the detail, with `+N` on a
    folded parent.
  - **Bars:** each bar shows its duration.
    - A folded parent shows its children's marks on its own bar.
    - A failed bar is red, with its error line.
    - A tool's wait is drawn as a hatched yellow stretch.
    - An interrupted bar has an open end.
  - **Axis and tags:** a "now" line while live; an axis with at least 10 s of
    range and round steps; a `Slowest` tag on the slowest leaf of the turn.
- **Tooltip:** kind, name, start offset, duration, model, tokens, status or
  error.
- **Keyboard:** Up and Down move the selection, Left and Right fold and unfold,
  Enter reveals the span in the transcript.
- **What a reveal reaches,** from the tab or from a lane: the nearest entry the
  transcript has for the span.

  | Span | Transcript entry it reaches |
  |---|---|
  | a main-thread tool | its own row |
  | a subagent | its `Agent` row |
  | a span inside a subagent, at any depth | the outermost `Agent` row above it |
  | a span waiting on a permission | the `Permission` entry |
  | a main-thread model call, or the root | the turn's user message |
- **Below 560 px of width,** the tab stacks each bar under its name.

## §6 Motion and repaint

- **Frames are requested only while a span is running *and* something that
  shows it is painted:**
  - the column, with the Trace card unfolded;
  - the Trace tab;
  - the folded title, whose timer ticks once a second.

  The Change request card's "painted" probe (`Frame.painted`) is the precedent.
  The trace's own animation is capped at 30 fps, as `update_card` is.
- **A finished turn requests nothing.**
- **gpui's reduced-motion setting** gives static marks, as `orbit.rs` does.
- **Ely motion:**
  - lanes enter and leave with `motion::AnimatePresence`, keyed by span id;
  - the "now" row's text is Ely `ShimmerText`, mounted only while running.
- **Colours** come from Sirio's theme (`Theme::get(cx)`), dark and light:

  | Element | Token |
  |---|---|
  | agent lanes | the grey ladder |
  | LLM | `theme.ely.chart[4]` (violet) |
  | tool | `theme.ely.chart[1]` (teal) |
  | failed | `ely.danger` |
  | waiting | `ely.warning` |
  | running | `ely.info` |
  | done | `ely.success` |

## §7 Per agent

| | Claude (native) | Claude via `claude-acp` | Codex / OpenCode / omp / Pi |
|---|---|---|---|
| Folded-by-default cards | ✓ | ✓ | ✓ |
| Trace card and tab | ✓ | — | — |

## §8 Constraints

- **Claude's stdio protocol is not a published contract** (CLAUDE.md, "A Claude
  chat talks to Claude Code"). Every field the trace adds is parsed to degrade,
  not reject:
  - a missing field loses its detail, never the span;
  - a missing `task_started` still leaves the `Agent` span, named after its
    tool.
- **Extend the live conformance coverage.** It must notice the day subagent
  lines, `task_started` or main-thread `message_start` stop arriving. It runs
  beside `claude_answers_the_native_handshake_it_claims`, SKIPs without the
  binary, and goes on the release gate's skip list.
- **The transcript is unchanged** (§1). The fold's parented lines feed the
  trace only.
- **Chat presentation is Ely.** Read
  `rust/vendor/ely-gpui-component/LOCAL-CHANGES.md` first. Keep Sirio's theme
  and chat controllers.
- **Text selection:** lanes, rows and bars are clickable, so no
  `selectable_text` inside them.
- **`sirio_perf`:** every label is a content-free `&'static str`. Never a tool
  input, a prompt, a description or a model name.
- **Version:** this cycle opened at 0.34.1. The work's first `feat:` commit runs
  `Scripts/set-workspace-version.sh 0.35.0`.

## §9 Proof

**End to end,** per the repository's conventions: a real, isolated Sirio driven
over the control socket. Each run ends in an artefact anyone can re-run: a
transcript plus PID-matched captures, dark and light. Extend
`Scripts/Tests/test-chat-session-panel-e2e.sh`.

- **Default folds:**
  - A fresh chat's `sessionPanel` has `environment` expanded and every other
    card folded.
  - Context and Connectors have their summaries while folded.
  - With the fake forge, **its request log stays empty** until
    `change_request` is unfolded, and the card then reads.
  - The existing scenarios unfold what they need (§2).
- **Trace,** against a stub `claude`: a new `trace` mode of
  `claude_fixture.py`, paced with sleeps so that live states can be asserted
  mid-turn.
  - **The scripted first turn includes:**
    - a main model call with stream events;
    - two parallel subagents with parented lines and task events;
    - a nested subagent;
    - an `Edit` that waits on a permission;
    - a failing `Bash`, then its retry;
    - a `result` with a cumulative cost.
  - **Further turns:**
    - a second turn proves the cost difference;
    - an interrupted turn proves *Interrupted* and the `—` cost;
    - a turn whose tool never gets its result proves *interrupted* spans;
    - a turn where the stub dies mid-run proves *Failed*, and that the frames
      stop.
  - **A subagent permission** must leave no stuck tool row in the transcript.
  - **The fixture's shapes copy the 2026-10-21 captures** (§12).
- **Assertions read state, never pixels:**
  - the column's trace section through `surface.chat.read`'s `sessionPanel`;
  - the tab through a new read verb: its spans with parent, kind, status and
    whether each ended.
  - A lane click goes through `surface.chat.panel activate <key>`. A new
    transcript read then reports the first visible entry, so the reveal can be
    asserted.
- **Idle:** with `SIRIO_PERF_TRACE`, assert that no trace render happens:
  - after the turn ends, for a stated window;
  - while the Trace card is folded and the tab is closed.

  This is the proof of the idle anti-goal.
- **Unit tests** cover the pure trace builder only, failure modes written first:
  - a span with an unknown parent;
  - repeated `message.id` lines;
  - a `result` with no `message_start`;
  - a `tool_result` with no `tool_use`;
  - a subagent's end arriving before its last tool result;
  - depth 2 and 3;
  - an interrupt mid-tool;
  - a permission denied;
  - task events for a non-agent task;
  - the first turn after a resume;
  - a counter that goes down.

## §10 Calls made by the designer, not by the user

- **The Change request card's fix** (§2): folded, it is drawn whenever a forge
  is known, and it reads only once unfolded.
- **A folded card's summary:** Context and Connectors keep theirs, Change
  request shows none.
- **Card order:** Trace goes between Connectors and Change request.
- **Timing:** span edges stamped when the worker reads each line, not on GPUI
  arrival. The CLI's own durations override them where they exist.
- **A subagent's model calls** are inferred from the gaps between its edges,
  because the CLI streams none for subagents.
- **Cost:** a turn's cost is the difference of the cumulative totals. After a
  resume the cost is unknown until proven. No per-span cost.
- **Unplaced spans hang under the root,** never dropped.
- **Memory:** 20 turns per chat.
- **The Trace tab:** never saved, one per chat, closes with its chat.
- **Behaviour taken from affectUI:** keyboard navigation, the `Slowest` tag
  and the stacked layout below 560 px.
- **Background tasks** (`is_backgrounded`, or any `task_type` other than
  `local_agent`): in this version, only the tool call that started them is
  traced. Work after its `tool_result` is not.
- **The turn's state:**
  - a user's cancel is *Interrupted*, whatever `is_error` says;
  - a lost connection is *Failed*;
  - an interrupted turn and the turn after it show no cost.
- **Reveal:** a span with no transcript entry reveals its nearest ancestor that
  has one.
- **The stuck tool row:** the transcript stops opening a top-level row for a
  subagent's permission. It is a small fix outside the trace, made because
  the trace would otherwise contradict the transcript.

## §11 Left to the builder

- **Where the trace builder lives** (`sirio_acp` beside the fold, or
  `sirio_ui::chat`) and the exact shape of the trace `AcpEvent`s.
- **The forge check without a request.** Check whether
  `ChangeRequestSource::connect` sends anything over the network. If it does,
  find the local check that decides "this worktree has a forge" (the remote's
  host against the configured forges).
- **What happens to the cumulative counters after `--resume`.** Settle it with
  a capture.
- **The span cap per turn.** When it is reached, the trace says how many spans
  were not drawn.
- **Presentation details:** spacing, icons, glyphs, exact strings, tick steps,
  the tooltip's layout.
- **How the folded title's one-second tick and the lanes' frames share the
  painted probe.**
- **The names of the new control verbs** and whether each serves release
  builds. A read-only verb may serve every build, as `surface.chat.read` does.

## §12 Measured facts

**`claude` 2.1.306, 2026-10-21.**
- *How it was captured.* A stream-json client launched with Sirio's own flags
  plus `--model haiku`, `--strict-mcp-config` and
  `--settings '{"disableAllHooks":true}'`. It ran two turns: the first
  delegated `ls` and a README read to one general-purpose subagent, the second
  replied "ok". The client lives in `~/.cache/st/claude-trace-probe/`.
- *Turn 1:*
  - `message_start`, `message_delta` and `message_stop` arrived for the main
    thread only.
  - `task_started` arrived 24 ms after the `Agent` `tool_use`.
  - `task_progress` arrived before each subagent tool.
  - There were two subagent `assistant` lines (`Bash`, `Read`), with no
    `assistant` line for the subagent's final report.
  - `task_updated` and `task_notification` arrived, then the `Agent`
    `tool_result`, whose `totalDurationMs` was 4537.
  - `result.duration_ms` was 10255, with `num_turns` 2.
- *Turn 2:* `total_cost_usd` went from 0.01284 to 0.01329, and `modelUsage`
  output tokens from 1345 to 1349. Both are cumulative.
- *No* `can_use_tool` arrived.

**A second capture, same setup, on the same day.**
- *Turn 1:* a subagent was told to run `mkdir`.
  - Its parented `assistant` line (`Bash`) arrived at 8.140 s.
  - At 8.162 s came a `can_use_tool` carrying `tool_use_id` (that call),
    `agent_id` (the `task_id`), `description` and `blocked_path`.
  - The probe denied it.
  - The parented `tool_result` came at 8.173 s with `is_error: true`.
  - No directory was created.
- *Turn 2:* an interrupt was sent 6 s in. The `result` came 6 ms later:
  - `subtype` `error_during_execution`, `is_error: true`, `stop_reason: null`;
  - `errors` held one diagnostic string;
  - `duration_ms` was 6002;
  - `total_cost_usd` was unchanged from turn 1.

**The ACP agents, for a later version** (`~/.cache/st/acp-meta-probe/`, same
day):
- **OpenCode** tags every child update with
  `_meta["opencode/child-session"] = {id, parentID, depth, title}`, which gives
  a full tree.
- **omp** streams a subagent's live progress inside the `task` tool call's
  `rawOutput.details.progress[]`: tokens, cost, tool count and model, with only
  the latest tools.
- **codex-acp** sends only a subagent's start and end
  (`_meta.codex.subagent`). The child's calls never reach ACP.
- **Pi** has no subagents.
