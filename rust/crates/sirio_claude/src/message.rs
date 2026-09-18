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
            "control_request" => Some(Self::ControlRequest(value.clone())),
            "control_response" => Some(Self::ControlResponse(value.clone())),
            _ => None,
        };
        Some(typed.unwrap_or(Self::Other { kind, subtype }))
    }
}

/// One MCP server's connection state, as `system/init` reports it.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct McpServerStatus {
    /// The server's name from the project's `.mcp.json`.
    pub name: String,
    /// The CLI's own word for its state, e.g. `connected` or `failed`.
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
    #[serde(default)]
    pub model: Option<String>,
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
            CliMessage::parse(r#"{"type":"rate_limit_event","kind":"five_hour","brand_new":1}"#),
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
    fn a_malformed_line_is_none_rather_than_a_panic() {
        assert!(CliMessage::parse("not json").is_none());
        assert!(CliMessage::parse("").is_none());
        assert!(CliMessage::parse("{}").is_none());
    }
}
