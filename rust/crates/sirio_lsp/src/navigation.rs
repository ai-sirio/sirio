//! The two questions sub-project 3 asks, and the plain Rust they answer in.
//!
//! Every LSP shape stops here. The app receives a `String` and a `Target`,
//! never a `Hover`, a `MarkupContent` or a three-variant
//! `GotoDefinitionResponse`: the protocol's vocabulary does not leak past
//! this crate, which is what lets `sirio_ui` stay free of `lsp-types`.

use std::path::{Path, PathBuf};

use lsp_types::Position;
use serde_json::Value;

use crate::connection::Client;
use crate::uri::{path_for_uri, uri_for_path};
use crate::LspError;

/// Where a definition lives. `line` and `character` stay in the protocol's
/// terms — `character` is UTF-16 — because only the caller holds the buffer
/// that turns them into a byte offset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    pub path: PathBuf,
    pub line: u32,
    pub character: u32,
}

/// The hover text at a position, already flattened to plain text. `None`
/// when the server has nothing to say there, which is the ordinary case
/// rather than a failure.
pub async fn hover(
    client: &Client,
    path: &Path,
    position: Position,
) -> Result<Option<String>, LspError> {
    let answer: Value = client
        .request(
            "textDocument/hover",
            serde_json::json!({
                "textDocument": { "uri": uri_for_path(path)? },
                "position": position,
            }),
        )
        .await?;
    Ok(hover_text(&answer))
}

/// Everywhere the symbol at a position is defined. Empty is a real answer.
pub async fn definition(
    client: &Client,
    path: &Path,
    position: Position,
) -> Result<Vec<Target>, LspError> {
    let answer: Value = client
        .request(
            "textDocument/definition",
            serde_json::json!({
                "textDocument": { "uri": uri_for_path(path)? },
                "position": position,
            }),
        )
        .await?;
    Ok(targets(&answer))
}

/// Flattens the three shapes `Hover.contents` may take and strips Markdown
/// code fences. The card paints plain text in the code font, so a fence is
/// three characters of noise occupying a line of a small popover.
pub fn hover_text(answer: &Value) -> Option<String> {
    let raw = match answer.get("contents")? {
        Value::String(text) => text.clone(),
        Value::Object(map) => map.get("value")?.as_str()?.to_owned(),
        Value::Array(items) => items
            .iter()
            .filter_map(|item| match item {
                Value::String(text) => Some(text.clone()),
                Value::Object(map) => Some(map.get("value")?.as_str()?.to_owned()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n"),
        _ => return None,
    };

    let stripped = raw
        .lines()
        .filter(|line| !line.trim_start().starts_with("```"))
        .collect::<Vec<_>>()
        .join("\n");
    let trimmed = stripped.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_owned())
}

/// `textDocument/definition` answers `null`, one `Location`, or an array of
/// them — and a server that ignores `linkSupport: false` answers
/// `LocationLink`s instead. All four are handled, because the cost is a few
/// lines and the failure is a jump that silently does nothing.
pub fn targets(answer: &Value) -> Vec<Target> {
    match answer {
        Value::Array(items) => items.iter().filter_map(target).collect(),
        Value::Object(_) => target(answer).into_iter().collect(),
        _ => Vec::new(),
    }
}

fn target(value: &Value) -> Option<Target> {
    let uri = value
        .get("uri")
        .or_else(|| value.get("targetUri"))?
        .as_str()?;
    let range = value
        .get("range")
        .or_else(|| value.get("targetSelectionRange"))
        .or_else(|| value.get("targetRange"))?;
    let start = range.get("start")?;
    Some(Target {
        path: path_for_uri(&uri.parse().ok()?)?,
        line: start.get("line")?.as_u64()? as u32,
        character: start.get("character")?.as_u64()? as u32,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_markup_hover_keeps_its_newlines_and_loses_its_fences() {
        // Raw newlines inside the payload are exactly why this crate frames
        // by Content-Length instead of reading lines. A hover is where they
        // show up first in practice.
        let answer = serde_json::json!({
            "contents": {
                "kind": "markdown",
                "value": "```rust\npub fn spawn(f: F) -> Task<T>\n```\n---\nRuns `f`."
            }
        });
        let text = hover_text(&answer).expect("a hover has text");
        assert_eq!(text, "pub fn spawn(f: F) -> Task<T>\n---\nRuns `f`.");
    }

    #[test]
    fn a_plain_string_hover_is_accepted() {
        let answer = serde_json::json!({ "contents": "u32" });
        assert_eq!(hover_text(&answer).as_deref(), Some("u32"));
    }

    #[test]
    fn an_array_hover_is_joined() {
        let answer = serde_json::json!({ "contents": ["first", {"value": "second"}] });
        assert_eq!(hover_text(&answer).as_deref(), Some("first\nsecond"));
    }

    #[test]
    fn a_null_hover_is_nothing_rather_than_an_error() {
        // The common case: the pointer is over whitespace. Not an error.
        assert_eq!(hover_text(&serde_json::json!(null)), None);
        assert_eq!(hover_text(&serde_json::json!({ "contents": "" })), None);
    }

    #[test]
    fn a_single_location_is_one_target() {
        let answer = serde_json::json!({
            "uri": "file:///tmp/a.rs",
            "range": { "start": { "line": 4, "character": 8 },
                       "end":   { "line": 4, "character": 12 } }
        });
        assert_eq!(
            targets(&answer),
            vec![Target {
                path: std::path::PathBuf::from("/tmp/a.rs"),
                line: 4,
                character: 8
            }]
        );
    }

    #[test]
    fn an_array_of_locations_is_many_targets() {
        let answer = serde_json::json!([
            { "uri": "file:///tmp/a.rs",
              "range": { "start": {"line": 1, "character": 0}, "end": {"line": 1, "character": 3} } },
            { "uri": "file:///tmp/b.rs",
              "range": { "start": {"line": 9, "character": 2}, "end": {"line": 9, "character": 5} } }
        ]);
        let found = targets(&answer);
        assert_eq!(found.len(), 2);
        assert_eq!(found[1].path, std::path::PathBuf::from("/tmp/b.rs"));
        assert_eq!(found[1].line, 9);
    }

    #[test]
    fn a_null_definition_is_an_empty_list() {
        // A client that handles only the array shape works against some
        // servers and silently does nothing against the rest.
        assert!(targets(&serde_json::json!(null)).is_empty());
    }

    #[test]
    fn a_location_link_is_still_understood() {
        // We ask for linkSupport: false, but a server that ignores the flag
        // should not cost the user their jump.
        let answer = serde_json::json!([{
            "targetUri": "file:///tmp/a.rs",
            "targetSelectionRange": { "start": {"line": 2, "character": 4},
                                      "end":   {"line": 2, "character": 9} }
        }]);
        assert_eq!(targets(&answer).len(), 1);
        assert_eq!(targets(&answer)[0].line, 2);
    }
}
