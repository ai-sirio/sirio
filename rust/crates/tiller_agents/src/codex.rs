//! The Codex adapter, ported from `TillerAgents/CodexAdapter.swift`.

use crate::error::PrepareError;
use crate::shell_quote::{json_string_literal, shell_quote};

/// Adapter for Cursor's Codex CLI.
///
/// `prepare` is a no-op because Codex configuration is global — and this
/// crate never touches it. Hook behaviour is delivered via the `-c` CLI
/// override in the command string instead.
pub struct CodexAdapter;

impl super::AgentAdapter for CodexAdapter {
    fn id(&self) -> &'static str {
        "codex"
    }

    fn display_name(&self) -> &'static str {
        "Codex"
    }

    fn has_native_hooks(&self) -> bool {
        true
    }

    fn prepare(
        &self,
        _worktree_path: &str,
        _pane_id: &str,
        _tillerctl_path: &str,
    ) -> Result<(), PrepareError> {
        // Codex config is global; prepare must not touch it, so there is
        // nothing to write.
        Ok(())
    }

    fn command(&self, _worktree_path: &str, pane_id: &str, tillerctl_path: &str) -> String {
        format!("codex -c {}", self.notify_override(pane_id, tillerctl_path))
    }

    fn resume_command(
        &self,
        _worktree_path: &str,
        pane_id: &str,
        tillerctl_path: &str,
        session_ref: &str,
    ) -> Option<String> {
        Some(format!(
            "codex -c {} resume {}",
            self.notify_override(pane_id, tillerctl_path),
            shell_quote(session_ref)
        ))
    }
}

impl CodexAdapter {
    /// Builds the `notify=[...]` override: the tillerctl invocation as a JSON
    /// array of JSON string literals, shell-quoted as a whole.
    ///
    /// Codex parses `-c key=value` overrides as TOML, so the literals MUST
    /// be built with [`json_string_literal`] — slash-escaped `\/` is not a
    /// valid TOML escape and would make Codex fail silently at config load.
    fn notify_override(&self, pane_id: &str, tillerctl_path: &str) -> String {
        let args = [
            tillerctl_path,
            "notify",
            "--session",
            pane_id,
            "--status",
            "needs-input",
        ];
        let json_args = args
            .iter()
            .map(|arg| json_string_literal(arg))
            .collect::<Vec<_>>()
            .join(",");
        shell_quote(&format!("notify=[{json_args}]"))
    }
}
