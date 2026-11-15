//! The JSON-RPC 2.0 / MCP shapes Claude Code speaks, built byte for byte as it reads them.

use std::path::PathBuf;

use serde_json::{Map, Value, json};

use crate::uri::to_file_uri;

pub const METHOD_NOT_FOUND: i64 = -32601;
pub const PARSE_ERROR: i64 = -32700;
pub const INVALID_REQUEST: i64 = -32600;
pub const INVALID_PARAMS: i64 = -32602;
/// The application error code: carries no decision for Claude (spec §10.4).
pub const SERVER_ERROR: i64 = -32000;

/// The protocol version answered when `initialize` names none.
const DEFAULT_PROTOCOL: &str = "2025-11-25";

/// A position as VS Code counts it: 0-based line, character in UTF-16 code units.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pos16 {
    pub line: u32,
    pub character: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
    Info,
    Hint,
}

impl Severity {
    fn name(self) -> &'static str {
        match self {
            Severity::Error => "Error",
            Severity::Warning => "Warning",
            Severity::Info => "Info",
            Severity::Hint => "Hint",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub message: String,
    pub severity: Severity,
    pub start: Pos16,
    pub end: Pos16,
    pub source: Option<String>,
    pub code: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileDiagnostics {
    pub path: PathBuf,
    pub diagnostics: Vec<Diagnostic>,
}

/// What Sirio tells a connected Claude on its own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Notification {
    SelectionChanged { path: PathBuf, text: String, start: Pos16, end: Pos16 },
    AtMentioned { path: PathBuf, line_start: u32, line_end: u32 },
}

fn pos(p: Pos16) -> Value {
    json!({"line": p.line, "character": p.character})
}

fn diagnostic(d: &Diagnostic) -> Value {
    let mut o = Map::new();
    o.insert("message".into(), json!(d.message));
    o.insert("severity".into(), json!(d.severity.name()));
    o.insert("range".into(), json!({"start": pos(d.start), "end": pos(d.end)}));
    if let Some(s) = &d.source {
        o.insert("source".into(), json!(s));
    }
    if let Some(c) = &d.code {
        o.insert("code".into(), json!(c));
    }
    Value::Object(o)
}

/// The notification frame for `note`.
pub fn notification(note: &Notification) -> String {
    let (method, params) = match note {
        Notification::SelectionChanged { path, text, start, end } => (
            "selection_changed",
            json!({
                "text": text,
                "filePath": path.to_string_lossy(),
                "fileUrl": to_file_uri(path),
                "selection": {"start": pos(*start), "end": pos(*end), "isEmpty": start == end},
            }),
        ),
        Notification::AtMentioned { path, line_start, line_end } => (
            "at_mentioned",
            json!({
                "filePath": path.to_string_lossy(),
                "lineStart": line_start,
                "lineEnd": line_end,
            }),
        ),
    };
    json!({"jsonrpc": "2.0", "method": method, "params": params}).to_string()
}

pub fn error(id: &Value, code: i64, message: &str) -> String {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}}).to_string()
}

pub fn result(id: &Value, result: Value) -> String {
    json!({"jsonrpc": "2.0", "id": id, "result": result}).to_string()
}

pub fn initialize_result(params: &Value) -> Value {
    let version = params.get("protocolVersion").and_then(Value::as_str).unwrap_or(DEFAULT_PROTOCOL);
    json!({
        "protocolVersion": version,
        "capabilities": {"tools": {}},
        "serverInfo": {"name": "sirio", "version": env!("CARGO_PKG_VERSION")},
    })
}

fn tool(name: &str, description: &str, properties: Value, required: &[&str]) -> Value {
    json!({
        "name": name,
        "description": description,
        "inputSchema": {"type": "object", "properties": properties, "required": required},
    })
}

pub fn tools_list_result() -> Value {
    let text = json!({"type": "string"});
    json!({"tools": [
        tool(
            "openDiff",
            "Open a diff of a file against proposed contents and wait for the user's decision.",
            json!({
                "old_file_path": text,
                "new_file_path": text,
                "new_file_contents": text,
                "tab_name": text,
            }),
            &["old_file_path", "new_file_path", "new_file_contents", "tab_name"],
        ),
        tool("close_tab", "Close a diff tab by name.", json!({"tab_name": text}), &["tab_name"]),
        tool("closeAllDiffTabs", "Close every diff tab.", json!({}), &[]),
        tool(
            "getDiagnostics",
            "Language diagnostics for one file (uri), or for every file when no uri is given.",
            json!({"uri": text}),
            &[],
        ),
    ]})
}

/// `tools/call` result carrying one text item.
pub fn text_result(text: &str) -> Value {
    json!({"content": [{"type": "text", "text": text}]})
}

/// The `getDiagnostics` answer: one text item holding the JSON array of files. `echo` is the
/// request's own `uri` string, given back byte for byte as each entry's `uri` (a per-file
/// request); without it each `uri` is spelt from the entry's path.
pub fn diagnostics_result(files: &[FileDiagnostics], echo: Option<&str>) -> Value {
    let files: Vec<Value> = files
        .iter()
        .map(|f| {
            json!({
                "uri": echo.map_or_else(|| to_file_uri(&f.path), str::to_string),
                "diagnostics": f.diagnostics.iter().map(diagnostic).collect::<Vec<_>>(),
            })
        })
        .collect();
    text_result(&Value::Array(files).to_string())
}
