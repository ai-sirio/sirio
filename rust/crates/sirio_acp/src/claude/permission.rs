//! Turning the CLI's one question into the card the surface draws, and the
//! card's answer back into the CLI's expected reply.

use serde_json::{Value, json};
use sirio_claude::{CanUseTool, PermissionResult};

use crate::{PermissionOption, PermissionQuestion, PermissionTextInput};

/// An answer, plus the mode change that has to follow it.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Answer {
    /// What to reply to the CLI.
    pub result: PermissionResult,
    /// A permission mode to set afterwards, when the choice implies one.
    pub then_mode: Option<String>,
}

/// The options this request offers.
pub(super) fn options_for(request: &CanUseTool) -> Vec<PermissionOption> {
    if request.tool_name == "ExitPlanMode" {
        // Approving a plan is a decision about what happens next, not just
        // about this call, so the options say what the session becomes.
        return vec![
            option(
                "approve_accept_edits",
                "Approve, auto-accept edits",
                "AllowAlways",
            ),
            option("approve_ask", "Approve, ask for each edit", "AllowOnce"),
            option("keep_planning", "Keep planning", "RejectOnce"),
        ];
    }
    let mut options = vec![option("allow_once", "Allow", "AllowOnce")];
    if !request.permission_suggestions.is_empty() {
        options.push(option("allow_always", "Always allow", "AllowAlways"));
    }
    options.push(option("reject_once", "Reject", "RejectOnce"));
    options
}

fn option(id: &str, name: &str, kind: &str) -> PermissionOption {
    PermissionOption {
        id: id.to_string(),
        name: name.to_string(),
        kind: kind.to_string(),
        description: None,
    }
}

/// The refusal sent when a card is withdrawn, expires, or the agent dies
/// with it open. It reaches the model, so it says what happened.
pub(super) const REFUSED: &str = "The user rejected this action.";

/// Maps a chosen option id onto the reply and any mode change.
///
/// An id nobody offered is a denial, never a guess: allowing a tool
/// because a lookup missed is the one failure here that cannot be undone.
pub(super) fn answer_for(request: &CanUseTool, option_id: &str) -> Answer {
    let offered = options_for(request);
    if !offered.iter().any(|option| option.id == option_id) {
        return Answer {
            result: PermissionResult::deny(REFUSED),
            then_mode: None,
        };
    }
    match option_id {
        "allow_once" => Answer {
            result: PermissionResult::allow(request.input.clone()),
            then_mode: None,
        },
        "allow_always" => Answer {
            result: PermissionResult::allow_with_permissions(
                request.input.clone(),
                request.permission_suggestions.clone(),
            ),
            then_mode: None,
        },
        "approve_accept_edits" => Answer {
            result: PermissionResult::allow(request.input.clone()),
            then_mode: Some("acceptEdits".into()),
        },
        "approve_ask" => Answer {
            result: PermissionResult::allow(request.input.clone()),
            then_mode: Some("default".into()),
        },
        // "reject_once", "keep_planning", and anything else offered but not
        // named above.
        _ => Answer {
            result: PermissionResult::deny(REFUSED),
            then_mode: None,
        },
    }
}

/// An `AskUserQuestion` answer: the chosen text is written into the tool's
/// own input under the question it answers, which is where the tool reads
/// it back from.
pub(super) fn answer_for_question(request: &CanUseTool, answer: &str) -> Answer {
    let mut input = request.input.clone();
    let question_text = input
        .get("questions")
        .and_then(|questions| questions.as_array())
        .and_then(|questions| questions.first())
        .and_then(|question| question.get("question"))
        .and_then(|question| question.as_str())
        .unwrap_or_default()
        .to_string();
    let answers = json!({ question_text: answer });
    if let Some(object) = input.as_object_mut() {
        object.insert("answers".into(), answers);
    } else {
        input = json!({"answers": answers});
    }
    Answer {
        result: PermissionResult::allow(input),
        then_mode: None,
    }
}

/// Whether this request is a question rather than an ordinary permission.
pub(super) fn is_question(request: &CanUseTool) -> bool {
    request.tool_name == "AskUserQuestion"
}

/// The value a question's answer carries: an option's own label, or the
/// text the user typed. Both arrive as the same `option_id` string from the
/// surface, which is why a question's options are its labels.
pub(super) fn question_options(request: &CanUseTool) -> Vec<PermissionOption> {
    request
        .input
        .get("questions")
        .and_then(|questions| questions.as_array())
        .and_then(|questions| questions.first())
        .and_then(|question| question.get("options"))
        .and_then(|options| options.as_array())
        .map(|options| {
            options
                .iter()
                .filter_map(|choice| {
                    let label = choice.get("label").and_then(Value::as_str)?;
                    Some(PermissionOption {
                        description: choice
                            .get("description")
                            .and_then(Value::as_str)
                            .map(str::trim)
                            .filter(|description| !description.is_empty())
                            .map(ToOwned::to_owned),
                        ..option(label, label, "AllowOnce")
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// A native question with a free-text field, declared or not. The typed
/// answer is written into the tool's `answers` as it arrives
/// (`answer_for_question`), so this transport can always take one; the ACP
/// transport cannot (typed text would reach the agent as an option id it
/// never offered), which is why this lives here and not in the surface.
pub(super) fn with_free_text(mut question: PermissionQuestion) -> PermissionQuestion {
    question.text_input.get_or_insert(PermissionTextInput {
        placeholder: None,
        prefill: None,
    });
    question
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn request(
        tool_name: &str,
        input: serde_json::Value,
        suggestions: Vec<serde_json::Value>,
    ) -> CanUseTool {
        CanUseTool {
            request_id: "cli-1".into(),
            tool_name: tool_name.into(),
            tool_use_id: Some("toolu_1".into()),
            input,
            permission_suggestions: suggestions,
            decision_reason: None,
        }
    }

    #[test]
    fn an_ordinary_tool_offers_allow_and_reject() {
        let options = options_for(&request("Bash", json!({"command": "ls"}), vec![]));
        assert_eq!(
            options
                .iter()
                .map(|option| option.id.as_str())
                .collect::<Vec<_>>(),
            ["allow_once", "reject_once"]
        );
        assert_eq!(options[0].name, "Allow");
        assert_eq!(options[0].kind, "AllowOnce");
        assert_eq!(options[1].kind, "RejectOnce");
    }

    #[test]
    fn always_allow_appears_only_when_there_is_a_rule_to_remember() {
        // Without suggestions there is nothing to store, so the option
        // would be indistinguishable from allowing once — a button that
        // lies about what it does.
        let with_rules = options_for(&request(
            "Bash",
            json!({"command": "ls"}),
            vec![json!({"type": "addRules", "behavior": "allow", "destination": "session"})],
        ));
        assert_eq!(
            with_rules
                .iter()
                .map(|option| option.id.as_str())
                .collect::<Vec<_>>(),
            ["allow_once", "allow_always", "reject_once"]
        );
        assert_eq!(with_rules[1].name, "Always allow");
    }

    #[test]
    fn exit_plan_mode_offers_the_two_approvals_and_the_refusal() {
        let options = options_for(&request("ExitPlanMode", json!({"plan": "do it"}), vec![]));
        assert_eq!(
            options
                .iter()
                .map(|option| option.id.as_str())
                .collect::<Vec<_>>(),
            ["approve_accept_edits", "approve_ask", "keep_planning"]
        );
        assert_eq!(options[0].name, "Approve, auto-accept edits");
        assert_eq!(options[1].name, "Approve, ask for each edit");
        assert_eq!(options[2].name, "Keep planning");
    }

    #[test]
    fn each_option_maps_to_the_answer_the_cli_expects() {
        let plain = request("Bash", json!({"command": "ls"}), vec![]);
        assert_eq!(
            answer_for(&plain, "allow_once"),
            Answer {
                result: PermissionResult::allow(json!({"command": "ls"})),
                then_mode: None
            }
        );
        assert!(matches!(
            answer_for(&plain, "reject_once").result,
            PermissionResult::Deny { .. }
        ));

        let suggestions = vec![json!({"type": "addRules"})];
        let remembered = request("Bash", json!({"command": "ls"}), suggestions.clone());
        assert_eq!(
            answer_for(&remembered, "allow_always").result,
            PermissionResult::allow_with_permissions(json!({"command": "ls"}), suggestions)
        );

        // Approving a plan allows the tool *and* moves the session's mode:
        // the approval is meaningless if the next edit still asks.
        let plan = request("ExitPlanMode", json!({"plan": "do it"}), vec![]);
        assert_eq!(
            answer_for(&plan, "approve_accept_edits")
                .then_mode
                .as_deref(),
            Some("acceptEdits")
        );
        assert_eq!(
            answer_for(&plan, "approve_ask").then_mode.as_deref(),
            Some("default")
        );
        assert!(matches!(
            answer_for(&plan, "keep_planning").result,
            PermissionResult::Deny { .. }
        ));
        assert_eq!(answer_for(&plan, "keep_planning").then_mode, None);
    }

    #[test]
    fn an_option_id_nobody_offered_is_refused_rather_than_guessed() {
        let plain = request("Bash", json!({"command": "ls"}), vec![]);
        assert!(matches!(
            answer_for(&plain, "allow_always").result,
            PermissionResult::Deny { .. }
        ));
        assert!(matches!(
            answer_for(&plain, "nonsense").result,
            PermissionResult::Deny { .. }
        ));
    }

    #[test]
    fn a_question_answer_is_written_back_into_the_tools_own_input() {
        let question = request(
            "AskUserQuestion",
            json!({"questions": [{"header": "Pick", "question": "Which one?",
                                  "options": [{"label": "A"}, {"label": "B"}]}]}),
            vec![],
        );
        let answer = answer_for_question(&question, "B");
        let PermissionResult::Allow { updated_input, .. } = answer.result else {
            panic!("answering a question allows the tool");
        };
        assert_eq!(updated_input["answers"]["Which one?"], "B");
        // The rest of the input survives: the tool reads its own questions
        // back out of it.
        assert!(updated_input["questions"].is_array());
    }

    #[test]
    fn a_question_option_carries_its_description() {
        let question = request(
            "AskUserQuestion",
            json!({"questions": [{"header": "Pick", "question": "Which one?",
                                  "options": [{"label": "A", "description": "The first"},
                                              {"label": "B"},
                                              {"label": "C", "description": "  "}]}]}),
            vec![],
        );
        assert_eq!(
            question_options(&question)
                .iter()
                .map(|option| option.description.as_deref())
                .collect::<Vec<_>>(),
            [Some("The first"), None, None],
            "a blank description is no description"
        );
    }

    #[test]
    fn a_permission_option_has_no_description() {
        let options = options_for(&request("Bash", json!({"command": "ls"}), vec![]));
        assert!(options.iter().all(|option| option.description.is_none()));
    }

    #[test]
    fn a_native_question_always_offers_free_text() {
        // The native transport writes whatever text arrives into the
        // tool's `answers` (`answer_for_question`), so the field is safe to
        // offer even when the tool input declared none.
        let bare = PermissionQuestion {
            header: "Pick".into(),
            prompt: "Which one?".into(),
            text_input: None,
        };
        assert_eq!(
            with_free_text(bare).text_input,
            Some(PermissionTextInput {
                placeholder: None,
                prefill: None
            })
        );
        // A declared field keeps its own placeholder and prefill.
        let declared = PermissionQuestion {
            header: "Pick".into(),
            prompt: "Which one?".into(),
            text_input: Some(PermissionTextInput {
                placeholder: Some("Branch".into()),
                prefill: Some("main".into()),
            }),
        };
        assert_eq!(
            with_free_text(declared.clone()).text_input,
            declared.text_input
        );
    }
}
