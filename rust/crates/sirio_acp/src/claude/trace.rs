//! Claude Code's lines in, trace events out.
//!
//! Pure, like [`super::events::Fold`] beside it: the worker owns the process
//! and the channels and hands this every line with the instant it read it
//! at. The transcript fold drops every line that belongs to a subagent; this
//! reads them all, because a subagent is where most of a long turn goes.
//!
//! What the lines give and do not give shapes everything below (measured on
//! `claude` 2.1.306, spec §3 and §12):
//! - no line carries a timestamp, so every edge is the `at` the worker
//!   stamped;
//! - the main thread streams: a model call is `message_start` to
//!   `message_stop`;
//! - a subagent does not stream. Its `assistant` lines arrive whole, so a
//!   call runs from the agent's previous edge to the line, and its last
//!   call, the report, sends no line at all: the gap before the `Agent`
//!   result is that call;
//! - the CLI's own durations (`result.duration_ms`, the `Agent` result's
//!   `totalDurationMs`) win where they exist and ride along as `measured_ms`.
//!
//! Degrade, never reject: a missing field drops its detail and never its
//! span, an unknown `system` subtype is ignored, and a span whose parent is
//! unknown hangs under the root.

use std::collections::{HashMap, HashSet};
use std::time::Instant;

use sirio_claude::message::{
    AgentToolResult, AssistantPayload, CliMessage, MessageLifecycle, MessageUsage, ResultPayload,
    StreamEventPayload, SystemPayload, ToolResult, ToolUse, UserPayload,
};
use sirio_claude::tools;

use crate::{SpanKind, SpanOutcome, TokenCount, TraceEvent, TurnOutcome};

/// A model call's name: the model is its own field, the span says nothing
/// more.
const CALL_NAME: &str = "Model call";

/// The longest a tool's one-line summary or a failure's note runs. These
/// travel as event text, not as `sirio_perf` labels.
const LINE_LIMIT: usize = 200;

/// One open span: a tool call or a subagent.
struct Span {
    /// Opening order, so children end in the order they started.
    seq: u64,
    /// The subagent it runs under, `None` for the root.
    parent: Option<String>,
    /// Set for a subagent.
    agent: Option<Agent>,
}

/// What a subagent needs to time and name its calls.
struct Agent {
    name: String,
    detail: String,
    /// The last moment the agent is known to have been alive: `task_started`,
    /// then each of its tool results and model calls. The next call starts
    /// here.
    edge: Instant,
}

/// A permission the user has not answered.
struct Wait {
    /// The span it waits on, the root when `None`.
    span: Option<String>,
    /// The tool call the request is about, which can end it even when the
    /// wait sits on the agent because the call's own line came later.
    tool_use_id: Option<String>,
}

/// The state of one `claude` process's trace: what is open, and the running
/// totals a turn's own cost is the difference of.
pub(crate) struct TraceFold {
    interrupted_this_turn: bool,
    interrupted_last_turn: bool,
    /// The main call that was open when the interrupt was sent. Its
    /// `message_stop` is the interrupt taking hold: that call did not finish.
    interrupted_call: Option<String>,
    /// The previous `result`'s cumulative cost. `None` is unknown: a
    /// resumed process's first turn, or a result that carried none.
    baseline_cost: Option<f64>,
    baseline_tokens: Option<TokenCount>,
    /// The main thread's open model call. `message_stop` names none.
    main_call: Option<String>,
    /// Model calls seen this turn, so a second line of one is not a call.
    seen_calls: HashSet<String>,
    spans: HashMap<String, Span>,
    /// `task_id` → the `Agent` call's id, for a permission that names its
    /// agent by `task_id`.
    tasks: HashMap<String, String>,
    /// Subagents that are over. A line that names one is ignored, where a
    /// line that names an id nobody opened hangs under the root. Kept for
    /// the process, because a background agent writes on after its turn.
    ended_agents: HashSet<String>,
    waits: HashMap<u64, Wait>,
    next_seq: u64,
}

impl TraceFold {
    /// `resumed` is whether this process was started with `--resume`: its
    /// cost counter has an unknown starting point.
    pub(crate) fn new(resumed: bool) -> Self {
        Self {
            interrupted_this_turn: false,
            interrupted_last_turn: false,
            interrupted_call: None,
            baseline_cost: (!resumed).then_some(0.0),
            baseline_tokens: (!resumed).then_some(TokenCount {
                input: 0,
                output: 0,
            }),
            main_call: None,
            seen_calls: HashSet::new(),
            spans: HashMap::new(),
            tasks: HashMap::new(),
            ended_agents: HashSet::new(),
            waits: HashMap::new(),
            next_seq: 0,
        }
    }

    /// The user's line was written to the CLI. Every prompt opens a turn of
    /// its own, whether or not the last one's `result` has come.
    pub(crate) fn prompt_sent(&mut self, at: Instant) -> Vec<TraceEvent> {
        // A Stop that landed after the last `result` belonged to no turn:
        // starting clean keeps it from marking this one.
        self.interrupted_this_turn = false;
        self.interrupted_call = None;
        vec![TraceEvent::TurnStarted { at }]
    }

    /// An interrupt was written to the CLI: the turn in flight is
    /// interrupted, whatever its `result` says, and so is the model call
    /// open at this moment.
    pub(crate) fn interrupt_sent(&mut self) {
        self.interrupted_this_turn = true;
        self.interrupted_call = self.main_call.clone();
    }

    /// Folds one line, read at `at`.
    pub(crate) fn line(&mut self, message: &CliMessage, at: Instant) -> Vec<TraceEvent> {
        match message {
            CliMessage::StreamEvent(event) => self.stream_event(event, at),
            CliMessage::Assistant(assistant) => self.assistant(assistant, at),
            CliMessage::User(user) => self.user(user, at),
            CliMessage::System(system) => self.system(system, at),
            CliMessage::Result(result) => self.result(result, at),
            _ => Vec::new(),
        }
    }

    /// A permission was asked. The wait goes on the call the request names;
    /// failing that, on the subagent whose `task_id` it carries (the request
    /// can beat its tool's own line); failing that, on the root.
    pub(crate) fn permission_asked(
        &mut self,
        tool_use_id: Option<&str>,
        agent_id: Option<&str>,
        request_id: u64,
        at: Instant,
    ) -> Vec<TraceEvent> {
        let span = tool_use_id
            .filter(|id| self.spans.contains_key(*id))
            .map(str::to_string)
            .or_else(|| {
                let agent = self.tasks.get(agent_id?)?;
                self.spans.contains_key(agent).then(|| agent.clone())
            });
        self.waits.insert(
            request_id,
            Wait {
                span: span.clone(),
                tool_use_id: tool_use_id.map(str::to_string),
            },
        );
        vec![TraceEvent::WaitStarted {
            span,
            request_id,
            at,
        }]
    }

    /// The answer was written, or the permission timed out and was denied.
    /// A wait the call's own result already closed is not closed twice.
    pub(crate) fn permission_answered(&mut self, request_id: u64, at: Instant) -> Vec<TraceEvent> {
        match self.waits.remove(&request_id) {
            Some(_) => vec![TraceEvent::WaitEnded { request_id, at }],
            None => Vec::new(),
        }
    }

    fn stream_event(&mut self, event: &StreamEventPayload, at: Instant) -> Vec<TraceEvent> {
        // A subagent sends no stream events; one that did would be a second
        // call on the main thread's line.
        if event.parent_tool_use_id.is_some() {
            return Vec::new();
        }
        match event.lifecycle() {
            Some(MessageLifecycle::Start { id, model, .. }) => {
                if id.is_some() && self.main_call == id {
                    return Vec::new();
                }
                let mut events = Vec::new();
                // A `message_start` before the last call's `message_stop`
                // means the stop never came: that call is over.
                if let Some(open) = self.main_call.take() {
                    events.push(ended(open, SpanOutcome::Done, at, None));
                }
                // A missing id drops the call's name, never the call: the fold
                // names it, and its delta and stop still find it.
                let id = id.unwrap_or_else(|| format!("call-{}", self.next_sequence()));
                if self.seen_calls.insert(id.clone()) {
                    self.main_call = Some(id.clone());
                    events.push(call_started(id, None, model, at));
                }
                events
            }
            Some(MessageLifecycle::Delta { usage }) => self
                .main_call
                .clone()
                .map(|id| TraceEvent::SpanUsage {
                    id,
                    tokens: tokens_of(&usage),
                })
                .into_iter()
                .collect(),
            Some(MessageLifecycle::Stop) => self
                .main_call
                .take()
                .map(|id| {
                    // The CLI stops the call within milliseconds of an
                    // interrupt: a call the user cut short did not finish.
                    let outcome = if self.interrupted_call.as_ref() == Some(&id) {
                        SpanOutcome::Interrupted
                    } else {
                        SpanOutcome::Done
                    };
                    ended(id, outcome, at, None)
                })
                .into_iter()
                .collect(),
            None => Vec::new(),
        }
    }

    fn assistant(&mut self, assistant: &AssistantPayload, at: Instant) -> Vec<TraceEvent> {
        let mut events = Vec::new();
        let owner = match assistant.parent_tool_use_id.as_deref() {
            None => None,
            Some(parent) if self.ended_agents.contains(parent) => return events,
            Some(parent) if self.is_agent(parent) => {
                self.subagent_call(parent, assistant, at, &mut events);
                Some(parent.to_string())
            }
            // Nobody opened it: the tools it names still show, under the
            // root. No call is invented, there is no edge to time one from.
            Some(_) => None,
        };
        for tool_use in assistant.tool_uses() {
            self.open_span(tool_use, owner.as_deref(), at, &mut events);
        }
        events
    }

    /// The model call a subagent's `assistant` line ends. It started at the
    /// agent's previous edge, and the line is the new one.
    fn subagent_call(
        &mut self,
        agent_id: &str,
        assistant: &AssistantPayload,
        at: Instant,
        events: &mut Vec<TraceEvent>,
    ) {
        let message = &assistant.message;
        let call = match &message.id {
            Some(id) => id.clone(),
            // Without an id each line is its own call: the span is shown
            // rather than dropped.
            None => format!("{agent_id}/call-{}", self.next_sequence()),
        };
        let Some(agent) = self
            .spans
            .get_mut(agent_id)
            .and_then(|span| span.agent.as_mut())
        else {
            return;
        };
        let started = agent.edge;
        agent.edge = at;
        if !self.seen_calls.insert(call.clone()) {
            return;
        }
        events.push(call_started(
            call.clone(),
            Some(agent_id.to_string()),
            message.model.clone(),
            started,
        ));
        if let Some(usage) = &message.usage {
            events.push(TraceEvent::SpanUsage {
                id: call.clone(),
                tokens: tokens_of(usage),
            });
        }
        events.push(ended(call, SpanOutcome::Done, at, None));
    }

    /// Opens the span for a `tool_use` block: a subagent for `Agent` (and
    /// the `Task` it was called before), at any depth, and a tool otherwise.
    fn open_span(
        &mut self,
        tool_use: &ToolUse,
        owner: Option<&str>,
        at: Instant,
        events: &mut Vec<TraceEvent>,
    ) {
        if self.spans.contains_key(&tool_use.id) {
            return;
        }
        let seq = self.next_sequence();
        let (kind, name, detail, agent) = if matches!(tool_use.name.as_str(), "Agent" | "Task") {
            let name = line_of(&tool_use.input, "subagent_type")
                .unwrap_or_else(|| one_line(&tool_use.name));
            let detail = line_of(&tool_use.input, "description").unwrap_or_default();
            let agent = Agent {
                name: name.clone(),
                detail: detail.clone(),
                edge: at,
            };
            (SpanKind::Subagent, name, detail, Some(agent))
        } else {
            let detail = one_line(&tools::describe(tool_use).title);
            (SpanKind::Tool, one_line(&tool_use.name), detail, None)
        };
        self.spans.insert(
            tool_use.id.clone(),
            Span {
                seq,
                parent: owner.map(str::to_string),
                agent,
            },
        );
        events.push(TraceEvent::SpanStarted {
            id: tool_use.id.clone(),
            parent: owner.map(str::to_string),
            kind,
            name,
            detail,
            model: None,
            at,
        });
    }

    fn user(&mut self, user: &UserPayload, at: Instant) -> Vec<TraceEvent> {
        if user
            .parent_tool_use_id
            .as_deref()
            .is_some_and(|parent| self.ended_agents.contains(parent))
        {
            return Vec::new();
        }
        let mut events = Vec::new();
        for result in user.tool_results() {
            self.close_waits_on(&result.tool_use_id, at, &mut events);
            let Some(span) = self.spans.get(&result.tool_use_id) else {
                continue;
            };
            let owner = span.parent.clone();
            if span.agent.is_some() {
                self.agent_returned(&result, user.tool_use_result.as_ref(), at, &mut events);
            } else {
                let outcome = outcome_of(&result);
                self.end_span(&result.tool_use_id, outcome, at, None, &mut events);
            }
            // The owner's next call starts when this one came back.
            if let Some(agent) = owner
                .as_deref()
                .and_then(|owner| self.spans.get_mut(owner))
                .and_then(|span| span.agent.as_mut())
            {
                agent.edge = at;
            }
        }
        events
    }

    /// An `Agent` call's `tool_result`: the subagent is over.
    fn agent_returned(
        &mut self,
        result: &ToolResult,
        tool_use_result: Option<&serde_json::Value>,
        at: Instant,
        events: &mut Vec<TraceEvent>,
    ) {
        let id = result.tool_use_id.as_str();
        let totals = tool_use_result.and_then(AgentToolResult::parse);
        let has_children = self
            .spans
            .values()
            .any(|span| span.parent.as_deref() == Some(id));
        let completed = !result.is_error
            && totals
                .as_ref()
                .and_then(|totals| totals.status.as_deref())
                .is_none_or(|status| status == "completed");
        // The report sends no `assistant` line: the gap since the agent's
        // last edge is that call. An agent that ended with work open, or
        // that did not complete, wrote no report to draw.
        if completed
            && !has_children
            && let Some(edge) = self
                .spans
                .get(id)
                .and_then(|span| span.agent.as_ref())
                .map(|agent| agent.edge)
        {
            let report = format!("{id}/final");
            events.push(TraceEvent::SpanStarted {
                id: report.clone(),
                parent: Some(id.to_string()),
                kind: SpanKind::Llm,
                name: CALL_NAME.into(),
                detail: "Final report".into(),
                model: totals
                    .as_ref()
                    .and_then(|totals| totals.resolved_model.clone()),
                at: edge,
            });
            if let Some(usage) = totals.as_ref().and_then(|totals| totals.usage.as_ref()) {
                events.push(TraceEvent::SpanUsage {
                    id: report.clone(),
                    tokens: tokens_of(usage),
                });
            }
            events.push(ended(report, SpanOutcome::Done, at, None));
        }
        let measured_ms = totals.and_then(|totals| totals.total_duration_ms);
        self.end_span(id, outcome_of(result), at, measured_ms, events);
    }

    fn system(&mut self, system: &SystemPayload, at: Instant) -> Vec<TraceEvent> {
        match system.subtype.as_str() {
            "task_started" => {
                let Some(id) = system.tool_use_id.as_deref() else {
                    return Vec::new();
                };
                // A foreground `Bash` raises `task_started` too, for its own
                // call: only a subagent this fold opened is named.
                let Some(agent) = self.spans.get_mut(id).and_then(|span| span.agent.as_mut())
                else {
                    return Vec::new();
                };
                agent.edge = agent.edge.max(at);
                if let Some(name) = system.subagent_type.as_deref().and_then(line) {
                    agent.name = name;
                }
                if let Some(detail) = system.description.as_deref().and_then(line) {
                    agent.detail = detail;
                }
                let event = TraceEvent::SpanNamed {
                    id: id.to_string(),
                    name: agent.name.clone(),
                    detail: agent.detail.clone(),
                };
                if let Some(task_id) = &system.task_id {
                    self.tasks.insert(task_id.clone(), id.to_string());
                }
                vec![event]
            }
            "task_progress" => {
                let id = system
                    .tool_use_id
                    .clone()
                    .or_else(|| self.tasks.get(system.task_id.as_deref()?).cloned());
                let Some(id) = id.filter(|id| self.is_agent(id)) else {
                    return Vec::new();
                };
                let tokens = system.usage.and_then(|usage| usage.total_tokens);
                let tool_uses = system.usage.and_then(|usage| usage.tool_uses);
                let activity = system
                    .description
                    .as_deref()
                    .and_then(line)
                    .or_else(|| system.last_tool_name.as_deref().and_then(line));
                if tokens.is_none() && tool_uses.is_none() && activity.is_none() {
                    return Vec::new();
                }
                vec![TraceEvent::SpanProgress {
                    id,
                    tokens,
                    tool_uses,
                    activity,
                }]
            }
            // `task_updated` and `task_notification` only confirm an end the
            // `Agent` result already carries.
            _ => Vec::new(),
        }
    }

    fn result(&mut self, result: &ResultPayload, at: Instant) -> Vec<TraceEvent> {
        // A card still waiting when the turn ends, a Stop pressed over it, is
        // answered by nobody: its wait ends here, ahead of the turn.
        let mut waiting: Vec<u64> = self.waits.keys().copied().collect();
        waiting.sort_unstable();
        let mut events: Vec<TraceEvent> = waiting
            .into_iter()
            .map(|request_id| TraceEvent::WaitEnded { request_id, at })
            .collect();
        let interrupted = self.interrupted_this_turn;
        // The turn after an interrupt is unknown too: whether the counters
        // catch up on the spend the interrupt cut short is not known.
        let known = !interrupted && !self.interrupted_last_turn;
        let cost_usd = self.turn_cost(result.total_cost_usd, known);
        let tokens = self.turn_tokens(result, known);
        let outcome = if interrupted {
            TurnOutcome::Interrupted
        } else if result.is_error {
            TurnOutcome::Failed {
                reason: result
                    .result
                    .as_deref()
                    .and_then(line)
                    .unwrap_or_else(|| one_line(&result.subtype)),
            }
        } else {
            TurnOutcome::Done
        };
        self.interrupted_last_turn = interrupted;
        self.interrupted_this_turn = false;
        self.end_turn_scope();
        events.push(TraceEvent::TurnEnded {
            at,
            outcome,
            measured_ms: result.duration_ms,
            cost_usd,
            tokens,
        });
        events
    }

    /// This turn's spend: the process's running total less the last
    /// `result`'s.
    fn turn_cost(&mut self, total: Option<f64>, known: bool) -> Option<f64> {
        turn_share(&mut self.baseline_cost, total, known, |before, now| {
            (now >= before).then_some(now - before)
        })
    }

    /// This turn's tokens, on the same terms, from the `modelUsage` sums.
    fn turn_tokens(&mut self, result: &ResultPayload, known: bool) -> Option<TokenCount> {
        let total = (!result.model_usage.is_empty()).then(|| {
            result.model_usage.values().fold(
                TokenCount {
                    input: 0,
                    output: 0,
                },
                |sum, usage| TokenCount {
                    input: sum
                        .input
                        .saturating_add(usage.input_tokens)
                        .saturating_add(usage.cache_read_input_tokens)
                        .saturating_add(usage.cache_creation_input_tokens),
                    output: sum.output.saturating_add(usage.output_tokens),
                },
            )
        });
        turn_share(&mut self.baseline_tokens, total, known, |before, now| {
            (now.input >= before.input && now.output >= before.output).then(|| TokenCount {
                input: now.input - before.input,
                output: now.output - before.output,
            })
        })
    }

    /// Forgets what belonged to the turn that just ended. Its open spans are
    /// the store's to close; a subagent still open is over for this fold's
    /// purposes, so its late lines are ignored and not misplaced.
    fn end_turn_scope(&mut self) {
        for (id, span) in self.spans.drain() {
            if span.agent.is_some() {
                self.ended_agents.insert(id);
            }
        }
        self.tasks.clear();
        self.waits.clear();
        self.main_call = None;
        self.interrupted_call = None;
        self.seen_calls.clear();
    }

    fn is_agent(&self, id: &str) -> bool {
        self.spans.get(id).is_some_and(|span| span.agent.is_some())
    }

    fn next_sequence(&mut self) -> u64 {
        self.next_seq += 1;
        self.next_seq
    }

    /// Ends a span, its open children first and interrupted, after closing
    /// the waits that sat on it.
    fn end_span(
        &mut self,
        id: &str,
        outcome: SpanOutcome,
        at: Instant,
        measured_ms: Option<u64>,
        events: &mut Vec<TraceEvent>,
    ) {
        // Only a subagent has children, so a tool's end costs no scan.
        let mut children: Vec<(u64, String)> = if self.is_agent(id) {
            self.spans
                .iter()
                .filter(|(_, span)| span.parent.as_deref() == Some(id))
                .map(|(child, span)| (span.seq, child.clone()))
                .collect()
        } else {
            Vec::new()
        };
        children.sort();
        for (_, child) in children {
            self.end_span(&child, SpanOutcome::Interrupted, at, None, events);
        }
        self.close_waits_on(id, at, events);
        let Some(span) = self.spans.remove(id) else {
            return;
        };
        if span.agent.is_some() {
            self.ended_agents.insert(id.to_string());
            self.tasks.retain(|_, agent| agent != id);
        }
        events.push(ended(id.to_string(), outcome, at, measured_ms));
    }

    /// Closes the waits on call `id`: the ones placed on its span and the
    /// ones whose request is about it.
    fn close_waits_on(&mut self, id: &str, at: Instant, events: &mut Vec<TraceEvent>) {
        let mut closing: Vec<u64> = self
            .waits
            .iter()
            .filter(|(_, wait)| {
                wait.span.as_deref() == Some(id) || wait.tool_use_id.as_deref() == Some(id)
            })
            .map(|(request_id, _)| *request_id)
            .collect();
        closing.sort_unstable();
        for request_id in closing {
            self.waits.remove(&request_id);
            events.push(TraceEvent::WaitEnded { request_id, at });
        }
    }
}

fn ended(id: String, outcome: SpanOutcome, at: Instant, measured_ms: Option<u64>) -> TraceEvent {
    TraceEvent::SpanEnded {
        id,
        outcome,
        at,
        measured_ms,
    }
}

fn call_started(
    id: String,
    parent: Option<String>,
    model: Option<String>,
    at: Instant,
) -> TraceEvent {
    TraceEvent::SpanStarted {
        id,
        parent,
        kind: SpanKind::Llm,
        name: CALL_NAME.into(),
        detail: String::new(),
        model,
        at,
    }
}

/// How a tool call ended: failed with the first line of its error.
fn outcome_of(result: &ToolResult) -> SpanOutcome {
    if result.is_error {
        SpanOutcome::Failed {
            note: one_line(&result.text),
        }
    } else {
        SpanOutcome::Done
    }
}

/// Everything the model read, and what it wrote.
fn tokens_of(usage: &MessageUsage) -> TokenCount {
    TokenCount {
        input: usage
            .input_tokens
            .saturating_add(usage.cache_read_input_tokens)
            .saturating_add(usage.cache_creation_input_tokens),
        output: usage.output_tokens,
    }
}

/// A string field of a tool's input as one bounded line, `None` when it is
/// absent or nothing is left of it.
fn line_of(input: &serde_json::Value, key: &str) -> Option<String> {
    input
        .get(key)
        .and_then(serde_json::Value::as_str)
        .and_then(line)
}

/// `text` as one bounded line, `None` when nothing is left of it.
fn line(text: &str) -> Option<String> {
    Some(one_line(text)).filter(|line| !line.is_empty())
}

/// A turn's own share of a counter that runs for the whole process: the
/// total less the last `result`'s, which `share` takes. The baseline moves on
/// every `result`, known or not, and a share is unknown when the turn's own
/// part cannot be told apart (`known` is false), when either side is missing,
/// or when `share` finds the counter went down.
fn turn_share<T: Clone, R>(
    baseline: &mut Option<T>,
    total: Option<T>,
    known: bool,
    share: impl FnOnce(&T, &T) -> Option<R>,
) -> Option<R> {
    let before = std::mem::replace(baseline, total.clone());
    match (known, before, total) {
        (true, Some(before), Some(now)) => share(&before, &now),
        _ => None,
    }
}

/// The first non-empty line, cut to [`LINE_LIMIT`] characters.
fn one_line(text: &str) -> String {
    let line = text
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or_default();
    match line.char_indices().nth(LINE_LIMIT) {
        Some((cut, _)) => format!("{}…", &line[..cut]),
        None => line.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use serde_json::{Value, json};
    use sirio_claude::CanUseTool;

    use super::*;

    const MESSAGE_START: &str =
        include_str!("../../../sirio_claude/tests/fixtures/trace_message_start.json");
    const MESSAGE_DELTA: &str =
        include_str!("../../../sirio_claude/tests/fixtures/trace_message_delta.json");
    const TASK_STARTED: &str =
        include_str!("../../../sirio_claude/tests/fixtures/trace_task_started.json");
    const TASK_PROGRESS: &str =
        include_str!("../../../sirio_claude/tests/fixtures/trace_task_progress.json");
    const SUBAGENT_ASSISTANT: &str =
        include_str!("../../../sirio_claude/tests/fixtures/trace_subagent_assistant.json");
    const AGENT_RESULT: &str =
        include_str!("../../../sirio_claude/tests/fixtures/trace_agent_result.json");
    const RESULT_INTERRUPTED: &str =
        include_str!("../../../sirio_claude/tests/fixtures/trace_result_interrupted.json");
    const CAN_USE_TOOL_SUBAGENT: &str =
        include_str!("../../../sirio_claude/tests/fixtures/trace_can_use_tool_subagent.json");
    const RESULT_SUCCESS: &str =
        include_str!("../../../sirio_claude/tests/fixtures/result_success.json");
    const TASK_STARTED_FOREGROUND: &str =
        include_str!("../../../sirio_claude/tests/fixtures/task_started_foreground.json");
    const TASK_NOTIFICATION: &str =
        include_str!("../../../sirio_claude/tests/fixtures/task_notification.json");

    /// The `Agent` call, and the ids around it, in the first capture.
    const AGENT: &str = "toolu_01YNUiEeefJ32LABu5jnbfHM";
    const BASH: &str = "toolu_01RgMcYEVioJhax5GzXjcGhA";
    const HAIKU: &str = "claude-haiku-5-5";

    /// One base instant plus offsets: the fold never looks at the clock.
    struct Clock(Instant);

    impl Clock {
        fn new() -> Self {
            Self(Instant::now())
        }

        fn at(&self, ms: u64) -> Instant {
            self.0 + Duration::from_millis(ms)
        }
    }

    fn parse(line: &str) -> CliMessage {
        CliMessage::parse(line).expect("fixture line parses")
    }

    fn msg(line: Value) -> CliMessage {
        parse(&line.to_string())
    }

    fn tool_use(id: &str, name: &str, input: Value) -> Value {
        json!({"type": "tool_use", "id": id, "name": name, "input": input})
    }

    /// An `assistant` line carrying `blocks` of the model call `id`.
    fn assistant(parent: Option<&str>, id: &str, blocks: Vec<Value>) -> CliMessage {
        msg(json!({
            "type": "assistant",
            "message": {
                "id": id, "model": HAIKU, "content": blocks,
                "usage": {"input_tokens": 2, "output_tokens": 8,
                          "cache_read_input_tokens": 0, "cache_creation_input_tokens": 100}
            },
            "parent_tool_use_id": parent
        }))
    }

    fn tool_result(parent: Option<&str>, id: &str, is_error: bool, text: &str) -> CliMessage {
        msg(json!({
            "type": "user",
            "message": {"role": "user", "content": [
                {"type": "tool_result", "tool_use_id": id, "content": text, "is_error": is_error}
            ]},
            "parent_tool_use_id": parent
        }))
    }

    /// The `tool_result` that ends the `Agent` call `id`, with the totals
    /// the CLI reports for the agent.
    fn agent_result(parent: Option<&str>, id: &str, duration_ms: Option<u64>) -> CliMessage {
        msg(json!({
            "type": "user",
            "message": {"role": "user", "content": [
                {"type": "tool_result", "tool_use_id": id, "content": "report"}
            ]},
            "parent_tool_use_id": parent,
            "tool_use_result": {
                "status": "completed", "agentType": "general-purpose",
                "resolvedModel": HAIKU, "totalDurationMs": duration_ms,
                "totalTokens": 26197,
                "usage": {"input_tokens": 2, "output_tokens": 187,
                          "cache_read_input_tokens": 25867, "cache_creation_input_tokens": 141}
            }
        }))
    }

    fn task_started(task_id: &str, tool_use_id: &str, depth: u32) -> CliMessage {
        msg(json!({
            "type": "system", "subtype": "task_started", "task_id": task_id,
            "tool_use_id": tool_use_id, "description": "work", "subagent_type": "worker",
            "spawn_depth": depth, "task_type": "local_agent"
        }))
    }

    fn result_line(cost: Option<f64>, usage: (u64, u64, u64, u64)) -> CliMessage {
        msg(json!({
            "type": "result", "subtype": "success", "is_error": false, "duration_ms": 1000,
            "result": "ok", "stop_reason": "end_turn", "total_cost_usd": cost,
            "modelUsage": {HAIKU: {
                "inputTokens": usage.0, "outputTokens": usage.1,
                "cacheReadInputTokens": usage.2, "cacheCreationInputTokens": usage.3
            }}
        }))
    }

    fn llm(id: &str, parent: Option<&str>, at: Instant) -> TraceEvent {
        TraceEvent::SpanStarted {
            id: id.into(),
            parent: parent.map(Into::into),
            kind: SpanKind::Llm,
            name: "Model call".into(),
            detail: String::new(),
            model: Some(HAIKU.into()),
            at,
        }
    }

    /// A subagent's final report: a call whose only mark is its detail.
    fn report(agent: &str, at: Instant) -> TraceEvent {
        match llm(&format!("{agent}/final"), Some(agent), at) {
            TraceEvent::SpanStarted {
                id,
                parent,
                kind,
                name,
                model,
                at,
                ..
            } => TraceEvent::SpanStarted {
                id,
                parent,
                kind,
                name,
                detail: "Final report".into(),
                model,
                at,
            },
            other => other,
        }
    }

    fn tool(id: &str, parent: Option<&str>, name: &str, detail: &str, at: Instant) -> TraceEvent {
        TraceEvent::SpanStarted {
            id: id.into(),
            parent: parent.map(Into::into),
            kind: SpanKind::Tool,
            name: name.into(),
            detail: detail.into(),
            model: None,
            at,
        }
    }

    fn usage(id: &str, input: u64, output: u64) -> TraceEvent {
        TraceEvent::SpanUsage {
            id: id.into(),
            tokens: TokenCount { input, output },
        }
    }

    fn span_ended(
        id: &str,
        outcome: SpanOutcome,
        at: Instant,
        measured_ms: Option<u64>,
    ) -> TraceEvent {
        TraceEvent::SpanEnded {
            id: id.into(),
            outcome,
            at,
            measured_ms,
        }
    }

    /// Every `SpanStarted` in `events`, as (id, parent).
    fn parents(events: &[TraceEvent]) -> Vec<(String, Option<String>)> {
        events
            .iter()
            .filter_map(|event| match event {
                TraceEvent::SpanStarted { id, parent, .. } => Some((id.clone(), parent.clone())),
                _ => None,
            })
            .collect()
    }

    /// The first capture's opening: the main thread starts the `Agent` call,
    /// `task_started` names it, and the subagent runs `Bash` and gets its
    /// result, which leaves 6 849 ms as the agent's last edge.
    fn run_capture_prefix(fold: &mut TraceFold, clock: &Clock) {
        fold.prompt_sent(clock.at(0));
        fold.line(
            &assistant(
                None,
                "msg_main",
                vec![tool_use(AGENT, "Agent", json!({"prompt": "go"}))],
            ),
            clock.at(4_879),
        );
        fold.line(&parse(TASK_STARTED), clock.at(4_903));
        fold.line(&parse(SUBAGENT_ASSISTANT), clock.at(6_785));
        fold.line(
            &tool_result(Some(AGENT), BASH, false, "README.md\nsrc"),
            clock.at(6_849),
        );
    }

    /// Ends a turn with `result` and returns its one event.
    fn finish(fold: &mut TraceFold, clock: &Clock, ms: u64, result: &CliMessage) -> TraceEvent {
        fold.prompt_sent(clock.at(ms));
        let mut events = fold.line(result, clock.at(ms + 10));
        assert_eq!(events.len(), 1, "a result is one TurnEnded: {events:?}");
        events.remove(0)
    }

    fn turn_end_cost(event: &TraceEvent) -> Option<f64> {
        match event {
            TraceEvent::TurnEnded { cost_usd, .. } => *cost_usd,
            other => panic!("not a TurnEnded: {other:?}"),
        }
    }

    fn turn_end_tokens(event: &TraceEvent) -> Option<TokenCount> {
        match event {
            TraceEvent::TurnEnded { tokens, .. } => tokens.clone(),
            other => panic!("not a TurnEnded: {other:?}"),
        }
    }

    fn turn_end_outcome(event: &TraceEvent) -> TurnOutcome {
        match event {
            TraceEvent::TurnEnded { outcome, .. } => outcome.clone(),
            other => panic!("not a TurnEnded: {other:?}"),
        }
    }

    fn assert_close(actual: Option<f64>, expected: f64) {
        let actual = actual.expect("a known cost");
        assert!(
            (actual - expected).abs() < 1e-9,
            "{actual} is not {expected}"
        );
    }

    #[test]
    fn a_main_thread_call_spans_message_start_to_message_stop() {
        let clock = Clock::new();
        let mut fold = TraceFold::new(false);
        assert_eq!(
            fold.prompt_sent(clock.at(0)),
            vec![TraceEvent::TurnStarted { at: clock.at(0) }]
        );
        let call = "msg_011CgG43GxW2BhRXx8chosrS";
        assert_eq!(
            fold.line(&parse(MESSAGE_START), clock.at(100)),
            vec![llm(call, None, clock.at(100))]
        );
        // The delta's usage is the call's final: 39 276 newly cached tokens
        // and 2 fresh ones read, 632 written.
        assert_eq!(
            fold.line(&parse(MESSAGE_DELTA), clock.at(2_700)),
            vec![usage(call, 39_278, 632)]
        );
        let stop = msg(json!({
            "type": "stream_event", "event": {"type": "message_stop"},
            "parent_tool_use_id": null
        }));
        assert_eq!(
            fold.line(&stop, clock.at(2_710)),
            vec![span_ended(call, SpanOutcome::Done, clock.at(2_710), None)]
        );
    }

    #[test]
    fn repeated_message_ids_are_one_call() {
        let clock = Clock::new();
        let mut fold = TraceFold::new(false);
        fold.prompt_sent(clock.at(0));
        fold.line(&parse(MESSAGE_START), clock.at(100));
        let call = "msg_011CgG43GxW2BhRXx8chosrS";
        // The main thread's call opened at `message_start`: the assistant
        // lines that follow, a thinking block and then a tool, are its
        // blocks and not calls of their own.
        assert_eq!(
            fold.line(
                &assistant(
                    None,
                    call,
                    vec![json!({"type": "thinking", "thinking": ""})]
                ),
                clock.at(1_500)
            ),
            Vec::new()
        );
        assert_eq!(
            fold.line(
                &assistant(
                    None,
                    call,
                    vec![tool_use(
                        "toolu_R",
                        "Read",
                        json!({"file_path": "/repo/a.rs"})
                    )]
                ),
                clock.at(2_000)
            ),
            vec![tool("toolu_R", None, "Read", "/repo/a.rs", clock.at(2_000))]
        );

        // A subagent has no `message_start`: the first line of an id is the
        // call, and a second line of it is not another.
        fold.line(
            &assistant(
                None,
                "msg_main2",
                vec![tool_use("toolu_X", "Agent", json!({}))],
            ),
            clock.at(2_100),
        );
        fold.line(&task_started("task_x", "toolu_X", 1), clock.at(2_110));
        assert_eq!(
            fold.line(
                &assistant(
                    Some("toolu_X"),
                    "msg_sub",
                    vec![json!({"type": "thinking", "thinking": ""})]
                ),
                clock.at(3_000)
            ),
            vec![
                llm("msg_sub", Some("toolu_X"), clock.at(2_110)),
                usage("msg_sub", 102, 8),
                span_ended("msg_sub", SpanOutcome::Done, clock.at(3_000), None),
            ]
        );
        assert_eq!(
            fold.line(
                &assistant(
                    Some("toolu_X"),
                    "msg_sub",
                    vec![tool_use("toolu_B", "Bash", json!({"command": "ls"}))]
                ),
                clock.at(3_010)
            ),
            vec![tool(
                "toolu_B",
                Some("toolu_X"),
                "Bash",
                "ls",
                clock.at(3_010)
            )]
        );
    }

    #[test]
    fn an_agent_tool_use_opens_a_subagent_and_task_started_names_it() {
        let clock = Clock::new();
        let mut fold = TraceFold::new(false);
        fold.prompt_sent(clock.at(0));
        assert_eq!(
            fold.line(
                &assistant(
                    None,
                    "msg_main",
                    vec![tool_use(AGENT, "Agent", json!({"prompt": "go"}))]
                ),
                clock.at(4_879)
            ),
            vec![TraceEvent::SpanStarted {
                id: AGENT.into(),
                parent: None,
                kind: SpanKind::Subagent,
                name: "Agent".into(),
                detail: String::new(),
                model: None,
                at: clock.at(4_879),
            }]
        );
        assert_eq!(
            fold.line(&parse(TASK_STARTED), clock.at(4_903)),
            vec![TraceEvent::SpanNamed {
                id: AGENT.into(),
                name: "general-purpose".into(),
                detail: "List the directory and read the README".into(),
            }]
        );
        assert_eq!(
            fold.line(&parse(TASK_PROGRESS), clock.at(6_780)),
            vec![TraceEvent::SpanProgress {
                id: AGENT.into(),
                tokens: Some(19_991),
                tool_uses: Some(1),
                activity: Some("Running List files in the working directory".into()),
            }]
        );

        // Before `task_started` the call's own input names the span, and the
        // pre-rename `Task` is a subagent like `Agent`.
        assert_eq!(
            fold.line(
                &assistant(
                    None,
                    "msg_main",
                    vec![tool_use(
                        "toolu_T",
                        "Task",
                        json!({"subagent_type": "reviewer", "description": "Review the diff"})
                    )]
                ),
                clock.at(7_000)
            ),
            vec![TraceEvent::SpanStarted {
                id: "toolu_T".into(),
                parent: None,
                kind: SpanKind::Subagent,
                name: "reviewer".into(),
                detail: "Review the diff".into(),
                model: None,
                at: clock.at(7_000),
            }]
        );
    }

    #[test]
    fn a_subagent_call_starts_at_its_previous_edge() {
        let clock = Clock::new();
        let mut fold = TraceFold::new(false);
        fold.prompt_sent(clock.at(0));
        fold.line(
            &assistant(
                None,
                "msg_main",
                vec![tool_use(AGENT, "Agent", json!({"prompt": "go"}))],
            ),
            clock.at(4_879),
        );
        fold.line(&parse(TASK_STARTED), clock.at(4_903));

        // The first call runs from `task_started`, not from the `Agent`
        // call 24 ms before it.
        assert_eq!(
            fold.line(&parse(SUBAGENT_ASSISTANT), clock.at(6_785)),
            vec![
                llm("msg_011CgG43YcaswnMdLpDH97Lj", Some(AGENT), clock.at(4_903)),
                usage("msg_011CgG43YcaswnMdLpDH97Lj", 19_689, 8),
                span_ended(
                    "msg_011CgG43YcaswnMdLpDH97Lj",
                    SpanOutcome::Done,
                    clock.at(6_785),
                    None
                ),
                tool(BASH, Some(AGENT), "Bash", "ls", clock.at(6_785)),
            ]
        );
        assert_eq!(
            fold.line(
                &tool_result(Some(AGENT), BASH, false, "README.md\nsrc"),
                clock.at(6_849)
            ),
            vec![span_ended(BASH, SpanOutcome::Done, clock.at(6_849), None)]
        );
        // The next call runs from that tool's result: not from the previous
        // call's end (6 785) and not from the start (4 903).
        assert_eq!(
            fold.line(
                &assistant(
                    Some(AGENT),
                    "msg_next",
                    vec![tool_use(
                        "toolu_R",
                        "Read",
                        json!({"file_path": "/repo/README.md"})
                    )]
                ),
                clock.at(7_935)
            ),
            vec![
                llm("msg_next", Some(AGENT), clock.at(6_849)),
                usage("msg_next", 102, 8),
                span_ended("msg_next", SpanOutcome::Done, clock.at(7_935), None),
                tool(
                    "toolu_R",
                    Some(AGENT),
                    "Read",
                    "/repo/README.md",
                    clock.at(7_935)
                ),
            ]
        );
    }

    #[test]
    fn a_subagent_final_report_is_the_gap_before_its_end_with_the_agent_results_usage() {
        let clock = Clock::new();
        let mut fold = TraceFold::new(false);
        run_capture_prefix(&mut fold, &clock);
        fold.line(
            &assistant(
                Some(AGENT),
                "msg_next",
                vec![tool_use(
                    "toolu_R",
                    "Read",
                    json!({"file_path": "/repo/README.md"}),
                )],
            ),
            clock.at(7_935),
        );
        fold.line(
            &tool_result(Some(AGENT), "toolu_R", false, "# money-kit"),
            clock.at(7_957),
        );

        // The report sends no `assistant` line: the gap from the last tool
        // result to the `Agent` result is that call, with the usage the
        // result carries (2 + 25 867 + 141 read, 187 written).
        assert_eq!(
            fold.line(&parse(AGENT_RESULT), clock.at(9_428)),
            vec![
                report(AGENT, clock.at(7_957)),
                usage(&format!("{AGENT}/final"), 26_010, 187),
                span_ended(
                    &format!("{AGENT}/final"),
                    SpanOutcome::Done,
                    clock.at(9_428),
                    None
                ),
                span_ended(AGENT, SpanOutcome::Done, clock.at(9_428), Some(4_537)),
            ]
        );
    }

    #[test]
    fn nested_subagents_hang_under_their_parent() {
        let clock = Clock::new();
        let mut fold = TraceFold::new(false);
        fold.prompt_sent(clock.at(0));
        let mut started = Vec::new();
        for (line, ms) in [
            (
                assistant(None, "m0", vec![tool_use("toolu_x1", "Agent", json!({}))]),
                100,
            ),
            (task_started("t1", "toolu_x1", 1), 110),
            (
                assistant(
                    Some("toolu_x1"),
                    "m1",
                    vec![tool_use("toolu_x2", "Agent", json!({}))],
                ),
                500,
            ),
            (task_started("t2", "toolu_x2", 2), 510),
            (
                assistant(
                    Some("toolu_x2"),
                    "m2",
                    vec![tool_use("toolu_x3", "Agent", json!({}))],
                ),
                900,
            ),
            (task_started("t3", "toolu_x3", 3), 910),
            (
                assistant(
                    Some("toolu_x3"),
                    "m3",
                    vec![tool_use("toolu_R", "Read", json!({"file_path": "/a"}))],
                ),
                1_300,
            ),
        ] {
            started.extend(parents(&fold.line(&line, clock.at(ms))));
        }
        let own = |id: &str| (id.to_string(), None::<String>);
        let under = |id: &str, parent: &str| (id.to_string(), Some(parent.to_string()));
        assert_eq!(
            started,
            vec![
                own("toolu_x1"),
                under("m1", "toolu_x1"),
                under("toolu_x2", "toolu_x1"),
                under("m2", "toolu_x2"),
                under("toolu_x3", "toolu_x2"),
                under("m3", "toolu_x3"),
                under("toolu_R", "toolu_x3"),
            ]
        );

        // The inner agent's return is its parent's edge: the parent's final
        // report runs from there.
        fold.line(
            &tool_result(Some("toolu_x3"), "toolu_R", false, "ok"),
            clock.at(1_500),
        );
        fold.line(
            &agent_result(Some("toolu_x2"), "toolu_x3", Some(600)),
            clock.at(1_700),
        );
        let events = fold.line(
            &agent_result(Some("toolu_x1"), "toolu_x2", Some(1_200)),
            clock.at(2_000),
        );
        assert_eq!(events.first(), Some(&report("toolu_x2", clock.at(1_700))));
    }

    #[test]
    fn interleaved_parallel_subagents_keep_their_own_edges() {
        let clock = Clock::new();
        let mut fold = TraceFold::new(false);
        fold.prompt_sent(clock.at(0));
        fold.line(
            &assistant(
                None,
                "m0",
                vec![
                    tool_use("toolu_a", "Agent", json!({})),
                    tool_use("toolu_b", "Agent", json!({})),
                ],
            ),
            clock.at(100),
        );
        fold.line(&task_started("ta", "toolu_a", 1), clock.at(1_000));
        fold.line(&task_started("tb", "toolu_b", 1), clock.at(1_010));
        let bash = |id: &str| tool_use(id, "Bash", json!({"command": "ls"}));
        let read = |id: &str| tool_use(id, "Read", json!({"file_path": "/a"}));

        let mut calls = Vec::new();
        let mut tools = Vec::new();
        for (line, ms) in [
            (assistant(Some("toolu_a"), "ma1", vec![bash("a1")]), 3_000),
            (assistant(Some("toolu_b"), "mb1", vec![bash("b1")]), 3_100),
            (tool_result(Some("toolu_a"), "a1", false, "ok"), 3_200),
            (tool_result(Some("toolu_b"), "b1", false, "ok"), 3_300),
            (assistant(Some("toolu_b"), "mb2", vec![read("b2")]), 4_000),
            (assistant(Some("toolu_a"), "ma2", vec![read("a2")]), 4_100),
        ] {
            for event in fold.line(&line, clock.at(ms)) {
                if let TraceEvent::SpanStarted {
                    id,
                    parent,
                    kind,
                    at,
                    ..
                } = event
                {
                    match kind {
                        SpanKind::Llm => calls.push((id, parent.expect("a parent"), at)),
                        _ => tools.push((id, parent.expect("a parent"))),
                    }
                }
            }
        }
        let call =
            |id: &str, parent: &str, ms: u64| (id.to_string(), parent.to_string(), clock.at(ms));
        assert_eq!(
            calls,
            vec![
                call("ma1", "toolu_a", 1_000),
                call("mb1", "toolu_b", 1_010),
                call("mb2", "toolu_b", 3_300),
                call("ma2", "toolu_a", 3_200),
            ]
        );
        let tool_in = |id: &str, parent: &str| (id.to_string(), parent.to_string());
        assert_eq!(
            tools,
            vec![
                tool_in("a1", "toolu_a"),
                tool_in("b1", "toolu_b"),
                tool_in("b2", "toolu_b"),
                tool_in("a2", "toolu_a"),
            ]
        );
    }

    #[test]
    fn a_line_whose_parent_is_unknown_hangs_under_the_root() {
        let clock = Clock::new();
        let mut fold = TraceFold::new(false);
        fold.prompt_sent(clock.at(0));
        // No `Agent` call ever named `toolu_ghost`: its tool is still shown,
        // under the root, and no call is invented for it because there is no
        // edge to time one from.
        assert_eq!(
            fold.line(
                &assistant(
                    Some("toolu_ghost"),
                    "msg_g",
                    vec![tool_use("toolu_R", "Read", json!({"file_path": "/a"}))]
                ),
                clock.at(500)
            ),
            vec![tool("toolu_R", None, "Read", "/a", clock.at(500))]
        );
        assert_eq!(
            fold.line(
                &tool_result(Some("toolu_ghost"), "toolu_R", false, "ok"),
                clock.at(600)
            ),
            vec![span_ended("toolu_R", SpanOutcome::Done, clock.at(600), None)]
        );
    }

    #[test]
    fn a_tool_result_with_no_tool_use_opens_nothing() {
        let clock = Clock::new();
        let mut fold = TraceFold::new(false);
        fold.prompt_sent(clock.at(0));
        fold.line(
            &assistant(
                None,
                "m0",
                vec![tool_use("toolu_R", "Read", json!({"file_path": "/a"}))],
            ),
            clock.at(100),
        );
        assert_eq!(
            fold.line(
                &tool_result(None, "toolu_ghost", false, "ok"),
                clock.at(200)
            ),
            Vec::new()
        );
        // The call that is open stays open and ends on its own result.
        assert_eq!(
            fold.line(&tool_result(None, "toolu_R", false, "ok"), clock.at(300)),
            vec![span_ended("toolu_R", SpanOutcome::Done, clock.at(300), None)]
        );
    }

    #[test]
    fn a_subagent_ending_with_an_open_tool_ends_that_tool_interrupted() {
        let clock = Clock::new();
        let mut fold = TraceFold::new(false);
        run_capture_prefix(&mut fold, &clock);
        fold.line(
            &assistant(
                Some(AGENT),
                "msg_next",
                vec![tool_use(
                    "toolu_open",
                    "Bash",
                    json!({"command": "sleep 100"}),
                )],
            ),
            clock.at(7_935),
        );
        // The tool never returned: it ends interrupted, before its parent,
        // and the gap is not drawn as a report the agent never wrote.
        assert_eq!(
            fold.line(&parse(AGENT_RESULT), clock.at(9_428)),
            vec![
                span_ended(
                    "toolu_open",
                    SpanOutcome::Interrupted,
                    clock.at(9_428),
                    None
                ),
                span_ended(AGENT, SpanOutcome::Done, clock.at(9_428), Some(4_537)),
            ]
        );
    }

    #[test]
    fn parented_lines_after_their_subagent_ended_are_ignored() {
        let clock = Clock::new();
        let mut fold = TraceFold::new(false);
        run_capture_prefix(&mut fold, &clock);
        // A background agent ends at its launch `tool_result` and goes on
        // writing. The end is the precondition: the lines below are ignored
        // because the agent is over, not because it was never there.
        let end = fold.line(&parse(AGENT_RESULT), clock.at(7_000));
        assert!(
            end.contains(&span_ended(
                AGENT,
                SpanOutcome::Done,
                clock.at(7_000),
                Some(4_537)
            )),
            "{end:?}"
        );
        assert_eq!(
            fold.line(
                &assistant(
                    Some(AGENT),
                    "msg_late",
                    vec![tool_use("toolu_late", "Bash", json!({"command": "ls"}))]
                ),
                clock.at(8_000)
            ),
            Vec::new()
        );
        assert_eq!(
            fold.line(
                &tool_result(Some(AGENT), "toolu_late", false, "ok"),
                clock.at(8_100)
            ),
            Vec::new()
        );
        assert_eq!(
            fold.line(&parse(TASK_PROGRESS), clock.at(8_200)),
            Vec::new()
        );
    }

    #[test]
    fn task_events_for_a_non_agent_task_are_ignored() {
        let clock = Clock::new();
        let mut fold = TraceFold::new(false);
        fold.prompt_sent(clock.at(0));
        // A foreground `Bash` raises `task_started` for its own call id.
        let bash = "toolu_011eUvQKr2fHYQbnd21omenv";
        assert_eq!(
            fold.line(
                &assistant(
                    None,
                    "m0",
                    vec![tool_use(bash, "Bash", json!({"command": "sleep 2"}))]
                ),
                clock.at(100)
            ),
            vec![tool(bash, None, "Bash", "sleep 2", clock.at(100))]
        );
        assert_eq!(
            fold.line(&parse(TASK_STARTED_FOREGROUND), clock.at(110)),
            Vec::new()
        );
        assert_eq!(
            fold.line(&parse(TASK_NOTIFICATION), clock.at(120)),
            Vec::new()
        );
        // The task did not turn the tool into a subagent.
        assert_eq!(
            fold.line(&tool_result(None, bash, false, "done"), clock.at(2_100)),
            vec![span_ended(bash, SpanOutcome::Done, clock.at(2_100), None)]
        );
    }

    #[test]
    fn a_permission_wait_lands_on_its_span_or_the_root() {
        let clock = Clock::new();
        let mut fold = TraceFold::new(false);
        fold.prompt_sent(clock.at(0));
        fold.line(
            &assistant(
                None,
                "m0",
                vec![tool_use("toolu_B", "Bash", json!({"command": "rm x"}))],
            ),
            clock.at(100),
        );
        assert_eq!(
            fold.permission_asked(Some("toolu_B"), None, 1, clock.at(150)),
            vec![TraceEvent::WaitStarted {
                span: Some("toolu_B".into()),
                request_id: 1,
                at: clock.at(150)
            }]
        );
        assert_eq!(
            fold.permission_answered(1, clock.at(900)),
            vec![TraceEvent::WaitEnded {
                request_id: 1,
                at: clock.at(900)
            }]
        );
        // No tool named, or one no span matches: the root waits.
        assert_eq!(
            fold.permission_asked(None, None, 2, clock.at(1_000)),
            vec![TraceEvent::WaitStarted {
                span: None,
                request_id: 2,
                at: clock.at(1_000)
            }]
        );
        assert_eq!(
            fold.permission_asked(Some("toolu_ghost"), None, 3, clock.at(1_100)),
            vec![TraceEvent::WaitStarted {
                span: None,
                request_id: 3,
                at: clock.at(1_100)
            }]
        );
        // An answer to nothing open says nothing.
        assert_eq!(fold.permission_answered(99, clock.at(1_200)), Vec::new());
    }

    #[test]
    fn a_subagent_permission_before_its_tool_line_waits_on_its_subagent() {
        let clock = Clock::new();
        let mut fold = TraceFold::new(false);
        fold.prompt_sent(clock.at(0));
        let value: Value = serde_json::from_str(CAN_USE_TOOL_SUBAGENT).expect("fixture is JSON");
        let request = CanUseTool::parse(&value).expect("a can_use_tool request");
        let (tool_use_id, agent_id) = (
            request.tool_use_id.expect("a tool id"),
            request.agent_id.expect("an agent id"),
        );
        let agent = "toolu_01BUnhq1CMpksZbqG3CnLgCr";
        fold.line(
            &assistant(None, "m0", vec![tool_use(agent, "Agent", json!({}))]),
            clock.at(100),
        );
        fold.line(&task_started(&agent_id, agent, 1), clock.at(120));

        // The permission beat its tool's own line: nothing is called
        // `tool_use_id` yet, so the wait goes on the agent the request names.
        assert_eq!(
            fold.permission_asked(Some(&tool_use_id), Some(&agent_id), 7, clock.at(200)),
            vec![TraceEvent::WaitStarted {
                span: Some(agent.into()),
                request_id: 7,
                at: clock.at(200)
            }]
        );
        fold.line(
            &assistant(
                Some(agent),
                "m1",
                vec![tool_use(
                    &tool_use_id,
                    "Bash",
                    json!({"command": "mkdir x"}),
                )],
            ),
            clock.at(222),
        );
        // The tool's result closes the wait through the request's own id,
        // although the wait sits on the agent and not on the tool.
        let events = fold.line(
            &tool_result(Some(agent), &tool_use_id, true, "not allowed"),
            clock.at(260),
        );
        assert_eq!(
            events,
            vec![
                TraceEvent::WaitEnded {
                    request_id: 7,
                    at: clock.at(260)
                },
                span_ended(
                    &tool_use_id,
                    SpanOutcome::Failed {
                        note: "not allowed".into()
                    },
                    clock.at(260),
                    None
                ),
            ]
        );
    }

    #[test]
    fn a_tool_result_closes_its_open_wait() {
        let clock = Clock::new();
        let mut fold = TraceFold::new(false);
        fold.prompt_sent(clock.at(0));
        fold.line(
            &assistant(
                None,
                "m0",
                vec![tool_use("toolu_B", "Bash", json!({"command": "mkdir x"}))],
            ),
            clock.at(100),
        );
        fold.permission_asked(Some("toolu_B"), None, 5, clock.at(150));

        // The worker's five-minute deny reached the CLI, not this fold: the
        // tool's own error result is what shows the wait is over.
        let text = "not allowed by the probe\nsecond line";
        assert_eq!(
            fold.line(&tool_result(None, "toolu_B", true, text), clock.at(300_150)),
            vec![
                TraceEvent::WaitEnded {
                    request_id: 5,
                    at: clock.at(300_150)
                },
                span_ended(
                    "toolu_B",
                    SpanOutcome::Failed {
                        note: "not allowed by the probe".into()
                    },
                    clock.at(300_150),
                    None
                ),
            ]
        );
        // Closed once: the answer that follows has nothing left to close.
        assert_eq!(fold.permission_answered(5, clock.at(300_160)), Vec::new());

        // And the other way round: an answer first leaves nothing for the
        // result.
        fold.line(
            &assistant(
                None,
                "m1",
                vec![tool_use("toolu_C", "Bash", json!({"command": "ls"}))],
            ),
            clock.at(301_000),
        );
        fold.permission_asked(Some("toolu_C"), None, 6, clock.at(301_050));
        assert_eq!(
            fold.permission_answered(6, clock.at(301_500)),
            vec![TraceEvent::WaitEnded {
                request_id: 6,
                at: clock.at(301_500)
            }]
        );
        assert_eq!(
            fold.line(
                &tool_result(None, "toolu_C", false, "ok"),
                clock.at(301_600)
            ),
            vec![span_ended("toolu_C", SpanOutcome::Done, clock.at(301_600), None)]
        );
    }

    #[test]
    fn an_interrupted_turn_is_interrupted_and_costs_nothing_known_nor_does_the_next() {
        let clock = Clock::new();
        let mut fold = TraceFold::new(false);
        let first = finish(
            &mut fold,
            &clock,
            0,
            &result_line(Some(0.005), (10, 100, 1_000, 500)),
        );
        assert_close(turn_end_cost(&first), 0.005);
        assert_eq!(
            turn_end_tokens(&first),
            Some(TokenCount {
                input: 1_510,
                output: 100
            })
        );

        // The capture's interrupt: `error_during_execution`, `is_error`,
        // total higher than the turn before, so only the interrupt can make
        // its cost unknown.
        fold.prompt_sent(clock.at(1_000));
        fold.interrupt_sent();
        let interrupted = fold.line(&parse(RESULT_INTERRUPTED), clock.at(7_000));
        assert_eq!(
            interrupted,
            vec![TraceEvent::TurnEnded {
                at: clock.at(7_000),
                outcome: TurnOutcome::Interrupted,
                measured_ms: Some(6_002),
                cost_usd: None,
                tokens: None,
            }]
        );

        // The turn after it is unknown too, though its totals are higher
        // than the baseline and a difference would have been computable.
        let after = finish(
            &mut fold,
            &clock,
            8_000,
            &result_line(Some(0.012), (100, 3_000, 90_000, 50_000)),
        );
        assert_eq!(turn_end_outcome(&after), TurnOutcome::Done);
        assert_eq!(turn_end_cost(&after), None);
        assert_eq!(turn_end_tokens(&after), None);

        // Two turns on, the baseline is the last result's and a difference
        // is known again.
        let known = finish(
            &mut fold,
            &clock,
            9_000,
            &result_line(Some(0.015), (110, 3_050, 90_300, 50_000)),
        );
        assert_close(turn_end_cost(&known), 0.003);
        assert_eq!(
            turn_end_tokens(&known),
            Some(TokenCount {
                input: 310,
                output: 50
            })
        );
    }

    #[test]
    fn turn_cost_is_the_difference_of_cumulative_totals() {
        let clock = Clock::new();
        let mut fold = TraceFold::new(false);
        // The capture's two turns: the process's total, then its total after
        // a second turn. Neither is a turn's own cost, the difference is.
        let first = finish(
            &mut fold,
            &clock,
            0,
            &result_line(Some(0.012_841), (10, 1_345, 84_830, 66_349)),
        );
        assert_close(turn_end_cost(&first), 0.012_841);
        assert_eq!(
            turn_end_tokens(&first),
            Some(TokenCount {
                input: 151_189,
                output: 1_345
            })
        );
        let second = finish(
            &mut fold,
            &clock,
            20_000,
            &result_line(Some(0.013_294_61), (12, 1_349, 124_106, 66_349)),
        );
        assert_close(turn_end_cost(&second), 0.000_453_61);
        assert_eq!(
            turn_end_tokens(&second),
            Some(TokenCount {
                input: 39_278,
                output: 4
            })
        );
    }

    #[test]
    fn a_resumed_process_reports_no_cost_for_its_first_turn() {
        let clock = Clock::new();
        let mut fold = TraceFold::new(true);
        // Nothing proves a resumed process's counter starts at zero, so its
        // first total is a baseline and not a turn's cost.
        let first = finish(
            &mut fold,
            &clock,
            0,
            &result_line(Some(0.5), (10, 100, 1_000, 500)),
        );
        assert_eq!(turn_end_cost(&first), None);
        assert_eq!(turn_end_tokens(&first), None);
        let second = finish(
            &mut fold,
            &clock,
            5_000,
            &result_line(Some(0.52), (20, 130, 1_900, 700)),
        );
        assert_close(turn_end_cost(&second), 0.02);
        assert_eq!(
            turn_end_tokens(&second),
            Some(TokenCount {
                input: 1_110,
                output: 30
            })
        );
    }

    #[test]
    fn a_counter_that_goes_down_reports_no_cost() {
        let clock = Clock::new();
        let mut fold = TraceFold::new(false);
        finish(
            &mut fold,
            &clock,
            0,
            &result_line(Some(0.02), (10, 100, 1_000, 500)),
        );
        let down = finish(
            &mut fold,
            &clock,
            5_000,
            &result_line(Some(0.01), (5, 50, 500, 200)),
        );
        assert_eq!(turn_end_cost(&down), None);
        assert_eq!(turn_end_tokens(&down), None);
        // The lower total is the baseline from then on.
        let next = finish(
            &mut fold,
            &clock,
            9_000,
            &result_line(Some(0.015), (15, 150, 1_500, 700)),
        );
        assert_close(turn_end_cost(&next), 0.005);
        assert_eq!(
            turn_end_tokens(&next),
            Some(TokenCount {
                input: 1_510,
                output: 100
            })
        );
    }

    #[test]
    fn a_result_without_message_start_still_ends_the_turn() {
        let clock = Clock::new();
        let mut fold = TraceFold::new(false);
        fold.prompt_sent(clock.at(0));
        assert_eq!(
            fold.line(&parse(RESULT_SUCCESS), clock.at(4_300)),
            vec![TraceEvent::TurnEnded {
                at: clock.at(4_300),
                outcome: TurnOutcome::Done,
                measured_ms: Some(4_210),
                cost_usd: Some(0.0421),
                tokens: Some(TokenCount {
                    input: 9_200,
                    output: 340
                }),
            }]
        );
    }

    #[test]
    fn a_result_with_is_error_and_no_interrupt_is_failed() {
        let clock = Clock::new();
        let mut fold = TraceFold::new(false);
        fold.prompt_sent(clock.at(0));
        let failed = msg(json!({
            "type": "result", "subtype": "error_max_turns", "is_error": true,
            "result": "Reached the maximum number of turns", "duration_ms": 1000
        }));
        assert_eq!(
            turn_end_outcome(&fold.line(&failed, clock.at(1_000))[0]),
            TurnOutcome::Failed {
                reason: "Reached the maximum number of turns".into()
            }
        );
        // No sentence from the CLI: its subtype is the reason.
        fold.prompt_sent(clock.at(2_000));
        let bare = msg(json!({
            "type": "result", "subtype": "error_during_execution", "is_error": true
        }));
        assert_eq!(
            turn_end_outcome(&fold.line(&bare, clock.at(3_000))[0]),
            TurnOutcome::Failed {
                reason: "error_during_execution".into()
            }
        );
    }

    #[test]
    fn the_clis_durations_ride_along_as_measured_ms() {
        let clock = Clock::new();
        let mut fold = TraceFold::new(false);
        fold.prompt_sent(clock.at(0));
        fold.line(
            &assistant(None, "m0", vec![tool_use(AGENT, "Agent", json!({}))]),
            clock.at(100),
        );
        fold.line(&parse(TASK_STARTED), clock.at(124));
        // The instants say 4 646 ms and the CLI says 4 537: the CLI's wins.
        let events = fold.line(&agent_result(None, AGENT, Some(4_537)), clock.at(4_746));
        assert_eq!(
            events.last(),
            Some(&span_ended(
                AGENT,
                SpanOutcome::Done,
                clock.at(4_746),
                Some(4_537)
            ))
        );
        // The turn: 10 255 ms by the CLI's clock, whatever the instants say.
        let turn = msg(json!({
            "type": "result", "subtype": "success", "is_error": false, "duration_ms": 10_255
        }));
        match &fold.line(&turn, clock.at(11_300))[0] {
            TraceEvent::TurnEnded { measured_ms, .. } => assert_eq!(*measured_ms, Some(10_255)),
            other => panic!("not a TurnEnded: {other:?}"),
        }
        // Neither clock reported: no measurement is invented.
        fold.prompt_sent(clock.at(12_000));
        fold.line(
            &assistant(None, "m1", vec![tool_use("toolu_2", "Agent", json!({}))]),
            clock.at(12_100),
        );
        let events = fold.line(&agent_result(None, "toolu_2", None), clock.at(13_000));
        assert_eq!(
            events.last(),
            Some(&span_ended("toolu_2", SpanOutcome::Done, clock.at(13_000), None))
        );
        let bare = msg(json!({"type": "result", "subtype": "success", "is_error": false}));
        match &fold.line(&bare, clock.at(14_000))[0] {
            TraceEvent::TurnEnded { measured_ms, .. } => assert_eq!(*measured_ms, None),
            other => panic!("not a TurnEnded: {other:?}"),
        }
    }

    #[test]
    fn a_queued_prompt_starts_its_own_turn_after_the_result() {
        let clock = Clock::new();
        let mut fold = TraceFold::new(false);
        fold.prompt_sent(clock.at(0));
        fold.line(&parse(MESSAGE_START), clock.at(100));
        fold.line(
            &assistant(
                None,
                "msg_011CgG43GxW2BhRXx8chosrS",
                vec![tool_use(
                    "toolu_left_open",
                    "Bash",
                    json!({"command": "sleep 9"}),
                )],
            ),
            clock.at(900),
        );
        let first_end = fold.line(
            &result_line(Some(0.01), (10, 100, 1_000, 500)),
            clock.at(1_000),
        );
        assert_eq!(first_end.len(), 1);
        assert_eq!(turn_end_outcome(&first_end[0]), TurnOutcome::Done);

        // The user hit Stop the instant the turn ended, and the composer's
        // queue wrote the next prompt right behind the result. The stray
        // interrupt belongs to nothing, and the prompt opens a turn of its
        // own.
        fold.interrupt_sent();
        assert_eq!(
            fold.prompt_sent(clock.at(1_001)),
            vec![TraceEvent::TurnStarted {
                at: clock.at(1_001)
            }]
        );
        // New turn, new call: the first turn's open tool is not carried over
        // to be ended here.
        assert_eq!(
            fold.line(&parse(MESSAGE_START), clock.at(1_200)),
            vec![llm("msg_011CgG43GxW2BhRXx8chosrS", None, clock.at(1_200))]
        );
        let second_end = fold.line(
            &result_line(Some(0.02), (20, 200, 2_000, 1_000)),
            clock.at(2_000),
        );
        assert_eq!(second_end.len(), 1, "{second_end:?}");
        assert_eq!(turn_end_outcome(&second_end[0]), TurnOutcome::Done);
        assert_close(turn_end_cost(&second_end[0]), 0.01);
    }

    fn message_stop() -> CliMessage {
        msg(json!({
            "type": "stream_event", "event": {"type": "message_stop"},
            "parent_tool_use_id": null
        }))
    }

    /// `message_start` with the model and input usage but no `message.id`.
    fn message_start_without_id() -> CliMessage {
        msg(json!({
            "type": "stream_event",
            "event": {"type": "message_start", "message": {
                "model": HAIKU, "usage": {"input_tokens": 2, "output_tokens": 6}
            }},
            "parent_tool_use_id": null
        }))
    }

    #[test]
    fn an_interrupt_during_a_model_call_ends_that_call_interrupted() {
        let clock = Clock::new();
        let mut fold = TraceFold::new(false);
        fold.prompt_sent(clock.at(0));
        let call = "msg_011CgG43GxW2BhRXx8chosrS";
        fold.line(&parse(MESSAGE_START), clock.at(100));
        fold.interrupt_sent();
        // The capture: the CLI stops the call within milliseconds of the
        // interrupt and answers with the `result` right behind it. A call
        // the user cut short is not drawn as one that finished.
        assert_eq!(
            fold.line(&message_stop(), clock.at(2_000)),
            vec![span_ended(
                call,
                SpanOutcome::Interrupted,
                clock.at(2_000),
                None
            )]
        );
        let end = fold.line(&parse(RESULT_INTERRUPTED), clock.at(2_010));
        assert_eq!(turn_end_outcome(&end[0]), TurnOutcome::Interrupted);
    }

    #[test]
    fn a_call_that_stopped_before_the_interrupt_stays_done() {
        let clock = Clock::new();
        let mut fold = TraceFold::new(false);
        fold.prompt_sent(clock.at(0));
        let call = "msg_011CgG43GxW2BhRXx8chosrS";
        fold.line(&parse(MESSAGE_START), clock.at(100));
        assert_eq!(
            fold.line(&message_stop(), clock.at(900)),
            vec![span_ended(call, SpanOutcome::Done, clock.at(900), None)]
        );
        // The interrupt lands in the gap between calls: nothing was open, so
        // nothing is ended again and the turn alone is interrupted.
        fold.interrupt_sent();
        assert_eq!(fold.line(&message_stop(), clock.at(950)), Vec::new());
        let end = fold.line(&parse(RESULT_INTERRUPTED), clock.at(1_000));
        assert_eq!(end.len(), 1, "{end:?}");
        assert_eq!(turn_end_outcome(&end[0]), TurnOutcome::Interrupted);
    }

    #[test]
    fn an_open_wait_ends_before_its_turn_ends() {
        let clock = Clock::new();
        let mut fold = TraceFold::new(false);
        fold.prompt_sent(clock.at(0));
        fold.line(
            &assistant(
                None,
                "m0",
                vec![tool_use("toolu_B", "Bash", json!({"command": "rm x"}))],
            ),
            clock.at(100),
        );
        // Two cards waiting when Stop is pressed: the CLI ends the turn
        // without a `tool_result` for either, and the worker's later deny
        // would find nothing open.
        fold.permission_asked(Some("toolu_B"), None, 4, clock.at(150));
        fold.permission_asked(None, None, 3, clock.at(160));
        fold.interrupt_sent();
        let end = fold.line(&parse(RESULT_INTERRUPTED), clock.at(900));
        assert_eq!(end.len(), 3, "{end:?}");
        assert_eq!(
            end[..2],
            [
                TraceEvent::WaitEnded {
                    request_id: 3,
                    at: clock.at(900)
                },
                TraceEvent::WaitEnded {
                    request_id: 4,
                    at: clock.at(900)
                },
            ]
        );
        assert_eq!(turn_end_outcome(&end[2]), TurnOutcome::Interrupted);
        // Closed once: the deny that follows has nothing left to close.
        assert_eq!(fold.permission_answered(4, clock.at(300_000)), Vec::new());
    }

    #[test]
    fn a_model_call_with_no_message_id_still_spans() {
        let clock = Clock::new();
        let mut fold = TraceFold::new(false);
        fold.prompt_sent(clock.at(0));
        // A missing field drops its detail, never its span: the call gets an
        // id of the fold's own, and its delta and stop still find it.
        let started = fold.line(&message_start_without_id(), clock.at(100));
        let [TraceEvent::SpanStarted { id, kind, model, .. }] = started.as_slice() else {
            panic!("an id-less message_start still opens a call: {started:?}");
        };
        assert!(!id.is_empty());
        assert_eq!(*kind, SpanKind::Llm);
        assert_eq!(model.as_deref(), Some(HAIKU));
        assert_eq!(
            fold.line(&parse(MESSAGE_DELTA), clock.at(900)),
            vec![usage(id, 39_278, 632)]
        );
        assert_eq!(
            fold.line(&message_stop(), clock.at(910)),
            vec![span_ended(id, SpanOutcome::Done, clock.at(910), None)]
        );
        // The next one is another call, not the first again.
        let next = fold.line(&message_start_without_id(), clock.at(1_000));
        let [TraceEvent::SpanStarted { id: next_id, .. }] = next.as_slice() else {
            panic!("the second id-less call opens too: {next:?}");
        };
        assert_ne!(next_id, id);
    }

    #[test]
    fn long_descriptions_are_cut_to_one_line() {
        let clock = Clock::new();
        let mut fold = TraceFold::new(false);
        fold.prompt_sent(clock.at(0));
        let long = format!("{}\nsecond line", "x".repeat(500));
        let cut = format!("{}…", "x".repeat(200));
        // The `Agent` call's own input names the span.
        let events = fold.line(
            &assistant(
                None,
                "m0",
                vec![tool_use(
                    "toolu_X",
                    "Agent",
                    json!({"subagent_type": long, "description": long}),
                )],
            ),
            clock.at(100),
        );
        assert_eq!(events.len(), 1);
        match &events[0] {
            TraceEvent::SpanStarted { name, detail, .. } => {
                assert_eq!(name, &cut);
                assert_eq!(detail, &cut);
            }
            other => panic!("not a SpanStarted: {other:?}"),
        }
        // As does `task_started`, and `task_progress` says what it is doing.
        let started = msg(json!({
            "type": "system", "subtype": "task_started", "task_id": "t",
            "tool_use_id": "toolu_X", "description": long, "subagent_type": long
        }));
        assert_eq!(
            fold.line(&started, clock.at(110)),
            vec![TraceEvent::SpanNamed {
                id: "toolu_X".into(),
                name: cut.clone(),
                detail: cut.clone(),
            }]
        );
        let progress = msg(json!({
            "type": "system", "subtype": "task_progress", "task_id": "t",
            "tool_use_id": "toolu_X", "description": long
        }));
        assert_eq!(
            fold.line(&progress, clock.at(120)),
            vec![TraceEvent::SpanProgress {
                id: "toolu_X".into(),
                tokens: None,
                tool_uses: None,
                activity: Some(cut.clone()),
            }]
        );
        // A tool's error and a turn's failure are text too.
        let failed = tool_result(None, "toolu_X", true, &long);
        assert!(fold.line(&failed, clock.at(130)).iter().any(|event| matches!(
            event,
            TraceEvent::SpanEnded { outcome: SpanOutcome::Failed { note }, .. } if note == &cut
        )));
        let error = msg(json!({
            "type": "result", "subtype": "error_max_turns", "is_error": true, "result": long
        }));
        assert_eq!(
            turn_end_outcome(&fold.line(&error, clock.at(140))[0]),
            TurnOutcome::Failed { reason: cut }
        );
    }

    #[test]
    fn an_interrupt_mid_tool_leaves_the_tool_to_the_store() {
        let clock = Clock::new();
        let mut fold = TraceFold::new(false);
        fold.prompt_sent(clock.at(0));
        assert_eq!(
            fold
                .line(
                    &assistant(
                        None,
                        "m0",
                        vec![tool_use("toolu_B", "Bash", json!({"command": "sleep 100"}))],
                    ),
                    clock.at(100),
                )
                .len(),
            1
        );
        fold.interrupt_sent();
        // Which spans the turn abandoned is the store's to say at the turn's
        // end: the fold ends the turn and does not end the tool.
        let end = fold.line(&parse(RESULT_INTERRUPTED), clock.at(900));
        assert_eq!(end.len(), 1, "{end:?}");
        assert_eq!(turn_end_outcome(&end[0]), TurnOutcome::Interrupted);
    }

    #[test]
    fn an_unknown_system_subtype_is_ignored() {
        let clock = Clock::new();
        let mut fold = TraceFold::new(false);
        fold.prompt_sent(clock.at(0));
        fold.line(
            &assistant(
                None,
                "m0",
                vec![tool_use("toolu_X", "Agent", json!({"prompt": "go"}))],
            ),
            clock.at(100),
        );
        let line = |subtype: &str| {
            msg(json!({
                "type": "system", "subtype": subtype, "task_id": "t",
                "tool_use_id": "toolu_X", "description": "Reading README.md",
                "usage": {"total_tokens": 10, "tool_uses": 1, "duration_ms": 5}
            }))
        };
        // The same line under a name this build knows says something, so the
        // silence of the others is the subtype's and not the line's.
        assert_eq!(fold.line(&line("task_progress"), clock.at(200)).len(), 1);
        for subtype in ["task_progress_v2", "task_updated", "task_notification", "init"] {
            assert_eq!(fold.line(&line(subtype), clock.at(300)), Vec::new(), "{subtype}");
        }
    }
}
