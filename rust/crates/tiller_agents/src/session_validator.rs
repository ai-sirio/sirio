//! Read-only pre-flight checks for native agent session references.

use std::path::Path;

/// Cheap checks used before attempting to resume a saved agent session.
pub struct AgentSessionValidator;

impl AgentSessionValidator {
    /// Returns false only when a checkable agent's expected session file is
    /// definitely absent. Agents without a known on-disk store are trusted.
    pub fn is_likely_valid(
        agent_id: &str,
        session_ref: &str,
        worktree_path: &str,
        claude_config_dir: impl AsRef<Path>,
        codex_home: impl AsRef<Path>,
    ) -> bool {
        match agent_id {
            "claude" => claude_config_dir
                .as_ref()
                .join("projects")
                .join(Self::claude_project_slug(worktree_path))
                .join(format!("{session_ref}.jsonl"))
                .is_file(),
            "codex" => Self::codex_session_exists(session_ref, codex_home),
            _ => true,
        }
    }

    /// Claude replaces every non-alphanumeric character with a hyphen when
    /// naming a project directory.
    pub fn claude_project_slug(worktree_path: &str) -> String {
        claude_project_slug(worktree_path)
    }

    /// Codex rollout files can be nested below `sessions`; a filename that
    /// contains the reference is enough to make a resume worth trying.
    pub fn codex_session_exists(session_ref: &str, codex_home: impl AsRef<Path>) -> bool {
        find_reference(&codex_home.as_ref().join("sessions"), session_ref)
    }
}

pub(crate) fn claude_project_slug(worktree_path: &str) -> String {
    worktree_path
        .chars()
        .map(|character| {
            if character.is_alphanumeric() {
                character
            } else {
                '-'
            }
        })
        .collect()
}

fn find_reference(path: &Path, session_ref: &str) -> bool {
    let Ok(entries) = std::fs::read_dir(path) else {
        return false;
    };
    entries.flatten().any(|entry| {
        let entry_path = entry.path();
        let file_type = entry.file_type();
        if file_type.as_ref().is_ok_and(std::fs::FileType::is_dir) {
            find_reference(&entry_path, session_ref)
        } else {
            entry_path
                .file_name()
                .is_some_and(|name| name.to_string_lossy().contains(session_ref))
        }
    })
}
