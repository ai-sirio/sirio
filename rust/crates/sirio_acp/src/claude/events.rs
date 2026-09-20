//! Protocol lines in, `AcpEvent`s out.
//!
//! Everything here is pure. The worker owns the process and the channels;
//! this owns the little state a stream needs — which tool calls are open,
//! what the session id turned out to be — so every case can be a unit test
//! with a JSON literal instead of a subprocess.

use std::collections::BTreeMap;
use std::path::PathBuf;

use sirio_claude::message::{CliMessage, Delta, ResultPayload, SystemPayload};
use sirio_claude::tools::{self, ToolContent, ToolInfo};

use crate::{AcpEvent, PlanEntryInfo, ToolCallContentInfo, ToolCallDiff, ToolCallLocationInfo};

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
    session_id: Option<String>,
    claude_version: Option<String>,
    current_mode: Option<String>,
    model: Option<String>,
    mcp_warnings: Vec<String>,
    last_result: Option<ResultSummary>,
}

impl Fold {
    pub(crate) fn new() -> Self {
        Self::default()
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

    /// What the last `result` reported.
    pub(crate) fn last_result(&self) -> Option<&ResultSummary> {
        self.last_result.as_ref()
    }

    /// Folds one line.
    pub(crate) fn apply(&mut self, message: CliMessage) -> Vec<AcpEvent> {
        match message {
            CliMessage::StreamEvent(event) => {
                // A subagent's deltas carry a parent id. They are not the
                // main thread's reply and must not be typed into it.
                if event.parent_tool_use_id.is_some() {
                    return Vec::new();
                }
                match event.delta() {
                    Some(Delta::Text(text)) => vec![AcpEvent::AgentMessageChunk(text)],
                    Some(Delta::Thinking(text)) => vec![AcpEvent::ThoughtChunk(text)],
                    None => Vec::new(),
                }
            }
            CliMessage::Assistant(assistant) => {
                if assistant.parent_tool_use_id.is_some() {
                    return Vec::new();
                }
                if let Some(model) = assistant.message.model.clone() {
                    self.model = Some(model);
                }
                let mut events = Vec::new();
                for tool_use in assistant.tool_uses() {
                    // A permission may have arrived first and already drawn
                    // this row; drawing it again would double the call.
                    if self.open_calls.contains_key(&tool_use.id) {
                        continue;
                    }
                    let info = tools::describe(tool_use);
                    events.push(started_event(&tool_use.id, &info, "InProgress"));
                    if let Some(entries) = plan_entries(&tool_use.name, &tool_use.input) {
                        events.push(AcpEvent::PlanUpdate { entries });
                    }
                    self.open_calls.insert(
                        tool_use.id.clone(),
                        OpenCall {
                            name: tool_use.name.clone(),
                            info,
                        },
                    );
                }
                events
            }
            CliMessage::User(user) => {
                if user.parent_tool_use_id.is_some() {
                    return Vec::new();
                }
                let mut events = Vec::new();
                for result in user.tool_results() {
                    let Some(open) = self.open_calls.remove(&result.tool_use_id) else {
                        // A result for a call nobody opened: report the
                        // traffic rather than inventing a row for it.
                        events.push(AcpEvent::OtherSessionUpdate {
                            kind: "tool_result(unmatched)".into(),
                        });
                        continue;
                    };
                    let content = completion_content(&open, &result, user.tool_use_result.as_ref());
                    events.push(AcpEvent::ToolCallCompleted {
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
                events
            }
            CliMessage::System(system) => self.apply_system(system),
            CliMessage::Result(result) => self.apply_result(result),
            // Control traffic is the worker's business, not the fold's.
            CliMessage::ControlRequest(_) | CliMessage::ControlResponse(_) => Vec::new(),
            CliMessage::Other { kind, subtype } => {
                vec![AcpEvent::OtherSessionUpdate {
                    kind: match subtype {
                        Some(subtype) => format!("{kind}/{subtype}"),
                        None => kind,
                    },
                }]
            }
        }
    }

    /// Marks an open call as waiting on the user's decision.
    pub(crate) fn mark_pending(&mut self, tool_use_id: &str) -> Vec<AcpEvent> {
        self.status_update(tool_use_id, "Pending")
    }

    /// Marks it running again once the decision was "allow".
    pub(crate) fn mark_running(&mut self, tool_use_id: &str) -> Vec<AcpEvent> {
        self.status_update(tool_use_id, "InProgress")
    }

    fn status_update(&mut self, tool_use_id: &str, status: &str) -> Vec<AcpEvent> {
        if !self.open_calls.contains_key(tool_use_id) {
            return Vec::new();
        }
        vec![AcpEvent::ToolCallUpdated {
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
    ) -> Vec<AcpEvent> {
        if self.open_calls.contains_key(tool_use_id) {
            return self.mark_pending(tool_use_id);
        }
        let tool_use = sirio_claude::message::ToolUse {
            id: tool_use_id.to_string(),
            name: tool_name.to_string(),
            input: input.clone(),
        };
        let info = tools::describe(&tool_use);
        let event = started_event(tool_use_id, &info, "Pending");
        self.open_calls.insert(
            tool_use_id.to_string(),
            OpenCall {
                name: tool_name.to_string(),
                info,
            },
        );
        vec![event]
    }

    fn apply_system(&mut self, system: SystemPayload) -> Vec<AcpEvent> {
        let mut events = Vec::new();
        if let Some(mode) = system.permission_mode.clone() {
            let changed = self.current_mode.as_deref() != Some(mode.as_str());
            self.current_mode = Some(mode.clone());
            if changed {
                events.push(AcpEvent::OtherSessionUpdate {
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
                        // mode change is: `AcpEvent` must not grow a variant
                        // that every consumer in the workspace would have to
                        // match.
                        events.push(AcpEvent::OtherSessionUpdate {
                            kind: "SessionIdentified".into(),
                        });
                    }
                }
                // MCP connection failures are intentionally not surfaced: the
                // native session loads the user's own `.mcp.json`/plugins and
                // a broken server would otherwise banner every chat.
            }
            "status" => {}
            other => events.push(AcpEvent::OtherSessionUpdate {
                kind: format!("system/{other}"),
            }),
        }
        events
    }

    fn apply_result(&mut self, result: ResultPayload) -> Vec<AcpEvent> {
        let mut events = Vec::new();
        if result.is_error
            && let Some(text) = result.result.clone().filter(|text| !text.is_empty())
        {
            // The user must see why a turn failed. The alternative — a bare
            // footer saying "ended" — is the exact silent failure this
            // transcript is responsible for not producing.
            events.push(AcpEvent::AgentMessageChunk(text));
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
            events.push(AcpEvent::TokenUsageBreakdown {
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
        events.push(AcpEvent::TurnEnded { stop_reason });
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

fn started_event(id: &str, info: &ToolInfo, status: &str) -> AcpEvent {
    AcpEvent::ToolCallStarted {
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
    fn text_and_thinking_deltas_stream_through() {
        let mut fold = Fold::new();
        assert_eq!(
            fold.apply(parse(json!({
                "type": "stream_event",
                "event": {"type": "content_block_delta",
                          "delta": {"type": "text_delta", "text": "Hel"}}
            }))),
            vec![AcpEvent::AgentMessageChunk("Hel".into())]
        );
        assert_eq!(
            fold.apply(parse(json!({
                "type": "stream_event",
                "event": {"type": "content_block_delta",
                          "delta": {"type": "thinking_delta", "thinking": "weighing"}}
            }))),
            vec![AcpEvent::ThoughtChunk("weighing".into())]
        );
    }

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
        let AcpEvent::ToolCallStarted {
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
            [AcpEvent::ToolCallUpdated { id, status: Some(status), .. }]
                if id == "toolu_1" && status == "Pending"
        ));
        let running = fold.mark_running("toolu_1");
        assert!(matches!(
            running.as_slice(),
            [AcpEvent::ToolCallUpdated { id, status: Some(status), .. }]
                if id == "toolu_1" && status == "InProgress"
        ));
    }

    #[test]
    fn a_permission_that_arrives_before_its_block_starts_the_call_itself() {
        let mut fold = Fold::new();
        let started = fold.start_from_permission("Bash", "toolu_9", &json!({"command": "ls"}));
        assert!(matches!(
            started.as_slice(),
            [AcpEvent::ToolCallStarted { id, status, .. }]
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
            AcpEvent::ToolCallCompleted {
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
            AcpEvent::ToolCallCompleted {
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
            AcpEvent::PlanUpdate { entries } => Some(entries),
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
                AcpEvent::TokenUsageBreakdown {
                    input_tokens: 1200,
                    output_tokens: 340,
                    cached_read_tokens: Some(8000),
                },
                AcpEvent::TurnEnded {
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
    fn a_cancelled_turn_uses_the_word_the_transcript_footer_knows() {
        // `sirio_ui::chat::turn_end_label` matches "Cancelled" exactly; any
        // other spelling silently renders as a plain "ended".
        let mut fold = Fold::new();
        let events = fold.apply(parse(json!({
            "type": "result", "subtype": "error_during_execution", "is_error": true,
            "result": "Interrupted by user", "stop_reason": "cancelled"
        })));
        assert!(events.contains(&AcpEvent::TurnEnded {
            stop_reason: "Cancelled".into()
        }));
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
            AcpEvent::AgentMessageChunk("Reached the turn limit".into())
        );
        assert!(matches!(events.last(), Some(AcpEvent::TurnEnded { .. })));
    }

    #[test]
    fn system_init_records_the_session_the_version_and_ignores_failed_mcp_servers() {
        let mut fold = Fold::new();
        let events = fold.apply(parse(json!({
            "type": "system", "subtype": "init",
            "session_id": "sess-1", "claude_code_version": "2.1.273",
            "model": "claude-fable-5-1", "permissionMode": "default",
            "mcp_servers": [{"name": "linear", "status": "failed"},
                            {"name": "github", "status": "connected"}]
        })));
        assert_eq!(fold.session_id(), Some("sess-1"));
        assert_eq!(fold.claude_version(), Some("2.1.273"));
        assert_eq!(fold.current_mode(), Some("default"));
        assert!(events.contains(&AcpEvent::OtherSessionUpdate {
            kind: "SessionIdentified".into()
        }));
        // Failed MCP servers must not banner the chat on the native path.
        assert!(fold.take_mcp_warnings().is_empty());
    }

    #[test]
    fn a_status_line_moves_the_mode_and_signals_a_re_read() {
        let mut fold = Fold::new();
        let events = fold.apply(parse(json!({
            "type": "system", "subtype": "status", "permissionMode": "plan"
        })));
        assert_eq!(fold.current_mode(), Some("plan"));
        assert_eq!(
            events,
            vec![AcpEvent::OtherSessionUpdate {
                kind: "CurrentModeUpdate(plan)".into()
            }]
        );
    }

    #[test]
    fn traffic_this_build_has_no_model_for_is_named_not_dropped() {
        let mut fold = Fold::new();
        for (line, expected) in [
            (
                json!({"type": "tool_progress", "tool_use_id": "t"}),
                "tool_progress",
            ),
            (json!({"type": "rate_limit_event"}), "rate_limit_event"),
            (
                json!({"type": "system", "subtype": "compact_boundary"}),
                "system/compact_boundary",
            ),
        ] {
            let events = fold.apply(parse(line));
            assert_eq!(
                events,
                vec![AcpEvent::OtherSessionUpdate {
                    kind: expected.into()
                }]
            );
        }
    }

    #[test]
    fn a_subagents_own_output_stays_out_of_the_top_level_transcript() {
        // Without --forward-subagent-text the CLI does not send subagent
        // text at all; if a future CLI does, it must not be mistaken for the
        // main thread's reply. Parity with what the wrapper-backed tab shows.
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
}
