//! The messages Claude Code writes on stdout in `--output-format stream-json`.
//!
//! Every type here is deliberately permissive. The stream-json control
//! channel is the Agent SDK's internal contract, versioned with the SDK and
//! not published as a stable public API, so a `claude` that self-updated an
//! hour ago may add a field or a subtype at any time. Nothing in this module
//! rejects a line for carrying something it does not recognise: unknown
//! `type`s become [`CliMessage::Other`], unknown fields are ignored, and a
//! line that is not an object at all yields `None` for the caller to skip.

use std::collections::BTreeMap;

use serde::Deserialize;

/// One line of Claude Code's stream-json output.
#[derive(Clone, Debug, PartialEq)]
pub enum CliMessage {
    /// `system` — `init` at the head of a turn, `status` after a mode
    /// change, `commands_changed`, `compact_boundary`, and others.
    System(SystemPayload),
    /// A Messages-API assistant message. With partial messages enabled its
    /// text also arrives as deltas, so callers read only its tool uses.
    Assistant(AssistantPayload),
    /// A user message the CLI echoes back — in practice the tool results it
    /// feeds to itself, which is where a tool call's structured output is.
    User(UserPayload),
    /// A raw Messages-API streaming event (`--include-partial-messages`).
    StreamEvent(StreamEventPayload),
    /// The end of a turn.
    Result(ResultPayload),
    /// The plan's rate-limit windows, pushed once per turn.
    RateLimit(RateLimitPush),
    /// A request the CLI makes of this client, e.g. `can_use_tool`.
    ControlRequest(serde_json::Value),
    /// The answer to a request this client made.
    ControlResponse(serde_json::Value),
    /// A line this build has no model for. Named, never dropped silently.
    Other {
        /// The wire `type`.
        kind: String,
        /// The wire `subtype`, when the line carried one.
        subtype: Option<String>,
    },
}

impl CliMessage {
    /// Parses one stream-json line. `None` for anything that is not a JSON
    /// object with a string `type` — the caller logs and skips.
    #[must_use]
    pub fn parse(line: &str) -> Option<Self> {
        let value: serde_json::Value = serde_json::from_str(line.trim()).ok()?;
        let kind = value.get("type")?.as_str()?.to_string();
        let subtype = value
            .get("subtype")
            .and_then(|subtype| subtype.as_str())
            .map(str::to_string);
        // A payload that fails to deserialize degrades to `Other` rather
        // than to `None`: the line was well-formed and named a type this
        // build knows, so the honest report is "could not model it", not
        // "it was not a message".
        let typed = match kind.as_str() {
            "system" => serde_json::from_value(value.clone()).ok().map(Self::System),
            "assistant" => serde_json::from_value(value.clone())
                .ok()
                .map(Self::Assistant),
            "user" => serde_json::from_value(value.clone()).ok().map(Self::User),
            "stream_event" => serde_json::from_value(value.clone())
                .ok()
                .map(Self::StreamEvent),
            "result" => serde_json::from_value(value.clone()).ok().map(Self::Result),
            "rate_limit_event" => RateLimitPush::parse(&value).map(Self::RateLimit),
            "control_request" => Some(Self::ControlRequest(value.clone())),
            "control_response" => Some(Self::ControlResponse(value.clone())),
            _ => None,
        };
        Some(typed.unwrap_or(Self::Other { kind, subtype }))
    }
}

/// One MCP server's connection state, as `system/init` reports it.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct McpServerStatus {
    /// The server's name from the project's `.mcp.json`.
    pub name: String,
    /// The CLI's own word for its state, e.g. `connected` or `failed`;
    /// empty when the entry carried none.
    pub status: String,
}

/// A `system` line. Every field beyond `subtype` is optional because the
/// subtypes share one wire type: `status` carries a permission mode and
/// nothing else, `init` carries the lot.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct SystemPayload {
    /// `init`, `status`, `commands_changed`, `compact_boundary`, …
    pub subtype: String,
    pub session_id: Option<String>,
    pub claude_code_version: Option<String>,
    pub cwd: Option<String>,
    pub model: Option<String>,
    #[serde(rename = "permissionMode")]
    pub permission_mode: Option<String>,
    /// Where an API key came from. **Not** a login check: a logged-in OAuth
    /// session reports `"none"` here too. Authentication is decided from the
    /// `initialize` response's `account.tokenSource` (see `catalog`).
    #[serde(rename = "apiKeySource")]
    pub api_key_source: Option<String>,
    pub tools: Vec<String>,
    pub mcp_servers: Vec<McpServerStatus>,
    pub slash_commands: Vec<String>,
    /// Commands whose UX is bound to a terminal; a desktop chat hides them.
    pub terminal_slash_commands: Vec<String>,
    /// Present on `commands_changed`.
    pub commands: Option<serde_json::Value>,
    /// Present on `compact_boundary`.
    pub compact_metadata: Option<CompactMetadata>,
    /// Present on `background_tasks_changed`: the whole list every time,
    /// a replacement rather than a delta. It drains as each task ends —
    /// and it drains *before* the matching `task_notification` arrives, so
    /// a surface driven by this alone reports a task vanishing, never a
    /// task finishing.
    pub tasks: Option<Vec<BackgroundTask>>,
    /// Present on `task_started`, `task_updated` and `task_notification`.
    pub task_id: Option<String>,
    /// Present on `task_started` and `background_tasks_changed`'s entries.
    pub description: Option<String>,
    /// Present on `task_started` **only**. Foreground `Bash` calls raise
    /// the same task messages as backgrounded ones, and this is the single
    /// field that tells them apart — a `task_notification` does not repeat
    /// it, so a caller that wants only background tasks has to remember
    /// the start keyed by `task_id`.
    pub is_backgrounded: Option<bool>,
    /// Present on `task_notification`, e.g. `completed`.
    pub status: Option<String>,
    /// Present on `task_notification`: the CLI's own sentence about the
    /// task, already written for a reader.
    pub summary: Option<String>,
    /// Present on `task_started`, `task_progress` and `task_notification`:
    /// the `tool_use` block that started the task. A subagent's is its
    /// `Agent` call; a foreground `Bash` call names its own.
    #[serde(deserialize_with = "lenient")]
    pub tool_use_id: Option<String>,
    /// Present on `task_started` and `task_progress` for a subagent: its
    /// type, e.g. `general-purpose`.
    #[serde(deserialize_with = "lenient")]
    pub subagent_type: Option<String>,
    /// Present on `task_started`: `local_agent` for a subagent,
    /// `local_bash` for a shell command.
    #[serde(deserialize_with = "lenient")]
    pub task_type: Option<String>,
    /// Present on `task_progress`: the tool the task ran last.
    #[serde(deserialize_with = "lenient")]
    pub last_tool_name: Option<String>,
    /// Present on `task_started`: how deep the subagent sits, 1 for one the
    /// main thread started.
    #[serde(deserialize_with = "lenient")]
    pub spawn_depth: Option<u32>,
    /// Present on `task_progress` and `task_notification`: the task's
    /// running totals.
    #[serde(deserialize_with = "lenient")]
    pub usage: Option<TaskUsage>,
}

/// A task's running totals, from a `task_progress` or `task_notification`.
/// Each is optional: a CLI that reports fewer must not cost the line.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct TaskUsage {
    #[serde(deserialize_with = "lenient")]
    pub total_tokens: Option<u64>,
    #[serde(deserialize_with = "lenient")]
    pub tool_uses: Option<u32>,
    #[serde(deserialize_with = "lenient")]
    pub duration_ms: Option<u64>,
}

/// Reads a field the trace wants without letting it veto the line.
///
/// These fields sit inside payloads the transcript depends on, so a `null`
/// or a word where a number belongs is read as absent. The alternative is
/// the whole `assistant` line degrading to [`CliMessage::Other`] and its
/// tool rows vanishing from the transcript.
fn lenient<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::de::DeserializeOwned,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    Ok(serde_json::from_value(value).ok())
}

/// A token count, read as zero when the CLI sent something else.
fn lenient_count<'de, D>(deserializer: D) -> Result<u64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    Ok(value.as_u64().unwrap_or(0))
}

/// What a `compact_boundary` says about the compaction it announces.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct CompactMetadata {
    /// `manual` when the user asked for it, `auto` when the context
    /// window forced it. The distinction is the whole point of showing
    /// the boundary: an automatic compaction is something that happened
    /// *to* the conversation.
    pub trigger: Option<String>,
    /// Context size in tokens before the compaction, and after it.
    pub pre_tokens: Option<u64>,
    pub post_tokens: Option<u64>,
}

/// One row of `background_tasks_changed`'s list.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct BackgroundTask {
    pub task_id: String,
    /// The CLI's own word for what runs it, e.g. `local_bash`.
    pub task_type: Option<String>,
    pub description: Option<String>,
}

/// A `rate_limit_event`: the plan's windows, pushed mid-session.
///
/// These are the same numbers [`crate::PlanUsage`] pulls from `get_usage`,
/// in a different shape **and a different scale**. Both were captured from
/// one session seconds apart: the pull reported `five_hour.utilization: 54`
/// where the push reported `0.53`, and the pull's ISO 8601 `resets_at`
/// matched the push's epoch `resetsAt` to the second. The rescale happens
/// here so that only one convention — percent — ever leaves this crate.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RateLimitPush {
    /// `allowed` while the plan has room. Anything else is the CLI
    /// reporting that it does not, and is the only field worth a warning.
    pub status: Option<String>,
    /// Which window `status` is about, e.g. `five_hour`.
    pub rate_limit_type: Option<String>,
    /// When that window resets, in unix seconds. This crate carries no
    /// date library by design, so the epoch stays an integer and the
    /// caller formats it.
    pub resets_at: Option<i64>,
    /// Five-hour window utilization on the **0–100 scale**.
    pub five_hour: Option<f64>,
    /// Seven-day window utilization on the **0–100 scale**.
    pub seven_day: Option<f64>,
}

impl RateLimitPush {
    /// Reads a `rate_limit_event` line. `None` only when it carries no
    /// `rate_limit_info` at all — every field inside it is optional, so a
    /// push that reports less than this one still parses.
    fn parse(value: &serde_json::Value) -> Option<Self> {
        let info = value.get("rate_limit_info")?;
        let window = |name: &str| {
            info.get("unifiedWindows")?
                .get(name)?
                .get("utilization")?
                .as_f64()
                .map(to_percent)
        };
        Some(Self {
            status: string(info, "status"),
            rate_limit_type: string(info, "rateLimitType"),
            resets_at: info.get("resetsAt").and_then(serde_json::Value::as_i64),
            five_hour: window("five_hour"),
            seven_day: window("seven_day"),
        })
    }
}

/// The push sends a fraction where the pull sends a percent.
fn to_percent(fraction: f64) -> f64 {
    fraction * 100.0
}

fn string(value: &serde_json::Value, key: &str) -> Option<String> {
    value.get(key).and_then(|v| v.as_str()).map(str::to_string)
}

/// One content block of an assistant message.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum ContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "thinking")]
    Thinking { thinking: String },
    #[serde(rename = "tool_use")]
    ToolUse(ToolUse),
    /// Any other block kind, including ones added after this build.
    #[serde(other)]
    Other,
}

/// A `tool_use` block: the call itself, before any result.
#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct ToolUse {
    /// The id every later update and result for this call carries.
    pub id: String,
    /// The tool's own name, e.g. `Read`, `Bash`, `mcp__linear__create_issue`.
    pub name: String,
    /// The tool's arguments, shaped per tool.
    #[serde(default)]
    pub input: serde_json::Value,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct AssistantPayload {
    pub message: AssistantMessage,
    /// Set when this message belongs to a subagent's own turn.
    #[serde(default)]
    pub parent_tool_use_id: Option<String>,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub uuid: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct AssistantMessage {
    /// The Messages-API message id. Several `assistant` lines can share
    /// one: each carries a block or two of the same model call.
    #[serde(default, deserialize_with = "lenient")]
    pub id: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    /// The call's usage as of this line.
    #[serde(default, deserialize_with = "lenient")]
    pub usage: Option<MessageUsage>,
    #[serde(default)]
    pub content: Vec<ContentBlock>,
    #[serde(default)]
    pub stop_reason: Option<String>,
}

impl AssistantPayload {
    /// The tool calls this message starts. Text and thinking are
    /// deliberately not exposed: with `--include-partial-messages` they
    /// already arrived as deltas, and reading them here would print every
    /// reply twice.
    pub fn tool_uses(&self) -> impl Iterator<Item = &ToolUse> {
        self.message.content.iter().filter_map(|block| match block {
            ContentBlock::ToolUse(tool_use) => Some(tool_use),
            _ => None,
        })
    }
}

/// A `user` line: in practice the tool results the CLI feeds back to itself.
#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct UserPayload {
    pub message: UserMessage,
    #[serde(default)]
    pub parent_tool_use_id: Option<String>,
    /// The tool's full structured output, keyed by the tool's own shape.
    /// This is where an `Edit`'s `originalFile` and `structuredPatch` live.
    #[serde(default)]
    pub tool_use_result: Option<serde_json::Value>,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub uuid: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct UserMessage {
    #[serde(default)]
    pub content: serde_json::Value,
}

/// One `tool_result` block found inside a user message.
#[derive(Clone, Debug, PartialEq)]
pub struct ToolResult {
    /// The `tool_use` id this result answers.
    pub tool_use_id: String,
    /// Whether the tool reported failure.
    pub is_error: bool,
    /// The result's text content, flattened; empty when it carried none.
    pub text: String,
}

impl UserPayload {
    /// The tool results carried by this message, if any.
    #[must_use]
    pub fn tool_results(&self) -> Vec<ToolResult> {
        let Some(blocks) = self.message.content.as_array() else {
            return Vec::new();
        };
        blocks
            .iter()
            .filter(|block| block.get("type").and_then(|t| t.as_str()) == Some("tool_result"))
            .filter_map(|block| {
                Some(ToolResult {
                    tool_use_id: block.get("tool_use_id")?.as_str()?.to_string(),
                    is_error: block
                        .get("is_error")
                        .and_then(serde_json::Value::as_bool)
                        .unwrap_or(false),
                    text: flatten_result_text(block.get("content")),
                })
            })
            .collect()
    }
}

/// A tool result's content is either a plain string or a block array; both
/// shapes appear in practice, so both flatten to the same text.
fn flatten_result_text(content: Option<&serde_json::Value>) -> String {
    match content {
        Some(serde_json::Value::String(text)) => text.clone(),
        Some(serde_json::Value::Array(blocks)) => blocks
            .iter()
            .filter_map(|block| block.get("text").and_then(|text| text.as_str()))
            .collect::<Vec<_>>()
            .join(""),
        _ => String::new(),
    }
}

/// A raw Messages-API streaming event.
#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct StreamEventPayload {
    pub event: serde_json::Value,
    #[serde(default)]
    pub parent_tool_use_id: Option<String>,
    #[serde(default)]
    pub session_id: Option<String>,
}

/// A text or thinking delta lifted out of a `stream_event`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Delta {
    /// Assistant reply text.
    Text(String),
    /// Internal reasoning.
    Thinking(String),
}

impl StreamEventPayload {
    /// The delta this event carries, when it carries one. Every other event
    /// kind (`message_start`, `content_block_stop`, `input_json_delta`, …)
    /// yields `None`; the caller treats that as protocol traffic to observe,
    /// not to render.
    #[must_use]
    pub fn delta(&self) -> Option<Delta> {
        if self.event.get("type").and_then(|kind| kind.as_str())? != "content_block_delta" {
            return None;
        }
        let delta = self.event.get("delta")?;
        match delta.get("type").and_then(|kind| kind.as_str())? {
            "text_delta" => Some(Delta::Text(delta.get("text")?.as_str()?.to_string())),
            "thinking_delta" => Some(Delta::Thinking(
                delta.get("thinking")?.as_str()?.to_string(),
            )),
            _ => None,
        }
    }

    /// Where this event sits in a model call's life, when it is one of the
    /// three that frame it. Deltas and block events yield `None`.
    ///
    /// `message_delta` and `message_stop` name no message: the call they end
    /// is the one the last `message_start` opened, which is the caller's to
    /// remember. A `message_start` that names no id yields `None`.
    #[must_use]
    pub fn lifecycle(&self) -> Option<MessageLifecycle> {
        let usage_of = |value: Option<&serde_json::Value>| {
            value
                .and_then(|usage| serde_json::from_value(usage.clone()).ok())
                .unwrap_or_default()
        };
        match self.event.get("type")?.as_str()? {
            "message_start" => {
                let message = self.event.get("message")?;
                Some(MessageLifecycle::Start {
                    id: string(message, "id")?,
                    model: string(message, "model"),
                    usage: usage_of(message.get("usage")),
                })
            }
            "message_delta" => Some(MessageLifecycle::Delta {
                usage: usage_of(self.event.get("usage")),
            }),
            "message_stop" => Some(MessageLifecycle::Stop),
            _ => None,
        }
    }
}

/// The three `stream_event`s that frame one model call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MessageLifecycle {
    /// `message_start`: the call begins.
    Start {
        id: String,
        model: Option<String>,
        /// The input side, known before any output: the tokens read, cached
        /// and newly cached.
        usage: MessageUsage,
    },
    /// `message_delta`: the call's final usage, just before it stops.
    Delta { usage: MessageUsage },
    /// `message_stop`: the call is over.
    Stop,
}

/// One model call's token usage, as the Messages API reports it. A field the
/// line lacks, or carries as anything but a count, is zero.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct MessageUsage {
    #[serde(deserialize_with = "lenient_count")]
    pub input_tokens: u64,
    #[serde(deserialize_with = "lenient_count")]
    pub output_tokens: u64,
    #[serde(deserialize_with = "lenient_count")]
    pub cache_read_input_tokens: u64,
    #[serde(deserialize_with = "lenient_count")]
    pub cache_creation_input_tokens: u64,
}

/// What an `Agent` tool call's result says about the subagent that ran it,
/// read from the user line's `tool_use_result`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AgentToolResult {
    /// `completed` for an agent that ran to its report.
    pub status: Option<String>,
    pub agent_type: Option<String>,
    pub resolved_model: Option<String>,
    /// The CLI's own clock for the subagent, which the trace prefers to
    /// anything it measured itself.
    pub total_duration_ms: Option<u64>,
    pub total_tokens: Option<u64>,
    /// The usage of the subagent's last call, its report.
    pub usage: Option<MessageUsage>,
}

impl AgentToolResult {
    /// Reads a `tool_use_result`. `None` for anything that is not an object
    /// (a denied call reports a bare string); every field of an object that
    /// is one is optional.
    #[must_use]
    pub fn parse(value: &serde_json::Value) -> Option<Self> {
        if !value.is_object() {
            return None;
        }
        Some(Self {
            status: string(value, "status"),
            agent_type: string(value, "agentType"),
            resolved_model: string(value, "resolvedModel"),
            total_duration_ms: value
                .get("totalDurationMs")
                .and_then(serde_json::Value::as_u64),
            total_tokens: value.get("totalTokens").and_then(serde_json::Value::as_u64),
            usage: value
                .get("usage")
                .filter(|usage| usage.is_object())
                .and_then(|usage| serde_json::from_value(usage.clone()).ok()),
        })
    }
}

/// Cumulative usage for one model over the session.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct ModelUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_input_tokens: u64,
    pub cache_creation_input_tokens: u64,
    pub cost_usd: f64,
    /// The model's context window. `None` on a CLI that does not report it,
    /// which is why the context meter has a fallback at all.
    pub context_window: Option<u64>,
}

/// The end of a turn.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct ResultPayload {
    /// `success`, `error_during_execution`, `error_max_turns`, …
    pub subtype: String,
    pub is_error: bool,
    /// The turn's final text, or the error sentence when `is_error`.
    pub result: Option<String>,
    pub stop_reason: Option<String>,
    /// The CLI's own clock for the turn.
    #[serde(deserialize_with = "lenient")]
    pub duration_ms: Option<u64>,
    pub total_cost_usd: Option<f64>,
    #[serde(rename = "modelUsage")]
    pub model_usage: BTreeMap<String, ModelUsage>,
    pub session_id: Option<String>,
    pub uuid: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    const INIT: &str = include_str!("../tests/fixtures/init.json");
    const ASSISTANT_TOOL_USE: &str = include_str!("../tests/fixtures/assistant_tool_use.json");
    const RESULT_SUCCESS: &str = include_str!("../tests/fixtures/result_success.json");
    const COMPACT_BOUNDARY: &str = include_str!("../tests/fixtures/compact_boundary.json");
    const RATE_LIMIT_EVENT: &str = include_str!("../tests/fixtures/rate_limit_event.json");
    const TASK_STARTED_BACKGROUND: &str =
        include_str!("../tests/fixtures/task_started_background.json");
    const TASK_STARTED_FOREGROUND: &str =
        include_str!("../tests/fixtures/task_started_foreground.json");
    const TASK_NOTIFICATION: &str = include_str!("../tests/fixtures/task_notification.json");
    const BACKGROUND_TASKS_CHANGED: &str =
        include_str!("../tests/fixtures/background_tasks_changed.json");
    const MESSAGE_START: &str = include_str!("../tests/fixtures/trace_message_start.json");
    const MESSAGE_DELTA: &str = include_str!("../tests/fixtures/trace_message_delta.json");
    const TASK_STARTED: &str = include_str!("../tests/fixtures/trace_task_started.json");
    const TASK_PROGRESS: &str = include_str!("../tests/fixtures/trace_task_progress.json");
    const SUBAGENT_ASSISTANT: &str =
        include_str!("../tests/fixtures/trace_subagent_assistant.json");
    const AGENT_RESULT: &str = include_str!("../tests/fixtures/trace_agent_result.json");
    const RESULT_INTERRUPTED: &str =
        include_str!("../tests/fixtures/trace_result_interrupted.json");

    fn stream_event(line: &str) -> StreamEventPayload {
        let Some(CliMessage::StreamEvent(event)) = CliMessage::parse(line) else {
            panic!("the line should parse as a stream event: {line}");
        };
        event
    }

    #[test]
    fn system_init_carries_the_session_and_the_version() {
        let Some(CliMessage::System(system)) = CliMessage::parse(INIT) else {
            panic!("init fixture should parse as a system message");
        };
        assert_eq!(system.subtype, "init");
        assert_eq!(
            system.session_id.as_deref(),
            Some("5bbcaeb8-e523-4087-b33c-559163f2dc07")
        );
        assert_eq!(system.claude_code_version.as_deref(), Some("2.1.273"));
        assert_eq!(system.model.as_deref(), Some("claude-fable-5-1"));
        assert_eq!(system.permission_mode.as_deref(), Some("default"));
        assert_eq!(
            system.mcp_servers,
            vec![McpServerStatus {
                name: "linear".into(),
                status: "failed".into()
            }]
        );
    }

    #[test]
    fn system_init_survives_an_mcp_entry_without_a_status() {
        let line = r#"{"type":"system","subtype":"init","session_id":"s","mcp_servers":[{"name":"x"}]}"#;
        let Some(CliMessage::System(system)) = CliMessage::parse(line) else {
            panic!("init with a statusless mcp entry should still parse as system");
        };
        assert_eq!(system.mcp_servers.len(), 1);
        assert_eq!(system.mcp_servers[0].name, "x");
        assert_eq!(system.mcp_servers[0].status, "");
    }

    #[test]
    fn an_assistant_message_exposes_only_its_tool_use_blocks_by_kind() {
        let Some(CliMessage::Assistant(assistant)) = CliMessage::parse(ASSISTANT_TOOL_USE) else {
            panic!("assistant fixture should parse as an assistant message");
        };
        let tool_uses: Vec<&ToolUse> = assistant.tool_uses().collect();
        assert_eq!(tool_uses.len(), 1);
        assert_eq!(tool_uses[0].id, "toolu_01ABC");
        assert_eq!(tool_uses[0].name, "Read");
        assert_eq!(
            tool_uses[0]
                .input
                .get("file_path")
                .and_then(|value| value.as_str()),
            Some("/repo/src/main.rs")
        );
        assert_eq!(assistant.parent_tool_use_id, None);
    }

    #[test]
    fn a_result_carries_cost_and_per_model_usage() {
        let Some(CliMessage::Result(result)) = CliMessage::parse(RESULT_SUCCESS) else {
            panic!("result fixture should parse as a result message");
        };
        assert_eq!(result.subtype, "success");
        assert!(!result.is_error);
        assert_eq!(result.total_cost_usd, Some(0.0421));
        let usage = result
            .model_usage
            .get("claude-fable-5-1")
            .expect("per-model usage");
        assert_eq!(usage.input_tokens, 1200);
        assert_eq!(usage.output_tokens, 340);
        assert_eq!(usage.cache_read_input_tokens, 8000);
        assert_eq!(usage.context_window, Some(200_000));
    }

    #[test]
    fn an_unknown_type_and_unknown_fields_never_fail() {
        assert!(matches!(
            CliMessage::parse(r#"{"type":"tool_progress","tool_use_id":"t","brand_new":1}"#),
            Some(CliMessage::Other { .. })
        ));
        // A field the CLI added after this build shipped must not reject the
        // whole line: the same message with an extra key still parses.
        let widened = INIT
            .trim_end()
            .replace("{\"type\"", "{\"invented_field\":true,\"type\"");
        assert!(matches!(
            CliMessage::parse(&widened),
            Some(CliMessage::System(_))
        ));
    }

    #[test]
    fn a_compaction_reports_what_it_dropped() {
        let Some(CliMessage::System(system)) = CliMessage::parse(COMPACT_BOUNDARY) else {
            panic!("the compact_boundary capture should parse as a system message");
        };
        assert_eq!(system.subtype, "compact_boundary");
        let compaction = system.compact_metadata.expect("compaction metadata");
        assert_eq!(compaction.trigger.as_deref(), Some("manual"));
        assert_eq!(compaction.pre_tokens, Some(43_134));
        assert_eq!(compaction.post_tokens, Some(11_574));
    }

    #[test]
    fn the_pushed_rate_limit_is_rescaled_to_the_percent_the_pull_speaks() {
        let Some(CliMessage::RateLimit(limit)) = CliMessage::parse(RATE_LIMIT_EVENT) else {
            panic!("the rate_limit_event capture should parse as a rate limit");
        };
        assert_eq!(limit.status.as_deref(), Some("allowed"));
        assert_eq!(limit.rate_limit_type.as_deref(), Some("five_hour"));
        assert_eq!(limit.resets_at, Some(1_789_990_800));
        // Captured seconds apart in one session, `get_usage` answered 54
        // and 10 for the windows this push reports as 0.53 and 0.1. Both
        // conventions reaching one program is how a half-full window ends
        // up drawn at half a percent.
        assert_eq!(limit.five_hour, Some(53.0));
        assert_eq!(limit.seven_day, Some(10.0));
    }

    #[test]
    fn a_task_says_whether_it_went_to_the_background() {
        let Some(CliMessage::System(background)) = CliMessage::parse(TASK_STARTED_BACKGROUND)
        else {
            panic!("the backgrounded task_started capture should parse");
        };
        assert_eq!(background.task_id.as_deref(), Some("bm0k5s6eo"));
        assert_eq!(background.is_backgrounded, Some(true));

        let Some(CliMessage::System(foreground)) = CliMessage::parse(TASK_STARTED_FOREGROUND)
        else {
            panic!("the foreground task_started capture should parse");
        };
        assert_eq!(foreground.is_backgrounded, Some(false));
    }

    #[test]
    fn a_finished_task_carries_the_clis_own_sentence() {
        let Some(CliMessage::System(system)) = CliMessage::parse(TASK_NOTIFICATION) else {
            panic!("the task_notification capture should parse as a system message");
        };
        assert_eq!(system.task_id.as_deref(), Some("bm0k5s6eo"));
        assert_eq!(system.status.as_deref(), Some("completed"));
        assert!(
            system
                .summary
                .as_deref()
                .is_some_and(|summary| summary.contains("exit code 0")),
            "the notification's own summary is what the row shows: {:?}",
            system.summary
        );
        // The discriminator lives on `task_started` alone; a notification
        // read on its own cannot tell a background task from a foreground
        // one, which is why the caller correlates by id.
        assert_eq!(system.is_backgrounded, None);
    }

    #[test]
    fn the_background_task_list_arrives_whole() {
        let Some(CliMessage::System(system)) = CliMessage::parse(BACKGROUND_TASKS_CHANGED) else {
            panic!("the background_tasks_changed capture should parse");
        };
        let tasks = system.tasks.expect("a task list");
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].task_id, "bm0k5s6eo");
        assert_eq!(
            tasks[0].description.as_deref(),
            Some("Sleep 25 seconds then echo FINISHED")
        );
    }

    #[test]
    fn a_malformed_line_is_none_rather_than_a_panic() {
        assert!(CliMessage::parse("not json").is_none());
        assert!(CliMessage::parse("").is_none());
        assert!(CliMessage::parse("{}").is_none());
    }

    #[test]
    fn a_message_start_names_the_call_its_model_and_input_usage() {
        let lifecycle = stream_event(MESSAGE_START).lifecycle();
        assert_eq!(
            lifecycle,
            Some(MessageLifecycle::Start {
                id: "msg_011CgG43GxW2BhRXx8chosrS".into(),
                model: Some("claude-haiku-5-5".into()),
                usage: MessageUsage {
                    input_tokens: 2,
                    output_tokens: 6,
                    cache_read_input_tokens: 0,
                    cache_creation_input_tokens: 39_276,
                },
            })
        );
    }

    #[test]
    fn a_message_delta_carries_the_final_usage() {
        // `message_delta` names no message: the call it ends is the one the
        // last `message_start` opened, which is the caller's to remember.
        assert_eq!(
            stream_event(MESSAGE_DELTA).lifecycle(),
            Some(MessageLifecycle::Delta {
                usage: MessageUsage {
                    input_tokens: 2,
                    output_tokens: 632,
                    cache_read_input_tokens: 0,
                    cache_creation_input_tokens: 39_276,
                },
            })
        );
    }

    #[test]
    fn a_message_stop_is_stop() {
        let line =
            r#"{"type":"stream_event","event":{"type":"message_stop"},"parent_tool_use_id":null}"#;
        assert_eq!(stream_event(line).lifecycle(), Some(MessageLifecycle::Stop));
    }

    #[test]
    fn a_text_delta_has_no_lifecycle() {
        let line = r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"ok"}},"parent_tool_use_id":null}"#;
        let event = stream_event(line);
        assert_eq!(event.lifecycle(), None);
        // The transcript still reads the same line as the text it carries.
        assert_eq!(event.delta(), Some(Delta::Text("ok".into())));
    }

    #[test]
    fn a_task_started_names_its_tool_call_and_depth() {
        let Some(CliMessage::System(system)) = CliMessage::parse(TASK_STARTED) else {
            panic!("the task_started capture should parse as a system message");
        };
        assert_eq!(
            system.tool_use_id.as_deref(),
            Some("toolu_01YNUiEeefJ32LABu5jnbfHM")
        );
        assert_eq!(system.task_id.as_deref(), Some("a893e1fc00e053191"));
        assert_eq!(system.spawn_depth, Some(1));
        assert_eq!(system.task_type.as_deref(), Some("local_agent"));
        assert_eq!(system.subagent_type.as_deref(), Some("general-purpose"));
        assert_eq!(
            system.description.as_deref(),
            Some("List the directory and read the README")
        );
    }

    #[test]
    fn a_task_progress_carries_its_running_totals() {
        let Some(CliMessage::System(system)) = CliMessage::parse(TASK_PROGRESS) else {
            panic!("the task_progress capture should parse as a system message");
        };
        assert_eq!(system.subtype, "task_progress");
        assert_eq!(
            system.tool_use_id.as_deref(),
            Some("toolu_01YNUiEeefJ32LABu5jnbfHM")
        );
        assert_eq!(system.last_tool_name.as_deref(), Some("Bash"));
        assert_eq!(
            system.description.as_deref(),
            Some("Running List files in the working directory")
        );
        assert_eq!(
            system.usage,
            Some(TaskUsage {
                total_tokens: Some(19_991),
                tool_uses: Some(1),
                duration_ms: Some(1_892),
            })
        );
    }

    #[test]
    fn a_subagent_assistant_line_has_its_id_and_usage() {
        let Some(CliMessage::Assistant(assistant)) = CliMessage::parse(SUBAGENT_ASSISTANT) else {
            panic!("the subagent assistant capture should parse as an assistant message");
        };
        assert_eq!(
            assistant.parent_tool_use_id.as_deref(),
            Some("toolu_01YNUiEeefJ32LABu5jnbfHM")
        );
        assert_eq!(
            assistant.message.id.as_deref(),
            Some("msg_011CgG43YcaswnMdLpDH97Lj")
        );
        assert_eq!(assistant.message.model.as_deref(), Some("claude-haiku-5-5"));
        assert_eq!(
            assistant.message.usage,
            Some(MessageUsage {
                input_tokens: 2,
                output_tokens: 8,
                cache_read_input_tokens: 0,
                cache_creation_input_tokens: 19_687,
            })
        );
        assert_eq!(assistant.tool_uses().count(), 1);
    }

    #[test]
    fn an_agent_result_carries_its_totals() {
        let Some(CliMessage::User(user)) = CliMessage::parse(AGENT_RESULT) else {
            panic!("the Agent tool_result capture should parse as a user message");
        };
        let result = user
            .tool_use_result
            .expect("the result's structured output");
        let agent = AgentToolResult::parse(&result).expect("an Agent result");
        assert_eq!(agent.status.as_deref(), Some("completed"));
        assert_eq!(agent.agent_type.as_deref(), Some("general-purpose"));
        assert_eq!(agent.resolved_model.as_deref(), Some("claude-haiku-5-5"));
        // The Agent result's own numbers, not the `task_notification`'s
        // (4535 ms, 26328 tokens), which come a few milliseconds earlier.
        assert_eq!(agent.total_duration_ms, Some(4_537));
        assert_eq!(agent.total_tokens, Some(26_197));
        assert_eq!(
            agent.usage,
            Some(MessageUsage {
                input_tokens: 2,
                output_tokens: 187,
                cache_read_input_tokens: 25_867,
                cache_creation_input_tokens: 141,
            })
        );
    }

    #[test]
    fn an_agent_result_missing_every_field_still_parses_to_nones() {
        let agent = AgentToolResult::parse(&serde_json::json!({})).expect("an empty object");
        assert_eq!(
            agent,
            AgentToolResult {
                status: None,
                agent_type: None,
                resolved_model: None,
                total_duration_ms: None,
                total_tokens: None,
                usage: None,
            }
        );
    }

    #[test]
    fn a_tool_result_that_is_not_an_object_is_not_an_agent_result() {
        // A denied call reports `"tool_use_result": "Error: …"`, a bare
        // string, as the second capture shows.
        assert_eq!(
            AgentToolResult::parse(&serde_json::json!("Error: not allowed by the probe")),
            None
        );
        assert_eq!(AgentToolResult::parse(&serde_json::Value::Null), None);
    }

    #[test]
    fn an_interrupted_result_reads_error_during_execution() {
        let Some(CliMessage::Result(result)) = CliMessage::parse(RESULT_INTERRUPTED) else {
            panic!("the interrupted result capture should parse as a result message");
        };
        assert_eq!(result.subtype, "error_during_execution");
        assert!(result.is_error);
        assert_eq!(result.stop_reason, None);
        assert_eq!(result.duration_ms, Some(6_002));
        assert_eq!(result.total_cost_usd, Some(0.008_954_355));
    }

    #[test]
    fn a_line_with_none_of_the_new_fields_parses_as_before() {
        // The fixtures that predate the trace carry some of the new keys
        // already (`msg_01`, `duration_ms`, a task's `tool_use_id`); the rest
        // must read as absent rather than reject the line.
        let Some(CliMessage::Assistant(assistant)) = CliMessage::parse(ASSISTANT_TOOL_USE) else {
            panic!("assistant fixture should parse as an assistant message");
        };
        assert_eq!(assistant.message.id.as_deref(), Some("msg_01"));
        assert_eq!(assistant.message.usage, None);
        assert_eq!(assistant.tool_uses().count(), 1);

        let Some(CliMessage::Result(result)) = CliMessage::parse(RESULT_SUCCESS) else {
            panic!("result fixture should parse as a result message");
        };
        assert_eq!(result.duration_ms, Some(4_210));
        assert_eq!(result.total_cost_usd, Some(0.0421));

        let Some(CliMessage::System(task)) = CliMessage::parse(TASK_STARTED_FOREGROUND) else {
            panic!("the foreground task_started capture should parse");
        };
        assert_eq!(task.task_type.as_deref(), Some("local_bash"));
        assert_eq!(
            task.tool_use_id.as_deref(),
            Some("toolu_011eUvQKr2fHYQbnd21omenv")
        );
        assert_eq!(task.subagent_type, None);
        assert_eq!(task.spawn_depth, None);
        assert_eq!(task.last_tool_name, None);
        assert_eq!(task.usage, None);

        let Some(CliMessage::System(init)) = CliMessage::parse(INIT) else {
            panic!("init fixture should parse as a system message");
        };
        assert_eq!(init.tool_use_id, None);
        assert_eq!(init.task_type, None);
        assert_eq!(init.usage, None);

        let bare = r#"{"type":"assistant","message":{"content":[]}}"#;
        let Some(CliMessage::Assistant(bare)) = CliMessage::parse(bare) else {
            panic!("a bare assistant line should parse as an assistant message");
        };
        assert_eq!(bare.message.id, None);
        assert_eq!(bare.message.usage, None);
    }

    #[test]
    fn a_malformed_trace_field_costs_the_line_nothing_else() {
        // These fields sit inside payloads the transcript depends on: a
        // `null` or a word where a number belongs must drop that one detail,
        // not turn the line into `Other` and take its tool rows with it.
        let assistant = r#"{"type":"assistant","message":{"id":5,"usage":{"input_tokens":null,"output_tokens":"many"},"content":[{"type":"tool_use","id":"toolu_1","name":"Read","input":{}}]},"parent_tool_use_id":null}"#;
        let Some(CliMessage::Assistant(assistant)) = CliMessage::parse(assistant) else {
            panic!("a malformed id and usage must not demote the assistant line");
        };
        assert_eq!(assistant.tool_uses().count(), 1);
        assert_eq!(assistant.message.id, None);
        assert_eq!(assistant.message.usage, Some(MessageUsage::default()));

        let task = r#"{"type":"system","subtype":"task_progress","spawn_depth":"deep","usage":"lots","last_tool_name":7,"task_id":"t"}"#;
        let Some(CliMessage::System(task)) = CliMessage::parse(task) else {
            panic!("a malformed task field must not demote the system line");
        };
        assert_eq!(task.subtype, "task_progress");
        assert_eq!(task.task_id.as_deref(), Some("t"));
        assert_eq!(task.spawn_depth, None);
        assert_eq!(task.usage, None);
        assert_eq!(task.last_tool_name, None);

        let result = r#"{"type":"result","subtype":"error_during_execution","is_error":true,"duration_ms":null}"#;
        let Some(CliMessage::Result(result)) = CliMessage::parse(result) else {
            panic!("a null duration must not demote the result line");
        };
        assert!(result.is_error);
        assert_eq!(result.duration_ms, None);
    }
}
