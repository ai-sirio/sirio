//! The Pi adapter, ported from `TillerAgents/PiAdapter.swift`.

use crate::error::PrepareError;
use crate::shell_quote::shell_quote;

/// Pi has no lifecycle hook mechanism — `prepare` is a no-op and Tiller
/// watches the pane's exit code.
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

    fn prepare(
        &self,
        _worktree_path: &str,
        _pane_id: &str,
        _tillerctl_path: &str,
    ) -> Result<(), PrepareError> {
        // No hook mechanism; nothing to write.
        Ok(())
    }

    fn command(&self, _worktree_path: &str, _pane_id: &str, _tillerctl_path: &str) -> String {
        "pi".to_string()
    }

    fn resume_command(
        &self,
        _worktree_path: &str,
        _pane_id: &str,
        _tillerctl_path: &str,
        session_ref: &str,
    ) -> Option<String> {
        Some(format!("pi --session {}", shell_quote(session_ref)))
    }

    fn acp_program(&self) -> Option<crate::AcpProgram> {
        // Pi runs inside a Node/Bun TUI with no ACP server of its own.
        // `None` is the honest answer: a chat tab must not be offered
        // for it.
        None
    }
}
