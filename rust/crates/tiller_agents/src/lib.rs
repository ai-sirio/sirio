//! The adapter interface and the fixed catalog of supported agent CLIs.

mod claude;
mod codex;
mod error;
mod omp;
mod opencode;
mod pi;
mod shell_quote;

pub use claude::ClaudeCodeAdapter;
pub use codex::CodexAdapter;
pub use error::PrepareError;
pub use omp::OhMyPiAdapter;
pub use opencode::OpenCodeAdapter;
pub use pi::PiAdapter;
pub use shell_quote::{json_string_literal, shell_quote};

/// One supported agent CLI, ported from `TillerAgents.AgentAdapter`.
///
/// An adapter knows how to prepare a worktree for its CLI (writing *only*
/// worktree-local hook configuration — never user-global config), the exact
/// shell command that starts a fresh session in a pane, and the command that
/// resumes a previously captured native session.
pub trait AgentAdapter {
    /// Stable identifier, e.g. "claude" or "codex".
    fn id(&self) -> &'static str;

    /// Human-readable name, e.g. "Claude Code" or "Codex".
    fn display_name(&self) -> &'static str;

    /// True when the agent notifies lifecycle events itself (hooks calling
    /// `tillerctl notify`). False → Tiller watches the pane's exit code
    /// instead.
    fn has_native_hooks(&self) -> bool;

    /// Writes per-worktree hook configuration without touching user-global
    /// config (never `~/.claude`, `~/.codex`, or anything under the home
    /// directory).
    fn prepare(
        &self,
        worktree_path: &str,
        pane_id: &str,
        tillerctl_path: &str,
    ) -> Result<(), PrepareError>;

    /// Full shell command to run inside the pane, which the pane executes
    /// with its cwd already set to the worktree.
    fn command(&self, worktree_path: &str, pane_id: &str, tillerctl_path: &str) -> String;

    /// Full shell command that relaunches the agent resuming a previously
    /// captured native session, or `None` when the agent cannot resume by
    /// reference. `session_ref` comes from the agent session table.
    fn resume_command(
        &self,
        worktree_path: &str,
        pane_id: &str,
        tillerctl_path: &str,
        session_ref: &str,
    ) -> Option<String>;
}

/// The fixed list of adapters shipped with Tiller, in display order —
/// ported from `AgentCatalog.all`.
pub const ALL: &[&dyn AgentAdapter] = &[
    &ClaudeCodeAdapter,
    &CodexAdapter,
    &OpenCodeAdapter,
    &PiAdapter,
    &OhMyPiAdapter,
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_has_the_five_adapters_in_order() {
        let ids: Vec<&str> = ALL.iter().map(|adapter| adapter.id()).collect();
        assert_eq!(ids, ["claude", "codex", "opencode", "pi", "omp"]);

        let names: Vec<&str> = ALL.iter().map(|adapter| adapter.display_name()).collect();
        assert_eq!(
            names,
            ["Claude Code", "Codex", "OpenCode", "Pi", "Oh-My-Pi"]
        );

        let hooks: Vec<bool> = ALL
            .iter()
            .map(|adapter| adapter.has_native_hooks())
            .collect();
        assert_eq!(hooks, [true, true, false, false, true]);
    }

    #[test]
    fn every_adapter_supports_resuming() {
        for adapter in ALL {
            let resume =
                adapter.resume_command("/wt", "pane-1", "/usr/local/bin/tillerctl", "sess-42");
            assert!(
                resume.is_some(),
                "{} must support resuming by session reference",
                adapter.id()
            );
        }
    }
}
