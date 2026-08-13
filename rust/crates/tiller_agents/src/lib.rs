//! The adapter interface and the fixed catalog of supported agent CLIs.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

mod claude;
mod codex;
mod error;
mod hook_migrator;
mod omp;
mod opencode;
mod pi;
mod session_validator;
mod shell_quote;
mod transcript;

pub use claude::ClaudeCodeAdapter;
pub use codex::CodexAdapter;
pub use error::PrepareError;
pub use hook_migrator::ClaudeHookMigrator;
pub use omp::OhMyPiAdapter;
pub use opencode::OpenCodeAdapter;
pub use pi::PiAdapter;
pub use session_validator::AgentSessionValidator;
pub use shell_quote::{json_string_literal, shell_quote};
pub use transcript::{ClaudeTranscriptSource, CodexTranscriptSource};

/// The result of resolving one provider's CLI on the current `PATH`.
///
/// This is deliberately based on the executable that the user can launch,
/// not on whether Tiller ships an adapter for the provider.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentAvailability {
    /// Stable provider identifier, also used as the CLI name for the built-in
    /// adapters.
    pub id: &'static str,
    /// Human-readable provider name.
    pub display_name: &'static str,
    /// The executable selected by PATH, when one was found.
    pub executable: Option<PathBuf>,
}

impl AgentAvailability {
    /// Whether the provider can be launched from this environment.
    pub fn is_available(&self) -> bool {
        self.executable.is_some()
    }

    /// A user-facing status that distinguishes a missing binary from an
    /// adapter that merely exists in the application.
    pub fn status_label(&self) -> &'static str {
        if self.is_available() {
            "Available"
        } else {
            "Not found on PATH"
        }
    }
}

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

    /// Resolves this adapter's CLI without launching it.
    fn availability(&self) -> AgentAvailability {
        AgentAvailability {
            id: self.id(),
            display_name: self.display_name(),
            executable: find_executable_on_path(self.id()),
        }
    }

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

/// Resolve a program using the process's current `PATH`.
pub fn find_executable_on_path(program: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    find_executable_in_path(program, &path)
}

/// Resolve a program against an explicit PATH value.
///
/// This public seam keeps discovery testable without changing the process's
/// environment. It only inspects filesystem metadata; it never executes the
/// candidate.
pub fn find_executable_in_path(program: &str, path: &OsStr) -> Option<PathBuf> {
    if program.is_empty() {
        return None;
    }

    let candidate = Path::new(program);
    if candidate.is_absolute() || program.contains('/') {
        return is_executable(candidate).then(|| candidate.to_path_buf());
    }

    std::env::split_paths(path)
        .map(|directory| directory.join(program))
        .find(|candidate| is_executable(candidate))
}

fn is_executable(path: &Path) -> bool {
    let Ok(metadata) = std::fs::metadata(path) else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o111 != 0
    }

    #[cfg(not(unix))]
    {
        true
    }
}

/// Discover every built-in provider in catalog order.
pub fn discover_availability() -> Vec<AgentAvailability> {
    ALL.iter().map(|adapter| adapter.availability()).collect()
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
