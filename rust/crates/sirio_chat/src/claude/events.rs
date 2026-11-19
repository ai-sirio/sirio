//! Protocol lines in, `ChatEvent`s out.
//!
//! Everything here is pure. The worker owns the process and the channels;
//! this owns the little state a stream needs — which tool calls are open,
//! what the session id turned out to be — so every case can be a unit test
//! with a JSON literal instead of a subprocess.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::PathBuf;

use sirio_claude::message::{
    CliMessage, ContentBlock, Delta, MessageLifecycle, ResultPayload, SystemPayload,
};
use sirio_claude::tools::{self, ToolContent, ToolInfo};

use sirio_chat_vocab::{
    BackgroundTaskEvent, BackgroundTaskRow, ChatEvent, CliState, McpToolItem, McpToolUpdate,
    PlanEntryInfo, SessionNotice, SubagentItem, SubagentUpdate, ToolCallContentInfo, ToolCallDiff,
    ToolCallLocationInfo,
};

/// What a finished turn reported, kept for the caller that asks after the
/// fact — the context meter, and the death report.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct ResultSummary {
    /// The stop reason, already spelled the way the transcript footer reads.
    pub stop_reason: String,
    /// Whether the CLI called this turn a failure.
    pub is_error: bool,
    /// The error sentence, when there was one.
    pub error_text: Option<String>,
    /// Cumulative session cost.
    pub total_cost_usd: Option<f64>,
    /// Cumulative tokens, summed across models.
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cached_read_tokens: u64,
    /// Cumulative tokens written to the prompt cache. Context like any
    /// other: the meter's fallback counts them in.
    pub cache_creation_tokens: u64,
    /// The context window of the model that answered, when reported. The
    /// context meter's fallback when `get_context_usage` is unavailable.
    pub context_window: Option<u64>,
}

/// One open tool call.
#[derive(Clone, Debug)]
struct OpenCall {
    /// The tool's own name, needed to read its result's shape.
    name: String,
    /// What the call proposed, kept so a completion that carries no better
    /// diff leaves the optimistic one standing.
    info: ToolInfo,
}

/// The stream's state.
#[derive(Debug, Default)]
pub(crate) struct Fold {
    open_calls: BTreeMap<String, OpenCall>,
    /// Ids of the calls a subagent's lines held. Those lines are dropped, so
    /// the call's result never reaches the transcript either: a row opened for
    /// one would never settle.
    subagent_calls: HashSet<String>,
    /// A parent tool-use id a subagent's lines carry, to the card that draws
    /// them and the label of the agent (`None` for the card's own agent).
    /// Kept past the turn end: a background agent keeps talking after its
    /// `result`. A few short strings per call; not reclaimed.
    subagent_cards: HashMap<String, (String, Option<String>)>,
    /// The calls a subagent has open, for their completion content.
    subagent_open: HashMap<String, OpenCall>,
    session_id: Option<String>,
    claude_version: Option<String>,
    current_mode: Option<String>,
    model: Option<String>,
    mcp_warnings: Vec<String>,
    /// The server list a `system/init` carried, until the worker takes it.
    init_mcp_servers: Option<Vec<sirio_claude::McpServerStatus>>,
    last_result: Option<ResultSummary>,
    notices: Vec<SessionNotice>,
    /// Ids of tasks whose `task_started` said they were backgrounded.
    /// The ending `task_notification` does not repeat the flag, and a
    /// foreground `Bash` call raises the very same pair — so without this
    /// every tool call in the session would post a "finished" row.
    backgrounded: BTreeSet<String>,
    /// How many background tasks the CLI last said were running.
    background_tasks: usize,
    /// The last rate-limit line announced, as (status, window, scope). The
    /// push repeats every turn, and re-announcing an unchanged one would
    /// stack one banner per turn until the window resets. An `allowed` push
    /// clears it, so the same warning is news again after the plan has room.
    rate_limit_seen: Option<(String, Option<String>, Option<String>)>,
    /// Ids of the main-thread messages a `message_start` announced since the
    /// last `result`. An `assistant` line with one of these ids has already
    /// been drawn from its deltas.
    streamed_ids: HashSet<String>,
    /// The main-thread message a `message_start` opened and no
    /// `message_stop` has closed: `Some(None)` when it named no id.
    open_message: Option<Option<String>>,
    /// A text delta arrived while no message was open. The real CLI never
    /// does this; the fixture's `state_text` does, and its `assistant` line
    /// repeats the text. Cleared by every main-thread `assistant` line.
    orphan_text: bool,
    /// The last event this turn that opened or extended a transcript row was
    /// an `AgentMessageChunk`, so the next chunk would run on into it.
    last_row_was_chunk: bool,
    /// The text of a local command's answer drawn this turn, until the turn
    /// ends. A command that fails repeats it as its `result`, and drawing
    /// that too would print the answer twice.
    local_command_text: Option<String>,
    /// The CLI listed every capability a widget needs in its `system/init`, so main-thread
    /// `mcp__` calls are announced as [`SessionNotice::McpTool`].
    mcp_apps: bool,
    /// Ids of the `mcp__` calls announced and not yet returned or cancelled, oldest first.
    mcp_announced: Vec<String>,
}

/// What an `assistant` line's model says when the CLI wrote the line itself
/// (an exhausted retry) rather than a model.
const SYNTHETIC_MODEL: &str = "<synthetic>";

impl Fold {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Turns the MCP tool notices on or off. The worker sets it from `system/init`.
    pub(crate) fn set_mcp_apps(&mut self, on: bool) {
        self.mcp_apps = on;
    }

    /// Whether a subagent's line held this tool call.
    pub(crate) fn is_subagent_call(&self, tool_use_id: &str) -> bool {
        self.subagent_calls.contains(tool_use_id)
    }

    /// The Claude session id, once a line has named it. This is what a
    /// later `--resume` needs.
    pub(crate) fn session_id(&self) -> Option<&str> {
        self.session_id.as_deref()
    }

    /// The Claude Code version this session is running, once `system/init`
    /// has named it. Shown in the death report and in Settings.
    pub(crate) fn claude_version(&self) -> Option<&str> {
        self.claude_version.as_deref()
    }

    /// The permission mode in force.
    pub(crate) fn current_mode(&self) -> Option<&str> {
        self.current_mode.as_deref()
    }

    /// The model that answered, once a line has named it.
    pub(crate) fn model(&self) -> Option<&str> {
        self.model.as_deref()
    }

    /// MCP warnings seen so far, draining them: the surface posts each one
    /// once, and re-reporting on every later turn is exactly the bug the
    /// ACP side's `mcp_warnings_shown` cursor exists to prevent.
    pub(crate) fn take_mcp_warnings(&mut self) -> Vec<String> {
        std::mem::take(&mut self.mcp_warnings)
    }

    /// The server list of the latest `system/init`, draining it. The worker
    /// keeps it for the connectors column until an `mcp_status` answer, which
    /// also names each server's origin, replaces it.
    pub(crate) fn take_init_mcp_servers(&mut self) -> Option<Vec<sirio_claude::McpServerStatus>> {
        self.init_mcp_servers.take()
    }

    /// Notices seen so far, draining them. The surface posts each one
    /// once, in the order it arrived, into the transcript.
    pub(crate) fn take_notices(&mut self) -> Vec<SessionNotice> {
        std::mem::take(&mut self.notices)
    }

    /// How many background tasks are running, as the CLI last reported.
    pub(crate) fn background_task_count(&self) -> usize {
        self.background_tasks
    }

    /// What the last `result` reported.
    pub(crate) fn last_result(&self) -> Option<&ResultSummary> {
        self.last_result.as_ref()
    }

    /// Folds one line.
    pub(crate) fn apply(&mut self, message: CliMessage) -> Vec<ChatEvent> {
        let events = self.apply_line(message);
        // A tool, a plan or a thought opens a row of its own, so the next
        // chunk needs no separator after one. Other events leave it as it
        // was. (A `CliNotice` clears the flag where it is pushed.)
        for event in &events {
            match event {
                ChatEvent::AgentMessageChunk(_) => self.last_row_was_chunk = true,
                ChatEvent::ThoughtChunk(_)
                | ChatEvent::ToolCallStarted { .. }
                | ChatEvent::ToolCallUpdated { .. }
                | ChatEvent::ToolCallCompleted { .. }
                | ChatEvent::PlanUpdate { .. } => self.last_row_was_chunk = false,
                // A turn starts with neither flag set, whatever chunk its
                // `result` itself carried (an error's text).
                ChatEvent::TurnEnded { .. } => {
                    self.last_row_was_chunk = false;
                    self.orphan_text = false;
                }
                _ => {}
            }
        }
        events
    }

    fn apply_line(&mut self, message: CliMessage) -> Vec<ChatEvent> {
        match message {
            CliMessage::StreamEvent(event) => {
                // A subagent's deltas carry a parent id. They are not the
                // main thread's reply and must not be typed into it.
                if event.parent_tool_use_id.is_some() {
                    return Vec::new();
                }
                match event.lifecycle() {
                    Some(MessageLifecycle::Start { id, .. }) => {
                        if let Some(id) = &id {
                            self.streamed_ids.insert(id.clone());
                        }
                        self.open_message = Some(id);
                    }
                    Some(MessageLifecycle::Stop) => self.open_message = None,
                    Some(MessageLifecycle::Delta { .. }) | None => {}
                }
                match event.delta() {
                    Some(Delta::Text(text)) => {
                        if self.open_message.is_none() {
                            self.orphan_text = true;
                        }
                        vec![ChatEvent::AgentMessageChunk(text)]
                    }
                    // A thought with no text is not a thought. Every model
                    // that does not expose its reasoning still sends the
                    // block -- one `thinking_delta` carrying `""`, then the
                    // signature that proves it happened -- and a section the
                    // reader expands onto nothing reads as content this
                    // build lost, not as content the CLI never sent.
                    // Exactly empty, never trimmed: a whitespace-only delta
                    // between two words of a real thought is the space
                    // between them, and dropping it would glue them
                    // together.
                    Some(Delta::Thinking(text)) if text.is_empty() => Vec::new(),
                    Some(Delta::Thinking(text)) => vec![ChatEvent::ThoughtChunk(text)],
                    None => Vec::new(),
                }
            }
            CliMessage::Assistant(assistant) => {
                if let Some(parent) = assistant.parent_tool_use_id.as_deref() {
                    // The line draws no row, but its calls are remembered: a
                    // permission asked for one must not draw a row either.
                    self.subagent_calls
                        .extend(assistant.tool_uses().map(|tool_use| tool_use.id.clone()));
                    return self.subagent_blocks(parent, &assistant.message.content);
                }
                let synthetic = assistant.message.model.as_deref() == Some(SYNTHETIC_MODEL);
                if let Some(model) = assistant.message.model.clone()
                    && !synthetic
                {
                    self.model = Some(model);
                }
                let mut events = Vec::new();
                // An answer no stream carried: the CLI sent the text whole.
                // Drawn only when nothing else drew it -- not a synthetic
                // line (the CLI's own text, never an answer), not a message whose
                // deltas arrived, not text the fixture streamed outside any
                // message, and not while a message with no id is open (its
                // id cannot be matched).
                let drawn_by_stream = self.orphan_text
                    || assistant
                        .message
                        .id
                        .as_ref()
                        .is_none_or(|id| self.streamed_ids.contains(id))
                    || matches!(self.open_message, Some(None));
                self.orphan_text = false;
                if synthetic && assistant.local_command_run.is_some() {
                    // The CLI answered a command itself (`/usage`): its text
                    // is drawn as the command's output, never as the model's.
                    let text = assistant.text();
                    let report = assistant
                        .usage_report
                        .clone()
                        .map(serde_json::Value::Object);
                    // An empty text is an answer only when the card can read
                    // the report; otherwise the row would be a label over nothing.
                    let readable = report
                        .as_ref()
                        .is_some_and(|report| sirio_claude::UsageReport::parse(report).is_some());
                    if !text.is_empty() || readable {
                        self.local_command_text = Some(text.clone()).filter(|t| !t.is_empty());
                        self.notices.push(SessionNotice::LocalCommandOutput {
                            command: assistant
                                .local_command_run
                                .as_ref()
                                .map(|run| run.command.clone())
                                .unwrap_or_default(),
                            text,
                            report,
                        });
                        // Its row sits between the text around it, so the
                        // next chunk opens a new row and needs no separator.
                        self.last_row_was_chunk = false;
                        events.push(ChatEvent::OtherSessionUpdate {
                            kind: "SessionNotice".into(),
                        });
                    }
                }
                if !synthetic && !drawn_by_stream {
                    let body = assistant.text();
                    if !body.is_empty() {
                        events.push(ChatEvent::AgentMessageChunk(if self.last_row_was_chunk {
                            format!("\n\n{body}")
                        } else {
                            body
                        }));
                    }
                }
                for tool_use in assistant.tool_uses() {
                    // A permission may have arrived first and already drawn
                    // this row; drawing it again would double the call.
                    if self.open_calls.contains_key(&tool_use.id) {
                        continue;
                    }
                    let info = tools::describe(tool_use);
                    if is_agent_call(&tool_use.name) {
                        self.open_card(&tool_use.id, &mut events);
                    }
                    events.push(started_event(&tool_use.id, &info, "InProgress"));
                    if let Some(entries) = plan_entries(&tool_use.name, &tool_use.input) {
                        events.push(ChatEvent::PlanUpdate { entries });
                    }
                    self.open_calls.insert(
                        tool_use.id.clone(),
                        OpenCall {
                            name: tool_use.name.clone(),
                            info,
                        },
                    );
                    self.announce_mcp_call(
                        &tool_use.id,
                        &tool_use.name,
                        &tool_use.input,
                        &mut events,
                    );
                }
                events
            }
            CliMessage::User(user) => {
                if let Some(parent) = user.parent_tool_use_id.as_deref() {
                    return self.subagent_results(
                        parent,
                        user.tool_results(),
                        user.tool_use_result.as_ref(),
                    );
                }
                let mut events = Vec::new();
                let results = user.tool_results();
                // The line's structured result belongs to its one call; with several in the
                // line it cannot be told whose it is.
                let structured = (results.len() == 1)
                    .then_some(user.tool_use_result.as_ref())
                    .flatten()
                    .filter(|value| !value.is_null());
                let mut returned = false;
                for result in results {
                    let Some(open) = self.open_calls.remove(&result.tool_use_id) else {
                        // A result for a call nobody opened: report the
                        // traffic rather than inventing a row for it.
                        events.push(ChatEvent::OtherSessionUpdate {
                            kind: "tool_result(unmatched)".into(),
                        });
                        continue;
                    };
                    let content = completion_content(&open, &result, user.tool_use_result.as_ref());
                    if let Some(at) = self
                        .mcp_announced
                        .iter()
                        .position(|id| *id == result.tool_use_id)
                    {
                        self.mcp_announced.remove(at);
                        let value = structured
                            .cloned()
                            .unwrap_or_else(|| serde_json::Value::String(result.text.clone()));
                        self.notices.push(SessionNotice::McpTool(McpToolUpdate {
                            id: result.tool_use_id.clone(),
                            item: McpToolItem::Returned {
                                result: sirio_claude::call_tool_result(&value, result.is_error),
                            },
                        }));
                        returned = true;
                    }
                    events.push(ChatEvent::ToolCallCompleted {
                        id: result.tool_use_id,
                        status: if result.is_error {
                            "Failed".into()
                        } else {
                            "Completed".into()
                        },
                        kind: Some(open.info.kind.as_str().to_string()),
                        content: Some(content),
                        locations: Some(locations(&open.info)),
                        raw_input: None,
                        raw_output: user
                            .tool_use_result
                            .as_ref()
                            .and_then(|value| serde_json::to_string_pretty(value).ok()),
                    });
                }
                if returned {
                    events.push(session_notice_signal());
                }
                events
            }
            CliMessage::System(system) => self.apply_system(system),
            CliMessage::Result(result) => self.apply_result(result),
            // Control traffic is the worker's business, not the fold's.
            CliMessage::ControlRequest(_)
            | CliMessage::ControlResponse(_)
            | CliMessage::ControlCancelRequest(_) => Vec::new(),
            CliMessage::RateLimit(limit) => {
                // The push arrives every turn and almost always says the
                // plan has room, which is not news. Only a change to the
                // (status, window, scope) of one that is not `allowed` is:
                // the CLI sends a new event for each scope.
                //
                // Known limit: a warning on a second window leaves the
                // event's top level unchanged (the second window only shows
                // up under `unifiedWindows`), so it has the same key and
                // adds no line.
                let status = limit.status.clone().unwrap_or_default();
                if status.is_empty() || status == "allowed" {
                    self.rate_limit_seen = None;
                    return Vec::new();
                }
                let key = (
                    status.clone(),
                    limit.rate_limit_type.clone(),
                    limit.limit_scope.clone(),
                );
                if self.rate_limit_seen.as_ref() == Some(&key) {
                    return Vec::new();
                }
                self.rate_limit_seen = Some(key);
                self.notices.push(SessionNotice::RateLimited {
                    status,
                    window: limit.rate_limit_type.clone(),
                    resets_at: limit.resets_at,
                    used_percent: limit.used_percent,
                    limit_scope: limit.limit_scope.clone(),
                });
                vec![ChatEvent::OtherSessionUpdate {
                    kind: "SessionNotice".into(),
                }]
            }
            CliMessage::Other { kind, subtype } => {
                vec![ChatEvent::OtherSessionUpdate {
                    kind: match subtype {
                        Some(subtype) => format!("{kind}/{subtype}"),
                        None => kind,
                    },
                }]
            }
        }
    }

    /// Marks an open call as waiting on the user's decision.
    pub(crate) fn mark_pending(&mut self, tool_use_id: &str) -> Vec<ChatEvent> {
        self.status_update(tool_use_id, "Pending")
    }

    /// Marks it running again once the decision was "allow".
    pub(crate) fn mark_running(&mut self, tool_use_id: &str) -> Vec<ChatEvent> {
        self.status_update(tool_use_id, "InProgress")
    }

    fn status_update(&mut self, tool_use_id: &str, status: &str) -> Vec<ChatEvent> {
        if self.is_subagent_call(tool_use_id) || !self.open_calls.contains_key(tool_use_id) {
            return Vec::new();
        }
        vec![ChatEvent::ToolCallUpdated {
            id: tool_use_id.to_string(),
            title: None,
            status: Some(status.to_string()),
            kind: None,
            content: None,
            locations: None,
            raw_input: None,
            raw_output: None,
        }]
    }

    /// Opens a call from a permission request that arrived before its
    /// `assistant` block. The ordering is not guaranteed by the protocol,
    /// and a permission card for a row the transcript never drew reads as a
    /// question about nothing.
    pub(crate) fn start_from_permission(
        &mut self,
        tool_name: &str,
        tool_use_id: &str,
        input: &serde_json::Value,
    ) -> Vec<ChatEvent> {
        if self.is_subagent_call(tool_use_id) {
            return Vec::new();
        }
        if self.open_calls.contains_key(tool_use_id) {
            return self.mark_pending(tool_use_id);
        }
        let tool_use = sirio_claude::message::ToolUse {
            id: tool_use_id.to_string(),
            name: tool_name.to_string(),
            input: input.clone(),
        };
        let info = tools::describe(&tool_use);
        let mut events = Vec::new();
        if is_agent_call(tool_name) {
            self.open_card(tool_use_id, &mut events);
        }
        events.push(started_event(tool_use_id, &info, "Pending"));
        self.open_calls.insert(
            tool_use_id.to_string(),
            OpenCall {
                name: tool_name.to_string(),
                info,
            },
        );
        self.announce_mcp_call(tool_use_id, tool_name, input, &mut events);
        events
    }

    /// Announces a main-thread `mcp__` call, once per id, when the notices are on. The signal
    /// follows the call's own row event, so the row exists when the notice is read.
    fn announce_mcp_call(
        &mut self,
        tool_use_id: &str,
        name: &str,
        input: &serde_json::Value,
        events: &mut Vec<ChatEvent>,
    ) {
        if !self.mcp_apps
            || !name.starts_with("mcp__")
            || self.mcp_announced.iter().any(|id| id == tool_use_id)
        {
            return;
        }
        self.mcp_announced.push(tool_use_id.to_string());
        self.notices.push(SessionNotice::McpTool(McpToolUpdate {
            id: tool_use_id.to_string(),
            item: McpToolItem::Called {
                name: name.to_string(),
                input: input.clone(),
            },
        }));
        events.push(session_notice_signal());
    }

    /// A main-thread `Agent`/`Task` call opens the card its subagent's lines
    /// will fill. The signal rides before the call's own `ToolCallStarted`.
    fn open_card(&mut self, tool_use_id: &str, events: &mut Vec<ChatEvent>) {
        self.subagent_cards
            .insert(tool_use_id.to_string(), (tool_use_id.to_string(), None));
        self.notices.push(SessionNotice::Subagent(SubagentUpdate {
            card: tool_use_id.to_string(),
            agent: None,
            item: SubagentItem::Opened,
        }));
        events.push(ChatEvent::OtherSessionUpdate {
            kind: "SessionNotice".into(),
        });
    }

    /// A subagent's `assistant` line as items of its card. A parent no
    /// main-thread `Agent` call opened draws nothing.
    fn subagent_blocks(&mut self, parent: &str, blocks: &[ContentBlock]) -> Vec<ChatEvent> {
        let Some((card, agent)) = self.subagent_cards.get(parent).cloned() else {
            return Vec::new();
        };
        let mut items = Vec::new();
        for block in blocks {
            match block {
                ContentBlock::Thinking { thinking } if !thinking.is_empty() => {
                    items.push(SubagentItem::Thinking(thinking.clone()));
                }
                ContentBlock::Text { text } if !text.is_empty() => {
                    items.push(SubagentItem::Text(text.clone()));
                }
                ContentBlock::ToolUse(tool_use) => {
                    let info = tools::describe(tool_use);
                    if is_agent_call(&tool_use.name) {
                        let label = tool_use
                            .input
                            .get("description")
                            .and_then(serde_json::Value::as_str)
                            .map_or_else(|| info.title.clone(), str::to_string);
                        self.subagent_cards
                            .insert(tool_use.id.clone(), (card.clone(), Some(label)));
                    }
                    items.push(SubagentItem::ToolStarted {
                        id: tool_use.id.clone(),
                        title: info.title.clone(),
                        kind: info.kind.as_str().to_string(),
                        status: "InProgress".into(),
                        content: info.content.iter().map(content_info).collect(),
                        locations: locations(&info),
                    });
                    self.subagent_open.insert(
                        tool_use.id.clone(),
                        OpenCall {
                            name: tool_use.name.clone(),
                            info,
                        },
                    );
                }
                _ => {}
            }
        }
        self.subagent_items(&card, agent, items)
    }

    /// Queues items for a card, with one signal for the lot.
    fn subagent_items(
        &mut self,
        card: &str,
        agent: Option<String>,
        items: Vec<SubagentItem>,
    ) -> Vec<ChatEvent> {
        if items.is_empty() {
            return Vec::new();
        }
        self.notices
            .extend(items.into_iter().map(|item| {
                SessionNotice::Subagent(SubagentUpdate {
                    card: card.to_string(),
                    agent: agent.clone(),
                    item,
                })
            }));
        vec![ChatEvent::OtherSessionUpdate {
            kind: "SessionNotice".into(),
        }]
    }

    /// A subagent's `user` line: its calls' results settle their items.
    fn subagent_results(
        &mut self,
        parent: &str,
        results: Vec<sirio_claude::message::ToolResult>,
        tool_use_result: Option<&serde_json::Value>,
    ) -> Vec<ChatEvent> {
        let Some((card, agent)) = self.subagent_cards.get(parent).cloned() else {
            return Vec::new();
        };
        let items = results
            .into_iter()
            .map(|result| {
                let open = self.subagent_open.remove(&result.tool_use_id);
                // A nested agent's report is already in the card as text; a
                // failure is not the report, so its error text stays.
                let content = open
                    .filter(|open| result.is_error || !is_agent_call(&open.name))
                    .map(|open| completion_content(&open, &result, tool_use_result));
                SubagentItem::ToolCompleted {
                    id: result.tool_use_id,
                    status: if result.is_error {
                        "Failed".into()
                    } else {
                        "Completed".into()
                    },
                    content,
                }
            })
            .collect();
        self.subagent_items(&card, agent, items)
    }

    fn apply_system(&mut self, system: SystemPayload) -> Vec<ChatEvent> {
        let mut events = Vec::new();
        if let Some(mode) = system.permission_mode.clone() {
            let changed = self.current_mode.as_deref() != Some(mode.as_str());
            self.current_mode = Some(mode.clone());
            if changed {
                events.push(ChatEvent::OtherSessionUpdate {
                    kind: format!("CurrentModeUpdate({mode})"),
                });
            }
        }
        match system.subtype.as_str() {
            "init" => {
                if let Some(version) = system.claude_code_version.clone() {
                    self.claude_version = Some(version);
                }
                if let Some(model) = system.model.clone() {
                    self.model = Some(model);
                }
                if let Some(session_id) = system.session_id.clone() {
                    let first = self.session_id.is_none();
                    self.session_id = Some(session_id);
                    if first {
                        // The id is read back through the client, the way a
                        // mode change is: `ChatEvent` must not grow a variant
                        // that every consumer in the workspace would have to
                        // match.
                        events.push(ChatEvent::OtherSessionUpdate {
                            kind: "SessionIdentified".into(),
                        });
                    }
                }
                // MCP connection failures are still not announced: the native
                // session loads the user's own `.mcp.json`/plugins and a
                // broken server would otherwise banner every chat. The list
                // is kept for the session panel's connectors column, which
                // is where a failed server is shown, quietly.
                self.init_mcp_servers = Some(system.mcp_servers.clone());
            }
            "status" => {}
            "compact_boundary" => {
                let metadata = system.compact_metadata.clone().unwrap_or_default();
                self.notices.push(SessionNotice::Compacted {
                    trigger: metadata.trigger,
                    pre_tokens: metadata.pre_tokens,
                    post_tokens: metadata.post_tokens,
                });
                events.push(ChatEvent::OtherSessionUpdate {
                    kind: "SessionNotice".into(),
                });
            }
            "api_retry" => {
                if let (Some(attempt), Some(max_retries)) = (system.attempt, system.max_retries) {
                    self.notices.push(SessionNotice::Retrying {
                        attempt,
                        max_retries,
                        retry_delay_ms: system.retry_delay_ms.unwrap_or(0),
                        error: system.error.clone(),
                        error_status: system.error_status,
                        no_response: system
                            .no_response
                            .as_ref()
                            .is_some_and(serde_json::Value::is_object),
                    });
                    events.push(ChatEvent::OtherSessionUpdate {
                        kind: "SessionNotice".into(),
                    });
                }
            }
            "notification" => {
                if let Some(text) = system.text.clone().filter(|text| !text.is_empty()) {
                    self.notices.push(SessionNotice::Notification {
                        key: system.key.clone().unwrap_or_default(),
                        text,
                        priority: system.priority.clone(),
                        color: system.color.clone(),
                        timeout_ms: system.timeout_ms,
                    });
                    events.push(ChatEvent::OtherSessionUpdate {
                        kind: "SessionNotice".into(),
                    });
                }
            }
            "model_refusal_fallback" | "model_refusal_no_fallback" | "informational" => {
                if let Some(text) = system.content.clone().filter(|text| !text.is_empty()) {
                    self.notices.push(SessionNotice::CliNotice {
                        text,
                        warning: system.subtype == "informational"
                            && system.level.as_deref() == Some("warning"),
                    });
                    // Its row sits between the text around it, so the next
                    // chunk opens a new row and needs no separator.
                    self.last_row_was_chunk = false;
                    events.push(ChatEvent::OtherSessionUpdate {
                        kind: "SessionNotice".into(),
                    });
                }
            }
            "task_started" => {
                if let Some(task_id) = system.task_id.clone()
                    && system.is_backgrounded == Some(true)
                {
                    self.backgrounded.insert(task_id.clone());
                    events.push(ChatEvent::BackgroundTask(BackgroundTaskEvent::Started {
                        task_id,
                        tool_use_id: system.tool_use_id.clone(),
                        task_type: system.task_type.clone(),
                        description: system.description.clone().unwrap_or_default(),
                    }));
                }
            }
            "task_updated" => {
                // Only a terminal status ends a task; any other patch is a
                // change of some other field.
                if let Some(task_id) = system.task_id.clone()
                    && let Some(status) = system
                        .patch
                        .as_ref()
                        .and_then(|patch| patch.status.clone())
                        .filter(|status| {
                            matches!(
                                status.as_str(),
                                "completed" | "killed" | "failed" | "stopped"
                            )
                        })
                {
                    events.push(ChatEvent::BackgroundTask(BackgroundTaskEvent::Ended {
                        task_id,
                        status,
                    }));
                }
            }
            "task_notification" => {
                // The status of a notification is always an end, whether or
                // not this task was a background one: the registry decides
                // whether it knows the id.
                if let Some(task_id) = system.task_id.clone()
                    && let Some(status) = system.status.clone().filter(|status| {
                        matches!(status.as_str(), "completed" | "failed" | "stopped")
                    })
                {
                    events.push(ChatEvent::BackgroundTask(BackgroundTaskEvent::Ended {
                        task_id,
                        status,
                    }));
                }
                // `is_backgrounded` is not repeated here, so the start is
                // the only place that ever said so.
                if let Some(task_id) = system.task_id.as_deref()
                    && self.backgrounded.remove(task_id)
                    && let Some(summary) = system.summary.clone().filter(|s| !s.trim().is_empty())
                {
                    self.notices
                        .push(SessionNotice::BackgroundTaskEnded { summary });
                    events.push(ChatEvent::OtherSessionUpdate {
                        kind: "SessionNotice".into(),
                    });
                }
            }
            "background_tasks_changed" => {
                events.push(ChatEvent::BackgroundTask(BackgroundTaskEvent::Rows(
                    system
                        .tasks
                        .iter()
                        .flatten()
                        // A row the CLI sent without an id cannot be told apart.
                        .filter(|task| !task.task_id.is_empty())
                        .map(|task| BackgroundTaskRow {
                            task_id: task.task_id.clone(),
                            task_type: task.task_type.clone(),
                            description: task.description.clone().unwrap_or_default(),
                        })
                        .collect(),
                )));
                let count = system.tasks.as_ref().map_or(0, Vec::len);
                if count != self.background_tasks {
                    self.background_tasks = count;
                    events.push(ChatEvent::OtherSessionUpdate {
                        kind: format!("BackgroundTasksUpdate({count})"),
                    });
                }
            }
            "elicitation_complete" => {
                if let (Some(server), Some(elicitation_id)) = (
                    system.mcp_server_name.clone(),
                    system.elicitation_id.clone(),
                ) {
                    events.push(ChatEvent::ElicitationCompleted {
                        server,
                        elicitation_id,
                    });
                }
            }
            "session_state_changed" => {
                if let Some(state) = system.state.as_deref().and_then(CliState::parse) {
                    events.push(ChatEvent::CliState { state });
                }
            }
            other => events.push(ChatEvent::OtherSessionUpdate {
                kind: format!("system/{other}"),
            }),
        }
        events
    }

    fn apply_result(&mut self, result: ResultPayload) -> Vec<ChatEvent> {
        let mut events = Vec::new();
        let local_command_text = self.local_command_text.take();
        if result.is_error
            && let Some(text) = result.result.clone().filter(|text| !text.is_empty())
            && local_command_text.as_deref() != Some(text.as_str())
        {
            // The user must see why a turn failed. The alternative — a bare
            // footer saying "ended" — is the exact silent failure this
            // transcript is responsible for not producing.
            events.push(ChatEvent::AgentMessageChunk(text));
        }
        let mut input_tokens = 0;
        let mut output_tokens = 0;
        let mut cached_read_tokens = 0;
        let mut cache_creation_tokens = 0;
        let mut context_window = None;
        for (model, usage) in &result.model_usage {
            input_tokens += usage.input_tokens;
            output_tokens += usage.output_tokens;
            cached_read_tokens += usage.cache_read_input_tokens;
            cache_creation_tokens += usage.cache_creation_input_tokens;
            if self.model.as_deref() == Some(model.as_str()) || context_window.is_none() {
                context_window = usage.context_window.or(context_window);
            }
        }
        if !result.model_usage.is_empty() {
            events.push(ChatEvent::TokenUsageBreakdown {
                input_tokens,
                output_tokens,
                cached_read_tokens: Some(cached_read_tokens),
            });
        }
        let stop_reason = stop_reason_label(&result);
        self.last_result = Some(ResultSummary {
            stop_reason: stop_reason.clone(),
            is_error: result.is_error,
            error_text: result.result.clone().filter(|_| result.is_error),
            total_cost_usd: result.total_cost_usd,
            input_tokens,
            output_tokens,
            cached_read_tokens,
            cache_creation_tokens,
            context_window,
        });
        // A turn that ends with calls still open had them abandoned; the
        // surface must not keep spinners running for work nobody will
        // report on.
        self.open_calls.clear();
        let cancelled = std::mem::take(&mut self.mcp_announced);
        if !cancelled.is_empty() {
            self.notices
                .extend(cancelled.into_iter().map(|id| {
                    SessionNotice::McpTool(McpToolUpdate {
                        id,
                        item: McpToolItem::Cancelled,
                    })
                }));
            events.push(session_notice_signal());
        }
        self.streamed_ids.clear();
        self.open_message = None;
        self.last_row_was_chunk = false;
        events.push(ChatEvent::TurnEnded { stop_reason });
        events
    }
}

/// The stop reason, spelled the way `sirio_ui::chat::turn_end_label` reads
/// it. That function matches ACP's own debug names exactly, so a native
/// turn has to arrive under the same words or every abnormal end silently
/// renders as an ordinary one.
fn stop_reason_label(result: &ResultPayload) -> String {
    let raw = result
        .stop_reason
        .as_deref()
        .unwrap_or(result.subtype.as_str());
    match raw {
        "cancelled" | "canceled" | "interrupted" => "Cancelled",
        "refusal" | "refused" => "Refusal",
        "max_tokens" | "error_max_structured_output_retries" => "MaxTokens",
        "error_max_turns" => "MaxTurnRequests",
        "end_turn" | "success" | "tool_use" => "EndTurn",
        other => other,
    }
    .to_string()
}

/// The signal that tells a surface to drain the notice queue.
fn session_notice_signal() -> ChatEvent {
    ChatEvent::OtherSessionUpdate {
        kind: "SessionNotice".into(),
    }
}

/// Whether a tool call starts a subagent.
fn is_agent_call(name: &str) -> bool {
    matches!(name, "Agent" | "Task")
}

fn started_event(id: &str, info: &ToolInfo, status: &str) -> ChatEvent {
    ChatEvent::ToolCallStarted {
        id: id.to_string(),
        title: info.title.clone(),
        status: status.to_string(),
        kind: info.kind.as_str().to_string(),
        content: info.content.iter().map(content_info).collect(),
        locations: locations(info),
        raw_input: None,
        raw_output: None,
    }
}

fn locations(info: &ToolInfo) -> Vec<ToolCallLocationInfo> {
    info.locations
        .iter()
        .map(|location| ToolCallLocationInfo {
            path: PathBuf::from(&location.path),
            line: location.line,
        })
        .collect()
}

fn content_info(content: &ToolContent) -> ToolCallContentInfo {
    match content {
        ToolContent::Text(text) => ToolCallContentInfo::Text(text.clone()),
        ToolContent::Diff {
            path,
            old_text,
            new_text,
        } => ToolCallContentInfo::Diff(ToolCallDiff {
            path: PathBuf::from(path),
            old_text: old_text.clone(),
            new_text: new_text.clone(),
        }),
    }
}

/// What a completed call shows: the result's own diff when it carries one,
/// the call's proposed diff when it does not, and the result text for
/// everything else.
fn completion_content(
    open: &OpenCall,
    result: &sirio_claude::message::ToolResult,
    tool_use_result: Option<&serde_json::Value>,
) -> Vec<ToolCallContentInfo> {
    if let Some(value) = tool_use_result
        && let Some(diff) = tools::diff_from_result(&open.name, value)
    {
        return vec![content_info(&diff)];
    }
    if let Some(diff) = open
        .info
        .content
        .iter()
        .find(|content| matches!(content, ToolContent::Diff { .. }))
    {
        return vec![content_info(diff)];
    }
    if result.text.is_empty() {
        Vec::new()
    } else {
        vec![ToolCallContentInfo::Text(result.text.clone())]
    }
}

/// A `TodoWrite`'s entries as plan rows, or `None` for any other tool.
fn plan_entries(tool_name: &str, input: &serde_json::Value) -> Option<Vec<PlanEntryInfo>> {
    if tool_name != "TodoWrite" {
        return None;
    }
    let todos = input.get("todos")?.as_array()?;
    Some(
        todos
            .iter()
            .filter_map(|todo| {
                Some(PlanEntryInfo {
                    content: todo.get("content")?.as_str()?.to_string(),
                    status: todo
                        .get("status")
                        .and_then(|status| status.as_str())
                        .unwrap_or("pending")
                        .to_string(),
                })
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn parse(line: serde_json::Value) -> CliMessage {
        CliMessage::parse(&line.to_string()).expect("fixture line parses")
    }

    #[test]
    fn only_the_three_known_states_become_events() {
        let state_line = |state: Option<&str>| {
            let mut line = json!({"type": "system", "subtype": "session_state_changed"});
            if let Some(state) = state {
                line["state"] = json!(state);
            }
            parse(line)
        };
        for (word, state) in [
            ("running", CliState::Running),
            ("idle", CliState::Idle),
            ("requires_action", CliState::RequiresAction),
        ] {
            let mut fold = Fold::new();
            assert_eq!(
                fold.apply(state_line(Some(word))),
                vec![ChatEvent::CliState { state }],
                "{word}"
            );
        }
        for unknown in [Some("paused"), None] {
            let mut fold = Fold::new();
            assert_eq!(fold.apply(state_line(unknown)), vec![], "{unknown:?}");
        }
    }

    #[test]
    fn text_and_thinking_deltas_stream_through() {
        let mut fold = Fold::new();
        assert_eq!(
            fold.apply(parse(json!({
                "type": "stream_event",
                "event": {"type": "content_block_delta",
                          "delta": {"type": "text_delta", "text": "Hel"}}
            }))),
            vec![ChatEvent::AgentMessageChunk("Hel".into())]
        );
        assert_eq!(
            fold.apply(parse(json!({
                "type": "stream_event",
                "event": {"type": "content_block_delta",
                          "delta": {"type": "thinking_delta", "thinking": "weighing"}}
            }))),
            vec![ChatEvent::ThoughtChunk("weighing".into())]
        );
    }

    #[test]
    fn an_empty_thinking_delta_is_not_a_thought() {
        // Claude sends a thinking block's plaintext only for the models
        // that expose it. For every other model the block still arrives --
        // one `thinking_delta` carrying `""`, then the signature that
        // proves the reasoning happened -- and forwarding that opens a
        // thought section the reader expands onto nothing, which reads as
        // content this build lost rather than content the CLI never sent.
        let mut fold = Fold::new();
        let events = fold.apply(parse(json!({
            "type": "stream_event",
            "event": {"type": "content_block_delta",
                      "delta": {"type": "thinking_delta", "thinking": "",
                                "estimated_tokens": null}}
        })));
        assert!(
            events.is_empty(),
            "a thought with no text is not a thought: {events:?}"
        );
    }

    /// The line has no `id`, so its text is never drawn (an answer is drawn
    /// only for a message whose id is known and was not streamed); what it
    /// contributes here is its tool calls.
    #[test]
    fn an_assistant_message_contributes_only_its_tool_calls() {
        let mut fold = Fold::new();
        let events = fold.apply(parse(json!({
            "type": "assistant",
            "message": {"content": [
                {"type": "text", "text": "already streamed as deltas"},
                {"type": "tool_use", "id": "toolu_1", "name": "Read",
                 "input": {"file_path": "/repo/a.rs"}}
            ]},
            "parent_tool_use_id": null
        })));
        assert_eq!(
            events.len(),
            1,
            "the text block must not be re-emitted: {events:?}"
        );
        let ChatEvent::ToolCallStarted {
            id,
            title,
            status,
            kind,
            locations,
            ..
        } = &events[0]
        else {
            panic!("expected a started tool call, got {events:?}");
        };
        assert_eq!(id, "toolu_1");
        assert_eq!(title, "/repo/a.rs");
        assert_eq!(status, "InProgress");
        assert_eq!(kind, "Read");
        assert_eq!(locations[0].path, std::path::PathBuf::from("/repo/a.rs"));
    }

    #[test]
    fn a_pending_permission_holds_the_call_then_releases_it() {
        let mut fold = Fold::new();
        fold.apply(parse(json!({
            "type": "assistant",
            "message": {"content": [{"type": "tool_use", "id": "toolu_1", "name": "Bash",
                                     "input": {"command": "ls"}}]}
        })));
        let pending = fold.mark_pending("toolu_1");
        assert!(matches!(
            pending.as_slice(),
            [ChatEvent::ToolCallUpdated { id, status: Some(status), .. }]
                if id == "toolu_1" && status == "Pending"
        ));
        let running = fold.mark_running("toolu_1");
        assert!(matches!(
            running.as_slice(),
            [ChatEvent::ToolCallUpdated { id, status: Some(status), .. }]
                if id == "toolu_1" && status == "InProgress"
        ));
    }

    #[test]
    fn a_permission_that_arrives_before_its_block_starts_the_call_itself() {
        let mut fold = Fold::new();
        let started = fold.start_from_permission("Bash", "toolu_9", &json!({"command": "ls"}));
        assert!(matches!(
            started.as_slice(),
            [ChatEvent::ToolCallStarted { id, status, .. }]
                if id == "toolu_9" && status == "Pending"
        ));
        // The assistant block for the same id arrives afterwards and must
        // not draw a second row.
        let again = fold.apply(parse(json!({
            "type": "assistant",
            "message": {"content": [{"type": "tool_use", "id": "toolu_9", "name": "Bash",
                                     "input": {"command": "ls"}}]}
        })));
        assert!(again.is_empty(), "the call was already started: {again:?}");
    }

    #[test]
    fn a_tool_result_completes_the_call_and_carries_its_diff() {
        let mut fold = Fold::new();
        fold.apply(parse(json!({
            "type": "assistant",
            "message": {"content": [{"type": "tool_use", "id": "toolu_2", "name": "Edit",
                "input": {"file_path": "/repo/a.rs", "old_string": "two", "new_string": "TWO"}}]}
        })));
        let events = fold.apply(parse(json!({
            "type": "user",
            "message": {"content": [{"type": "tool_result", "tool_use_id": "toolu_2",
                                     "is_error": false, "content": "Edited /repo/a.rs"}]},
            "tool_use_result": {
                "filePath": "/repo/a.rs",
                "originalFile": "one\ntwo\nthree\n",
                "structuredPatch": [{"oldStart": 2, "oldLines": 1, "newStart": 2, "newLines": 1,
                                     "lines": [" one", "-two", "+TWO", " three"]}]
            }
        })));
        let [
            ChatEvent::ToolCallCompleted {
                id,
                status,
                content,
                ..
            },
        ] = events.as_slice()
        else {
            panic!("expected one completed call, got {events:?}");
        };
        assert_eq!(id, "toolu_2");
        assert_eq!(status, "Completed");
        let content = content.as_ref().expect("the completion replaces content");
        assert_eq!(
            content,
            &vec![ToolCallContentInfo::Diff(ToolCallDiff {
                path: std::path::PathBuf::from("/repo/a.rs"),
                old_text: Some("one\ntwo\nthree\n".into()),
                new_text: "one\nTWO\nthree\n".into(),
            })]
        );
    }

    #[test]
    fn a_failed_tool_result_says_so_and_shows_its_text() {
        let mut fold = Fold::new();
        fold.apply(parse(json!({
            "type": "assistant",
            "message": {"content": [{"type": "tool_use", "id": "toolu_3", "name": "Bash",
                                     "input": {"command": "false"}}]}
        })));
        let events = fold.apply(parse(json!({
            "type": "user",
            "message": {"content": [{"type": "tool_result", "tool_use_id": "toolu_3",
                                     "is_error": true, "content": "exit status 1"}]}
        })));
        let [
            ChatEvent::ToolCallCompleted {
                status, content, ..
            },
        ] = events.as_slice()
        else {
            panic!("expected a completed call, got {events:?}");
        };
        assert_eq!(status, "Failed");
        assert_eq!(
            content.as_ref().expect("failure text"),
            &vec![ToolCallContentInfo::Text("exit status 1".into())]
        );
    }

    #[test]
    fn a_todo_write_publishes_the_plan_alongside_its_call() {
        let mut fold = Fold::new();
        let events = fold.apply(parse(json!({
            "type": "assistant",
            "message": {"content": [{"type": "tool_use", "id": "toolu_4", "name": "TodoWrite",
                "input": {"todos": [
                    {"content": "Read the spec", "status": "completed"},
                    {"content": "Write the test", "status": "in_progress"}
                ]}}]}
        })));
        let plan = events.iter().find_map(|event| match event {
            ChatEvent::PlanUpdate { entries } => Some(entries),
            _ => None,
        });
        assert_eq!(
            plan.expect("a plan update"),
            &vec![
                PlanEntryInfo {
                    content: "Read the spec".into(),
                    status: "completed".into()
                },
                PlanEntryInfo {
                    content: "Write the test".into(),
                    status: "in_progress".into()
                },
            ]
        );
    }

    #[test]
    fn a_result_reports_usage_then_ends_the_turn() {
        let mut fold = Fold::new();
        let events = fold.apply(parse(json!({
            "type": "result", "subtype": "success", "is_error": false,
            "result": "Done.", "total_cost_usd": 0.0421,
            "modelUsage": {"claude-fable-5-1": {
                "inputTokens": 1200, "outputTokens": 340, "cacheReadInputTokens": 8000,
                "cacheCreationInputTokens": 500, "costUSD": 0.0421, "contextWindow": 200000}}
        })));
        assert_eq!(
            events,
            vec![
                ChatEvent::TokenUsageBreakdown {
                    input_tokens: 1200,
                    output_tokens: 340,
                    cached_read_tokens: Some(8000),
                },
                ChatEvent::TurnEnded {
                    stop_reason: "EndTurn".into()
                },
            ]
        );
        let summary = fold.last_result().expect("a recorded result");
        assert_eq!(summary.context_window, Some(200_000));
        assert_eq!(summary.total_cost_usd, Some(0.0421));
        // Recorded, not folded into another number: the meter's fallback
        // adds each term itself.
        assert_eq!(summary.cache_creation_tokens, 500);
    }

    #[test]
    fn an_errored_result_shows_its_sentence_before_ending_the_turn() {
        let mut fold = Fold::new();
        let events = fold.apply(parse(json!({
            "type": "result", "subtype": "error_max_turns", "is_error": true,
            "result": "Reached the turn limit"
        })));
        assert_eq!(
            events[0],
            ChatEvent::AgentMessageChunk("Reached the turn limit".into())
        );
        assert!(matches!(events.last(), Some(ChatEvent::TurnEnded { .. })));
    }

    #[test]
    fn a_rate_limit_is_silent_while_it_still_allows_the_turn() {
        let mut fold = Fold::new();
        let events = fold.apply(parse(json!({
            "type": "rate_limit_event",
            "rate_limit_info": {"status": "allowed", "rateLimitType": "five_hour",
                                "resetsAt": 1789990800,
                                "unifiedWindows": {"five_hour": {"utilization": 0.53}}}
        })));
        assert!(
            events.is_empty(),
            "the usual push says the plan has room and is not news: {events:?}"
        );
        assert!(fold.take_notices().is_empty());
    }

    #[test]
    fn a_rate_limit_that_stops_the_plan_is_announced_once() {
        let mut fold = Fold::new();
        let line = json!({
            "type": "rate_limit_event",
            "rate_limit_info": {"status": "rejected", "rateLimitType": "seven_day",
                                "resetsAt": 1790496000,
                                "unifiedWindows": {"seven_day": {"utilization": 1.0}}}
        });
        assert_eq!(
            fold.apply(parse(line.clone())),
            vec![ChatEvent::OtherSessionUpdate {
                kind: "SessionNotice".into()
            }]
        );
        assert_eq!(
            fold.take_notices(),
            vec![SessionNotice::RateLimited {
                status: "rejected".into(),
                window: Some("seven_day".into()),
                resets_at: Some(1_790_496_000),
                used_percent: Some(100),
                limit_scope: None,
            }]
        );
        // The push repeats every turn. Saying it again while nothing has
        // changed would stack one banner per turn for the rest of the day.
        assert!(fold.apply(parse(line)).is_empty());
        assert!(fold.take_notices().is_empty());
    }

    // Failure list, written before the code: a rejection under a new scope,
    // or on another window, swallowed by the one before it; the same warning
    // announced twice because one carried `unifiedWindows`; the same warning
    // swallowed after an `allowed` in between.
    #[test]
    fn a_rate_limit_is_announced_again_only_when_status_window_or_scope_changes() {
        let push = |info: serde_json::Value| {
            json!({"type": "rate_limit_event", "rate_limit_info": info})
        };
        let warning = || {
            json!({"status": "allowed_warning", "rateLimitType": "five_hour",
                   "utilization": 0.92, "resetsAt": 1789990800})
        };
        let mut fold = Fold::new();
        let announced = |fold: &mut Fold, info: serde_json::Value| {
            fold.apply(parse(push(info)));
            fold.take_notices().len()
        };
        // Guard: holds before the change. The first warning is announced.
        assert_eq!(announced(&mut fold, warning()), 1);
        // Guard: holds before the change. The same warning, with or without
        // the per-window detail, adds nothing.
        let mut detailed = warning();
        detailed["unifiedWindows"] = json!({"five_hour": {"utilization": 0.92}});
        assert_eq!(announced(&mut fold, detailed), 0);
        assert_eq!(announced(&mut fold, warning()), 0);
        // Guard: holds before the change (the old status-only key also saw
        // the change of status). The same window refusing is announced once.
        let mut rejected = warning();
        rejected["status"] = json!("rejected");
        assert_eq!(announced(&mut fold, rejected.clone()), 1);
        assert_eq!(announced(&mut fold, rejected.clone()), 0);
        // A new scope is a new event, with the same status and window.
        let mut scoped = rejected.clone();
        scoped["limitScope"] = json!("group_pool");
        assert_eq!(announced(&mut fold, scoped.clone()), 1);
        // Guard: holds before the change. The repeat of it adds nothing.
        assert_eq!(announced(&mut fold, scoped), 0);
        // Another window with the same status is a change of window.
        let mut weekly = rejected;
        weekly["rateLimitType"] = json!("seven_day");
        assert_eq!(announced(&mut fold, weekly), 1);
        // Guard: holds before the change. The plan having room clears the
        // memory: the warning is news again.
        let mut allowed = warning();
        allowed["status"] = json!("allowed");
        assert_eq!(announced(&mut fold, allowed), 0);
        assert_eq!(announced(&mut fold, warning()), 1);
    }

    #[test]
    fn a_subagents_own_output_stays_out_of_the_top_level_transcript() {
        // A subagent's output goes to its card (a notice), never to the top
        // level; a line whose parent opened no card draws nothing at all.
        let mut fold = Fold::new();
        let events = fold.apply(parse(json!({
            "type": "assistant",
            "message": {"content": [{"type": "tool_use", "id": "toolu_5", "name": "Read",
                                     "input": {"file_path": "/repo/b.rs"}}]},
            "parent_tool_use_id": "toolu_parent"
        })));
        assert!(
            events.is_empty(),
            "subagent traffic must not draw rows: {events:?}"
        );
    }

    // Failure list for the subagent card items:
    // - an item lands in the wrong card;
    // - a nested call draws a top-level row;
    // - a grandchild opens a second card;
    // - a background line after `result` is lost;
    // - an empty or redacted block draws an empty item;
    // - `Opened` comes after `ToolCallStarted`;
    // - a permission-first `Agent` call opens twice;
    // - a subagent's permission draws a row;
    // - a nested `Agent` result repeats its report.

    fn agent_call(id: &str, description: &str) -> serde_json::Value {
        json!({
            "type": "assistant",
            "message": {"id": "msg_main", "content": [{
                "type": "tool_use", "id": id, "name": "Agent",
                "input": {"description": description, "prompt": "go"}
            }]},
            "parent_tool_use_id": null
        })
    }

    fn child_line(parent: &str, block: serde_json::Value) -> CliMessage {
        parse(json!({
            "type": "assistant",
            "message": {"id": "msg_kid", "content": [block]},
            "parent_tool_use_id": parent
        }))
    }

    fn child_result(parent: &str, id: &str, text: &str) -> CliMessage {
        parse(json!({
            "type": "user",
            "message": {"role": "user", "content": [
                {"type": "tool_result", "tool_use_id": id, "content": text, "is_error": false}
            ]},
            "parent_tool_use_id": parent
        }))
    }

    fn signal() -> ChatEvent {
        ChatEvent::OtherSessionUpdate {
            kind: "SessionNotice".into(),
        }
    }

    fn updates(notices: Vec<SessionNotice>) -> Vec<SubagentUpdate> {
        notices
            .into_iter()
            .filter_map(|notice| match notice {
                SessionNotice::Subagent(update) => Some(update),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn an_agent_call_opens_its_card_before_its_row() {
        let mut fold = Fold::new();
        let events = fold.apply(parse(agent_call("toolu_a", "Look around")));
        assert!(matches!(
            events.as_slice(),
            [ChatEvent::OtherSessionUpdate { kind }, ChatEvent::ToolCallStarted { id, .. }]
                if kind == "SessionNotice" && id == "toolu_a"
        ));
        assert_eq!(
            fold.take_notices(),
            vec![SessionNotice::Subagent(SubagentUpdate {
                card: "toolu_a".into(),
                agent: None,
                item: SubagentItem::Opened,
            })]
        );

        // The permission asked first: the card opens there, once.
        let mut fold = Fold::new();
        let input = json!({"description": "Look around", "prompt": "go"});
        let events = fold.start_from_permission("Agent", "toolu_b", &input);
        assert!(matches!(
            events.as_slice(),
            [ChatEvent::OtherSessionUpdate { .. }, ChatEvent::ToolCallStarted { id, .. }]
                if id == "toolu_b"
        ));
        fold.apply(parse(agent_call("toolu_b", "Look around")));
        let opened = updates(fold.take_notices());
        assert_eq!(opened.len(), 1);
        assert_eq!(opened[0].item, SubagentItem::Opened);
    }

    #[test]
    fn a_subagents_blocks_become_items_in_order() {
        let mut fold = Fold::new();
        fold.apply(parse(agent_call("toolu_a", "Look around")));
        fold.take_notices();
        let mut events = Vec::new();
        for block in [
            json!({"type": "thinking", "thinking": "Let me look first.", "signature": "SIG"}),
            json!({"type": "text", "text": "I will look. First I read the file."}),
            json!({"type": "tool_use", "id": "toolu_r", "name": "Read",
                   "input": {"file_path": "/repo/a.rs"}}),
        ] {
            events.extend(fold.apply(child_line("toolu_a", block)));
        }
        events.extend(fold.apply(child_result("toolu_a", "toolu_r", "fn main() {}")));
        assert!(
            events.iter().all(|event| *event == signal()),
            "a subagent's output must reach the card only"
        );
        let items = updates(fold.take_notices());
        assert!(items.iter().all(|u| u.card == "toolu_a" && u.agent.is_none()));
        assert!(matches!(
            items.iter().map(|u| &u.item).collect::<Vec<_>>().as_slice(),
            [
                SubagentItem::Thinking(thought),
                SubagentItem::Text(text),
                SubagentItem::ToolStarted { id, title, kind, status, .. },
                SubagentItem::ToolCompleted { id: done, status: end, content: Some(_) },
            ] if thought == "Let me look first."
                && text == "I will look. First I read the file."
                && id == "toolu_r" && done == "toolu_r"
                && title == "/repo/a.rs" && kind == "Read"
                && status == "InProgress" && end == "Completed"
        ));
    }

    #[test]
    fn a_subagents_own_subagent_goes_into_the_outer_card() {
        let mut fold = Fold::new();
        fold.apply(parse(agent_call("toolu_a", "Look around")));
        fold.take_notices();
        let events = fold.apply(child_line(
            "toolu_a",
            json!({"type": "tool_use", "id": "toolu_g", "name": "Agent",
                   "input": {"description": "Check one thing", "prompt": "check"}}),
        ));
        assert_eq!(events, vec![signal()]);
        fold.apply(child_line("toolu_g", json!({"type": "text", "text": "Checked."})));
        fold.apply(child_result("toolu_a", "toolu_g", "the long report"));
        let items = updates(fold.take_notices());
        // A nested agent that failed keeps its error text: it is not a report.
        fold.apply(child_line(
            "toolu_a",
            json!({"type": "tool_use", "id": "toolu_h", "name": "Agent",
                   "input": {"description": "Denied one", "prompt": "x"}}),
        ));
        fold.apply(parse(json!({
            "type": "user",
            "message": {"role": "user", "content": [
                {"type": "tool_result", "tool_use_id": "toolu_h",
                 "content": "permission denied", "is_error": true}
            ]},
            "parent_tool_use_id": "toolu_a"
        })));
        let failed = updates(fold.take_notices());
        assert!(matches!(
            failed.last().map(|u| &u.item),
            Some(SubagentItem::ToolCompleted { id, status, content: Some(c) })
                if id == "toolu_h" && status == "Failed" && !c.is_empty()
        ));
        assert!(matches!(
            items.as_slice(),
            [
                SubagentUpdate { card, agent: None, item: SubagentItem::ToolStarted { id, .. } },
                SubagentUpdate { card: c2, agent: Some(label), item: SubagentItem::Text(t) },
                SubagentUpdate {
                    card: c3,
                    agent: None,
                    item: SubagentItem::ToolCompleted { id: done, content: None, .. },
                },
            ] if card == "toolu_a" && c2 == "toolu_a" && c3 == "toolu_a"
                && id == "toolu_g" && done == "toolu_g"
                && label == "Check one thing" && t == "Checked."
        ));
    }

    #[test]
    fn two_agents_lines_go_to_their_own_cards() {
        let mut fold = Fold::new();
        fold.apply(parse(agent_call("toolu_a", "One")));
        fold.apply(parse(agent_call("toolu_b", "Two")));
        fold.take_notices();
        for (parent, text) in [("toolu_a", "a1"), ("toolu_b", "b1"), ("toolu_a", "a2")] {
            fold.apply(child_line(parent, json!({"type": "text", "text": text})));
        }
        let seen: Vec<(String, SubagentItem)> = updates(fold.take_notices())
            .into_iter()
            .map(|u| (u.card, u.item))
            .collect();
        assert_eq!(
            seen,
            vec![
                ("toolu_a".to_string(), SubagentItem::Text("a1".into())),
                ("toolu_b".to_string(), SubagentItem::Text("b1".into())),
                ("toolu_a".to_string(), SubagentItem::Text("a2".into())),
            ]
        );
    }

    #[test]
    fn a_background_agents_lines_after_the_result_reach_its_card() {
        let mut fold = Fold::new();
        fold.apply(parse(agent_call("toolu_a", "In the background")));
        fold.apply(parse(json!({
            "type": "result", "subtype": "success", "is_error": false,
            "stop_reason": "end_turn", "session_id": "s", "result": "ok"
        })));
        fold.apply(parse(json!({
            "type": "assistant",
            "message": {"id": "msg_next", "content": [{"type": "text", "text": "next turn"}]},
            "parent_tool_use_id": null
        })));
        fold.take_notices();
        fold.apply(child_line("toolu_a", json!({"type": "text", "text": "late"})));
        let items = updates(fold.take_notices());
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].card, "toolu_a");
        assert_eq!(items[0].item, SubagentItem::Text("late".into()));
    }

    #[test]
    fn lines_of_an_unknown_parent_draw_nothing() {
        let mut fold = Fold::new();
        let text = json!({"type": "text", "text": "x"});
        let mut events = fold.apply(child_line("toolu_nobody", text));
        events.extend(fold.apply(child_result("toolu_nobody", "toolu_r", "x")));
        assert_eq!(events, vec![]);
        assert_eq!(fold.take_notices(), vec![]);
    }

    #[test]
    fn empty_and_redacted_blocks_draw_nothing() {
        let mut fold = Fold::new();
        fold.apply(parse(agent_call("toolu_a", "Look around")));
        fold.take_notices();
        let mut events = Vec::new();
        for block in [
            json!({"type": "thinking", "thinking": "", "signature": "SIG"}),
            json!({"type": "text", "text": ""}),
            json!({"type": "redacted_thinking", "data": "opaque"}),
        ] {
            events.extend(fold.apply(child_line("toolu_a", block)));
        }
        assert_eq!(events, vec![]);
        assert_eq!(fold.take_notices(), vec![]);
    }

    #[test]
    fn a_subagents_permission_opens_no_tool_row() {
        // The subagent's own lines never reach the transcript, so a row
        // opened for its permission would never get its result: it would sit
        // at the top level, running, for the rest of the turn.
        let mut fold = Fold::new();
        fold.apply(parse(json!({
            "type": "assistant",
            "message": {"content": [{"type": "tool_use", "id": "toolu_sub", "name": "Bash",
                                     "input": {"command": "ls"}}]},
            "parent_tool_use_id": "toolu_parent"
        })));
        assert!(fold.is_subagent_call("toolu_sub"));
        assert_eq!(
            fold.start_from_permission("Bash", "toolu_sub", &json!({"command": "ls"})),
            vec![]
        );
        assert_eq!(fold.mark_pending("toolu_sub"), vec![]);
        assert_eq!(fold.mark_running("toolu_sub"), vec![]);
    }

    #[test]
    fn a_main_thread_permission_still_opens_its_row() {
        let mut fold = Fold::new();
        fold.apply(parse(json!({
            "type": "assistant",
            "message": {"content": [{"type": "tool_use", "id": "toolu_sub", "name": "Bash",
                                     "input": {"command": "ls"}}]},
            "parent_tool_use_id": "toolu_parent"
        })));
        // A different call, on the main thread, asks while the subagent's
        // call is known.
        assert!(!fold.is_subagent_call("toolu_main"));
        let started = fold.start_from_permission("Bash", "toolu_main", &json!({"command": "ls"}));
        assert!(matches!(
            started.as_slice(),
            [ChatEvent::ToolCallStarted { id, status, .. }]
                if id == "toolu_main" && status == "Pending"
        ));
        let running = fold.mark_running("toolu_main");
        assert!(matches!(
            running.as_slice(),
            [ChatEvent::ToolCallUpdated { id, status: Some(status), .. }]
                if id == "toolu_main" && status == "InProgress"
        ));
        let pending = fold.mark_pending("toolu_main");
        assert!(matches!(
            pending.as_slice(),
            [ChatEvent::ToolCallUpdated { id, status: Some(status), .. }]
                if id == "toolu_main" && status == "Pending"
        ));
    }

    // --- A local command's answer (`/usage`, `/context`).

    fn local_command_line(model: &str, run: serde_json::Value, text: &str) -> serde_json::Value {
        json!({
            "type": "assistant",
            "message": {"id": "msg_local", "model": model,
                        "content": [{"type": "text", "text": text}]},
            "local_command_run": run,
            "parent_tool_use_id": null
        })
    }

    fn output_signal() -> Vec<ChatEvent> {
        vec![ChatEvent::OtherSessionUpdate {
            kind: "SessionNotice".into(),
        }]
    }

    #[test]
    fn a_synthetic_answer_to_a_local_command_is_one_output_notice() {
        let mut fold = Fold::new();
        fold.last_row_was_chunk = true;
        let line = local_command_line(
            "<synthetic>",
            json!({"command": "usage", "args": ""}),
            "Estimated cost: $0",
        );
        assert_eq!(fold.apply(parse(line)), output_signal());
        assert_eq!(
            fold.take_notices(),
            vec![SessionNotice::LocalCommandOutput {
                command: "usage".into(),
                text: "Estimated cost: $0".into(),
                report: None,
            }]
        );
        assert!(
            !fold.last_row_was_chunk,
            "the output is a row of its own: the next chunk must not run on into it"
        );
    }

    #[test]
    fn the_usage_report_travels_raw_and_only_as_an_object() {
        let report = json!({"rate_limits": null, "limits": [{"kind": "session", "percent": 12}]});
        let mut fold = Fold::new();
        let mut line = local_command_line("<synthetic>", json!({"command": "usage"}), "text");
        line["usage_report"] = report.clone();
        fold.apply(parse(line));
        assert_eq!(
            fold.take_notices(),
            vec![SessionNotice::LocalCommandOutput {
                command: "usage".into(),
                text: "text".into(),
                report: Some(report),
            }]
        );
        for unusable in [json!(null), json!([1]), json!("x"), json!(7)] {
            let mut line = local_command_line("<synthetic>", json!({"command": "usage"}), "text");
            line["usage_report"] = unusable;
            fold.apply(parse(line));
            assert!(matches!(
                fold.take_notices().as_slice(),
                [SessionNotice::LocalCommandOutput { report: None, .. }]
            ));
        }
    }

    #[test]
    fn a_report_with_no_text_is_an_answer_only_when_the_card_can_read_it() {
        let mut fold = Fold::new();
        let mut line = local_command_line("<synthetic>", json!({"command": "usage"}), "");
        // Guard: holds before the change. A synthetic line with no text drew
        // nothing, marker or not; only the report-only answer below is new.
        assert_eq!(fold.apply(parse(line.clone())), vec![]);
        assert!(fold.take_notices().is_empty());
        // An object the card cannot read (it names neither `session` nor
        // `rate_limits`) over no text would be a bare label over nothing.
        line["usage_report"] = json!({"limits": []});
        assert_eq!(fold.apply(parse(line.clone())), vec![]);
        assert!(fold.take_notices().is_empty());
        for readable in [json!({"rate_limits": null}), json!({"session": {}})] {
            line["usage_report"] = readable;
            assert_eq!(fold.apply(parse(line.clone())), output_signal());
            assert!(matches!(
                fold.take_notices().as_slice(),
                [SessionNotice::LocalCommandOutput { text, report: Some(_), .. }] if text.is_empty()
            ));
        }
    }

    #[test]
    fn a_model_answer_that_carries_the_marker_is_left_to_the_ordinary_rules() {
        // Guard: holds before the change. The unknown marker was ignored, so an
        // unstreamed model answer was already drawn as a chunk.
        let mut fold = Fold::new();
        let line = local_command_line(
            "claude-opus-5",
            json!({"command": "usage", "args": ""}),
            "an answer no stream carried",
        );
        assert_eq!(
            fold.apply(parse(line)),
            vec![ChatEvent::AgentMessageChunk("an answer no stream carried".into())]
        );
        assert!(fold.take_notices().is_empty());
    }

    #[test]
    fn a_subagents_message_is_not_a_local_command_answer() {
        // Guard: holds before the change. A subagent's line drew nothing.
        let mut fold = Fold::new();
        let mut line = local_command_line("<synthetic>", json!({"command": "usage"}), "text");
        line["parent_tool_use_id"] = json!("toolu_parent");
        assert_eq!(fold.apply(parse(line)), vec![]);
        assert!(fold.take_notices().is_empty());
    }

    #[test]
    fn a_wrongly_typed_marker_draws_nothing_and_the_line_still_parses() {
        // Guard: holds before the change. The unknown field was ignored, so the
        // line parsed and a synthetic one drew nothing.
        for run in [json!("usage"), json!({"command": 5}), json!([]), json!(null)] {
            let line = local_command_line("<synthetic>", run, "text");
            let message = parse(line);
            assert!(
                matches!(message, CliMessage::Assistant(_)),
                "a wrong type costs the field, never the line"
            );
            let mut fold = Fold::new();
            assert_eq!(fold.apply(message), vec![]);
            assert!(fold.take_notices().is_empty());
        }
    }

    #[test]
    fn an_assistant_line_with_neither_field_still_parses_as_assistant() {
        // `AssistantPayload` has no container default: the two fields must carry
        // their own, or every ordinary line would demote to `Other`.
        let message = parse(json!({
            "type": "assistant",
            "message": {"id": "msg_1", "model": "claude-opus-5",
                        "content": [{"type": "text", "text": "hi"}]}
        }));
        // Guard: holds before the change. An ordinary assistant line already
        // parsed as `Assistant`; only the two `None`s below are new.
        let CliMessage::Assistant(assistant) = message else {
            panic!("an ordinary assistant line must parse as Assistant");
        };
        assert!(assistant.local_command_run.is_none() && assistant.usage_report.is_none());
    }

    #[test]
    fn a_failing_command_is_drawn_once_not_again_as_the_results_error() {
        let mut fold = Fold::new();
        fold.apply(parse(local_command_line(
            "<synthetic>",
            json!({"command": "usage"}),
            "the block",
        )));
        let events = fold.apply(parse(json!({
            "type": "result", "subtype": "success", "is_error": true, "result": "the block",
            "num_turns": 0
        })));
        assert!(
            !events.iter().any(|e| matches!(e, ChatEvent::AgentMessageChunk(_))),
            "a failed result repeating the drawn text must add no chunk"
        );
        // Guard: holds before the change. `apply_result` always ends the turn.
        assert!(matches!(events.last(), Some(ChatEvent::TurnEnded { .. })));

        // Guard: holds before the change (every failed result was drawn). The
        // remembered text ends with its turn: the same words as a later
        // turn's failure are that turn's own error and are drawn.
        let events = fold.apply(parse(json!({
            "type": "result", "subtype": "success", "is_error": true, "result": "the block"
        })));
        assert!(events.iter().any(|e| matches!(e, ChatEvent::AgentMessageChunk(_))));
    }

    #[test]
    fn a_failing_result_with_other_words_is_still_drawn() {
        // Guard: holds before the change. Every failed result was drawn.
        let mut fold = Fold::new();
        fold.apply(parse(local_command_line(
            "<synthetic>",
            json!({"command": "usage"}),
            "the block",
        )));
        let events = fold.apply(parse(json!({
            "type": "result", "subtype": "success", "is_error": true, "result": "a real failure"
        })));
        assert!(events.contains(&ChatEvent::AgentMessageChunk("a real failure".into())));
    }

    // Failure list for the MCP tool notices:
    // - a call announced twice, whether its permission or its `assistant` block came first;
    // - a call announced with the gate shut, or a tool that is not an `mcp__` one;
    // - a result announced for a call nobody announced;
    // - an errored result that does not say so when the CLI gave no structured result;
    // - a call still open at the turn's end that is not cancelled, or one that already returned
    //   and is cancelled all the same, or cancelled again by the next turn.
    fn mcp_updates(fold: &mut Fold) -> Vec<sirio_chat_vocab::McpToolUpdate> {
        fold.take_notices()
            .into_iter()
            .filter_map(|notice| match notice {
                SessionNotice::McpTool(update) => Some(update),
                _ => None,
            })
            .collect()
    }

    fn mcp_call_line(id: &str, name: &str) -> CliMessage {
        parse(json!({
            "type": "assistant",
            "message": {"content": [{"type": "tool_use", "id": id, "name": name,
                                     "input": {"text": "hi"}}]}
        }))
    }

    fn mcp_result_line(id: &str, is_error: bool, text: &str) -> CliMessage {
        parse(json!({
            "type": "user",
            "message": {"content": [{"type": "tool_result", "tool_use_id": id,
                                     "is_error": is_error, "content": text}]}
        }))
    }

    #[test]
    fn an_mcp_call_is_announced_once_whichever_line_comes_first() {
        use sirio_chat_vocab::McpToolItem::Called;
        let mut fold = Fold::new();
        fold.set_mcp_apps(true);
        fold.apply(mcp_call_line("toolu_a", "mcp__apps__show"));
        fold.start_from_permission("mcp__apps__show", "toolu_a", &json!({"text": "hi"}));
        fold.start_from_permission("mcp__apps__show", "toolu_b", &json!({"text": "hi"}));
        fold.apply(mcp_call_line("toolu_b", "mcp__apps__show"));
        fold.apply(mcp_call_line("toolu_c", "Bash"));
        let ids: Vec<(String, bool)> = mcp_updates(&mut fold)
            .into_iter()
            .map(|update| (update.id, matches!(update.item, Called { .. })))
            .collect();
        assert_eq!(
            ids,
            [("toolu_a".to_string(), true), ("toolu_b".to_string(), true)]
        );
        let mut shut = Fold::new();
        shut.apply(mcp_call_line("toolu_a", "mcp__apps__show"));
        assert!(mcp_updates(&mut shut).is_empty(), "the gate is shut by default");
    }

    #[test]
    fn an_errored_mcp_result_says_so_without_a_structured_result() {
        use sirio_chat_vocab::McpToolItem::Returned;
        let mut fold = Fold::new();
        fold.set_mcp_apps(true);
        fold.apply(mcp_call_line("toolu_a", "mcp__apps__show"));
        fold.apply(mcp_result_line("toolu_a", true, "the server said no"));
        // A result nobody announced is not announced.
        fold.apply(mcp_result_line("toolu_unknown", false, "late"));
        let updates = mcp_updates(&mut fold);
        let [_, last] = updates.as_slice() else {
            panic!("one call and one result, got {updates:?}");
        };
        let Returned { result } = &last.item else {
            panic!("the result follows the call");
        };
        assert_eq!(result["isError"], true);
        assert_eq!(result["content"][0]["text"], "the server said no");
    }

    #[test]
    fn an_mcp_call_open_at_the_turns_end_is_cancelled_once() {
        use sirio_chat_vocab::McpToolItem::Cancelled;
        let mut fold = Fold::new();
        fold.set_mcp_apps(true);
        fold.apply(mcp_call_line("toolu_done", "mcp__apps__show"));
        fold.apply(mcp_call_line("toolu_open", "mcp__apps__show"));
        fold.apply(mcp_result_line("toolu_done", false, "ok"));
        fold.take_notices();
        let result = parse(json!({"type": "result", "subtype": "success", "is_error": false}));
        fold.apply(result.clone());
        let cancelled: Vec<String> = mcp_updates(&mut fold)
            .into_iter()
            .filter(|update| update.item == Cancelled)
            .map(|update| update.id)
            .collect();
        assert_eq!(cancelled, ["toolu_open"]);
        fold.apply(result);
        assert!(mcp_updates(&mut fold).is_empty(), "a call is cancelled once");
    }
}
