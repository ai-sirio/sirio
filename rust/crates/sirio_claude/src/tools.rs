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

/// The tools whose result carries a file diff.
const DIFF_TOOLS: [&str; 3] = ["Edit", "Write", "NotebookEdit"];

/// Rebuilds the after-text of an edit from the tool's own structured
/// output, so the transcript shows what the file became rather than what
/// the call proposed.
///
/// `None` means "nothing better than the call's own diff is available" —
/// an unrecognised tool, a missing or empty patch, or a patch whose hunks
/// do not line up with the original it names. Refusing is deliberate: a
/// diff assembled from mismatched halves would be a change nobody made.
#[must_use]
pub fn diff_from_result(
    tool_name: &str,
    tool_use_result: &serde_json::Value,
) -> Option<ToolContent> {
    if !DIFF_TOOLS.contains(&tool_name) {
        return None;
    }
    let path = tool_use_result.get("filePath")?.as_str()?.to_string();
    let hunks = tool_use_result.get("structuredPatch")?.as_array()?;
    if hunks.is_empty() {
        return None;
    }
    let original = tool_use_result
        .get("originalFile")
        .and_then(|file| file.as_str());
    let old_lines: Vec<&str> = original.map(split_keeping_trailing).unwrap_or_default();

    // Walk the original once, splicing each hunk's replacement in at the
    // line it names. Hunks arrive in order, so a single cursor suffices.
    let mut new_lines: Vec<String> = Vec::new();
    let mut cursor = 0usize;
    for hunk in hunks {
        let old_start = hunk.get("oldStart")?.as_u64()? as usize;
        // A hunk is 1-based; `oldStart` 0 means "before the first line",
        // which is how a brand-new file's single hunk is expressed.
        // The header names the changed range, but `lines` carries the
        // surrounding context with it: step back over the leading context
        // lines to where the hunk's first line actually aligns, so the
        // context is verified against the original instead of duplicating
        // what was already copied and running past the end of the file.
        let hunk_lines = hunk.get("lines")?.as_array()?;
        let leading_context = hunk_lines
            .iter()
            .take_while(|line| line.as_str().is_some_and(|text| text.starts_with(' ')))
            .count();
        let start = old_start.saturating_sub(1).saturating_sub(leading_context);
        if start > old_lines.len() {
            return None;
        }
        new_lines.extend(
            old_lines[cursor..start]
                .iter()
                .map(|line| (*line).to_string()),
        );
        cursor = start;
        for line in hunk_lines {
            let line = line.as_str()?;
            let (marker, text) =
                line.split_at(line.char_indices().next().map_or(0, |(_, c)| c.len_utf8()));
            match marker {
                " " => {
                    // Context: it must be there in the original too, and
                    // say the same thing. A line that merely fits by
                    // position is not the same line.
                    if old_lines.get(cursor) != Some(&text) {
                        return None;
                    }
                    new_lines.push(old_lines[cursor].to_string());
                    cursor += 1;
                }
                "-" => {
                    // Likewise: the original has to actually carry the
                    // line the patch claims to be removing.
                    if old_lines.get(cursor) != Some(&text) {
                        return None;
                    }
                    cursor += 1;
                }
                "+" => new_lines.push(text.to_string()),
                // `\ No newline at end of file` and anything else the
                // differ emits: not a line of the file.
                _ => {}
            }
        }
    }
    new_lines.extend(old_lines[cursor..].iter().map(|line| (*line).to_string()));

    let mut new_text = new_lines.join("\n");
    // A file that ended with a newline still does; one that did not, does
    // not. `split_keeping_trailing` drops the final empty element, so the
    // terminator is restored here rather than carried as a phantom line.
    if original.is_none_or(|original| original.ends_with('\n')) && !new_text.is_empty() {
        new_text.push('\n');
    }
    Some(ToolContent::Diff {
        path,
        old_text: original.map(str::to_string),
        new_text,
    })
}

/// Splits a file into lines without inventing a trailing empty one for a
/// file that ends in a newline.
fn split_keeping_trailing(text: &str) -> Vec<&str> {
    let mut lines: Vec<&str> = text.split('\n').collect();
    if text.ends_with('\n') {
        lines.pop();
    }
    lines
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

    #[test]
    fn an_edit_result_rebuilds_the_file_from_its_hunks() {
        // `structuredPatch` is a unified diff already parsed: each hunk
        // names where it starts in the new file and carries its lines with
        // the leading ' ', '-' or '+' still attached.
        let result = json!({
            "filePath": "/repo/a.rs",
            "originalFile": "one\ntwo\nthree\n",
            "structuredPatch": [{
                "oldStart": 2, "oldLines": 1, "newStart": 2, "newLines": 1,
                "lines": [" one", "-two", "+TWO", " three"]
            }]
        });
        let diff = diff_from_result("Edit", &result).expect("a diff");
        assert_eq!(
            diff,
            ToolContent::Diff {
                path: "/repo/a.rs".into(),
                old_text: Some("one\ntwo\nthree\n".into()),
                new_text: "one\nTWO\nthree\n".into(),
            }
        );
    }

    #[test]
    fn a_write_result_over_an_existing_file_shows_what_it_replaced() {
        let result = json!({
            "filePath": "/repo/a.rs",
            "originalFile": "old\n",
            "structuredPatch": [{
                "oldStart": 1, "oldLines": 1, "newStart": 1, "newLines": 1,
                "lines": ["-old", "+new"]
            }]
        });
        let diff = diff_from_result("Write", &result).expect("a diff");
        assert_eq!(
            diff,
            ToolContent::Diff {
                path: "/repo/a.rs".into(),
                old_text: Some("old\n".into()),
                new_text: "new\n".into(),
            }
        );
    }

    #[test]
    fn a_new_file_has_no_original_to_show() {
        let result = json!({
            "filePath": "/repo/new.rs",
            "originalFile": null,
            "structuredPatch": [{
                "oldStart": 0, "oldLines": 0, "newStart": 1, "newLines": 1,
                "lines": ["+fn main() {}"]
            }]
        });
        let diff = diff_from_result("Write", &result).expect("a diff");
        assert_eq!(
            diff,
            ToolContent::Diff {
                path: "/repo/new.rs".into(),
                old_text: None,
                new_text: "fn main() {}\n".into(),
            }
        );
    }

    #[test]
    fn a_result_with_nothing_to_rebuild_from_keeps_the_calls_own_diff() {
        // The SDK documents this lane: a Write whose previous content was
        // too large to diff arrives with an empty patch and a null original.
        // Returning None here is what makes the start-frame diff survive.
        let empty = json!({"filePath": "/repo/a.rs", "originalFile": null, "structuredPatch": []});
        assert_eq!(diff_from_result("Write", &empty), None);
        // Not an edit tool at all.
        assert_eq!(diff_from_result("Bash", &json!({"stdout": "ok"})), None);
        // Shape the CLI changed under us.
        assert_eq!(
            diff_from_result("Edit", &json!({"filePath": "/repo/a.rs"})),
            None
        );
        assert_eq!(diff_from_result("Edit", &json!(null)), None);
    }

    #[test]
    fn a_hunk_that_does_not_fit_the_original_is_refused_rather_than_guessed() {
        // A patch that starts past the end of the file it claims to patch
        // means the two came from different states. Showing a diff built
        // from that would be inventing a change nobody made.
        let mismatched = json!({
            "filePath": "/repo/a.rs",
            "originalFile": "one\n",
            "structuredPatch": [{
                "oldStart": 9, "oldLines": 1, "newStart": 9, "newLines": 1,
                "lines": ["-nine", "+NINE"]
            }]
        });
        assert_eq!(diff_from_result("Edit", &mismatched), None);
    }

    #[test]
    fn a_context_line_that_disagrees_with_the_original_is_refused() {
        // The hunk lands inside the file, so nothing runs off the end —
        // but it quotes a line the file does not contain. The two halves
        // came from different states of the file just as surely as an
        // out-of-range hunk did, and rebuilding from them would show a
        // before-side that never existed.
        let drifted = json!({
            "filePath": "/repo/a.rs",
            "originalFile": "one\ntwo\nthree\n",
            "structuredPatch": [{
                "oldStart": 2, "oldLines": 2, "newStart": 2, "newLines": 2,
                "lines": [" TWO", "-three", "+THREE"]
            }]
        });
        assert_eq!(diff_from_result("Edit", &drifted), None);
    }
}
