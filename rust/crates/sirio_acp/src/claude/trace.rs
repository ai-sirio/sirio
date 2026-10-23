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
//! - the main thread streams: a model call runs from the request
//!   (`status: requesting`, the CLI sending it) to `message_stop`.
//!   `message_start` is only its first token and names its model, so the
//!   wait for that token, a second or three, is part of the call. A CLI
//!   that sends no `requesting` falls back to `message_start` as the start;
//! - a subagent does not stream. Its `assistant` lines arrive whole, so a
//!   call runs from the agent's previous edge to the line, and its last
//!   call, the report, sends no line at all: the gap before the `Agent`
//!   result is that call;
//! - the CLI's own durations (`result.duration_ms`, the `Agent` result's
//!   `totalDurationMs`) win where they exist and ride along as `measured_ms`;
//! - a subagent that runs in the background (`is_backgrounded` with a
//!   `task_type` of `local_agent`, or none) outlives its `tool_result`, which
//!   is only its launch, and the main thread's `result` too. Its lines keep
//!   feeding its lane until `task_updated` or `task_notification` says it
//!   ended, and its turn stays open until then. Any other background task (a
//!   `task_type` other than `local_agent`) is traced as its launch only: its
//!   `tool_result` ends the call at once, with no report and no duration of
//!   the agent's, and what it writes after is not traced.
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
    /// The model the CLI resolved for a background agent, from its launch
    /// result: the final report names it.
    model: Option<String>,
}

/// A prompt-started turn whose main thread has ended while background
/// agents of its own still work.
struct WaitingTurn {
    /// One of the turn's agents: a span id its turn holds, which the store
    /// finds the turn by.
    anchor: String,
    /// The background agents that were open at the main `result`. The turn
    /// ends when none of them is.
    agents: HashSet<String>,
    cost_usd: Option<f64>,
    tokens: Option<TokenCount>,
    /// A prompt was written while the turn waited: the next `result` mixes
    /// the new turn's spend with the agents', so this turn's can no longer
    /// grow.
    frozen: bool,
}

/// The latest prompt-started turn: whose spend a CLI-started `result` adds
/// to. It outlives the turn's end, because the CLI's reactions to a
/// background agent come after it.
struct Owner {
    /// The turn's own share was told apart from the process's running total.
    /// A turn after an interrupt, or one that ran while another turn's agents
    /// did, cannot say; nothing is added to it then, and a figure that is
    /// not known is never made up.
    known: bool,
    /// The turn's spend so far, reactions included.
    cost_usd: Option<f64>,
    tokens: Option<TokenCount>,
    /// While the turn still waits on agents: the anchor of its
    /// [`WaitingTurn`], where the spend goes until `AgentsEnded` carries it.
    anchor: Option<String>,
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
    /// A prompt was written and its `result` has not come.
    turn_open: bool,
    /// Prompts written while a turn was open: each starts its turn at the
    /// `result` of the one before it.
    queued_prompts: usize,
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
    /// The Messages API id of `main_call`, once its `message_start` came.
    main_message: Option<String>,
    /// `main_call` was opened by a request and its first token has not come.
    main_waiting: bool,
    /// Model calls seen this turn, so a second line of one is not a call.
    seen_calls: HashSet<String>,
    /// Model calls a subagent made, by the agent that made them. They outlive
    /// a turn with their agent, where `seen_calls` does not.
    agent_calls: HashMap<String, String>,
    spans: HashMap<String, Span>,
    /// `task_id` → the `Agent` call's id, for a permission that names its
    /// agent by `task_id`.
    tasks: HashMap<String, String>,
    /// Subagents that are over. A line that names one is ignored, where a
    /// line that names an id nobody opened hangs under the root. Kept for
    /// the process, because a background agent writes on after its turn.
    ended_agents: HashSet<String>,
    /// The open background tasks that are not subagents: `task_started` said
    /// a `task_type` other than `local_agent`. Their `tool_result` is all
    /// that is traced of them.
    backgrounded: HashSet<String>,
    /// The open background agents. Their `tool_result` is only the launch:
    /// they end on the CLI's word, and outlive the turn's scope.
    bg_agents: HashSet<String>,
    /// The background agents the main thread launched in the turn now open.
    turn_agents: HashSet<String>,
    /// The turns whose main thread ended with background agents still open.
    waiting: Vec<WaitingTurn>,
    /// The agents launched while no prompt-started turn was open (by a turn
    /// the CLI started on its own). They are not drawn, nor are their lines.
    foreign_agents: HashSet<String>,
    /// Whom a CLI-started `result` charges.
    owner: Option<Owner>,
    /// This turn started while another turn's agents worked, so its spend
    /// cannot be told apart from theirs.
    mixed_this_turn: bool,
    waits: HashMap<u64, Wait>,
    next_seq: u64,
}

impl TraceFold {
    /// `resumed` is whether this process was started with `--resume`: its
    /// cost counter has an unknown starting point.
    pub(crate) fn new(resumed: bool) -> Self {
        Self {
            turn_open: false,
            queued_prompts: 0,
            interrupted_this_turn: false,
            interrupted_last_turn: false,
            interrupted_call: None,
            baseline_cost: (!resumed).then_some(0.0),
            baseline_tokens: (!resumed).then_some(TokenCount {
                input: 0,
                output: 0,
            }),
            main_call: None,
            main_message: None,
            main_waiting: false,
            seen_calls: HashSet::new(),
            agent_calls: HashMap::new(),
            spans: HashMap::new(),
            tasks: HashMap::new(),
            ended_agents: HashSet::new(),
            backgrounded: HashSet::new(),
            bg_agents: HashSet::new(),
            turn_agents: HashSet::new(),
            waiting: Vec::new(),
            foreign_agents: HashSet::new(),
            owner: None,
            mixed_this_turn: false,
            waits: HashMap::new(),
            next_seq: 0,
        }
    }

    /// The user's line was written to the CLI. Every prompt has a turn of its
    /// own. When the last turn's `result` has not come, the CLI is still
    /// working on that one (a stopped turn winds down after the composer is
    /// free again), so the new turn starts at that `result`: opening it now
    /// would end the old one without an outcome, and let the old `result`
    /// end the new one.
    pub(crate) fn prompt_sent(&mut self, at: Instant) -> Vec<TraceEvent> {
        if self.turn_open {
            self.queued_prompts += 1;
            return Vec::new();
        }
        self.start_turn(at)
    }

    fn start_turn(&mut self, at: Instant) -> Vec<TraceEvent> {
        self.turn_open = true;
        // A Stop that landed after the last `result` belonged to no turn:
        // starting clean keeps it from marking this one.
        self.interrupted_this_turn = false;
        self.interrupted_call = None;
        self.forget_main_call();
        // Agents of an earlier turn are at work: this turn's `result` will
        // carry their spend too.
        self.mixed_this_turn = !self.waiting.is_empty();
        for turn in &mut self.waiting {
            turn.frozen = true;
        }
        vec![TraceEvent::TurnStarted { at }]
    }

    /// An interrupt was written to the CLI: the turn in flight is
    /// interrupted, whatever its `result` says, and so is the model call
    /// open at this moment.
    pub(crate) fn interrupt_sent(&mut self) {
        // With no turn open only agents run (their turn's main thread
        // ended): the Stop kills them, and is no turn's.
        if !self.turn_open {
            return;
        }
        self.interrupted_this_turn = true;
        self.interrupted_call = self.main_call.clone();
    }

    /// Folds one line, read at `at`.
    pub(crate) fn line(&mut self, message: &CliMessage, at: Instant) -> Vec<TraceEvent> {
        let mut events = match message {
            CliMessage::StreamEvent(event) => self.stream_event(event, at),
            CliMessage::Assistant(assistant) => self.assistant(assistant, at),
            CliMessage::User(user) => self.user(user, at),
            CliMessage::System(system) => self.system(system, at),
            CliMessage::Result(result) => self.result(result, at),
            _ => Vec::new(),
        };
        // An agent ends on a `user` line (a launch that failed) or a `system`
        // one: the turn it was the last of ends with it, after its spans.
        if matches!(message, CliMessage::System(_) | CliMessage::User(_)) {
            events.extend(self.release_waiting(at));
        }
        events
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
        // A card of a turn the CLI started on its own is no turn's wait.
        if span.is_none() && !self.turn_open {
            return Vec::new();
        }
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
        // The calls of a turn the CLI started on its own are not drawn.
        if !self.turn_open {
            return Vec::new();
        }
        match event.lifecycle() {
            Some(MessageLifecycle::Start { id, model, .. }) => {
                if id.is_some() && (self.main_message == id || self.main_call == id) {
                    return Vec::new();
                }
                let mut events = Vec::new();
                // The request opened this call: its first token only names
                // the model.
                if self.main_waiting {
                    self.main_waiting = false;
                    if let Some(id) = &id {
                        self.seen_calls.insert(id.clone());
                    }
                    self.main_message = id;
                    if let (Some(call), Some(model)) = (self.main_call.clone(), model) {
                        events.push(TraceEvent::SpanModel { id: call, model });
                    }
                    return events;
                }
                // A `message_start` before the last call's `message_stop`
                // means the stop never came: that call is over.
                if let Some(open) = self.main_call.take() {
                    events.push(ended(open, SpanOutcome::Done, at, None));
                }
                self.main_message = None;
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
            Some(MessageLifecycle::Stop) => {
                self.main_message = None;
                self.main_waiting = false;
                self.main_call
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
                    .collect()
            }
            None => Vec::new(),
        }
    }

    fn assistant(&mut self, assistant: &AssistantPayload, at: Instant) -> Vec<TraceEvent> {
        let mut events = Vec::new();
        let owner = match assistant.parent_tool_use_id.as_deref() {
            None => None,
            Some(parent) if self.foreign_agents.contains(parent) => {
                // What a foreign agent launches is foreign too.
                self.adopt_foreign(assistant);
                return events;
            }
            Some(parent) if self.ended_agents.contains(parent) => return events,
            Some(parent) if self.is_agent(parent) => {
                self.subagent_call(parent, assistant, at, &mut events);
                Some(parent.to_string())
            }
            // Nobody opened it: the tools it names still show, under the
            // root. No call is invented, there is no edge to time one from.
            Some(_) => None,
        };
        // A root line outside a prompt's turn is the CLI's own turn: its
        // tools are not drawn, and an agent it launches is remembered so
        // that nothing of it is.
        if owner.is_none() && !self.turn_open {
            self.adopt_foreign(assistant);
            return events;
        }
        for tool_use in assistant.tool_uses() {
            self.open_span(tool_use, owner.as_deref(), at, &mut events);
        }
        events
    }

    /// Remembers the agents `assistant` launches as foreign.
    fn adopt_foreign(&mut self, assistant: &AssistantPayload) {
        for tool_use in assistant.tool_uses() {
            if is_agent_call(tool_use) {
                self.foreign_agents.insert(tool_use.id.clone());
            }
        }
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
        if self
            .agent_calls
            .insert(call.clone(), agent_id.to_string())
            .is_some()
        {
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
        let (kind, name, detail, agent) = if is_agent_call(tool_use) {
            let name = line_of(&tool_use.input, "subagent_type")
                .unwrap_or_else(|| one_line(&tool_use.name));
            let detail = line_of(&tool_use.input, "description").unwrap_or_default();
            let agent = Agent {
                name: name.clone(),
                detail: detail.clone(),
                edge: at,
                model: None,
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
        if user.parent_tool_use_id.as_deref().is_some_and(|parent| {
            self.ended_agents.contains(parent) || self.foreign_agents.contains(parent)
        }) {
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

    /// An `Agent` call's `tool_result`: the subagent is over, unless it runs
    /// in the background and the result is only its launch.
    fn agent_returned(
        &mut self,
        result: &ToolResult,
        tool_use_result: Option<&serde_json::Value>,
        at: Instant,
        events: &mut Vec<TraceEvent>,
    ) {
        let id = result.tool_use_id.as_str();
        let totals = tool_use_result.and_then(AgentToolResult::parse);
        // A background agent goes on after its launch: the span stays open
        // and the launch is its next call's start. An agent that never
        // launched sends no notification, so its error ends it here.
        // An agent that was done before its launch came back sends no
        // notification either, and ends here like a foreground agent.
        let launched = totals
            .as_ref()
            .and_then(|totals| totals.status.as_deref())
            .is_none_or(|status| status != "completed");
        if self.bg_agents.contains(id) && !result.is_error && launched {
            if let Some(agent) = self
                .spans
                .get_mut(id)
                .and_then(|span| span.agent.as_mut())
            {
                agent.edge = at;
                agent.model = totals.and_then(|totals| totals.resolved_model);
            }
            return;
        }
        // Another background task is traced as its launch only (spec §10):
        // the result ends the call, and says nothing of the agent's report,
        // tokens or duration, which are not drawn from it.
        if self.backgrounded.contains(id) {
            self.end_span(id, outcome_of(result), at, None, events);
            return;
        }
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
        if completed && !has_children {
            self.final_report(
                id,
                totals
                    .as_ref()
                    .and_then(|totals| totals.resolved_model.clone()),
                totals.as_ref().and_then(|totals| totals.usage.as_ref()),
                at,
                events,
            );
        }
        let measured_ms = totals.and_then(|totals| totals.total_duration_ms);
        self.end_span(id, outcome_of(result), at, measured_ms, events);
    }

    /// The call an agent's report was: from its last edge to `at`.
    fn final_report(
        &self,
        id: &str,
        model: Option<String>,
        usage: Option<&MessageUsage>,
        at: Instant,
        events: &mut Vec<TraceEvent>,
    ) {
        let Some(edge) = self
            .spans
            .get(id)
            .and_then(|span| span.agent.as_ref())
            .map(|agent| agent.edge)
        else {
            return;
        };
        let report = format!("{id}/final");
        events.push(TraceEvent::SpanStarted {
            id: report.clone(),
            parent: Some(id.to_string()),
            kind: SpanKind::Llm,
            name: CALL_NAME.into(),
            detail: "Final report".into(),
            model,
            at: edge,
        });
        if let Some(usage) = usage {
            events.push(TraceEvent::SpanUsage {
                id: report.clone(),
                tokens: tokens_of(usage),
            });
        }
        events.push(ended(report, SpanOutcome::Done, at, None));
    }

    /// The CLI says the background agent `id` is over, with `status` its word
    /// for how. A second word about an agent already over finds nobody.
    fn background_ended(
        &mut self,
        id: &str,
        status: &str,
        at: Instant,
        measured_ms: Option<u64>,
        events: &mut Vec<TraceEvent>,
    ) {
        if !self.bg_agents.contains(id) {
            return;
        }
        let outcome = match status {
            "completed" => SpanOutcome::Done,
            "killed" | "stopped" => SpanOutcome::Interrupted,
            other => SpanOutcome::Failed {
                note: one_line(other),
            },
        };
        let has_children = self
            .spans
            .values()
            .any(|span| span.parent.as_deref() == Some(id));
        if outcome == SpanOutcome::Done && !has_children {
            let model = self
                .spans
                .get(id)
                .and_then(|span| span.agent.as_ref())
                .and_then(|agent| agent.model.clone());
            self.final_report(id, model, None, at, events);
        }
        self.end_span(id, outcome, at, measured_ms, events);
    }

    /// Ends the turns whose last background agent is over, each after the
    /// span of that agent.
    fn release_waiting(&mut self, at: Instant) -> Vec<TraceEvent> {
        let mut events = Vec::new();
        let mut index = 0;
        while index < self.waiting.len() {
            if self.waiting[index]
                .agents
                .iter()
                .any(|agent| self.bg_agents.contains(agent))
            {
                index += 1;
                continue;
            }
            let turn = self.waiting.remove(index);
            // Its agents are over: what the CLI does next is a reaction,
            // charged to the turn as a running total.
            if let Some(owner) = self
                .owner
                .as_mut()
                .filter(|owner| owner.anchor.as_deref() == Some(turn.anchor.as_str()))
            {
                owner.anchor = None;
            }
            events.push(TraceEvent::AgentsEnded {
                anchor: turn.anchor,
                at,
                cost_usd: turn.cost_usd,
                tokens: turn.tokens,
                at_least: true,
            });
        }
        events
    }

    fn system(&mut self, system: &SystemPayload, at: Instant) -> Vec<TraceEvent> {
        match system.subtype.as_str() {
            "task_started" => {
                let Some(id) = system.tool_use_id.as_deref() else {
                    return Vec::new();
                };
                // A foreground `Bash` raises `task_started` too, for its own
                // call: only a subagent this fold opened is named.
                let in_turn_scope = self.turn_open && self.reaches_main_on_foot(id);
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
                let is_agent_task = system
                    .task_type
                    .as_deref()
                    .is_none_or(|task_type| task_type == "local_agent");
                let on_its_own = system.is_backgrounded == Some(true);
                let detail = if on_its_own && is_agent_task {
                    self.bg_agents.insert(id.to_string());
                    // The turn that ends with it still working waits for it.
                    if in_turn_scope {
                        self.turn_agents.insert(id.to_string());
                    }
                    in_background(&agent.detail)
                } else if on_its_own || !is_agent_task {
                    self.backgrounded.insert(id.to_string());
                    in_background(&agent.detail)
                } else {
                    agent.detail.clone()
                };
                let event = TraceEvent::SpanNamed {
                    id: id.to_string(),
                    name: agent.name.clone(),
                    detail,
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
            "status" if system.status.as_deref() == Some("requesting") => self.requested(at),
            // A background agent's end comes as `task_updated` (named by its
            // `task_id`) and then `task_notification`: the first one wins. A
            // foreground agent's `Agent` result carries its end already.
            "task_updated" => {
                let terminal = system
                    .patch
                    .as_ref()
                    .and_then(|patch| patch.status.as_deref())
                    .filter(|status| {
                        matches!(*status, "completed" | "killed" | "stopped" | "failed")
                    });
                let id = system
                    .task_id
                    .as_deref()
                    .and_then(|task_id| self.tasks.get(task_id))
                    .cloned();
                let mut events = Vec::new();
                if let (Some(status), Some(id)) = (terminal, id) {
                    self.background_ended(&id, status, at, None, &mut events);
                }
                events
            }
            "task_notification" => {
                let mut events = Vec::new();
                if let Some(id) = system.tool_use_id.as_deref() {
                    let status = system.status.as_deref().unwrap_or("ended");
                    let measured_ms = system.usage.and_then(|usage| usage.duration_ms);
                    self.background_ended(id, status, at, measured_ms, &mut events);
                }
                events
            }
            _ => Vec::new(),
        }
    }

    /// The CLI sent a main-thread API request: the model call starts now,
    /// and `message_start` will only name it. Any other `status` line is a
    /// permission mode and falls through to the ignored subtypes.
    fn requested(&mut self, at: Instant) -> Vec<TraceEvent> {
        // A request outside a prompt's turn is the CLI's own turn.
        if !self.turn_open {
            return Vec::new();
        }
        // A second request before any token is the same answer again.
        if self.main_waiting {
            return Vec::new();
        }
        let mut events = Vec::new();
        // The open call was streaming and its stop never came.
        if let Some(open) = self.main_call.take() {
            events.push(ended(open, SpanOutcome::Done, at, None));
        }
        let id = format!("call-{}", self.next_sequence());
        self.main_call = Some(id.clone());
        self.main_message = None;
        self.main_waiting = true;
        events.push(call_started(id, None, None, at));
        events
    }

    fn result(&mut self, result: &ResultPayload, at: Instant) -> Vec<TraceEvent> {
        if !self.turn_open {
            return self.cli_started_result(result);
        }
        let interrupted = self.interrupted_this_turn;
        // The turn's background agents still at work. A Stop kills them, and
        // an interrupted turn waits for nobody: they are over with it.
        let mut working: Vec<(u64, String)> = Vec::new();
        let mut abandoned = HashSet::new();
        if interrupted {
            abandoned = self.turn_agents.clone();
        } else {
            for id in &self.turn_agents {
                if let Some(span) = self.spans.get(id).filter(|_| self.bg_agents.contains(id)) {
                    working.push((span.seq, id.clone()));
                }
            }
            working.sort();
        }
        let surviving = self.surviving_spans(&abandoned);
        // A card still waiting when the turn ends, a Stop pressed over it, is
        // answered by nobody: its wait ends here, ahead of the turn. A card
        // an agent that lives on is waiting on is still its to answer.
        let mut waiting: Vec<u64> = self
            .waits
            .iter()
            .filter(|(_, wait)| {
                !wait
                    .span
                    .as_ref()
                    .is_some_and(|span| surviving.contains(span))
            })
            .map(|(request_id, _)| *request_id)
            .collect();
        waiting.sort_unstable();
        let mut events: Vec<TraceEvent> = waiting
            .into_iter()
            .map(|request_id| TraceEvent::WaitEnded { request_id, at })
            .collect();
        // The turn after an interrupt is unknown too: whether the counters
        // catch up on the spend the interrupt cut short is not known.
        let known = !interrupted && !self.interrupted_last_turn && !self.mixed_this_turn;
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
        self.mixed_this_turn = false;
        self.end_turn_scope(&surviving);
        self.turn_open = false;
        self.owner = Some(Owner {
            known,
            cost_usd,
            tokens: tokens.clone(),
            anchor: working.first().map(|(_, anchor)| anchor.clone()),
        });
        match working.first() {
            // The main thread is over and its agents are not: the turn goes
            // on, with what is known of its spend so far.
            Some((_, anchor)) => {
                events.push(TraceEvent::MainEnded {
                    at,
                    outcome,
                    cost_usd,
                    tokens: tokens.clone(),
                });
                self.waiting.push(WaitingTurn {
                    anchor: anchor.clone(),
                    agents: working.into_iter().map(|(_, id)| id).collect(),
                    cost_usd,
                    tokens,
                    frozen: false,
                });
            }
            None => events.push(TraceEvent::TurnEnded {
                at,
                outcome,
                measured_ms: result.duration_ms,
                cost_usd,
                tokens,
            }),
        }
        // A prompt written while this turn was open starts its turn now.
        if self.queued_prompts > 0 {
            self.queued_prompts -= 1;
            events.extend(self.start_turn(at));
        }
        events
    }

    /// A `result` of a turn the CLI started on its own (the reaction to a
    /// background agent that ended). It ends no turn and draws nothing; its
    /// spend belongs to the latest prompt-started turn, whose running total
    /// it grows. While that turn waits on agents the sum rides in its
    /// [`WaitingTurn`] and nothing is said yet. After the last agent, the
    /// total is final bar later reactions, and says so.
    fn cli_started_result(&mut self, result: &ResultPayload) -> Vec<TraceEvent> {
        // The baseline moves on every `result`, charged or not.
        let cost = self.turn_cost(result.total_cost_usd, true);
        let tokens = self.turn_tokens(result, true);
        let Some(owner) = self.owner.as_mut().filter(|owner| owner.known) else {
            return Vec::new();
        };
        owner.cost_usd = owner.cost_usd.zip(cost).map(|(sum, share)| sum + share);
        owner.tokens = owner.tokens.take().zip(tokens).map(|(sum, share)| TokenCount {
            input: sum.input.saturating_add(share.input),
            output: sum.output.saturating_add(share.output),
        });
        if let Some(anchor) = &owner.anchor {
            if let Some(turn) = self
                .waiting
                .iter_mut()
                .find(|turn| turn.anchor == *anchor && !turn.frozen)
            {
                turn.cost_usd = owner.cost_usd;
                turn.tokens = owner.tokens.clone();
            }
            return Vec::new();
        }
        vec![TraceEvent::TurnSpend {
            cost_usd: owner.cost_usd,
            tokens: owner.tokens.clone(),
            at_least: false,
        }]
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

    /// Whether the chain of parents above span `id` reaches the main thread
    /// without crossing a background agent. A background agent that does
    /// not is ended by the one above it; one that does lives on its own, and
    /// the turn waits for it.
    fn reaches_main_on_foot(&self, id: &str) -> bool {
        let mut current = self.spans.get(id).and_then(|span| span.parent.as_deref());
        // The bound keeps a parent loop from spinning.
        for _ in 0..=self.spans.len() {
            let Some(parent) = current else {
                return true;
            };
            if self.bg_agents.contains(parent) {
                return false;
            }
            current = self
                .spans
                .get(parent)
                .and_then(|span| span.parent.as_deref());
        }
        false
    }

    /// The open spans that belong to a background agent still at work, bar
    /// those of the agents in `abandoned`: the agent's own and everything
    /// under it. They are the turn that launched the agent's to carry on.
    fn surviving_spans(&self, abandoned: &HashSet<String>) -> HashSet<String> {
        self.spans
            .keys()
            .filter(|id| {
                let mut current = Some(id.as_str());
                // The bound keeps a parent loop from spinning.
                for _ in 0..=self.spans.len() {
                    let Some(span) = current else {
                        return false;
                    };
                    if self.bg_agents.contains(span) && !abandoned.contains(span) {
                        return true;
                    }
                    current = self
                        .spans
                        .get(span)
                        .and_then(|span| span.parent.as_deref());
                }
                false
            })
            .cloned()
            .collect()
    }

    /// Forgets what belonged to the turn that just ended, bar what `surviving`
    /// holds: a background agent still at work keeps its spans, its tasks,
    /// its waits and its calls. The rest of the open spans are the store's to
    /// close; a subagent still open is over for this fold's purposes, so its
    /// late lines are ignored and not misplaced.
    fn end_turn_scope(&mut self, surviving: &HashSet<String>) {
        let over: Vec<String> = self
            .spans
            .keys()
            .filter(|id| !surviving.contains(*id))
            .cloned()
            .collect();
        for id in over {
            if let Some(span) = self.spans.remove(&id)
                && span.agent.is_some()
            {
                self.ended_agents.insert(id);
            }
        }
        self.tasks.retain(|_, agent| surviving.contains(agent));
        self.backgrounded.retain(|id| surviving.contains(id));
        self.bg_agents.retain(|id| surviving.contains(id));
        self.turn_agents.clear();
        self.waits.retain(|_, wait| {
            wait.span
                .as_ref()
                .is_some_and(|span| surviving.contains(span))
        });
        self.agent_calls.retain(|_, agent| surviving.contains(agent));
        self.forget_main_call();
        self.interrupted_call = None;
        self.seen_calls.clear();
    }

    /// No main-thread call is open. The turn that ends, or starts, owns
    /// none: a request the CLI made outside any turn must not read as the
    /// retry of the next turn's.
    fn forget_main_call(&mut self) {
        self.main_call = None;
        self.main_message = None;
        self.main_waiting = false;
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
            // A background agent the turn waits for is not its launcher's to
            // end: it ends on the CLI's word.
            self.spans
                .iter()
                .filter(|(child, span)| {
                    span.parent.as_deref() == Some(id) && !self.turn_agents.contains(*child)
                })
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
            self.backgrounded.remove(id);
            self.bg_agents.remove(id);
            self.tasks.retain(|_, agent| agent != id);
            self.agent_calls.retain(|_, agent| agent != id);
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

/// Whether the call opens a subagent: `Agent`, and the `Task` it was called
/// before.
fn is_agent_call(tool_use: &ToolUse) -> bool {
    matches!(tool_use.name.as_str(), "Agent" | "Task")
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

/// What a background agent's span says beside its name: what it was asked
/// to do, and that it runs on its own.
fn in_background(detail: &str) -> String {
    if detail.is_empty() {
        "in background".to_string()
    } else {
        format!("{detail} · in background")
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

    /// A `task_started` for the `Agent` call `tool_use_id`, with the two
    /// fields that say the agent runs on its own, `is_backgrounded` and
    /// `task_type`. The first capture's own is `false` and `local_agent`.
    fn task_started_as(
        task_id: &str,
        tool_use_id: &str,
        is_backgrounded: Option<bool>,
        task_type: &str,
    ) -> CliMessage {
        let mut line = json!({
            "type": "system", "subtype": "task_started", "task_id": task_id,
            "tool_use_id": tool_use_id, "description": "work", "subagent_type": "worker",
            "spawn_depth": 1, "task_type": task_type
        });
        if let Some(flag) = is_backgrounded {
            line["is_backgrounded"] = json!(flag);
        }
        msg(line)
    }

    /// The `tool_result` of an `Agent` call that launched its agent and came
    /// back at once. No capture of one exists: this is the first capture's
    /// `tool_use_result`, `usage` and `totalDurationMs` included (the
    /// fields a launch result might carry too), with the `status` the case
    /// names, or none.
    fn launch_result(id: &str, status: Option<&str>) -> CliMessage {
        let mut totals = json!({
            "agentType": "general-purpose", "resolvedModel": HAIKU,
            "totalDurationMs": 4537, "totalTokens": 26197,
            "usage": {"input_tokens": 2, "output_tokens": 187,
                      "cache_read_input_tokens": 25867, "cache_creation_input_tokens": 141}
        });
        if let Some(status) = status {
            totals["status"] = json!(status);
        }
        msg(json!({
            "type": "user",
            "message": {"role": "user", "content": [
                {"type": "tool_result", "tool_use_id": id, "content": "launched"}
            ]},
            "parent_tool_use_id": null,
            "tool_use_result": totals
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

    /// What a background task that is not a subagent is drawn as (spec §10):
    /// the tool call that launched it, and nothing of the work after. Its
    /// result is the launch, so no report is invented from it and no
    /// duration of its is the agent's, whatever the result carries.
    #[test]
    fn a_background_task_that_is_not_an_agent_is_traced_as_its_launch_only() {
        // (the case, the `task_started`'s `is_backgrounded` and `task_type`,
        // the launch result's `status`)
        let cases = [
            ("another task type, no flag, no status", None, "remote_agent", None),
            ("another task type, completed", Some(false), "remote_agent", Some("completed")),
        ];
        for (case, backgrounded, task_type, status) in cases {
            let clock = Clock::new();
            let mut fold = TraceFold::new(false);
            fold.prompt_sent(clock.at(0));
            fold.line(
                &assistant(
                    None,
                    "msg_main",
                    vec![tool_use(AGENT, "Agent", json!({"prompt": "go"}))],
                ),
                clock.at(100),
            );
            assert_eq!(
                fold.line(
                    &task_started_as("task-bg", AGENT, backgrounded, task_type),
                    clock.at(124)
                ),
                vec![TraceEvent::SpanNamed {
                    id: AGENT.into(),
                    name: "worker".into(),
                    detail: "work · in background".into(),
                }],
                "{case}: the launch is named as a background agent"
            );
            assert_eq!(
                fold.line(&launch_result(AGENT, status), clock.at(400)),
                vec![span_ended(AGENT, SpanOutcome::Done, clock.at(400), None)],
                "{case}: the call ends at its result, with no report and no duration of the \
                 agent's"
            );
            // What the agent does after its launch is not traced.
            assert_eq!(
                fold.line(
                    &assistant(
                        Some(AGENT),
                        "msg_late",
                        vec![tool_use("toolu_late", "Bash", json!({"command": "ls"}))]
                    ),
                    clock.at(900)
                ),
                Vec::new(),
                "{case}"
            );
            assert_eq!(
                fold.line(&parse(TASK_PROGRESS), clock.at(950)),
                Vec::new(),
                "{case}"
            );
        }
    }

    /// A `task_updated` line: the CLI's patch of one task's state.
    fn task_updated(task_id: &str, status: &str) -> CliMessage {
        msg(json!({
            "type": "system", "subtype": "task_updated", "task_id": task_id,
            "patch": {"status": status, "end_time": 1_761_200_000_000u64}
        }))
    }

    /// A `task_notification` for the agent `tool_use_id`, with the duration
    /// the CLI measured.
    fn task_notification_of(
        task_id: &str,
        tool_use_id: &str,
        status: &str,
        duration_ms: u64,
    ) -> CliMessage {
        msg(json!({
            "type": "system", "subtype": "task_notification", "task_id": task_id,
            "tool_use_id": tool_use_id, "status": status, "summary": "the agent's sentence",
            "usage": {"total_tokens": 1_000, "tool_uses": 2, "duration_ms": duration_ms}
        }))
    }

    /// The main thread launches the background agent `id` (task `task_id`)
    /// at `ms`: its `Agent` call, `task_started` 24 ms later and the launch
    /// result 300 ms after the call, which leaves `ms + 300` as the agent's
    /// last edge. The turn must be open.
    fn launch_background(fold: &mut TraceFold, clock: &Clock, id: &str, task_id: &str, ms: u64) {
        fold.line(
            &assistant(
                None,
                &format!("msg_{id}"),
                vec![tool_use(id, "Agent", json!({"prompt": "go"}))],
            ),
            clock.at(ms),
        );
        fold.line(
            &task_started_as(task_id, id, Some(true), "local_agent"),
            clock.at(ms + 24),
        );
        fold.line(
            &launch_result(id, Some("async_launched")),
            clock.at(ms + 300),
        );
    }

    /// The main thread's `result`, $0.50 for the turn.
    fn main_result() -> CliMessage {
        result_line(Some(0.5), (10, 100, 1_000, 500))
    }

    /// What a `MainEnded` for `main_result()` carries.
    fn main_ended(at: Instant) -> TraceEvent {
        TraceEvent::MainEnded {
            at,
            outcome: TurnOutcome::Done,
            cost_usd: Some(0.5),
            tokens: Some(TokenCount {
                input: 1_510,
                output: 100,
            }),
        }
    }

    #[test]
    fn a_background_launch_result_leaves_the_agent_open() {
        let clock = Clock::new();
        let mut fold = TraceFold::new(false);
        fold.prompt_sent(clock.at(0));
        fold.line(
            &assistant(
                None,
                "msg_main",
                vec![tool_use(AGENT, "Agent", json!({"prompt": "go"}))],
            ),
            clock.at(100),
        );
        fold.line(
            &task_started_as("task-bg", AGENT, Some(true), "local_agent"),
            clock.at(124),
        );
        assert_eq!(
            fold.line(
                &launch_result(AGENT, Some("async_launched")),
                clock.at(400)
            ),
            Vec::new(),
            "the launch is not the end of the work"
        );
        // The agent is still there to take lines: its progress is drawn.
        assert_eq!(fold.line(&parse(TASK_PROGRESS), clock.at(950)).len(), 1);
    }

    #[test]
    fn a_background_agents_lines_after_the_main_result_still_feed_it() {
        let clock = Clock::new();
        let mut fold = TraceFold::new(false);
        fold.prompt_sent(clock.at(0));
        launch_background(&mut fold, &clock, AGENT, "task-bg", 100);
        assert_eq!(
            fold.line(&main_result(), clock.at(1_000)),
            vec![main_ended(clock.at(1_000))]
        );

        // The agent's call runs from its edge (the launch result at 400).
        assert_eq!(
            fold.line(
                &assistant(
                    Some(AGENT),
                    "msg_late",
                    vec![tool_use("toolu_late", "Bash", json!({"command": "ls"}))]
                ),
                clock.at(5_000)
            ),
            vec![
                llm("msg_late", Some(AGENT), clock.at(400)),
                usage("msg_late", 102, 8),
                span_ended("msg_late", SpanOutcome::Done, clock.at(5_000), None),
                tool("toolu_late", Some(AGENT), "Bash", "ls", clock.at(5_000)),
            ]
        );
        assert_eq!(
            fold.line(
                &tool_result(Some(AGENT), "toolu_late", false, "README.md"),
                clock.at(5_200)
            ),
            vec![span_ended(
                "toolu_late",
                SpanOutcome::Done,
                clock.at(5_200),
                None
            )]
        );
    }

    #[test]
    fn the_main_result_with_an_agent_open_is_main_ended_not_turn_ended() {
        let clock = Clock::new();
        let mut fold = TraceFold::new(false);
        fold.prompt_sent(clock.at(0));
        launch_background(&mut fold, &clock, AGENT, "task-bg", 100);
        let events = fold.line(&main_result(), clock.at(1_000));
        assert_eq!(events, vec![main_ended(clock.at(1_000))]);
        assert!(
            !events
                .iter()
                .any(|event| matches!(event, TraceEvent::TurnEnded { .. })),
            "the turn lives on its agents: {events:?}"
        );
    }

    #[test]
    fn task_updated_ends_the_agent_and_maps_each_status() {
        // (the status, the agent's outcome, whether a final report is drawn)
        let cases = [
            ("completed", SpanOutcome::Done, true),
            ("killed", SpanOutcome::Interrupted, false),
            (
                "failed",
                SpanOutcome::Failed {
                    note: "failed".into(),
                },
                false,
            ),
        ];
        for (status, outcome, reported) in cases {
            let clock = Clock::new();
            let mut fold = TraceFold::new(false);
            fold.prompt_sent(clock.at(0));
            launch_background(&mut fold, &clock, AGENT, "task-bg", 100);
            let mut expected = Vec::new();
            if reported {
                expected.push(report(AGENT, clock.at(400)));
                expected.push(span_ended(
                    &format!("{AGENT}/final"),
                    SpanOutcome::Done,
                    clock.at(3_000),
                    None,
                ));
            }
            expected.push(span_ended(AGENT, outcome, clock.at(3_000), None));
            assert_eq!(
                fold.line(&task_updated("task-bg", status), clock.at(3_000)),
                expected,
                "{status}"
            );
        }
    }

    #[test]
    fn the_first_terminal_line_ends_the_agent_once() {
        let notification = || task_notification_of("task-bg", AGENT, "completed", 2_800);
        let updated = || task_updated("task-bg", "completed");
        // Either line can come first, and the notification's duration rides
        // only when it does.
        let orders: [(CliMessage, Option<u64>, CliMessage); 2] = [
            (notification(), Some(2_800), updated()),
            (updated(), None, notification()),
        ];
        for (first, measured, second) in orders {
            let clock = Clock::new();
            let mut fold = TraceFold::new(false);
            fold.prompt_sent(clock.at(0));
            launch_background(&mut fold, &clock, AGENT, "task-bg", 100);
            fold.line(&main_result(), clock.at(1_000));

            let events = fold.line(&first, clock.at(3_000));
            let ends = events
                .iter()
                .filter(|event| {
                    matches!(event, TraceEvent::SpanEnded { id, .. } if id.as_str() == AGENT)
                })
                .count();
            assert_eq!(ends, 1, "the first line ends the agent: {events:?}");
            assert_eq!(
                events.last(),
                Some(&TraceEvent::AgentsEnded {
                    anchor: AGENT.into(),
                    at: clock.at(3_000),
                    cost_usd: Some(0.5),
                    tokens: Some(TokenCount {
                        input: 1_510,
                        output: 100
                    }),
                    at_least: true,
                })
            );
            assert!(
                events.contains(&span_ended(
                    AGENT,
                    SpanOutcome::Done,
                    clock.at(3_000),
                    measured
                )),
                "{events:?}"
            );
            assert_eq!(
                fold.line(&second, clock.at(3_100)),
                Vec::new(),
                "a second terminal line ends nothing, not the agent and not the turn"
            );
        }
    }

    #[test]
    fn the_last_agent_ending_ends_the_waiting_turn() {
        let clock = Clock::new();
        let mut fold = TraceFold::new(false);
        fold.prompt_sent(clock.at(0));
        launch_background(&mut fold, &clock, "toolu_A", "task-a", 100);
        launch_background(&mut fold, &clock, "toolu_B", "task-b", 200);
        assert_eq!(
            fold.line(&main_result(), clock.at(1_000)),
            vec![main_ended(clock.at(1_000))]
        );

        let first = fold.line(
            &task_notification_of("task-a", "toolu_A", "completed", 3_000),
            clock.at(4_000),
        );
        assert!(
            first
                .iter()
                .all(|event| !matches!(event, TraceEvent::AgentsEnded { .. })),
            "one agent still works: {first:?}"
        );
        assert!(first.contains(&span_ended(
            "toolu_A",
            SpanOutcome::Done,
            clock.at(4_000),
            Some(3_000)
        )));

        let last = fold.line(
            &task_notification_of("task-b", "toolu_B", "completed", 5_000),
            clock.at(6_000),
        );
        match last.last() {
            Some(TraceEvent::AgentsEnded {
                anchor,
                at,
                cost_usd,
                at_least,
                ..
            }) => {
                assert!(["toolu_A", "toolu_B"].contains(&anchor.as_str()));
                assert_eq!(*at, clock.at(6_000));
                assert_close(*cost_usd, 0.5);
                assert!(*at_least, "the agents' spend is not in it yet");
            }
            other => panic!("the last agent ends the turn: {other:?}"),
        }
        assert_eq!(
            last.iter()
                .filter(|event| matches!(event, TraceEvent::AgentsEnded { .. }))
                .count(),
            1
        );
    }

    #[test]
    fn a_late_permission_lands_on_its_background_agent() {
        let clock = Clock::new();
        let mut fold = TraceFold::new(false);
        fold.prompt_sent(clock.at(0));
        launch_background(&mut fold, &clock, AGENT, "task-bg", 100);
        fold.line(&main_result(), clock.at(1_000));

        // The request names the agent by `task_id`, as the CLI writes it, and
        // its tool's own line has not come.
        assert_eq!(
            fold.permission_asked(Some("toolu_not_yet"), Some("task-bg"), 7, clock.at(5_000)),
            vec![TraceEvent::WaitStarted {
                span: Some(AGENT.into()),
                request_id: 7,
                at: clock.at(5_000),
            }]
        );
    }

    #[test]
    fn an_interrupt_kills_background_agents_before_the_result() {
        let clock = Clock::new();
        let mut fold = TraceFold::new(false);
        fold.prompt_sent(clock.at(0));
        launch_background(&mut fold, &clock, AGENT, "task-bg", 100);
        fold.interrupt_sent();

        // The CLI kills the agent before it writes the turn's result.
        assert_eq!(
            fold.line(&task_updated("task-bg", "killed"), clock.at(5_000)),
            vec![span_ended(
                AGENT,
                SpanOutcome::Interrupted,
                clock.at(5_000),
                None
            )]
        );
        assert_eq!(
            fold.line(
                &task_notification_of("task-bg", AGENT, "stopped", 4_000),
                clock.at(5_010)
            ),
            Vec::new()
        );
        assert_eq!(
            fold.line(&parse(RESULT_INTERRUPTED), clock.at(7_000)),
            vec![TraceEvent::TurnEnded {
                at: clock.at(7_000),
                outcome: TurnOutcome::Interrupted,
                measured_ms: Some(6_002),
                cost_usd: None,
                tokens: None,
            }],
            "nothing waits: the turn ends at its result"
        );
    }

    #[test]
    fn a_background_agent_a_subagent_launched_keeps_the_turn_waiting() {
        let clock = Clock::new();
        let mut fold = TraceFold::new(false);
        fold.prompt_sent(clock.at(0));
        // The main thread's foreground subagent F launches background agent B.
        fold.line(
            &assistant(
                None,
                "msg_main",
                vec![tool_use("toolu_F", "Agent", json!({"prompt": "go"}))],
            ),
            clock.at(100),
        );
        fold.line(&task_started("task-f", "toolu_F", 1), clock.at(110));
        fold.line(
            &assistant(
                Some("toolu_F"),
                "msg_f",
                vec![tool_use("toolu_B", "Agent", json!({"prompt": "go"}))],
            ),
            clock.at(200),
        );
        fold.line(
            &task_started_as("task-b", "toolu_B", Some(true), "local_agent"),
            clock.at(210),
        );
        fold.line(
            &msg(json!({
                "type": "user",
                "message": {"role": "user", "content": [
                    {"type": "tool_result", "tool_use_id": "toolu_B", "content": "launched"}
                ]},
                "parent_tool_use_id": "toolu_F",
                "tool_use_result": {"status": "async_launched", "isAsync": true}
            })),
            clock.at(260),
        );
        // F returns before the main thread does, B still works.
        fold.line(&agent_result(None, "toolu_F", Some(500)), clock.at(600));

        assert_eq!(
            fold.line(&main_result(), clock.at(1_000)),
            vec![main_ended(clock.at(1_000))]
        );
        let end = fold.line(
            &task_notification_of("task-b", "toolu_B", "completed", 4_000),
            clock.at(5_000),
        );
        let ended = end
            .iter()
            .position(|event| {
                matches!(event, TraceEvent::SpanEnded { id, .. } if id.as_str() == "toolu_B")
            })
            .expect("B ends");
        assert_eq!(end.len(), ended + 2, "B's end is the second last: {end:?}");
        assert_eq!(
            end.last(),
            Some(&TraceEvent::AgentsEnded {
                anchor: "toolu_B".into(),
                at: clock.at(5_000),
                cost_usd: Some(0.5),
                tokens: Some(TokenCount {
                    input: 1_510,
                    output: 100
                }),
                at_least: true,
            })
        );
    }

    #[test]
    fn a_stop_while_only_agents_run_leaves_the_next_turn_known() {
        let clock = Clock::new();
        let mut fold = TraceFold::new(false);
        fold.prompt_sent(clock.at(0));
        launch_background(&mut fold, &clock, AGENT, "task-bg", 100);
        fold.line(&main_result(), clock.at(1_000));

        // No turn is open, only the agent: Stop kills it.
        fold.interrupt_sent();
        let killed = fold.line(&task_updated("task-bg", "killed"), clock.at(2_000));
        assert!(
            matches!(killed.last(), Some(TraceEvent::AgentsEnded { .. })),
            "{killed:?}"
        );

        // The next prompt is an ordinary turn: it is not the interrupted one.
        let next = finish(
            &mut fold,
            &clock,
            3_000,
            &result_line(Some(0.8), (20, 200, 2_000, 1_000)),
        );
        assert_eq!(turn_end_outcome(&next), TurnOutcome::Done);
        assert_close(turn_end_cost(&next), 0.3);
    }

    #[test]
    fn a_background_launch_that_returns_completed_ends_the_agent() {
        let clock = Clock::new();
        let mut fold = TraceFold::new(false);
        fold.prompt_sent(clock.at(0));
        fold.line(
            &assistant(
                None,
                "msg_main",
                vec![tool_use(AGENT, "Agent", json!({"prompt": "go"}))],
            ),
            clock.at(100),
        );
        fold.line(
            &task_started_as("task-bg", AGENT, Some(true), "local_agent"),
            clock.at(124),
        );
        // The agent was done before the launch result came back: nothing
        // will follow, so the result ends it as a foreground agent's would.
        assert_eq!(
            fold.line(&launch_result(AGENT, Some("completed")), clock.at(400)),
            vec![
                report(AGENT, clock.at(124)),
                usage(&format!("{AGENT}/final"), 26_010, 187),
                span_ended(
                    &format!("{AGENT}/final"),
                    SpanOutcome::Done,
                    clock.at(400),
                    None
                ),
                span_ended(AGENT, SpanOutcome::Done, clock.at(400), Some(4_537)),
            ]
        );
    }

    #[test]
    fn a_background_agent_survives_a_later_turns_result() {
        let clock = Clock::new();
        let mut fold = TraceFold::new(false);
        fold.prompt_sent(clock.at(0));
        launch_background(&mut fold, &clock, AGENT, "task-bg", 100);
        assert_eq!(
            fold.line(&main_result(), clock.at(1_000)),
            vec![main_ended(clock.at(1_000))]
        );

        // A second prompt runs and ends while the agent still works.
        let second = finish(
            &mut fold,
            &clock,
            2_000,
            &result_line(Some(0.8), (20, 200, 2_000, 1_000)),
        );
        assert_eq!(turn_end_outcome(&second), TurnOutcome::Done);

        assert_eq!(
            fold.permission_asked(None, Some("task-bg"), 9, clock.at(4_000)),
            vec![TraceEvent::WaitStarted {
                span: Some(AGENT.into()),
                request_id: 9,
                at: clock.at(4_000),
            }]
        );
        let end = fold.line(&task_updated("task-bg", "completed"), clock.at(5_000));
        assert!(
            end.contains(&span_ended(AGENT, SpanOutcome::Done, clock.at(5_000), None)),
            "{end:?}"
        );
        assert_eq!(
            end.last(),
            Some(&TraceEvent::AgentsEnded {
                anchor: AGENT.into(),
                at: clock.at(5_000),
                cost_usd: Some(0.5),
                tokens: Some(TokenCount {
                    input: 1_510,
                    output: 100
                }),
                at_least: true,
            }),
            "the first turn's agents end the first turn"
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

    /// Spec §9, "a permission denied": the user presses Deny. The worker
    /// writes the answer, which ends the wait; the CLI refuses the call and
    /// sends its error result a moment later, which ends the tool failed and
    /// has no wait left to end.
    #[test]
    fn a_denied_permission_ends_its_wait_once_and_its_tool_failed() {
        let clock = Clock::new();
        let mut fold = TraceFold::new(false);
        fold.prompt_sent(clock.at(0));
        fold.line(
            &assistant(
                None,
                "m0",
                vec![tool_use(
                    "toolu_E",
                    "Edit",
                    json!({"file_path": "/repo/a.rs", "old_string": "a", "new_string": "b"}),
                )],
            ),
            clock.at(100),
        );
        fold.permission_asked(Some("toolu_E"), None, 8, clock.at(150));

        assert_eq!(
            fold.permission_answered(8, clock.at(2_000)),
            vec![TraceEvent::WaitEnded {
                request_id: 8,
                at: clock.at(2_000)
            }]
        );
        let refusal = "The user rejected this action.";
        assert_eq!(
            fold.line(
                &tool_result(None, "toolu_E", true, &format!("{refusal}\nTry something else")),
                clock.at(2_011)
            ),
            vec![span_ended(
                "toolu_E",
                SpanOutcome::Failed {
                    note: refusal.into()
                },
                clock.at(2_011),
                None
            )],
            "the answer already ended the wait, so the result ends only the tool"
        );
        // The turn goes on, and ends with nothing of the denial left open.
        let end = fold.line(&result_line(Some(0.01), (2, 40, 9_000, 300)), clock.at(3_000));
        assert_eq!(end.len(), 1, "a result is one TurnEnded: {end:?}");
        assert_eq!(turn_end_outcome(&end[0]), TurnOutcome::Done);
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

    #[test]
    fn a_prompt_written_before_the_interrupted_result_starts_its_turn_after_it() {
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
            fold.prompt_sent(clock.at(1_000)),
            vec![TraceEvent::TurnStarted {
                at: clock.at(1_000)
            }]
        );
        fold.line(&parse(MESSAGE_START), clock.at(1_100));
        // Stop frees the composer at once, so the next prompt is written
        // while the CLI is still winding the stopped turn down: its `result`
        // has not come. The stopped turn is still the one open, so the prompt
        // waits for that result and neither ends the turn nor clears the
        // interrupt.
        fold.interrupt_sent();
        assert_eq!(
            fold.prompt_sent(clock.at(1_500)),
            Vec::new(),
            "a turn is open, so the prompt opens none yet"
        );

        // The stopped turn's result ends it interrupted, and the prompt that
        // waited starts its turn right behind it.
        assert_eq!(
            fold.line(&parse(RESULT_INTERRUPTED), clock.at(2_000)),
            vec![
                TraceEvent::TurnEnded {
                    at: clock.at(2_000),
                    outcome: TurnOutcome::Interrupted,
                    measured_ms: Some(6_002),
                    cost_usd: None,
                    tokens: None,
                },
                TraceEvent::TurnStarted {
                    at: clock.at(2_000)
                },
            ]
        );

        // What follows belongs to the new turn, which the late result no
        // longer ends early, and the cost rules go on: unknown after an
        // interrupt, known again the turn after.
        assert_eq!(
            fold.line(&parse(MESSAGE_START), clock.at(2_200)),
            vec![llm("msg_011CgG43GxW2BhRXx8chosrS", None, clock.at(2_200))]
        );
        let second = fold.line(
            &result_line(Some(0.02), (20, 200, 2_000, 1_000)),
            clock.at(3_000),
        );
        assert_eq!(second.len(), 1, "{second:?}");
        assert_eq!(turn_end_outcome(&second[0]), TurnOutcome::Done);
        assert_eq!(turn_end_cost(&second[0]), None);
        let third = finish(
            &mut fold,
            &clock,
            4_000,
            &result_line(Some(0.025), (30, 300, 3_000, 1_500)),
        );
        assert_close(turn_end_cost(&third), 0.005);
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

    /// The line the CLI writes when it sends a main-thread API request.
    fn requesting() -> CliMessage {
        msg(json!({
            "type": "system", "subtype": "status", "status": "requesting",
            "session_id": "f336cf12-ec43-4857-b267-6c76fb060431",
            "uuid": "6f1f0c0e-2b7e-4e7a-9a53-0d6d3c2c8a11"
        }))
    }

    /// `message_start` of the call `id`, with the model and no more.
    fn message_start_of(id: &str) -> CliMessage {
        msg(json!({
            "type": "stream_event",
            "event": {"type": "message_start", "message": {
                "id": id, "model": HAIKU, "usage": {"input_tokens": 2, "output_tokens": 6}
            }},
            "parent_tool_use_id": null
        }))
    }

    /// The call a `requesting` opened: asserts it is the one main-thread
    /// call, with no model yet, started at `at`, and returns its id.
    fn requested(fold: &mut TraceFold, at: Instant) -> String {
        let events = fold.line(&requesting(), at);
        let [
            TraceEvent::SpanStarted {
                id,
                parent,
                kind,
                name,
                detail,
                model,
                at: started,
            },
        ] = events.as_slice()
        else {
            panic!("a request opens exactly one span: {events:?}");
        };
        assert!(id.starts_with("call-"), "{id}");
        assert_eq!(*parent, None);
        assert_eq!(*kind, SpanKind::Llm);
        assert_eq!(name, "Model call");
        assert_eq!(detail, "");
        assert_eq!(*model, None);
        assert_eq!(*started, at);
        id.clone()
    }

    #[test]
    fn a_request_opens_the_main_thread_call_at_its_own_instant() {
        let clock = Clock::new();
        let mut fold = TraceFold::new(false);
        fold.prompt_sent(clock.at(0));
        requested(&mut fold, clock.at(100));
    }

    #[test]
    fn message_start_after_a_request_names_the_model_of_the_open_call() {
        let clock = Clock::new();
        let mut fold = TraceFold::new(false);
        fold.prompt_sent(clock.at(0));
        let call = requested(&mut fold, clock.at(100));
        // The first token, 800 ms later: no second span, nothing ended.
        assert_eq!(
            fold.line(&parse(MESSAGE_START), clock.at(900)),
            vec![TraceEvent::SpanModel {
                id: call.clone(),
                model: HAIKU.into()
            }]
        );
        assert_eq!(
            fold.line(&parse(MESSAGE_DELTA), clock.at(2_700)),
            vec![usage(&call, 39_278, 632)]
        );
        assert_eq!(
            fold.line(&message_stop(), clock.at(2_710)),
            vec![span_ended(&call, SpanOutcome::Done, clock.at(2_710), None)]
        );
    }

    #[test]
    fn the_wait_for_the_first_token_is_part_of_the_next_call() {
        let clock = Clock::new();
        let mut fold = TraceFold::new(false);
        fold.prompt_sent(clock.at(0));
        let first = requested(&mut fold, clock.at(100));
        fold.line(&message_start_of("msg_one"), clock.at(900));
        fold.line(
            &assistant(
                None,
                "msg_one",
                vec![tool_use(
                    "toolu_R",
                    "Read",
                    json!({"file_path": "/repo/a.rs"}),
                )],
            ),
            clock.at(1_000),
        );
        fold.line(&message_stop(), clock.at(1_010));
        fold.line(
            &tool_result(None, "toolu_R", false, "text"),
            clock.at(1_200),
        );
        // The request goes out at 1 210; the first token is 2 s behind it.
        let second = requested(&mut fold, clock.at(1_210));
        assert_ne!(first, second);
        assert_eq!(
            fold.line(&message_start_of("msg_two"), clock.at(3_200)),
            vec![TraceEvent::SpanModel {
                id: second,
                model: HAIKU.into()
            }]
        );
    }

    #[test]
    fn a_status_line_that_is_not_a_request_opens_nothing() {
        let clock = Clock::new();
        let mut fold = TraceFold::new(false);
        fold.prompt_sent(clock.at(0));
        for line in [
            json!({"type": "system", "subtype": "status", "status": null,
                   "permissionMode": "plan"}),
            json!({"type": "system", "subtype": "status", "permissionMode": "default"}),
        ] {
            assert_eq!(fold.line(&msg(line), clock.at(100)), Vec::new());
        }
        // Nothing was opened, so the first token still opens its own call.
        let started = fold.line(&parse(MESSAGE_START), clock.at(200));
        assert_eq!(started.len(), 1, "{started:?}");
        assert!(matches!(started[0], TraceEvent::SpanStarted { .. }));
    }

    #[test]
    fn a_second_request_for_the_same_answer_is_still_one_call() {
        let clock = Clock::new();
        let mut fold = TraceFold::new(false);
        fold.prompt_sent(clock.at(0));
        let call = requested(&mut fold, clock.at(100));
        // A retry before any token arrived.
        assert_eq!(fold.line(&requesting(), clock.at(5_000)), Vec::new());
        assert_eq!(
            fold.line(&parse(MESSAGE_START), clock.at(6_000)),
            vec![TraceEvent::SpanModel {
                id: call.clone(),
                model: HAIKU.into()
            }]
        );
        assert_eq!(
            fold.line(&message_stop(), clock.at(7_000)),
            vec![span_ended(&call, SpanOutcome::Done, clock.at(7_000), None)]
        );
    }

    #[test]
    fn a_request_while_a_call_streams_ends_that_call_and_opens_the_next() {
        let clock = Clock::new();
        let mut fold = TraceFold::new(false);
        fold.prompt_sent(clock.at(0));
        let first = requested(&mut fold, clock.at(100));
        fold.line(&parse(MESSAGE_START), clock.at(900));
        // The stop never came.
        let events = fold.line(&requesting(), clock.at(4_000));
        let [ended_first, TraceEvent::SpanStarted { id: second, at, .. }] = events.as_slice()
        else {
            panic!("the open call ends and the next opens: {events:?}");
        };
        assert_eq!(
            *ended_first,
            span_ended(&first, SpanOutcome::Done, clock.at(4_000), None)
        );
        assert_ne!(*second, first);
        assert!(second.starts_with("call-"));
        assert_eq!(*at, clock.at(4_000));
    }

    #[test]
    fn an_interrupt_while_a_call_waits_ends_it_interrupted_at_its_stop() {
        let clock = Clock::new();
        let mut fold = TraceFold::new(false);
        fold.prompt_sent(clock.at(0));
        let call = requested(&mut fold, clock.at(100));
        fold.interrupt_sent();
        assert_eq!(
            fold.line(&message_stop(), clock.at(900)),
            vec![span_ended(
                &call,
                SpanOutcome::Interrupted,
                clock.at(900),
                None
            )]
        );
    }

    #[test]
    fn an_interrupt_while_a_call_waits_leaves_it_to_the_store_when_no_stop_comes() {
        let clock = Clock::new();
        let mut fold = TraceFold::new(false);
        fold.prompt_sent(clock.at(0));
        requested(&mut fold, clock.at(100));
        fold.interrupt_sent();
        let end = fold.line(&parse(RESULT_INTERRUPTED), clock.at(900));
        assert_eq!(end.len(), 1, "{end:?}");
        assert_eq!(turn_end_outcome(&end[0]), TurnOutcome::Interrupted);
    }

    #[test]
    fn a_repeated_message_start_of_the_bound_call_changes_nothing() {
        let clock = Clock::new();
        let mut fold = TraceFold::new(false);
        fold.prompt_sent(clock.at(0));
        let call = requested(&mut fold, clock.at(100));
        assert_eq!(fold.line(&parse(MESSAGE_START), clock.at(900)).len(), 1);
        assert_eq!(fold.line(&parse(MESSAGE_START), clock.at(910)), Vec::new());
        // Still open: the stop ends it, Done.
        assert_eq!(
            fold.line(&message_stop(), clock.at(2_000)),
            vec![span_ended(&call, SpanOutcome::Done, clock.at(2_000), None)]
        );
    }

    #[test]
    fn a_request_the_turn_abandoned_does_not_swallow_the_next_turns() {
        let clock = Clock::new();
        let mut fold = TraceFold::new(false);
        fold.prompt_sent(clock.at(0));
        let first = requested(&mut fold, clock.at(100));
        // The turn ends with the request unanswered.
        let end = fold.line(&result_line(Some(0.01), (1, 1, 0, 0)), clock.at(500));
        assert_eq!(end.len(), 1, "{end:?}");
        fold.prompt_sent(clock.at(1_000));
        let second = requested(&mut fold, clock.at(1_100));
        assert_ne!(first, second);
    }

    #[test]
    fn a_message_start_without_an_id_after_a_request_still_binds() {
        let clock = Clock::new();
        let mut fold = TraceFold::new(false);
        fold.prompt_sent(clock.at(0));
        let call = requested(&mut fold, clock.at(100));
        assert_eq!(
            fold.line(&message_start_without_id(), clock.at(900)),
            vec![TraceEvent::SpanModel {
                id: call.clone(),
                model: HAIKU.into()
            }]
        );
        assert_eq!(
            fold.line(&message_stop(), clock.at(1_000)),
            vec![span_ended(&call, SpanOutcome::Done, clock.at(1_000), None)]
        );
    }

    /// Every `TurnSpend` in `events`, as (cost, tokens, at_least).
    fn spend_of(events: &[TraceEvent]) -> Vec<(Option<f64>, Option<TokenCount>, bool)> {
        events
            .iter()
            .filter_map(|event| match event {
                TraceEvent::TurnSpend {
                    cost_usd,
                    tokens,
                    at_least,
                } => Some((*cost_usd, tokens.clone(), *at_least)),
                _ => None,
            })
            .collect()
    }

    /// The `system init` line a CLI-started turn opens with.
    fn init_line() -> CliMessage {
        msg(json!({"type": "system", "subtype": "init", "session_id": "f336cf12"}))
    }

    /// The lines of a turn the CLI started on its own, up to and not
    /// including its `result`: the lines the fold must not draw.
    fn cli_started_lines(fold: &mut TraceFold, clock: &Clock, ms: u64) -> Vec<TraceEvent> {
        let mut events = Vec::new();
        for (offset, line) in [
            init_line(),
            requesting(),
            message_start_of("msg_cli"),
            assistant(
                None,
                "msg_cli",
                vec![tool_use("toolu_cli_bash", "Bash", json!({"command": "ls"}))],
            ),
            message_stop(),
        ]
        .iter()
        .enumerate()
        {
            events.extend(fold.line(line, clock.at(ms + offset as u64 * 10)));
        }
        events
    }

    #[test]
    fn a_cli_started_turns_lines_open_nothing() {
        let clock = Clock::new();
        let mut fold = TraceFold::new(false);
        fold.prompt_sent(clock.at(0));
        let end = fold.line(&main_result(), clock.at(1_000));
        assert!(matches!(end.as_slice(), [TraceEvent::TurnEnded { .. }]), "{end:?}");

        let mut events = cli_started_lines(&mut fold, &clock, 2_000);
        assert_eq!(events, Vec::new(), "no span opens outside a prompt's turn");
        // Its result adds its own $0.20 and 30 output tokens to the turn.
        events.extend(fold.line(
            &result_line(Some(0.7), (12, 130, 1_000, 500)),
            clock.at(2_100),
        ));
        assert!(
            events
                .iter()
                .all(|event| !matches!(event, TraceEvent::TurnEnded { .. } | TraceEvent::SpanStarted { .. })),
            "{events:?}"
        );
        let spend = spend_of(&events);
        let [(cost, tokens, at_least)] = spend.as_slice() else {
            panic!("exactly one TurnSpend: {events:?}");
        };
        assert_eq!(events.len(), 1, "{events:?}");
        assert_close(*cost, 0.7);
        assert_eq!(
            *tokens,
            Some(TokenCount {
                input: 1_512,
                output: 130
            })
        );
        assert!(!at_least);
    }

    #[test]
    fn an_agent_a_cli_started_turn_launches_is_dropped() {
        let clock = Clock::new();
        let mut fold = TraceFold::new(false);
        fold.prompt_sent(clock.at(0));
        fold.line(&main_result(), clock.at(1_000));

        let id = "toolu_cli_agent";
        let progress = msg(json!({
            "type": "system", "subtype": "task_progress", "task_id": "task-cli",
            "tool_use_id": id, "description": "Running work",
            "usage": {"total_tokens": 900, "tool_uses": 1, "duration_ms": 500},
            "last_tool_name": "Bash"
        }));
        let lines = [
            assistant(None, "msg_cli", vec![tool_use(id, "Agent", json!({"prompt": "go"}))]),
            task_started_as("task-cli", id, Some(true), "local_agent"),
            launch_result(id, Some("async_launched")),
            assistant(
                Some(id),
                "msg_cli_agent",
                vec![tool_use("toolu_cli_inner", "Bash", json!({"command": "ls"}))],
            ),
            tool_result(Some(id), "toolu_cli_inner", false, "README.md"),
            progress,
            task_updated("task-cli", "completed"),
            task_notification_of("task-cli", id, "completed", 2_000),
        ];
        for (index, line) in lines.iter().enumerate() {
            assert_eq!(
                fold.line(line, clock.at(2_000 + index as u64 * 100)),
                Vec::new(),
                "line {index} of a CLI-started agent draws nothing"
            );
        }
        // The agent keeps no turn open: a later prompt's result ends it.
        let next = finish(
            &mut fold,
            &clock,
            4_000,
            &result_line(Some(0.9), (20, 200, 2_000, 1_000)),
        );
        assert_eq!(turn_end_outcome(&next), TurnOutcome::Done);
    }

    #[test]
    fn a_notification_nobody_opened_changes_nothing() {
        let clock = Clock::new();
        let mut fold = TraceFold::new(false);
        fold.prompt_sent(clock.at(0));
        launch_background(&mut fold, &clock, AGENT, "task-bg", 100);
        fold.line(&main_result(), clock.at(1_000));

        for line in [
            task_notification_of("task-ghost", "toolu_ghost", "completed", 10),
            task_updated("task-ghost", "completed"),
            task_notification_of("task-ghost", "toolu_ghost", "failed", 10),
        ] {
            assert_eq!(fold.line(&line, clock.at(2_000)), Vec::new());
        }
        // The turn still waits on its own agent.
        let end = fold.line(
            &task_notification_of("task-bg", AGENT, "completed", 3_000),
            clock.at(3_000),
        );
        assert!(
            matches!(end.last(), Some(TraceEvent::AgentsEnded { anchor, .. }) if anchor.as_str() == AGENT),
            "{end:?}"
        );
    }

    #[test]
    fn the_cost_after_the_last_agent_is_final_and_reactions_add_to_it() {
        let clock = Clock::new();
        let mut fold = TraceFold::new(false);
        fold.prompt_sent(clock.at(0));
        launch_background(&mut fold, &clock, AGENT, "task-bg", 100);
        assert!(matches!(
            fold.line(&result_line(Some(1.0), (10, 100, 1_000, 500)), clock.at(1_000)).as_slice(),
            [TraceEvent::MainEnded { .. }]
        ));
        let ended = fold.line(
            &task_notification_of("task-bg", AGENT, "completed", 3_000),
            clock.at(3_000),
        );
        match ended.last() {
            Some(TraceEvent::AgentsEnded {
                cost_usd, at_least, ..
            }) => {
                assert_close(*cost_usd, 1.0);
                assert!(*at_least, "no result after the agent has come yet");
            }
            other => panic!("the agent ends the turn: {other:?}"),
        }

        // The CLI reacts to the notification, then to a second one.
        let first = fold.line(
            &result_line(Some(1.5), (20, 300, 2_000, 1_000)),
            clock.at(4_000),
        );
        let spend = spend_of(&first);
        assert_eq!(first.len(), 1, "{first:?}");
        assert_close(spend[0].0, 1.5);
        assert_eq!(
            spend[0].1,
            Some(TokenCount {
                input: 3_020,
                output: 300
            })
        );
        assert!(!spend[0].2);
        let second = fold.line(
            &result_line(Some(1.6), (22, 320, 2_000, 1_000)),
            clock.at(5_000),
        );
        assert_eq!(second.len(), 1, "{second:?}");
        assert_close(spend_of(&second)[0].0, 1.6);
        assert!(!spend_of(&second)[0].2);
    }

    #[test]
    fn an_agent_that_ended_before_the_main_result_still_gets_its_reaction_spend() {
        let clock = Clock::new();
        let mut fold = TraceFold::new(false);
        fold.prompt_sent(clock.at(0));
        launch_background(&mut fold, &clock, AGENT, "task-bg", 100);
        fold.line(&task_updated("task-bg", "completed"), clock.at(800));
        let end = fold.line(&result_line(Some(1.0), (10, 100, 1_000, 500)), clock.at(1_000));
        assert_eq!(end.len(), 1, "{end:?}");
        assert_close(turn_end_cost(&end[0]), 1.0);

        let reaction = fold.line(
            &result_line(Some(1.3), (15, 150, 1_000, 500)),
            clock.at(2_000),
        );
        assert_eq!(reaction.len(), 1, "{reaction:?}");
        let spend = spend_of(&reaction);
        assert_close(spend[0].0, 1.3);
        assert!(!spend[0].2);
    }

    #[test]
    fn a_prompt_while_agents_run_freezes_the_old_cost_and_hides_the_new() {
        let clock = Clock::new();
        let mut fold = TraceFold::new(false);
        fold.prompt_sent(clock.at(0));
        launch_background(&mut fold, &clock, AGENT, "task-bg", 100);
        assert!(matches!(
            fold.line(&result_line(Some(1.0), (10, 100, 1_000, 500)), clock.at(1_000)).as_slice(),
            [TraceEvent::MainEnded { .. }]
        ));

        // Turn 2's spend and the agent's cannot be told apart.
        let second = finish(
            &mut fold,
            &clock,
            2_000,
            &result_line(Some(1.8), (20, 200, 2_000, 1_000)),
        );
        assert_eq!(turn_end_outcome(&second), TurnOutcome::Done);
        assert_eq!(turn_end_cost(&second), None);
        assert_eq!(turn_end_tokens(&second), None);

        let end = fold.line(&task_updated("task-bg", "completed"), clock.at(5_000));
        match end.last() {
            Some(TraceEvent::AgentsEnded {
                anchor,
                cost_usd,
                at_least,
                ..
            }) => {
                assert_eq!(anchor.as_str(), AGENT);
                assert_close(*cost_usd, 1.0);
                assert!(*at_least, "turn 1's cost stays a floor");
            }
            other => panic!("the agent ends turn 1: {other:?}"),
        }
        // Reactions to a turn whose cost is unknown add nothing to a dash.
        let reaction = fold.line(
            &result_line(Some(2.0), (22, 220, 2_000, 1_000)),
            clock.at(6_000),
        );
        assert_eq!(reaction, Vec::new());
    }
}
