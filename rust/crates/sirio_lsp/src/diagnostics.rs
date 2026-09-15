//! Reading a `textDocument/publishDiagnostics` notification.
//!
//! Positions stay in the protocol's terms here. Turning them into byte
//! offsets needs the buffer, and the buffer belongs to the app — so this
//! module reports what the server said and stops.

use std::path::PathBuf;

use lsp_types::Position;
use serde_json::Value;

use crate::uri::path_for_uri;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    /// Ordered worst-first, so `min()` over a line picks the mark to paint.
    Error,
    Warning,
    Information,
    Hint,
}

impl Severity {
    fn from_code(code: Option<u64>) -> Self {
        match code {
            Some(1) => Self::Error,
            Some(2) => Self::Warning,
            Some(3) => Self::Information,
            Some(4) => Self::Hint,
            // The protocol lets a server omit this and leaves the meaning
            // to the client. Under-reporting an error is worse than
            // over-reporting a hint, so an unlabelled finding is an error.
            _ => Self::Error,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawDiagnostic {
    pub start: Position,
    pub end: Position,
    pub severity: Severity,
    pub message: String,
}

/// The file a notification is about and what it says, or `None` when the
/// payload names something that is not a local file.
///
/// An empty list is a real answer — it is how a server says the errors are
/// gone — so it returns `Some(path, vec![])`, never `None`.
pub fn parse_publish(params: &Value) -> Option<(PathBuf, Vec<RawDiagnostic>)> {
    let uri = params.get("uri")?.as_str()?;
    let path = path_for_uri(&uri.parse().ok()?)?;
    let found = params
        .get("diagnostics")
        .and_then(Value::as_array)
        .map(|items| items.iter().filter_map(one).collect())
        .unwrap_or_default();
    Some((path, found))
}

fn one(value: &Value) -> Option<RawDiagnostic> {
    let range = value.get("range")?;
    let read = |key: &str| -> Option<Position> {
        let point = range.get(key)?;
        Some(Position {
            line: point.get("line")?.as_u64()? as u32,
            character: point.get("character")?.as_u64()? as u32,
        })
    };
    Some(RawDiagnostic {
        start: read("start")?,
        end: read("end")?,
        severity: Severity::from_code(value.get("severity").and_then(Value::as_u64)),
        message: value.get("message")?.as_str()?.to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_publish_notification_names_its_file_and_its_findings() {
        let params = serde_json::json!({
            "uri": "file:///tmp/a.rs",
            "diagnostics": [
                { "range": { "start": {"line": 3, "character": 4},
                             "end":   {"line": 3, "character": 9} },
                  "severity": 1, "message": "cannot find value `x`" },
                { "range": { "start": {"line": 7, "character": 0},
                             "end":   {"line": 7, "character": 2} },
                  "severity": 2, "message": "unused import" }
            ]
        });
        let (path, found) = parse_publish(&params).expect("a publish parses");
        assert_eq!(path, std::path::PathBuf::from("/tmp/a.rs"));
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].severity, Severity::Error);
        assert_eq!(found[0].message, "cannot find value `x`");
        assert_eq!(found[0].start.line, 3);
        assert_eq!(found[1].severity, Severity::Warning);
    }

    #[test]
    fn an_empty_list_still_names_the_file() {
        // This is how a server says "the errors are gone". Dropping it
        // leaves stale marks in the gutter forever.
        let params = serde_json::json!({ "uri": "file:///tmp/a.rs", "diagnostics": [] });
        let (path, found) = parse_publish(&params).expect("an empty publish parses");
        assert_eq!(path, std::path::PathBuf::from("/tmp/a.rs"));
        assert!(found.is_empty());
    }

    #[test]
    fn a_diagnostic_with_no_severity_is_treated_as_an_error() {
        let params = serde_json::json!({
            "uri": "file:///tmp/a.rs",
            "diagnostics": [ { "range": { "start": {"line": 0, "character": 0},
                                          "end":   {"line": 0, "character": 1} },
                               "message": "something" } ]
        });
        let (_, found) = parse_publish(&params).expect("parses");
        assert_eq!(found[0].severity, Severity::Error);
    }

    #[test]
    fn a_publish_about_something_that_is_not_a_file_is_ignored() {
        let params = serde_json::json!({ "uri": "untitled:Untitled-1", "diagnostics": [] });
        assert!(parse_publish(&params).is_none());
    }
}
