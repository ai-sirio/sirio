//! The control channel: what Sirio asks of the CLI, and the one thing the
//! CLI asks of Sirio.
//!
//! Requests are built as `serde_json::Value` rather than typed structs
//! because each subtype has its own payload shape and the set grows without
//! notice; a builder per verb keeps the wire spelling in one place while
//! leaving the envelope uniform.

use serde_json::{Value, json};

/// How much of the model's reasoning the CLI sends, in the CLI's own
/// spelling. Verified against claude 2.1.278 by being refused: anything
/// outside this set and `null` comes back as an error naming the three.
pub const THINKING_DISPLAYS: [&str; 3] = ["summarized", "highlights", "omitted"];

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
    /// `forwardSubagentText` asks the CLI to forward a subagent's text and
    /// thinking as lines of their own, for the subagent card.
    #[must_use]
    pub fn initialize(request_id: &str) -> Value {
        Self::envelope(
            request_id,
            json!({"subtype": "initialize", "hooks": {}, "forwardSubagentText": true}),
        )
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

    /// Sets the effort level. `None` clears the level, leaving the model on
    /// its own default.
    ///
    /// Ultracode is a level in the picker and in the CLI's own `/effort`
    /// menu, but never on the wire: `apply_flag_settings` carries
    /// `effortLevel` and a separate `ultracode` boolean, and a session with
    /// `ultracode` standing runs at xhigh whatever `effortLevel` says. So
    /// the pseudo-level is translated here, at the one place that owns the
    /// wire format, and *both* keys are always written: the settings
    /// **merge** rather than replace, so leaving `ultracode` out is what
    /// would let it stand through a switch to another level.
    ///
    /// That merge is also why this request may leave the rest of the
    /// session's flag settings — `fastMode` among them — alone.
    #[must_use]
    pub fn set_effort(request_id: &str, level: Option<&str>) -> Value {
        let ultracode = level == Some(crate::catalog::EFFORT_ULTRACODE);
        let level = level.filter(|_| !ultracode);
        Self::envelope(
            request_id,
            json!({
                "subtype": "apply_flag_settings",
                "settings": {"effortLevel": level, "ultracode": ultracode}
            }),
        )
    }

    /// Turns fast mode on or off for this session.
    ///
    /// `off` is written rather than omitted, for the reason [`Self::set_effort`]
    /// gives: the settings merge, so a key left out keeps the value the last
    /// call left standing.
    #[must_use]
    pub fn set_fast_mode(request_id: &str, enabled: bool) -> Value {
        Self::envelope(
            request_id,
            json!({
                "subtype": "apply_flag_settings",
                "settings": {"fastMode": enabled}
            }),
        )
    }

    /// Sets how much of the model's reasoning the CLI sends — one of
    /// [`THINKING_DISPLAYS`], or `None` to leave the CLI on its own answer.
    ///
    /// The budget half of this request is always `null`. The effort picker
    /// already governs how hard the model works; a second number beside it
    /// would be two controls over one thing, disagreeing in public.
    ///
    /// Nothing reads this back: neither the handshake nor `get_settings`
    /// reports the session's current display, so the surface knows only
    /// what it has itself chosen.
    #[must_use]
    pub fn set_thinking_display(request_id: &str, display: Option<&str>) -> Value {
        Self::envelope(
            request_id,
            json!({
                "subtype": "set_max_thinking_tokens",
                "max_thinking_tokens": Value::Null,
                "thinking_display": display
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

    /// Every MCP server the session knows, with its connection state.
    #[must_use]
    pub fn mcp_status(request_id: &str) -> Value {
        Self::envelope(request_id, json!({"subtype": "mcp_status"}))
    }

    /// The session's live permission rules and workspace directories. The
    /// floor CLI refuses it as an unsupported subtype.
    #[must_use]
    pub fn list_permission_rules(request_id: &str) -> Value {
        Self::envelope(request_id, json!({"subtype": "list_permission_rules"}))
    }

    /// The hooks the session loaded, as the CLI's `/hooks` menu lists them.
    /// Older CLIs refuse it as an unsupported subtype.
    #[must_use]
    pub fn get_hooks_listing(request_id: &str) -> Value {
        Self::envelope(request_id, json!({"subtype": "get_hooks_listing"}))
    }

    /// Asks the CLI to connect a server again.
    #[must_use]
    pub fn mcp_reconnect(request_id: &str, server: &str) -> Value {
        Self::envelope(
            request_id,
            json!({"subtype": "mcp_reconnect", "serverName": server}),
        )
    }

    /// Starts a server's sign-in; the answer names the page to open.
    #[must_use]
    pub fn mcp_authenticate(request_id: &str, server: &str) -> Value {
        Self::envelope(
            request_id,
            json!({"subtype": "mcp_authenticate", "serverName": server}),
        )
    }

    /// Switches one server on or off. The CLI saves the choice in its own
    /// settings for the folder and answers with no body.
    #[must_use]
    pub fn mcp_toggle(request_id: &str, server: &str, enabled: bool) -> Value {
        Self::envelope(
            request_id,
            json!({"subtype": "mcp_toggle", "serverName": server, "enabled": enabled}),
        )
    }

    /// Reloads plugins. `hold` asks the CLI to leave the session alone when
    /// the reload would change its tool list and say so instead;
    /// `refuse_if_busy` asks it to refuse while a turn runs. A flag that is
    /// not asked for is absent, never `false`.
    #[must_use]
    pub fn reload_plugins(request_id: &str, hold: bool, refuse_if_busy: bool) -> Value {
        let mut request = json!({"subtype": "reload_plugins"});
        if let Some(fields) = request.as_object_mut() {
            if hold {
                fields.insert("hold_on_cache_impact".to_string(), json!(true));
            }
            if refuse_if_busy {
                fields.insert("refuse_if_busy".to_string(), json!(true));
            }
        }
        Self::envelope(request_id, request)
    }

    /// Reloads the skills on disk. A changed list follows as `commands_changed`.
    #[must_use]
    pub fn reload_skills(request_id: &str) -> Value {
        Self::envelope(request_id, json!({"subtype": "reload_skills"}))
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

    /// Stops one background task. The CLI answers `success` at once, for an
    /// unknown or ended task too; the task's end arrives as `task_*` lines.
    #[must_use]
    pub fn stop_task(request_id: &str, task_id: &str) -> Value {
        Self::envelope(
            request_id,
            json!({"subtype": "stop_task", "task_id": task_id}),
        )
    }

    /// Renames the session on the CLI's side. `source: "host"` marks it as the
    /// host's doing; `session_id` names the session the host means, and a
    /// current CLI refuses one that is not its own.
    #[must_use]
    pub fn rename_session(request_id: &str, title: &str, session_id: Option<&str>) -> Value {
        let mut request = json!({
            "subtype": "rename_session",
            "title": title,
            "source": "host",
        });
        if let (Some(session_id), Some(fields)) = (session_id, request.as_object_mut()) {
            fields.insert("session_id".to_string(), json!(session_id));
        }
        Self::envelope(request_id, request)
    }

    /// Reads the tail of a background shell's or Monitor's output.
    #[must_use]
    pub fn get_task_output(request_id: &str, task_id: &str) -> Value {
        Self::envelope(
            request_id,
            json!({"subtype": "get_task_output", "task_id": task_id}),
        )
    }
}

/// Whether an error answers a verb the CLI does not have, as against an
/// action it has and could not do.
///
/// ADR 0001: a refusal retires the one feature for the session, quietly;
/// any other error is that one action's failure and the feature stays. The
/// 2.1.257 floor says `Unsupported control request subtype: <verb>`, an
/// older build `Unknown control request subtype`.
#[must_use]
pub fn is_refusal(error: &str) -> bool {
    error.contains("Unknown control request subtype")
        || error.contains("Unsupported control request subtype")
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
    /// The CLI's machine-readable reason for an error, when it gives one
    /// (`reload_plugins_busy`, `unsupported_request`).
    pub error_code: Option<String>,
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
        let error_code = response
            .get("error_code")
            .and_then(|code| code.as_str())
            .map(str::to_string);
        Some(Self {
            request_id,
            error_code,
            payload: response
                .get("response")
                .cloned()
                .filter(|_| error.is_none()),
            error,
        })
    }
}

/// The CLI relaying an MCP server's `elicitation/create`: the server wants
/// the user to fill a form or visit a page.
#[derive(Clone, Debug, PartialEq)]
pub struct ElicitationRequest {
    /// The id the answer must be sent under.
    pub request_id: String,
    /// The MCP server asking.
    pub server: String,
    pub message: String,
    /// `form` or `url`; `form` when the line names none.
    pub mode: String,
    pub url: Option<String>,
    pub elicitation_id: Option<String>,
    /// The restricted JSON schema of a form, verbatim.
    pub requested_schema: Option<Value>,
    /// The server's field order, which JSON object order does not carry
    /// through every serializer.
    pub property_order: Option<Vec<String>>,
}

impl ElicitationRequest {
    /// Reads a `control_request` line, when it is an `elicitation` with an id.
    #[must_use]
    pub fn parse(line: &Value) -> Option<Self> {
        let request = line.get("request")?;
        if request.get("subtype")?.as_str()? != "elicitation" {
            return None;
        }
        let text = |key: &str| request.get(key).and_then(Value::as_str).map(str::to_string);
        Some(Self {
            request_id: line.get("request_id")?.as_str()?.to_string(),
            server: text("mcp_server_name").unwrap_or_default(),
            message: text("message").unwrap_or_default(),
            mode: text("mode")
                .filter(|mode| !mode.is_empty())
                .unwrap_or_else(|| "form".to_string()),
            url: text("url"),
            elicitation_id: text("elicitation_id"),
            requested_schema: request.get("requested_schema").cloned(),
            property_order: request
                .get("property_order")
                .and_then(Value::as_array)
                .map(|names| {
                    names
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect()
                }),
        })
    }
}

/// The complete line answering an elicitation. `action` is `accept`,
/// `decline` or `cancel`; `content` rides along only when given.
#[must_use]
pub fn elicitation_response(
    request_id: &str,
    action: &str,
    content: Option<serde_json::Map<String, Value>>,
) -> Value {
    let mut body = json!({"action": action});
    if let Some(content) = content {
        body["content"] = Value::Object(content);
    }
    json!({
        "type": "control_response",
        "response": {"subtype": "success", "request_id": request_id, "response": body}
    })
}

/// The refusal of a request this client does not handle. Unanswered, the CLI
/// would wait on it for the rest of the turn.
#[must_use]
pub fn control_error(request_id: &str, message: &str) -> Value {
    json!({
        "type": "control_response",
        "response": {"subtype": "error", "request_id": request_id, "error": message}
    })
}

/// The `request.subtype` of a `control_request` line.
#[must_use]
pub fn control_request_subtype(line: &Value) -> Option<&str> {
    line.get("request")?.get("subtype")?.as_str()
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
    /// The subagent that is asking, when one is: the `task_id` its
    /// `task_started` named. A main-thread request carries none.
    pub agent_id: Option<String>,
    /// The tool's arguments, echoed back verbatim in an `allow`.
    pub input: Value,
    /// Rules the CLI offers to remember. Non-empty is what makes an
    /// "Always allow" option honest: with nothing to remember, choosing it
    /// would be indistinguishable from allowing once.
    pub permission_suggestions: Vec<Value>,
    /// Why the CLI escalated, when it says. May carry ANSI escapes.
    pub decision_reason: Option<String>,
    /// The title a host-worded ask carries, when it does.
    pub title: Option<String>,
    /// What the tool is about to do, in the CLI's words.
    pub description: Option<String>,
    /// Why the CLI asks, as a type (`rule`, `workingDir`, ...).
    pub decision_reason_type: Option<String>,
    /// For a `rule` reason, the settings source that holds the rule.
    pub decision_reason_rule_source: Option<String>,
    /// The path a Bash command writes (the CLI's `blocked_path`), not a
    /// blocked location.
    pub blocked_path: Option<String>,
    /// The connector this tool belongs to: name and source, only when both
    /// are strings.
    pub mcp_server: Option<(String, String)>,
    /// The CLI would rather the answer be no.
    pub default_to_no: bool,
    /// The CLI asks that no "always allow" rule be offered.
    pub suppress_always_allow_rule: bool,
    /// The CLI says only a person may answer. Carried, not shown.
    pub requires_user_interaction: bool,
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
            agent_id: request
                .get("agent_id")
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
            title: text(request, "title"),
            description: text(request, "description"),
            decision_reason_type: text(request, "decision_reason_type"),
            decision_reason_rule_source: text(request, "decision_reason_rule_source"),
            blocked_path: text(request, "blocked_path"),
            mcp_server: request.get("mcp_server").and_then(|server| {
                Some((
                    server.get("name")?.as_str()?.to_string(),
                    server.get("source")?.as_str()?.to_string(),
                ))
            }),
            default_to_no: flag(request, "default_to_no"),
            suppress_always_allow_rule: flag(request, "suppress_always_allow_rule"),
            requires_user_interaction: flag(request, "requires_user_interaction"),
        })
    }
}

/// A string field, or absent when it is missing or of another type.
fn text(request: &Value, key: &str) -> Option<String> {
    request.get(key)?.as_str().map(str::to_string)
}

/// A boolean field, false when it is missing or of another type.
fn flag(request: &Value, key: &str) -> bool {
    request.get(key).and_then(Value::as_bool).unwrap_or(false)
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
    const CAN_USE_TOOL_SUBAGENT: &str =
        include_str!("../tests/fixtures/trace_can_use_tool_subagent.json");

    #[test]
    fn ultracode_is_a_boolean_beside_the_level_not_a_level() {
        // Verified against claude 2.1.274: `{"ultracode": true}` reports
        // back as `applied: {effort: "xhigh", ultracode: true}`, while
        // `{"effortLevel": "ultracode"}` is not a value the CLI's settings
        // normaliser keeps — so the pseudo-level must never reach the wire.
        assert_eq!(
            ControlRequest::set_effort("req-uc", Some("ultracode"))["request"],
            serde_json::json!({"subtype": "apply_flag_settings",
                "settings": {"effortLevel": null, "ultracode": true}})
        );
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
        // A null level is how the wire says "no level of mine".
        assert_eq!(
            ControlRequest::set_effort("req-4", None)["request"],
            serde_json::json!({"subtype": "apply_flag_settings",
                "settings": {"effortLevel": null, "ultracode": false}})
        );
        assert_eq!(
            ControlRequest::set_effort("req-5", Some("high"))["request"],
            serde_json::json!({"subtype": "apply_flag_settings",
                "settings": {"effortLevel": "high", "ultracode": false}})
        );
        // `xhigh` and `max` are ordinary levels: the CLI has taken them on
        // `--effort` all along, and `apply_flag_settings` applies them.
        assert_eq!(
            ControlRequest::set_effort("req-5b", Some("max"))["request"],
            serde_json::json!({"subtype": "apply_flag_settings",
                "settings": {"effortLevel": "max", "ultracode": false}})
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

    // `is_refusal` failure list, written before the function:
    //  - a refusal read as an action's error: the floor's "Unsupported control
    //    request subtype: get_task_output", and the older "Unknown control
    //    request subtype" with and without a verb after it;
    //  - an action's error read as a refusal, which would retire a feature that
    //    works: "get_task_output: no shell or Monitor task ...", the CLI's
    //    answer for a task it cannot read, and "Server not found: x";
    //  - an empty string read as a refusal.
    #[test]
    fn a_refusal_is_told_from_an_actions_own_error() {
        for refusal in [
            "Unsupported control request subtype: get_task_output",
            "Unknown control request subtype",
            "Unknown control request subtype: rewind_files",
        ] {
            assert!(is_refusal(refusal), "{refusal}");
        }
        for failure in [
            "get_task_output: no shell or Monitor task with that task_id in this session",
            "Server not found: x",
            "the agent refused the request",
            "",
        ] {
            assert!(!is_refusal(failure), "{failure:?}");
        }
    }

    #[test]
    fn fast_mode_is_one_key_and_off_is_written_rather_than_omitted() {
        // Verified against claude 2.1.278: `apply_flag_settings` merges
        // into the session's flag settings instead of replacing them —
        // sending `{fastMode}` then `{effortLevel, ultracode}` leaves all
        // three standing. So this request carries nothing but its own key,
        // and `false` has to be written, because an omitted key keeps
        // whatever the last call left there.
        assert_eq!(
            ControlRequest::set_fast_mode("req-fm", true)["request"],
            serde_json::json!({"subtype": "apply_flag_settings",
                "settings": {"fastMode": true}})
        );
        assert_eq!(
            ControlRequest::set_fast_mode("req-fm-off", false)["request"],
            serde_json::json!({"subtype": "apply_flag_settings",
                "settings": {"fastMode": false}})
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
    fn a_subagent_can_use_tool_carries_its_agent_id() {
        let line: serde_json::Value =
            serde_json::from_str(CAN_USE_TOOL_SUBAGENT).expect("fixture is valid JSON");
        let request = CanUseTool::parse(&line).expect("a can_use_tool request");
        assert_eq!(request.tool_name, "Bash");
        assert_eq!(
            request.tool_use_id.as_deref(),
            Some("toolu_014DYePo4ygeZxzAcyxNMG5q")
        );
        // The id a `task_started` calls `task_id`, which is how a permission
        // that beats its tool's own line still finds its subagent.
        assert_eq!(request.agent_id.as_deref(), Some("a8e0a4cef6a86147f"));

        // A main-thread request names no agent.
        let main: serde_json::Value =
            serde_json::from_str(CAN_USE_TOOL).expect("fixture is valid JSON");
        assert_eq!(
            CanUseTool::parse(&main)
                .expect("a can_use_tool request")
                .agent_id,
            None
        );
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

    #[test]
    fn the_mcp_builders_name_the_server() {
        assert_eq!(
            ControlRequest::mcp_status("r0")["request"],
            serde_json::json!({"subtype": "mcp_status"})
        );
        assert_eq!(
            ControlRequest::mcp_reconnect("r1", "codegraph")["request"],
            serde_json::json!({"subtype": "mcp_reconnect", "serverName": "codegraph"})
        );
        assert_eq!(
            ControlRequest::mcp_authenticate("r2", "claude.ai Notion")["request"],
            serde_json::json!({"subtype": "mcp_authenticate", "serverName": "claude.ai Notion"})
        );
    }

    // Elicitation wire, ways it can fail:
    // - a form request loses its schema or `property_order`, so the dock
    //   draws fields in the wrong order or none;
    // - a missing or empty `mode` is read as something other than a form;
    // - another subtype, or a request with no id, is taken for an
    //   elicitation and answered under a wrong id;
    // - the reply is not the same JSON value the CLI accepted (extra
    //   `content` key on decline/cancel, wrong envelope; key order is not
    //   compared);
    // - the refusal for an unknown subtype does not name the request, so the
    //   CLI never matches it and the session waits forever.

    const FORM_LINE: &str = r#"{"type": "control_request", "request_id": "c14d6b43-15df-4502-859f-8df48cd7c5cb", "request": {"subtype": "elicitation", "mcp_server_name": "elic", "message": "Pick a colour for the probe", "mode": "form", "requested_schema": {"type": "object", "properties": {"colour": {"type": "string", "title": "Colour", "enum": ["red", "blue"]}, "count": {"type": "integer", "title": "How many", "minimum": 1}, "note": {"type": "string", "description": "free text", "format": "email"}, "ok": {"type": "boolean", "default": true}}, "required": ["colour"]}, "property_order": ["colour", "count", "note", "ok"]}}"#;
    const URL_LINE: &str = r#"{"type": "control_request", "request_id": "1c918e92-5158-479e-a491-bd887d0f1882", "request": {"subtype": "elicitation", "mcp_server_name": "elic", "message": "Sign in to the probe service", "mode": "url", "url": "https://example.com/auth?x=1", "elicitation_id": "el-2"}}"#;

    fn line(text: &str) -> Value {
        serde_json::from_str(text).unwrap()
    }

    #[test]
    fn an_elicitation_request_is_read_with_its_schema_and_order() {
        let request = ElicitationRequest::parse(&line(FORM_LINE)).unwrap();
        assert_eq!(request.request_id, "c14d6b43-15df-4502-859f-8df48cd7c5cb");
        assert_eq!(request.server, "elic");
        assert_eq!(request.message, "Pick a colour for the probe");
        assert_eq!(request.mode, "form");
        assert_eq!(request.url, None);
        assert_eq!(
            request.requested_schema.as_ref().unwrap()["required"],
            json!(["colour"])
        );
        assert_eq!(
            request.property_order,
            Some(vec![
                "colour".into(),
                "count".into(),
                "note".into(),
                "ok".into()
            ])
        );
        assert_eq!(
            control_request_subtype(&line(FORM_LINE)),
            Some("elicitation")
        );
    }

    #[test]
    fn a_url_elicitation_carries_url_and_id() {
        let request = ElicitationRequest::parse(&line(URL_LINE)).unwrap();
        assert_eq!(request.mode, "url");
        assert_eq!(request.url.as_deref(), Some("https://example.com/auth?x=1"));
        assert_eq!(request.elicitation_id.as_deref(), Some("el-2"));
        assert_eq!(request.requested_schema, None);
    }

    #[test]
    fn a_missing_mode_is_a_form() {
        let bare = json!({"type": "control_request", "request_id": "r",
            "request": {"subtype": "elicitation", "mcp_server_name": "s", "message": "m"}});
        assert_eq!(ElicitationRequest::parse(&bare).unwrap().mode, "form");
        let empty = json!({"type": "control_request", "request_id": "r",
            "request": {"subtype": "elicitation", "mode": "", "message": "m"}});
        assert_eq!(ElicitationRequest::parse(&empty).unwrap().mode, "form");
    }

    #[test]
    fn parse_is_none_for_other_subtypes_and_without_request_id() {
        let other = json!({"type": "control_request", "request_id": "r",
            "request": {"subtype": "can_use_tool"}});
        assert_eq!(ElicitationRequest::parse(&other), None);
        let no_id = json!({"type": "control_request", "request": {"subtype": "elicitation"}});
        assert_eq!(ElicitationRequest::parse(&no_id), None);
    }

    #[test]
    fn elicitation_response_matches_the_wire() {
        let mut content = serde_json::Map::new();
        content.insert("colour".into(), json!("blue"));
        content.insert("count".into(), json!(2));
        content.insert("ok".into(), json!(true));
        assert_eq!(
            elicitation_response("r1", "accept", Some(content)),
            json!({"type": "control_response", "response": {"subtype": "success",
                "request_id": "r1", "response": {"action": "accept",
                    "content": {"colour": "blue", "count": 2, "ok": true}}}})
        );
        let bare = elicitation_response("r2", "decline", None);
        assert_eq!(bare["response"]["response"], json!({"action": "decline"}));
        assert!(bare["response"]["response"].get("content").is_none());
    }

    #[test]
    fn control_error_names_the_request() {
        assert_eq!(
            control_error("r9", "Sirio does not handle future_thing"),
            json!({"type": "control_response", "response": {"subtype": "error",
                "request_id": "r9", "error": "Sirio does not handle future_thing"}})
        );
    }
}
