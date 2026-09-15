//! `textDocument/documentSymbol`, flattened.
//!
//! The protocol answers this request in two incompatible shapes, and a
//! client that parses one of them shows an empty outline against every
//! server that speaks the other — with no error anywhere to say so. Both
//! are handled here, and both come out as one flat list.
//!
//! Flattening is deliberate rather than a shortcut. The outline filters by
//! name, and a filtered *tree* has no good answer: keeping the parents of a
//! match shows rows the user did not ask for, dropping them loses the
//! context the indent was carrying. A flat list with a `depth` field
//! degrades by itself — filtered, the indent simply stops being drawn.

use std::path::Path;

use serde_json::Value;

use crate::LspError;
use crate::connection::Client;
use crate::uri::uri_for_path;

/// The kinds the outline distinguishes, which is fewer than the protocol's
/// 26. `lsp_types::SymbolKind` is a newtype over `u32` whose meaning is a
/// protocol table; what a view wants is a handful of cases it can draw.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymbolKind {
    Function,
    Method,
    Struct,
    Enum,
    Interface,
    Field,
    Constant,
    Variable,
    Module,
    Other,
}

impl SymbolKind {
    /// The protocol's own numbering (LSP 3.17, `SymbolKind`). Anything not
    /// named here is `Other` — never a dropped symbol, which would leave a
    /// hole in the outline with no way to notice it.
    fn from_code(code: u64) -> Self {
        match code {
            2 => Self::Module,
            5 => Self::Struct,
            6 => Self::Method,
            7 | 8 => Self::Field,
            10 => Self::Enum,
            11 => Self::Interface,
            12 => Self::Function,
            13 => Self::Variable,
            14 | 22 => Self::Constant,
            23 => Self::Struct,
            _ => Self::Other,
        }
    }
}

/// One entry of a file's outline. `line` is the protocol's own zero-based
/// numbering, which is also what `open_at_line` in the app expects.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Symbol {
    pub name: String,
    pub detail: Option<String>,
    pub kind: SymbolKind,
    pub line: u32,
    /// Nesting depth in the tree this came from; `0` for the flat shape.
    pub depth: usize,
}

/// A file's symbols, flattened. An empty list is a real answer: a file may
/// genuinely have none.
pub async fn document_symbols(client: &Client, path: &Path) -> Result<Vec<Symbol>, LspError> {
    let answer: Value = client
        .request(
            "textDocument/documentSymbol",
            serde_json::json!({
                "textDocument": { "uri": uri_for_path(path)? },
            }),
        )
        .await?;
    Ok(parse_symbols(&answer))
}

/// Both response shapes, in one flat list. Pure, so the branch that decides
/// between them is testable without a process.
pub fn parse_symbols(answer: &Value) -> Vec<Symbol> {
    let Value::Array(items) = answer else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for item in items {
        push_symbol(item, 0, &mut out);
    }
    out
}

/// Depth-first, so a symbol's children follow it immediately and `depth` is
/// the indent the outline draws.
fn push_symbol(value: &Value, depth: usize, out: &mut Vec<Symbol>) {
    let Some(name) = value.get("name").and_then(Value::as_str) else {
        return;
    };
    // The nested shape points at the symbol's own name with
    // `selectionRange`; the flat one carries a whole `Location`. A symbol
    // with neither is malformed, and placing it at line 0 would send the
    // reader to the top of the file behind what looks like a working jump.
    let Some(line) = start_line(value) else {
        return;
    };
    let kind = value
        .get("kind")
        .and_then(Value::as_u64)
        .map_or(SymbolKind::Other, SymbolKind::from_code);

    out.push(Symbol {
        name: name.to_owned(),
        detail: value
            .get("detail")
            .and_then(Value::as_str)
            .map(str::to_owned),
        kind,
        line,
        depth,
    });

    if let Some(Value::Array(children)) = value.get("children") {
        for child in children {
            push_symbol(child, depth + 1, out);
        }
    }
}

/// `selectionRange` first: it names the identifier, where `range` covers the
/// whole body. Jumping to the body's first line is right far more often than
/// not, but the identifier is what the reader asked for.
fn start_line(value: &Value) -> Option<u32> {
    let range = value
        .get("selectionRange")
        .or_else(|| value.get("range"))
        .or_else(|| value.get("location").and_then(|at| at.get("range")))?;
    range.get("start")?.get("line")?.as_u64().map(|line| line as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_nested_answer_is_flattened_depth_first_with_its_indent() {
        // rust-analyzer's shape. A child must follow its parent immediately
        // and carry depth 1, which is the whole of the outline's indent.
        let answer = serde_json::json!([
            {
                "name": "LspSupervisor",
                "detail": "struct",
                "kind": 23,
                "range": { "start": {"line": 10, "character": 0},
                           "end":   {"line": 40, "character": 1} },
                "selectionRange": { "start": {"line": 10, "character": 11},
                                    "end":   {"line": 10, "character": 24} },
                "children": [
                    {
                        "name": "servers",
                        "kind": 8,
                        "range": { "start": {"line": 12, "character": 4},
                                   "end":   {"line": 12, "character": 40} },
                        "selectionRange": { "start": {"line": 12, "character": 4},
                                            "end":   {"line": 12, "character": 11} }
                    }
                ]
            },
            {
                "name": "answer_for",
                "kind": 12,
                "range": { "start": {"line": 50, "character": 0},
                           "end":   {"line": 60, "character": 1} },
                "selectionRange": { "start": {"line": 50, "character": 7},
                                    "end":   {"line": 50, "character": 17} }
            }
        ]);

        let found = parse_symbols(&answer);
        assert_eq!(found.len(), 3);

        assert_eq!(found[0].name, "LspSupervisor");
        assert_eq!(found[0].kind, SymbolKind::Struct);
        assert_eq!(found[0].detail.as_deref(), Some("struct"));
        assert_eq!(found[0].line, 10, "the selectionRange names the symbol");
        assert_eq!(found[0].depth, 0);

        assert_eq!(found[1].name, "servers", "the child follows its parent");
        assert_eq!(found[1].kind, SymbolKind::Field);
        assert_eq!(found[1].depth, 1);

        assert_eq!(found[2].name, "answer_for");
        assert_eq!(found[2].kind, SymbolKind::Function);
        assert_eq!(found[2].depth, 0, "a sibling returns to depth 0");
    }

    #[test]
    fn a_flat_answer_is_understood_too() {
        // The deprecated SymbolInformation shape. A client that parses only
        // the nested one shows an empty outline here, with no error.
        let answer = serde_json::json!([
            {
                "name": "main",
                "kind": 12,
                "containerName": "app",
                "location": {
                    "uri": "file:///tmp/a.rs",
                    "range": { "start": {"line": 3, "character": 0},
                               "end":   {"line": 6, "character": 1} }
                }
            }
        ]);

        let found = parse_symbols(&answer);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "main");
        assert_eq!(found[0].line, 3);
        assert_eq!(
            found[0].depth, 0,
            "containerName is a name, not a depth; inventing a tree from it misorders the list"
        );
    }

    #[test]
    fn an_empty_or_absent_answer_is_an_empty_list() {
        // A file with no symbols, and a server with nothing to say, are both
        // ordinary. Neither is an error.
        assert!(parse_symbols(&serde_json::json!([])).is_empty());
        assert!(parse_symbols(&serde_json::json!(null)).is_empty());
    }

    #[test]
    fn an_unknown_kind_is_other_rather_than_a_dropped_symbol() {
        // Losing a symbol because its kind is one of the 26 we do not name
        // would leave a hole in the outline with no way to notice.
        let answer = serde_json::json!([{
            "name": "mystery",
            "kind": 26,
            "selectionRange": { "start": {"line": 1, "character": 0},
                                "end":   {"line": 1, "character": 7} },
            "range": { "start": {"line": 1, "character": 0},
                       "end":   {"line": 1, "character": 7} }
        }]);
        let found = parse_symbols(&answer);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].kind, SymbolKind::Other);
    }

    #[test]
    fn a_symbol_missing_its_position_is_skipped_not_placed_at_zero() {
        // A malformed entry placed at line 0 sends the reader to the top of
        // the file and looks like a working jump, which is worse than an
        // absent row.
        let answer = serde_json::json!([{ "name": "broken", "kind": 12 }]);
        assert!(parse_symbols(&answer).is_empty());
    }
}
