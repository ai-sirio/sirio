//! What a tool call looks like in a transcript.
//!
//! Claude Code names its tools; ACP names *kinds*. This table is the join,
//! ported from the official ACP wrapper's own `tools.js` so a native chat
//! draws a call exactly as the wrapper-backed one did. A tool this build
//! has never heard of is not an error: it draws as itself, under the
//! generic kind.

use crate::message::ToolUse;

/// The tool categories the transcript keys icons and verbs on. The strings
/// are the contract — see [`ToolKind::as_str`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ToolKind {
    Read,
    Edit,
    Execute,
    Search,
    Fetch,
    Think,
    Delete,
    Move,
    SwitchMode,
    Other,
}

impl ToolKind {
    /// The exact spelling `sirio_ui::chat::tool_calls` matches on. These are
    /// ACP's `ToolKind` debug names, which is what the surface was written
    /// against; changing one here does not fail to compile, it silently
    /// drops a call to the generic icon.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Read => "Read",
            Self::Edit => "Edit",
            Self::Execute => "Execute",
            Self::Search => "Search",
            Self::Fetch => "Fetch",
            Self::Think => "Think",
            Self::Delete => "Delete",
            Self::Move => "Move",
            Self::SwitchMode => "SwitchMode",
            Self::Other => "Other",
        }
    }
}

/// A file this call reads or touches.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ToolLocation {
    /// Absolute path, as the tool named it.
    pub path: String,
    /// The line the call starts at, when it named one.
    pub line: Option<u32>,
}

/// A piece of what a call shows.
#[derive(Clone, Debug, PartialEq)]
pub enum ToolContent {
    /// Plain text: a command's description, a prompt, an output.
    Text(String),
    /// A file modification.
    Diff {
        /// The file being changed.
        path: String,
        /// Content before; `None` for a file being created.
        old_text: Option<String>,
        /// Content after.
        new_text: String,
    },
}

/// Everything a transcript needs to draw one call.
#[derive(Clone, Debug, PartialEq)]
pub struct ToolInfo {
    /// The row's heading.
    pub title: String,
    /// Its category.
    pub kind: ToolKind,
    /// Files it touches, for follow-along.
    pub locations: Vec<ToolLocation>,
    /// What it shows below the heading.
    pub content: Vec<ToolContent>,
}

/// Reads a tool call into its transcript row.
#[must_use]
pub fn describe(tool_use: &ToolUse) -> ToolInfo {
    let input = &tool_use.input;
    match tool_use.name.as_str() {
        "Task" | "Agent" => ToolInfo {
            title: string(input, "description").unwrap_or_else(|| "Task".into()),
            kind: ToolKind::Think,
            locations: Vec::new(),
            content: string(input, "prompt")
                .map(ToolContent::Text)
                .into_iter()
                .collect(),
        },
        "Bash" => ToolInfo {
            title: string(input, "command").unwrap_or_else(|| "Terminal".into()),
            kind: ToolKind::Execute,
            locations: Vec::new(),
            content: string(input, "description")
                .map(ToolContent::Text)
                .into_iter()
                .collect(),
        },
        "Read" | "NotebookRead" => {
            let path = string(input, "file_path");
            let offset = number(input, "offset").map(|offset| offset as u32);
            let limit = number(input, "limit");
            let title = match (&path, offset, limit) {
                (Some(path), Some(offset), Some(limit)) if limit > 0 => {
                    format!("{path} ({offset} - {})", offset as u64 + limit - 1)
                }
                (Some(path), _, _) => path.clone(),
                (None, _, _) => tool_use.name.clone(),
            };
            ToolInfo {
                title,
                kind: ToolKind::Read,
                locations: path
                    .map(|path| vec![ToolLocation { path, line: offset }])
                    .unwrap_or_default(),
                content: Vec::new(),
            }
        }
        "Edit" | "NotebookEdit" => {
            let path = string(input, "file_path");
            let content = match (&path, string(input, "new_string")) {
                (Some(path), Some(new_text)) => vec![ToolContent::Diff {
                    path: path.clone(),
                    old_text: string(input, "old_string"),
                    new_text,
                }],
                _ => Vec::new(),
            };
            ToolInfo {
                title: path.clone().unwrap_or_else(|| tool_use.name.clone()),
                kind: ToolKind::Edit,
                locations: path
                    .map(|path| vec![ToolLocation { path, line: None }])
                    .unwrap_or_default(),
                content,
            }
        }
        "Write" => {
            let path = string(input, "file_path");
            let content = match (&path, string(input, "content")) {
                (Some(path), Some(new_text)) => vec![ToolContent::Diff {
                    path: path.clone(),
                    // A `Write` states the whole file; whether it replaced
                    // anything is only known once the result arrives with
                    // its `originalFile`.
                    old_text: None,
                    new_text,
                }],
                _ => Vec::new(),
            };
            ToolInfo {
                title: path.clone().unwrap_or_else(|| "Write".into()),
                kind: ToolKind::Edit,
                locations: path
                    .map(|path| vec![ToolLocation { path, line: None }])
                    .unwrap_or_default(),
                content,
            }
        }
        "Glob" | "Grep" => {
            let path = string(input, "path");
            ToolInfo {
                title: string(input, "pattern").unwrap_or_else(|| tool_use.name.clone()),
                kind: ToolKind::Search,
                locations: path
                    .map(|path| vec![ToolLocation { path, line: None }])
                    .unwrap_or_default(),
                content: Vec::new(),
            }
        }
        "WebFetch" => ToolInfo {
            title: string(input, "url").unwrap_or_else(|| "Fetch".into()),
            kind: ToolKind::Fetch,
            locations: Vec::new(),
            content: Vec::new(),
        },
        "WebSearch" => ToolInfo {
            title: string(input, "query").unwrap_or_else(|| "Search the web".into()),
            kind: ToolKind::Fetch,
            locations: Vec::new(),
            content: Vec::new(),
        },
        "TodoWrite" | "TaskCreate" | "TaskUpdate" | "TaskList" | "TaskGet" => ToolInfo {
            title: todo_title(input),
            kind: ToolKind::Think,
            locations: Vec::new(),
            content: Vec::new(),
        },
        "ExitPlanMode" => ToolInfo {
            title: "Exit plan mode".into(),
            kind: ToolKind::SwitchMode,
            locations: Vec::new(),
            content: string(input, "plan")
                .map(ToolContent::Text)
                .into_iter()
                .collect(),
        },
        "Skill" => ToolInfo {
            title: string(input, "name")
                .or_else(|| string(input, "skill"))
                .unwrap_or_else(|| "Skill".into()),
            kind: ToolKind::Other,
            locations: Vec::new(),
            content: Vec::new(),
        },
        "AskUserQuestion" => ToolInfo {
            title: "Question".into(),
            kind: ToolKind::Other,
            locations: Vec::new(),
            content: Vec::new(),
        },
        name if name.starts_with("mcp__") => ToolInfo {
            title: mcp_title(name),
            kind: ToolKind::Other,
            locations: Vec::new(),
            content: Vec::new(),
        },
        name => ToolInfo {
            title: name.to_string(),
            kind: ToolKind::Other,
            locations: Vec::new(),
            content: Vec::new(),
        },
    }
}

/// `mcp__linear__create_issue` reads as `linear: create_issue`. A name that
/// does not split into three parts is shown whole rather than mangled.
fn mcp_title(name: &str) -> String {
    let mut parts = name.splitn(3, "__");
    match (parts.next(), parts.next(), parts.next()) {
        (Some("mcp"), Some(server), Some(tool)) => format!("{server}: {tool}"),
        _ => name.to_string(),
    }
}

fn todo_title(input: &serde_json::Value) -> String {
    let entries: Vec<String> = input
        .get("todos")
        .and_then(|todos| todos.as_array())
        .map(|todos| {
            todos
                .iter()
                .filter_map(|todo| todo.get("content").and_then(|text| text.as_str()))
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    if entries.is_empty() {
        "Update TODOs".to_string()
    } else {
        format!("Update TODOs: {}", entries.join(", "))
    }
}

fn string(input: &serde_json::Value, key: &str) -> Option<String> {
    input
        .get(key)
        .and_then(|value| value.as_str())
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn number(input: &serde_json::Value, key: &str) -> Option<u64> {
    input.get(key).and_then(serde_json::Value::as_u64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn tool(name: &str, input: serde_json::Value) -> ToolUse {
        ToolUse {
            id: "toolu_01".into(),
            name: name.into(),
            input,
        }
    }

    #[test]
    fn a_read_names_the_file_and_points_at_the_line() {
        let info = describe(&tool("Read", json!({"file_path": "/repo/src/main.rs"})));
        assert_eq!(info.title, "/repo/src/main.rs");
        assert_eq!(info.kind, ToolKind::Read);
        assert_eq!(
            info.locations,
            vec![ToolLocation {
                path: "/repo/src/main.rs".into(),
                line: None
            }]
        );

        let ranged = describe(&tool(
            "Read",
            json!({"file_path": "/repo/src/main.rs", "offset": 40, "limit": 10}),
        ));
        assert_eq!(ranged.title, "/repo/src/main.rs (40 - 49)");
        assert_eq!(ranged.locations[0].line, Some(40));
    }

    #[test]
    fn a_bash_call_shows_the_command_it_will_run() {
        let info = describe(&tool(
            "Bash",
            json!({"command": "cargo test -p sirio_claude", "description": "Run the tests"}),
        ));
        assert_eq!(info.title, "cargo test -p sirio_claude");
        assert_eq!(info.kind, ToolKind::Execute);
        assert_eq!(
            info.content,
            vec![ToolContent::Text("Run the tests".into())]
        );
    }

    #[test]
    fn an_edit_carries_its_diff_from_the_first_frame() {
        let info = describe(&tool(
            "Edit",
            json!({
                "file_path": "/repo/a.rs",
                "old_string": "let x = 1;",
                "new_string": "let x = 2;"
            }),
        ));
        assert_eq!(info.kind, ToolKind::Edit);
        assert_eq!(info.title, "/repo/a.rs");
        assert_eq!(
            info.content,
            vec![ToolContent::Diff {
                path: "/repo/a.rs".into(),
                old_text: Some("let x = 1;".into()),
                new_text: "let x = 2;".into(),
            }]
        );
    }

    #[test]
    fn a_write_is_a_diff_from_nothing() {
        let info = describe(&tool(
            "Write",
            json!({"file_path": "/repo/new.rs", "content": "fn main() {}"}),
        ));
        assert_eq!(info.kind, ToolKind::Edit);
        assert_eq!(
            info.content,
            vec![ToolContent::Diff {
                path: "/repo/new.rs".into(),
                old_text: None,
                new_text: "fn main() {}".into(),
            }]
        );
    }

    #[test]
    fn search_fetch_and_think_tools_take_their_own_titles() {
        assert_eq!(
            describe(&tool("Glob", json!({"pattern": "**/*.rs"}))).kind,
            ToolKind::Search
        );
        assert_eq!(
            describe(&tool("Grep", json!({"pattern": "fn main"}))).title,
            "fn main"
        );
        assert_eq!(
            describe(&tool("WebFetch", json!({"url": "https://example.com"}))).kind,
            ToolKind::Fetch
        );
        assert_eq!(
            describe(&tool("WebSearch", json!({"query": "gpui list state"}))).title,
            "gpui list state"
        );
        let task = describe(&tool(
            "Task",
            json!({"description": "Audit the parser", "prompt": "Read every call site"}),
        ));
        assert_eq!(task.kind, ToolKind::Think);
        assert_eq!(task.title, "Audit the parser");
        assert_eq!(
            task.content,
            vec![ToolContent::Text("Read every call site".into())]
        );
    }

    #[test]
    fn exit_plan_mode_is_its_own_kind_and_shows_the_plan() {
        let info = describe(&tool("ExitPlanMode", json!({"plan": "1. Read\n2. Write"})));
        assert_eq!(info.kind, ToolKind::SwitchMode);
        assert_eq!(info.title, "Exit plan mode");
        assert_eq!(
            info.content,
            vec![ToolContent::Text("1. Read\n2. Write".into())]
        );
    }

    #[test]
    fn todo_write_titles_itself_from_its_entries() {
        let info = describe(&tool(
            "TodoWrite",
            json!({"todos": [
                {"content": "Read the spec", "status": "completed"},
                {"content": "Write the test", "status": "in_progress"}
            ]}),
        ));
        assert_eq!(info.kind, ToolKind::Think);
        assert_eq!(info.title, "Update TODOs: Read the spec, Write the test");
    }

    #[test]
    fn an_mcp_tool_names_its_server_and_an_unknown_tool_names_itself() {
        let mcp = describe(&tool("mcp__linear__create_issue", json!({})));
        assert_eq!(mcp.kind, ToolKind::Other);
        assert_eq!(mcp.title, "linear: create_issue");

        let unknown = describe(&tool("SomethingNewInTheNextRelease", json!({})));
        assert_eq!(unknown.kind, ToolKind::Other);
        assert_eq!(unknown.title, "SomethingNewInTheNextRelease");
        assert!(unknown.content.is_empty());
    }

    #[test]
    fn a_tool_whose_input_is_missing_its_field_still_has_a_title() {
        // A malformed or truncated input must never produce an empty title:
        // a nameless row in the transcript is indistinguishable from a bug.
        assert_eq!(describe(&tool("Read", json!({}))).title, "Read");
        assert_eq!(describe(&tool("Bash", json!({}))).title, "Terminal");
        assert_eq!(describe(&tool("Edit", json!(null))).title, "Edit");
    }

    #[test]
    fn every_kind_spells_itself_the_way_the_transcript_keys_its_icons() {
        // `sirio_ui::chat::tool_calls::tool_icon` matches these exact
        // strings; a rename here silently falls back to the generic widget.
        assert_eq!(ToolKind::Read.as_str(), "Read");
        assert_eq!(ToolKind::Edit.as_str(), "Edit");
        assert_eq!(ToolKind::Execute.as_str(), "Execute");
        assert_eq!(ToolKind::Search.as_str(), "Search");
        assert_eq!(ToolKind::Fetch.as_str(), "Fetch");
        assert_eq!(ToolKind::Think.as_str(), "Think");
        assert_eq!(ToolKind::Delete.as_str(), "Delete");
        assert_eq!(ToolKind::Move.as_str(), "Move");
        assert_eq!(ToolKind::SwitchMode.as_str(), "SwitchMode");
        assert_eq!(ToolKind::Other.as_str(), "Other");
    }
}
