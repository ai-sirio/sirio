use crate::{PermissionQuestion, PermissionTextInput};

/// Folds an `AskUserQuestion`-shaped tool input (`{questions: [{header,
/// question, options: [{label, description}]}]}`) into the display fields a
/// question card needs. Only the first question is surfaced; multi-question
/// payloads are rare and the extra ones would need a second card. A
/// `_sirioTextInput` metadata object declares the free-text affordance.
pub fn parse_permission_question(raw_input: &serde_json::Value) -> Option<PermissionQuestion> {
    let input = raw_input.as_object()?;
    let questions = input.get("questions")?.as_array()?;
    let first = questions.first()?.as_object()?;
    let prompt = first.get("question")?.as_str()?.to_string();
    let header = first
        .get("header")
        .and_then(|value| value.as_str())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| prompt.clone());
    // Pi's `ui/select` repeats its header in `question`; a card that prints
    // both would show the same sentence twice.
    let body = if header.trim() == prompt.trim() {
        String::new()
    } else {
        prompt
    };
    // The key is part of the contract an agent writes, not one we control, so
    // the pre-rename spelling stays readable: an agent still emitting
    // `_tillerTextInput` would otherwise lose its text field with no error.
    let text_input = input
        .get("_sirioTextInput")
        .or_else(|| input.get("_tillerTextInput"))
        .and_then(|value| {
            let object = value.as_object()?;
            Some(PermissionTextInput {
                placeholder: object
                    .get("placeholder")
                    .and_then(|value| value.as_str())
                    .map(ToOwned::to_owned),
                prefill: object
                    .get("prefill")
                    .and_then(|value| value.as_str())
                    .map(ToOwned::to_owned),
            })
        });
    Some(PermissionQuestion {
        header,
        prompt: body,
        text_input,
    })
}
