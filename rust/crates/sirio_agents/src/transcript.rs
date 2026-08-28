//! File-backed transcript readers for native Claude and Codex sessions.

use std::path::{Path, PathBuf};

use crate::session_validator::claude_project_slug;

/// Reads Claude's JSONL transcript for one worktree and session reference.
pub struct ClaudeTranscriptSource {
    worktree_path: String,
    session_ref: String,
    home_directory: PathBuf,
}

impl ClaudeTranscriptSource {
    pub fn new(
        worktree_path: impl Into<String>,
        session_ref: impl Into<String>,
        home_directory: impl Into<PathBuf>,
    ) -> Self {
        Self {
            worktree_path: worktree_path.into(),
            session_ref: session_ref.into(),
            home_directory: home_directory.into(),
        }
    }

    /// Returns all readable message text in source order, or None when no
    /// transcript file or usable text is available.
    pub fn recent_text(&self) -> Option<String> {
        let project_slug = claude_project_slug(&self.worktree_path);
        let path = self
            .home_directory
            .join(".claude/projects")
            .join(project_slug)
            .join(format!("{}.jsonl", self.session_ref));
        let data = std::fs::read(path).ok()?;
        let mut chunks = Vec::new();
        for line in String::from_utf8_lossy(&data).lines() {
            let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
                continue;
            };
            let Some(content) = value
                .get("message")
                .and_then(|message| message.get("content"))
            else {
                continue;
            };
            append_content(content, &mut chunks);
        }
        (!chunks.is_empty()).then(|| chunks.join("\n"))
    }
}

/// Reads the Codex rollout JSONL file whose filename ends in the session id.
pub struct CodexTranscriptSource {
    session_ref: String,
    home_directory: PathBuf,
}

impl CodexTranscriptSource {
    pub fn new(session_ref: impl Into<String>, home_directory: impl Into<PathBuf>) -> Self {
        Self {
            session_ref: session_ref.into(),
            home_directory: home_directory.into(),
        }
    }

    /// Returns text blocks in rollout order, or None when no matching rollout
    /// or usable text is available.
    pub fn recent_text(&self) -> Option<String> {
        let sessions_root = self.home_directory.join(".codex/sessions");
        let suffix = format!("-{}.jsonl", self.session_ref.to_ascii_lowercase());
        let path = find_rollout(&sessions_root, &suffix)?;
        let data = std::fs::read(path).ok()?;
        let mut chunks = Vec::new();
        for line in String::from_utf8_lossy(&data).lines() {
            let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
                continue;
            };
            append_rollout_content(&value, &mut chunks);
        }
        (!chunks.is_empty()).then(|| chunks.join("\n"))
    }
}

fn append_rollout_content(value: &serde_json::Value, chunks: &mut Vec<String>) {
    if let Some(content) = value.get("content") {
        append_content(content, chunks);
    }
    if let Some(payload) = value.get("payload") {
        append_rollout_content(payload, chunks);
    }
}

fn append_content(content: &serde_json::Value, chunks: &mut Vec<String>) {
    match content {
        serde_json::Value::String(text) => chunks.push(text.clone()),
        serde_json::Value::Array(blocks) => {
            for block in blocks {
                if matches!(
                    block.get("type").and_then(serde_json::Value::as_str),
                    Some("text") | Some("output_text")
                ) && let Some(text) = block.get("text").and_then(serde_json::Value::as_str)
                {
                    chunks.push(text.to_string());
                }
            }
        }
        _ => {}
    }
}

fn find_rollout(path: &Path, suffix: &str) -> Option<PathBuf> {
    let entries = std::fs::read_dir(path).ok()?;
    for entry in entries.flatten() {
        let entry_path = entry.path();
        let name = entry.file_name();
        if name.to_string_lossy().starts_with('.') {
            continue;
        }
        let file_type = entry.file_type().ok()?;
        if file_type.is_dir() {
            if let Some(found) = find_rollout(&entry_path, suffix) {
                return Some(found);
            }
        } else if name
            .to_string_lossy()
            .to_ascii_lowercase()
            .ends_with(suffix)
        {
            return Some(entry_path);
        }
    }
    None
}
