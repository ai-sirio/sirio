//! The control channel: what Sirio asks of the CLI, and the one thing the
//! CLI asks of Sirio.
//!
//! Requests are built as `serde_json::Value` rather than typed structs
//! because each subtype has its own payload shape and the set grows without
//! notice; a builder per verb keeps the wire spelling in one place while
//! leaving the envelope uniform.

use serde_json::{Value, json};

/// The requests Sirio sends to the CLI, one constructor per verb. Each
/// returns the complete line to write, including the envelope.
pub struct ControlRequest;

impl ControlRequest {
    fn envelope(request_id: &str, request: Value) -> Value {
        json!({"type": "control_request", "request_id": request_id, "request": request})
    }

    /// The handshake. `hooks` is an empty object: Sirio registers no
    /// in-process hooks, so the CLI keeps the ones the user's own settings
    /// files declare — the same hooks the terminal pane gets.
    #[must_use]
    pub fn initialize(request_id: &str) -> Value {
        Self::envelope(request_id, json!({"subtype": "initialize", "hooks": {}}))
    }

    /// Ends the turn in flight. The turn still reports its own `result`.
    #[must_use]
    pub fn interrupt(request_id: &str) -> Value {
        Self::envelope(request_id, json!({"subtype": "interrupt"}))
    }

    /// Switches the permission mode mid-session. The CLI confirms with a
    /// `system/status` line carrying the new mode.
    #[must_use]
    pub fn set_permission_mode(request_id: &str, mode: &str) -> Value {
        Self::envelope(
            request_id,
            json!({"subtype": "set_permission_mode", "mode": mode}),
        )
    }

    /// Switches the model. `"default"` resets to the session default.
    #[must_use]
    pub fn set_model(request_id: &str, model: &str) -> Value {
        Self::envelope(request_id, json!({"subtype": "set_model", "model": model}))
    }

    /// Sets the effort level. `None` clears the flag layer, which is how
    /// the wire spells "back to whatever the settings say".
    #[must_use]
    pub fn set_effort(request_id: &str, level: Option<&str>) -> Value {
        Self::envelope(
            request_id,
            json!({
                "subtype": "apply_flag_settings",
                "settings": {"effortLevel": level}
            }),
        )
    }

    /// Context-window usage. `summary` answers from the last response and
    /// local estimates; `full` would issue per-category token-count API
    /// calls, which is not worth a round trip after every turn.
    #[must_use]
    pub fn get_context_usage(request_id: &str) -> Value {
        Self::envelope(
            request_id,
            json!({"subtype": "get_context_usage", "detail": "summary"}),
        )
    }

    /// Session cost plus plan rate-limit windows — the structured form of
    /// what `/usage` renders.
    #[must_use]
    pub fn get_usage(request_id: &str) -> Value {
        Self::envelope(request_id, json!({"subtype": "get_usage"}))
    }

    /// Restores tracked files to their state at a user message. With
    /// `dry_run` the CLI reports what it would change and touches nothing.
    #[must_use]
    pub fn rewind_files(request_id: &str, user_message_id: &str, dry_run: bool) -> Value {
        Self::envelope(
            request_id,
            json!({
                "subtype": "rewind_files",
                "user_message_id": user_message_id,
                "dry_run": dry_run
            }),
        )
    }
}

/// A control response, reduced to what a waiting caller needs: which
/// request it answers, what it carried, and why it failed if it did.
#[derive(Clone, Debug, PartialEq)]
pub struct ControlEnvelope {
    /// The `request_id` of the request this answers.
    pub request_id: String,
    /// The response body on success.
    pub payload: Option<Value>,
    /// The CLI's own error text when the request was refused — including
    /// the refusal a CLI gives for a subtype it does not know, which is how
    /// a feature degrades instead of the session dying.
    pub error: Option<String>,
}

impl ControlEnvelope {
    /// Reads a `control_response` line. `None` when it is not one, or
    /// carries no request id to match it against.
    #[must_use]
    pub fn parse(line: &Value) -> Option<Self> {
        let response = line.get("response")?;
        let request_id = response.get("request_id")?.as_str()?.to_string();
        let error = response
            .get("error")
            .and_then(|error| error.as_str())
            .map(str::to_string)
            .or_else(|| {
                (response.get("subtype").and_then(|s| s.as_str()) == Some("error"))
                    .then(|| "the agent refused the request".to_string())
            });
        Some(Self {
            request_id,
            payload: response
                .get("response")
                .cloned()
                .filter(|_| error.is_none()),
            error,
        })
    }
}

/// The CLI asking whether a tool may run.
#[derive(Clone, Debug, PartialEq)]
pub struct CanUseTool {
    /// The id the answer must be sent under.
    pub request_id: String,
    /// The tool's own name.
    pub tool_name: String,
    /// The `tool_use` block this decision belongs to, when named.
    pub tool_use_id: Option<String>,
    /// The tool's arguments, echoed back verbatim in an `allow`.
    pub input: Value,
    /// Rules the CLI offers to remember. Non-empty is what makes an
    /// "Always allow" option honest: with nothing to remember, choosing it
    /// would be indistinguishable from allowing once.
    pub permission_suggestions: Vec<Value>,
    /// Why the CLI escalated, when it says. May carry ANSI escapes.
    pub decision_reason: Option<String>,
}

impl CanUseTool {
    /// Reads a `control_request` line, when it is a `can_use_tool`.
    #[must_use]
    pub fn parse(line: &Value) -> Option<Self> {
        let request = line.get("request")?;
        if request.get("subtype")?.as_str()? != "can_use_tool" {
            return None;
        }
        Some(Self {
            request_id: line.get("request_id")?.as_str()?.to_string(),
            tool_name: request.get("tool_name")?.as_str()?.to_string(),
            tool_use_id: request
                .get("tool_use_id")
                .and_then(|id| id.as_str())
                .map(str::to_string),
            input: request.get("input").cloned().unwrap_or(Value::Null),
            permission_suggestions: request
                .get("permission_suggestions")
                .and_then(|value| value.as_array())
                .cloned()
                .unwrap_or_default(),
            decision_reason: request
                .get("decision_reason")
                .and_then(|reason| reason.as_str())
                .map(str::to_string),
        })
    }
}

/// The answer to a [`CanUseTool`].
#[derive(Clone, Debug, PartialEq)]
pub enum PermissionResult {
    /// Run it, with this input.
    Allow {
        /// The input to run with — the request's own, unless a tool the
        /// client shapes (a question's answers) replaced it.
        updated_input: Value,
        /// Rules to remember, from the request's suggestions.
        updated_permissions: Vec<Value>,
    },
    /// Refuse, with a sentence the model reads.
    Deny {
        /// Why. This text reaches the model, not just the user.
        message: String,
    },
}

impl PermissionResult {
    /// Allow this call only.
    #[must_use]
    pub fn allow(updated_input: Value) -> Self {
        Self::Allow {
            updated_input,
            updated_permissions: Vec::new(),
        }
    }

    /// Allow, and remember the CLI's suggested rules.
    #[must_use]
    pub fn allow_with_permissions(updated_input: Value, updated_permissions: Vec<Value>) -> Self {
        Self::Allow {
            updated_input,
            updated_permissions,
        }
    }

    /// Refuse.
    #[must_use]
    pub fn deny(message: impl Into<String>) -> Self {
        Self::Deny {
            message: message.into(),
        }
    }

    /// The complete line to write, answering `request_id`.
    #[must_use]
    pub fn into_response(self, request_id: &str) -> Value {
        let body = match self {
            Self::Allow {
                updated_input,
                updated_permissions,
            } => {
                let mut body = json!({"behavior": "allow", "updatedInput": updated_input});
                if !updated_permissions.is_empty() {
                    body["updatedPermissions"] = Value::Array(updated_permissions);
                }
                body
            }
            Self::Deny { message } => json!({"behavior": "deny", "message": message}),
        };
        json!({
            "type": "control_response",
            "response": {"subtype": "success", "request_id": request_id, "response": body}
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CAN_USE_TOOL: &str = include_str!("../tests/fixtures/can_use_tool.json");

    #[test]
    fn initialize_is_the_first_line_and_carries_no_hooks() {
        let line = ControlRequest::initialize("req-1");
        assert_eq!(line["type"], "control_request");
        assert_eq!(line["request_id"], "req-1");
        assert_eq!(line["request"]["subtype"], "initialize");
        assert_eq!(line["request"]["hooks"], serde_json::json!({}));
    }

    #[test]
    fn each_command_request_names_its_subtype_and_payload() {
        assert_eq!(
            ControlRequest::set_permission_mode("req-2", "plan")["request"],
            serde_json::json!({"subtype": "set_permission_mode", "mode": "plan"})
        );
        assert_eq!(
            ControlRequest::set_model("req-3", "sonnet")["request"],
            serde_json::json!({"subtype": "set_model", "model": "sonnet"})
        );
        // "default" is how the wire says "reset to the session default".
        assert_eq!(
            ControlRequest::set_effort("req-4", None)["request"],
            serde_json::json!({"subtype": "apply_flag_settings", "settings": {"effortLevel": null}})
        );
        assert_eq!(
            ControlRequest::set_effort("req-5", Some("high"))["request"],
            serde_json::json!({"subtype": "apply_flag_settings", "settings": {"effortLevel": "high"}})
        );
        assert_eq!(
            ControlRequest::interrupt("req-6")["request"],
            serde_json::json!({"subtype": "interrupt"})
        );
        assert_eq!(
            ControlRequest::get_context_usage("req-7")["request"],
            serde_json::json!({"subtype": "get_context_usage", "detail": "summary"})
        );
        assert_eq!(
            ControlRequest::get_usage("req-8")["request"],
            serde_json::json!({"subtype": "get_usage"})
        );
        assert_eq!(
            ControlRequest::rewind_files("req-9", "uuid-1", true)["request"],
            serde_json::json!({
                "subtype": "rewind_files",
                "user_message_id": "uuid-1",
                "dry_run": true
            })
        );
    }

    #[test]
    fn a_success_response_is_matched_by_request_id() {
        let line = serde_json::json!({
            "type": "control_response",
            "response": {"subtype": "success", "request_id": "req-1", "response": {"mode": "plan"}}
        });
        let envelope = ControlEnvelope::parse(&line).expect("a control response");
        assert_eq!(envelope.request_id, "req-1");
        assert_eq!(envelope.payload.expect("payload")["mode"], "plan");
        assert!(envelope.error.is_none());
    }

    #[test]
    fn an_error_response_names_its_failure_without_a_payload() {
        let line = serde_json::json!({
            "type": "control_response",
            "response": {
                "subtype": "error",
                "request_id": "req-9",
                "error": "Unknown control request subtype: rewind_files"
            }
        });
        let envelope = ControlEnvelope::parse(&line).expect("a control response");
        assert_eq!(envelope.request_id, "req-9");
        assert!(envelope.payload.is_none());
        assert_eq!(
            envelope.error.as_deref(),
            Some("Unknown control request subtype: rewind_files")
        );
    }

    #[test]
    fn a_can_use_tool_request_exposes_the_tool_and_its_suggestions() {
        let line: serde_json::Value =
            serde_json::from_str(CAN_USE_TOOL).expect("fixture is valid JSON");
        let request = CanUseTool::parse(&line).expect("a can_use_tool request");
        assert_eq!(request.request_id, "cli-req-7");
        assert_eq!(request.tool_name, "Bash");
        assert_eq!(request.tool_use_id.as_deref(), Some("toolu_01XYZ"));
        assert_eq!(
            request
                .input
                .get("command")
                .and_then(|value| value.as_str()),
            Some("rm -rf build")
        );
        assert_eq!(request.permission_suggestions.len(), 1);
    }

    #[test]
    fn a_permission_answer_is_a_control_response_under_the_cli_request_id() {
        let input = serde_json::json!({"command": "ls"});
        let allow = PermissionResult::allow(input.clone()).into_response("cli-req-7");
        assert_eq!(allow["type"], "control_response");
        assert_eq!(allow["response"]["subtype"], "success");
        assert_eq!(allow["response"]["request_id"], "cli-req-7");
        assert_eq!(allow["response"]["response"]["behavior"], "allow");
        assert_eq!(allow["response"]["response"]["updatedInput"], input);

        let suggestions = vec![serde_json::json!({"type": "addRules"})];
        let always = PermissionResult::allow_with_permissions(input.clone(), suggestions.clone())
            .into_response("cli-req-7");
        assert_eq!(
            always["response"]["response"]["updatedPermissions"],
            serde_json::Value::Array(suggestions)
        );

        let deny =
            PermissionResult::deny("The user rejected this action.").into_response("cli-req-7");
        assert_eq!(deny["response"]["response"]["behavior"], "deny");
        assert_eq!(
            deny["response"]["response"]["message"],
            "The user rejected this action."
        );
    }
}
