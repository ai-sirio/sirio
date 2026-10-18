//! The Pi adapter, ported from `SirioAgents/PiAdapter.swift`.

use crate::error::PrepareError;
use crate::shell_quote::{prompt_argument, shell_quote};

/// Pi has no lifecycle hook mechanism — `prepare` writes only the worktree's
/// Sirio skill, and Sirio watches the pane's exit code.
pub struct PiAdapter;

impl super::AgentAdapter for PiAdapter {
    fn id(&self) -> &'static str {
        "pi"
    }

    fn display_name(&self) -> &'static str {
        "Pi"
    }

    fn has_native_hooks(&self) -> bool {
        false
    }

    fn skill_markdown(&self) -> Option<&'static str> {
        // F-AGENT-SAFE-01: see `ClaudeCodeAdapter::skill_markdown`.
        Some(include_str!("../../../../skills/sirio/SKILL.md"))
    }

    fn prepare(
        &self,
        worktree_path: &str,
        _pane_id: &str,
        _sirioctl_path: &str,
    ) -> Result<(), PrepareError> {
        // No hook mechanism; the worktree's skill is the one file written.
        if let Some(markdown) = self.skill_markdown() {
            crate::install_skill(markdown, self.id(), worktree_path)?;
        }
        Ok(())
    }

    fn command(&self, _worktree_path: &str, _pane_id: &str, _sirioctl_path: &str) -> String {
        "pi".to_string()
    }

    fn command_with_prompt(&self, worktree_path: &str, pane_id: &str, sirioctl_path: &str, prompt: &str) -> String {
        format!("{} {}", self.command(worktree_path, pane_id, sirioctl_path), prompt_argument(prompt))
    }

    fn resume_command(
        &self,
        _worktree_path: &str,
        _pane_id: &str,
        _sirioctl_path: &str,
        session_ref: &str,
    ) -> Option<String> {
        Some(format!("pi --session {}", shell_quote(session_ref)))
    }

    fn summarizer_command(&self, prompt: &str) -> Option<String> {
        Some(format!("pi --print --no-tools {}", shell_quote(prompt)))
    }
}
