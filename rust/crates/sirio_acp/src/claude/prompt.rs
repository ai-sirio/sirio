//! Turning a composer draft into a `user` message.
//!
//! Three things ride one turn: the text, the files the user picked from the
//! `@` menu, and any images they attached. Mentions are rendered into the
//! text as markdown links rather than sent as their own blocks — that is
//! what the official wrapper does, and it is what makes the model read a
//! path it can actually open.

use std::path::Path;

use serde_json::{Value, json};

use crate::ImageAttachment;

/// Builds the `user` line for one turn. `uuid` is the client-generated id
/// this turn is known by — the handle a later rewind names.
#[must_use]
pub(super) fn user_message(
    text: &str,
    mention_paths: &[String],
    images: &[ImageAttachment],
    cwd: &Path,
    uuid: &str,
) -> Value {
    let mut blocks: Vec<Value> = Vec::new();
    let body = with_mentions(text, mention_paths, cwd);
    if !body.trim().is_empty() {
        blocks.push(json!({"type": "text", "text": body}));
    }
    for image in images {
        blocks.push(json!({
            "type": "image",
            "source": {
                "type": "base64",
                "media_type": image.mime_type,
                "data": image.base64_data,
            }
        }));
    }
    json!({
        "type": "user",
        "uuid": uuid,
        "message": {"role": "user", "content": blocks},
    })
}

/// Appends one markdown link per mention. The draft is left untouched: the
/// user wrote `@main.rs` and should still see it in their own message.
fn with_mentions(text: &str, mention_paths: &[String], cwd: &Path) -> String {
    if mention_paths.is_empty() {
        return text.to_string();
    }
    let mut body = text.to_string();
    for path in mention_paths {
        let absolute = Path::new(path);
        let absolute = if absolute.is_absolute() {
            absolute.to_path_buf()
        } else {
            cwd.join(absolute)
        };
        let name = absolute
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.clone());
        if !body.is_empty() {
            body.push('\n');
        }
        body.push_str(&format!("[@{name}](file://{})", absolute.display()));
    }
    body
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn a_mention_becomes_a_link_the_model_can_follow() {
        // The wrapper's own rendering: the model reads the path, and the
        // surface can still recognise the token it inserted.
        let message = user_message(
            "look at @main.rs",
            &["src/main.rs".to_string()],
            &[],
            Path::new("/repo"),
            "uuid-2",
        );
        let text = message["message"]["content"][0]["text"]
            .as_str()
            .expect("text");
        assert!(
            text.contains("look at @main.rs"),
            "the draft survives: {text}"
        );
        assert!(
            text.contains("[@main.rs](file:///repo/src/main.rs)"),
            "the mention resolves to an absolute file link: {text}"
        );
    }

    #[test]
    fn an_empty_draft_with_an_attachment_still_sends_the_attachment() {
        let message = user_message(
            "   ",
            &[],
            &[crate::ImageAttachment {
                mime_type: "image/png".into(),
                base64_data: "AAAA".into(),
            }],
            Path::new("/repo"),
            "uuid-5",
        );
        let blocks = message["message"]["content"].as_array().expect("blocks");
        assert_eq!(
            blocks.len(),
            1,
            "a blank text block is not sent: {blocks:?}"
        );
        assert_eq!(blocks[0]["type"], "image");
    }
}
