# The trace follows background agents to their end

**Date:** 2026-10-23
**Status:** design
**Amends:** `2026-10-21-chat-trace-waterfall-design.md` §10 ("Background tasks": only the
launch of a background agent is traced). Where the two differ, this document wins.

## §0 Intent

The user ran a turn in which Claude Code launched five `fork` subagents in parallel. The
main thread answered "Ho avviato 5 agent in parallelo…" and its turn ended after 48 s. The
Trace tab then read `Done · 48s · $3.28`, with five `fork` rows of about 350 ms each, while
the five agents went on working for minutes. In the user's words, the trace *"si è fermato"*.

The cause is a design decision, not a fault. A `fork` agent is launched in the background:
`task_started` says `is_backgrounded: true`, and its `tool_result` comes back within about
50 ms with `tool_use_result.status: "async_launched"`. The 2026-10-21 design traces such an
agent as its launch only. Everything it does after that `tool_result` is dropped.

What the user wants, in their words: *"mi serve vedere anche come stanno lavorando i
subagent"*. The waterfall exists to show how the agents' work is going while it runs
(2026-10-21 §0). An agent that works in the background is the case where that matters most.

Decided with the user, in order:

1. **Background agents stay open** in the turn that launched them, until they end. Of the
   three changes offered, the user picked this one. Turns that the CLI starts on its own
   stay out (§1 non-goals).
2. **The turn is `Live` until its last agent ends, and its clock keeps running.** Answer:
   *"Live finché l'ultimo agent non finisce, il tempo continua"*.
3. **A turn's cost includes what its background agents spend, even when that spend is
   reported minutes later.** Answer: "si".
4. **The folded Trace card always shows the latest turn,** even when an older turn is still
   `Live` on its agents. Answer: *"sempre l'ultimo turno"*. The older turn's lanes are
   reached by paging the Trace tab.
5. **The rest of the play-back was accepted as written** ("no va bene"). That covers the
   edge cases in §4 and the calls marked as the designer's.

## §1 Goals, non-goals, anti-goals

**Goals**
- A background agent's lane is drawn like a foreground agent's. It shows its model calls,
  its tools, the nested agents it starts, its live totals from `task_progress`, and what it
  is doing now. All of it grows live, until the CLI says the agent ended.
- With five agents running in parallel, five lanes advance together. The user can see which
  one is working, which one waits, and which one is done.
- The turn reads `Live`, and the frame loop keeps running, for as long as any of its agents
  works.
- The turn's cost and tokens include its agents' spend.

**Non-goals**
- **Turns the CLI starts on its own.** When a background agent ends, the CLI writes `system
  init`, `requesting`, a model call and a `result` with no prompt from Sirio, to react to the
  notification. These turns are not drawn. Their spend counts towards the cost of the turn
  whose agents caused them (§3).
- **Background `Bash`** (`task_type` `local_bash` with `is_backgrounded: true`) and any other
  `task_type` other than `local_agent`. These stay traced as their launch only, as today.
- **Stopping a background agent** from the trace. The transcript, the chat's own "background
  tasks" indicator and the composer do not change.
- **The prompt collision.** The user can write a prompt while a CLI-started turn runs, so the
  CLI-started turn's `result` ends the user's turn. This is the residual already recorded for
  the 2026-10-21 delivery, and this change does not fix it.

**Anti-goals** (failures even if everything else works)
- A turn that reads `Done` while one of its agents still works.
- A wrong cost. When the cost cannot be told, it shows as "at least" or a dash, never as a
  plausible-looking wrong number.
- A background agent's lines landing in the wrong turn, for example the latest turn instead
  of the one that launched the agent.
- A main-thread call of a CLI-started turn drawn as if it belonged to the user's turn.

## §2 What the CLI sends (measured)

All of the following was measured live on `claude` 2.1.309 in Sirio's stream-json mode
(`--input-format stream-json`, `--include-partial-messages`, `--permission-prompt-tool
stdio`), with Haiku, on 2026-10-23.

- **Launch.**
  - The `Agent` `tool_use` comes first.
  - Then `system/task_started` arrives with `{tool_use_id, task_id, subagent_type: "fork",
    task_type: "local_agent", is_backgrounded: true, description}`.
  - About 50 ms later the main-thread `tool_result` arrives, with `tool_use_result:
    {isAsync: true, status: "async_launched", agentId, resolvedModel, …}`.
  - The main thread goes on and can reach its `result` while the agent works.
- **Work.**
  - The agent's `assistant` lines (thinking, text, `tool_use`) and `user` lines
    (`tool_result`) carry `parent_tool_use_id` = the `Agent` call's id, exactly as a
    foreground agent's do.
  - `system/task_progress` arrives with `{tool_use_id, task_id, description, usage:
    {total_tokens, tool_uses, duration_ms}, last_tool_name}`.
  - All of this keeps streaming **after** the launching turn's `result`.
- **End.**
  - `system/task_updated` arrives with `{task_id, patch: {status, end_time}}`, where
    `end_time` is epoch ms.
  - Then `system/task_notification` arrives with `{tool_use_id, task_id, status, summary,
    usage: {total_tokens, tool_uses, duration_ms}, output_file}`.
  - The statuses seen were `completed`, and `killed` in `task_updated` with `stopped` in the
    notification. A `failed` status is expected but was not seen.
- **After a completed agent.**
  - The CLI starts a turn on its own: `system init`, `system/status: requesting`, a
    main-thread model call, then `result` with `num_turns: 1`.
  - That `result` is the first one whose cumulative `total_cost_usd` includes the agent's
    spend. Measured with two agents: `0.00912` (the launching turn), then `0.01190`, then
    `0.01258`.
- **Stop during the launching turn.**
  - Sirio's `interrupt` kills the background agents. `task_updated {status: "killed"}` and
    `task_notification {status: "stopped"}` arrive *before* the turn's
    `error_during_execution` result.
  - No CLI-started turn follows.

The live conformance test `claude_streams_the_lines_the_trace_reads`
(`rust/crates/sirio_agents/tests/acp_conformance.rs`) should come to require the lines §3
relies on. Those are a background `task_started`, parented lines after the launch, and a
`task_notification` naming the `tool_use_id`. The protocol is not a published contract.

## §3 Behaviour

### A background agent's span

- **It opens as today.** The `Agent` `tool_use` opens it and `task_started` names it.
  Its detail keeps the "· in background" suffix, so the user can tell a background lane
  from a foreground one.
- **The launch `tool_result` no longer ends it.** It stays open, and its lines keep feeding
  its lane: model calls from each parented `assistant` line, timed from the agent's previous
  edge, tools, nested agents and `task_progress` totals. This is the same path a foreground
  agent uses today.
- **The `task_notification` naming its `tool_use_id` ends it.**
  - **The instant** is `task_updated`'s `end_time` when one came for that `task_id`.
    Otherwise it is the notification's own read instant.
  - **The outcome:** `completed` → Done; `stopped` or `killed` → Interrupted; anything else
    → Failed, with the status word as the note.
  - **The duration:** the notification's `usage.duration_ms` rides as `measured_ms`, the way
    a foreground agent's `totalDurationMs` does today.
- **The final report.** A completed foreground agent gets a final-report model call, drawn
  from its last edge to the end. A completed background agent gets the same call, from its
  last edge to its end.
- **Its open children.** A background agent's open tools and nested agents end Interrupted
  when it ends, as a foreground agent's do.

### The turn

- **The main thread's `result` while background agents are still open.** The turn does
  *not* end.
  - The fold keeps what the result said: outcome, the CLI's `duration_ms`, cost and tokens.
  - The store keeps the turn open. Its state reads `Live` and its clock keeps running.
  - `Turn::is_moving` stays true while any of its agents runs, so the card and the tab keep
    their frame loop.
  - The lanes draw the main thread as finished (its last call ended) and the agents as
    running.
- **The turn ends when its last open background agent ends,** at that agent's end instant.
  - **The outcome** is the main thread's result outcome: Done or Failed. Interrupted stays
    Interrupted, as today: a user's cancel wins.
  - **The duration** runs from the prompt to that end. The main result's `duration_ms` no
    longer measures this turn, so it is not used as `measured` when background agents
    outlived the main thread (the designer's call).
- **When no background agent is open at the main result,** the turn ends exactly as today.
- **Stop during the turn.** The kills arrive before the result, so each agent ends
  Interrupted and the turn ends Interrupted at its result, as today. Nothing waits.
- **The CLI dies, or the transport fails, while agents are open.** The turn ends Failed,
  and every open span ends Interrupted, as `fail_open_turn` does today.

### Which turn a line belongs to

- **Background agents outlive their turn.** The user can write the next prompt while the
  agents of turn 13 still work. Turn 14 then opens and runs, while turn 13 stays `Live`
  until its agents end.
- **Every agent line lands in the turn that holds the agent's span,** not in the latest
  turn. This covers the agent's parented lines, its `task_progress` and its notification.
  The store therefore routes span events by span id across its kept turns, not only to the
  latest open turn.
- **Opening turn 14 no longer fails turn 13 as abandoned** when turn 13 is waiting only on
  its background agents. A turn whose *main thread* never got its `result` is still
  abandoned, as today.
- **Retention.** The store keeps 20 turns. A turn that is still `Live` is never dropped to
  make room. When every kept turn is live, the store keeps more than 20 for as long as that
  lasts (the designer's call).
- **"Follow the latest turn"** and the turn pager need no change: turn 13 can be `Live`
  behind turn 14.
- **The folded card** keeps reading the latest turn (§0, decision 4). Everything that only
  asks whether *the latest* turn runs, such as the card's title tick and its frame loop,
  keeps doing so. What must look at *every* kept turn is what ends or routes work:
  - `fail_open_turn`, when the CLI dies;
  - the routing of span events by id;
  - the Trace tab's frame loop when the shown turn is an older one that is still live.

### Main-thread lines outside a prompt-started turn

The CLI-started turn's main-thread lines (`requesting`, `message_start`, `message_stop`, its
tool uses) arrive while no prompt-started turn is open in the fold. They must open no span.
**This is new work:** today `requested()`, the `message_start` path and the root `tool_use`s
in `assistant()` emit spans whether or not the fold's turn is open. Only the store's "no open
turn" rule hides them, and that rule stops protecting once turn 13 stays open in the store.
The fold must drop main-thread spans when its own turn is not open.

An `Agent` that a CLI-started turn launches belongs to that turn, so it is not drawn either
(the designer's call, following the user's exclusion of CLI-started turns). The fold
remembers such agents and drops their lines, their `task_progress` and their notification.
This overrides, for these lines only, the 2026-10-21 rule that a span whose parent is unknown
hangs under the root. Such agents do not keep any turn `Live`. Their spend reaches the
cumulative totals and is counted by the cost rule below.

The `result` of a CLI-started turn never ends a user's turn. Its only effect is on cost.

### Cost and tokens

The CLI reports cost only as a cumulative per-process total at each `result` (2026-10-21
§12). The rule:

- At the main result, the turn's spend so far is the delta against the previous result, as
  today. When background agents are open, that figure is **provisional**. The card and the
  tab show it as "at least" (`≥ $3.28`).
- Every later `result` before the next prompt is written is a CLI-started turn's result. Its
  delta is added to the turn that is waiting on agents. When several turns are waiting, it
  goes to the most recent of them (the designer's call: in practice only one turn's agents
  run when no prompt was written in between).
- **The cost becomes final** at the first `result` that arrives after the turn's last agent
  ended, with no prompt written in between. Until then, a turn that already reads `Done`
  shows its cost as "at least" the sum so far.
  - **Why that result is enough.** Every API call an agent makes finishes before the agent
    ends, so a cumulative total read after the last agent's end already contains all of the
    agents' spend.
  - **What arrives after it.** Later CLI-started results carry only the CLI's own reactions
    to the notifications. One reaction turn per completed agent is what was measured, but its
    reliability with agents ending close together, or with failed or killed agents, was not.
    The rule therefore does not depend on counting reaction turns.
  - **Reaction spend.** Each later CLI-started result's delta, before the next prompt is
    written, is still added to the turn, and the final figure grows by it (the designer's
    call). The agents' part is complete at that point, so the figure is not marked "at
    least".
  - **One rule for every CLI-started result.** The delta of every CLI-started `result` before
    the next prompt goes to the most recent prompt-started turn. That covers a turn with no
    open agents, whose agent ended before its main result and whose reaction turn follows.
- **When a prompt is written while the turn's agents run,** the next `result` mixes the new
  turn's spend with the agents' spend, and the two cannot be told apart.
  - The waiting turn's cost freezes as "at least" what was known.
  - The new turn's cost is unknown (a dash), on the same terms as the turn after an
    interrupt today.
- **When the agents were killed** (Stop), no CLI-started turn follows and the turn is
  Interrupted. Its cost is unknown, as for any interrupted turn today.
- **Tokens** follow the same rule as cost, from `modelUsage`, with the same "at least" and
  dash states.

## §4 How it shows

- **Header and folded card.**
  - While the main thread works, or its agents do, the turn reads `Live · <clock>`.
  - At the end it reads `Done · <duration> · <cost>`, where the cost is `≥ $x` while
    provisional.
  - The `Live` state does not tell "the main thread works" apart from "only background
    agents work" (the designer's call: the lanes show it).
- **Lanes.**
  - A background agent's lane looks like a foreground agent's. Its detail ends with
    "· in background".
  - When it ends, its summary, the notification's `summary` reduced to one line, may replace
    the "now" text as its detail. The builder decides whether the summary is shown.
- **`surface.trace.read`.** The tab's spans already report `state` and `ended`. A
  background agent reads `running` until its notification. Nothing new is required, but the
  turn's live state must be readable, as today.

## §5 Testing

- **Fold unit tests,** in `rust/crates/sirio_acp/src/claude/trace.rs`, written before the
  code. The failure list is the builder's to complete. It covers at least:
  - the launch `tool_result` no longer ends a background agent;
  - parented lines after the main `result` still feed the agent;
  - the notification ends the agent at `end_time`, with each status mapped;
  - the turn does not end while an agent is open, and ends at the last agent's end;
  - a CLI-started turn's lines open nothing and its result ends no turn;
  - the cost rules, including the provisional, final, "at least" and dash cases;
  - Stop kills before the result;
  - the next prompt while agents run.
- **Store unit tests** for routing by span across turns, the retention exception and the
  not-abandoned rule.
- **E2E.** One new scripted turn in `claude_fixture.py`'s trace mode and one scenario in
  `Scripts/Tests/test-chat-session-panel-e2e.sh`, both following the existing trace
  scenarios. The turn launches background agents whose lines keep coming after the `result`,
  then ends them with `task_updated` and `task_notification`, then runs a CLI-started turn
  with its own cumulative cost. The scenario asserts:
  - the turn stays `Live` after the result, with the agent lanes running and growing;
  - `Done` arrives only after the last notification;
  - the final cost includes the CLI-started turn's delta;
  - no extra root call appears.

  A second scripted turn sends a prompt while agents run, and asserts the "at least" and
  dash costs. Every E2E run ends in its artifact, as the existing ones do.

## §6 Left to the builder

These were settled by the reviewer reading this document as a builder, and are recorded so
the plan does not reopen them:
- **Instants.** `task_updated.end_time` (epoch ms) is converted to the trace's read instants
  against the wall clock at the moment the line is read.
- **The CLI dies.** Every open turn ends Failed, not only the latest.
- **The main thread's lane** is drawn as finished while the turn is `Live` on agents only.
- **A missing notification.** `background_tasks_changed` (already parsed in
  `sirio_acp::claude::events`) dropping a task is the fallback end when no notification
  comes.
- **Retention.** Turns are dropped only from the oldest end. The store grows past 20 while
  the oldest turn is live, so turn numbers stay contiguous and the pager is unchanged.
- **A Stop in turn 14 that kills turn 13's agents.** Turn 13 ends with its main result's
  outcome, and those agents' lanes end Interrupted.

Still open to the builder:

- **The shape of the events** between the fold and the store. Options include a new
  `TraceEvent` for "main thread done, agents still running", or the fold withholding
  `TurnEnded` until the last agent ends and carrying the main result's facts with it. Either
  way, the store must route span events by id across turns.
- **Where the provisional cost lives** (fold or store), and how "at least" is carried in the
  event and drawn.
- **Whether the end-of-agent summary is shown.**
- **How the fold tracks background agents across its own turn scope.** Today
  `end_turn_scope` drains the spans and marks every agent ended. A background agent that is
  still running must survive that.
