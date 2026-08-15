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

    /// The ACP server program that would back a chat tab for this
    /// provider, or `None` when the agent has no ACP server.
    pub fn acp_program(&self) -> Option<AcpProgram> {
        ALL.iter()
            .find(|adapter| adapter.id() == self.id)
            .and_then(|adapter| adapter.acp_program())
    }

    /// A user-facing label for the provider's ACP chat support. An
    /// adapter without an ACP server is marked as such — never silently
    /// offered another agent's server.
    pub fn acp_status_label(&self) -> &'static str {
        if self.acp_program().is_some() {
            "ACP chat available"
        } else {
            "No ACP server"
        }
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

/// A program invocation that speaks the Agent Client Protocol.
///
/// This is deliberately a static description, not a shell line: it is the
/// ACP counterpart of [`AgentAdapter::command`], which remains the
/// terminal/PTY command for launching the CLI in a pane. An adapter whose
/// [`AgentAdapter::acp_program`] returns `None` cannot back a chat tab.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AcpProgram {
    /// Executable name or absolute path, resolved through `PATH` at spawn.
    pub program: &'static str,
    /// Arguments passed without shell parsing.
    pub args: &'static [&'static str],
}

impl AcpProgram {
    /// Builds a program invocation from a static program name and args.
    #[must_use]
    pub const fn new(program: &'static str, args: &'static [&'static str]) -> Self {
        Self { program, args }
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

    /// Executable name resolved for availability and launch discovery.
    /// Stable adapter ids may differ from the distribution's binary name.
    fn executable_name(&self) -> &'static str {
        self.id()
    }

    /// True when the agent notifies lifecycle events itself (hooks calling
    /// `tillerctl notify`). False → Tiller watches the pane's exit code
    /// instead.
    fn has_native_hooks(&self) -> bool;

    /// Resolves this adapter's CLI without launching it.
    fn availability(&self) -> AgentAvailability {
        AgentAvailability {
            id: self.id(),
            display_name: self.display_name(),
            executable: find_executable_on_path(self.executable_name()),
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

    /// The ACP server program that backs a chat tab for this adapter, or
    /// `None` when the agent has no ACP server.
    ///
    /// `None` is an honest answer: inventing a program name would fail at
    /// spawn. Consumers must mark the adapter as chat-unavailable rather
    /// than fall back to another agent's server.
    fn acp_program(&self) -> Option<AcpProgram>;

    /// Full shell command that runs the CLI noninteractively over `prompt`
    /// and prints the answer to stdout — the auto-naming summarizer's
    /// invocation, ported from `AgentAdapter.summarizerCommand`.
    ///
    /// The default is `None`: an adapter whose noninteractive mode has not
    /// been ported answers honestly rather than guessing an argv. Consumers
    /// must skip summarization for such an adapter, never substitute
    /// another agent's command.
    fn summarizer_command(&self, _prompt: &str) -> Option<String> {
        None
    }

    /// The Tiller-authored skill markdown this adapter installs into the
    /// worktree during `prepare` (F-AGENT-SAFE-01), or `None` when this
    /// port installs no skill for it yet. Mirrors the `skillMarkdown`
    /// argument `AgentAdapter.prepare` takes in the Swift app — there it is
    /// supplied by the caller (a bundled resource); here each adapter that
    /// has ported the install carries its own copy so `prepare` alone is
    /// enough to exercise it.
    fn skill_markdown(&self) -> Option<&'static str> {
        None
    }
}

/// Installs `markdown` as `agent_id`'s Tiller skill file inside
/// `worktree_path`, refusing to overwrite a file that exists there but
/// carries no Tiller managed-file marker — ported from
/// `TillerSkillProvisioner.install` in the Swift app (F-AGENT-SAFE-01).
pub fn install_skill(
    markdown: &str,
    agent_id: &str,
    worktree_path: &str,
) -> Result<(), PrepareError> {
    if !markdown.contains(SKILL_MANAGED_MARKER) {
        return Err(PrepareError::MissingSkillMarker);
    }
    let relative = match agent_id {
        "claude" => ".claude/skills/tiller/SKILL.md",
        "codex" | "opencode" | "pi" | "omp" => ".agents/skills/tiller/SKILL.md",
        other => return Err(PrepareError::UnsupportedSkillAgent(other.to_string())),
    };
    let destination = Path::new(worktree_path).join(relative);
    if destination.exists() {
        let existing = std::fs::read_to_string(&destination)?;
        if !existing.contains(SKILL_MANAGED_PREFIX) {
            return Err(PrepareError::UnmanagedSkillFile(destination));
        }
    }
    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&destination, markdown)?;
    Ok(())
}

/// The exact sentinel every skill file Tiller writes carries in its
/// content — matches `TillerSkillProvisioner.marker` verbatim.
pub const SKILL_MANAGED_MARKER: &str =
    "<!-- Machine-managed by Tiller. Do not edit this installed copy. -->";

/// Stable prefix shared by every marker sentence Tiller has ever written.
/// Recognizing an existing file by this prefix, rather than the exact
/// current [`SKILL_MANAGED_MARKER`] string, lets the wording evolve across
/// releases without locking out worktrees a previous build provisioned.
const SKILL_MANAGED_PREFIX: &str = "<!-- Machine-managed by Tiller";

/// Why an availability sweep could not produce an answer (F-SET-17).
///
/// Discovery distinguishes two negatives that the infallible API collapses:
/// "the binary is not on PATH" (a clean [`None`]) and "I could not check"
/// (this error). Only the first may be rendered as *Not found on PATH*;
/// the second is the registry error state the settings screen surfaces
/// with a retry.
#[derive(Debug)]
pub enum DiscoveryError {
    /// `PATH` is not set at all, so no probe can even start.
    PathUnset,
    /// A candidate path could not be probed (for example `EACCES` on a
    /// PATH directory) *and* the program was not found anywhere else, so
    /// claiming absence would be a guess.
    Probe {
        /// The executable name being resolved.
        program: String,
        /// The candidate path whose probe failed.
        path: PathBuf,
        /// The underlying filesystem error.
        source: std::io::Error,
    },
}

impl std::fmt::Display for DiscoveryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PathUnset => write!(f, "PATH is not set in the environment"),
            Self::Probe {
                program,
                path,
                source,
            } => write!(
                f,
                "could not probe {} for {program}: {source}",
                path.display()
            ),
        }
    }
}

impl std::error::Error for DiscoveryError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::PathUnset => None,
            Self::Probe { source, .. } => Some(source),
        }
    }
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
///
/// This is the infallible view: a probe failure collapses into [`None`].
/// Callers that must distinguish *absent* from *unknowable* use
/// [`find_executable_in_path_checked`].
pub fn find_executable_in_path(program: &str, path: &OsStr) -> Option<PathBuf> {
    find_executable_in_path_checked(program, path)
        .ok()
        .flatten()
}

/// Resolve a program against an explicit PATH value, keeping probe
/// failures observable (F-SET-17).
///
/// A hit anywhere on PATH wins over an earlier probe error — an unreadable
/// directory is irrelevant once the binary is found elsewhere. The error is
/// returned only when it makes the absence claim unsafe: the program was
/// found nowhere *and* at least one candidate could not be checked. A clean
/// miss is `Ok(None)`.
pub fn find_executable_in_path_checked(
    program: &str,
    path: &OsStr,
) -> Result<Option<PathBuf>, DiscoveryError> {
    if program.is_empty() {
        return Ok(None);
    }

    let probe_error = |path: PathBuf, source: std::io::Error| DiscoveryError::Probe {
        program: program.to_string(),
        path,
        source,
    };

    let candidate = Path::new(program);
    if candidate.is_absolute() || program.contains('/') {
        return match probe_executable(candidate) {
            Ok(true) => Ok(Some(candidate.to_path_buf())),
            Ok(false) => Ok(None),
            Err(error) => Err(probe_error(candidate.to_path_buf(), error)),
        };
    }

    let mut first_error: Option<DiscoveryError> = None;
    for candidate in std::env::split_paths(path).map(|directory| directory.join(program)) {
        match probe_executable(&candidate) {
            Ok(true) => return Ok(Some(candidate)),
            Ok(false) => {}
            Err(error) if first_error.is_none() => {
                first_error = Some(probe_error(candidate, error));
            }
            Err(_) => {}
        }
    }
    match first_error {
        Some(error) => Err(error),
        None => Ok(None),
    }
}

/// Whether `path` names an executable regular file. A missing file (or a
/// PATH component that is not a directory) is a clean `false`; any other
/// filesystem error is surfaced so the caller can tell *absent* from
/// *unknowable*.
fn probe_executable(path: &Path) -> Result<bool, std::io::Error> {
    use std::io::ErrorKind;

    let metadata = match std::fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if matches!(error.kind(), ErrorKind::NotFound | ErrorKind::NotADirectory) => {
            return Ok(false);
        }
        Err(error) => return Err(error),
    };
    if !metadata.is_file() {
        return Ok(false);
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        Ok(metadata.permissions().mode() & 0o111 != 0)
    }

    #[cfg(not(unix))]
    {
        Ok(true)
    }
}

/// Discover every built-in provider in catalog order.
///
/// This is the infallible view consumed by surfaces without an error state
/// (the tab bar): a sweep that could not be completed reports the affected
/// providers as unavailable. The settings registry uses
/// [`try_discover_availability`] so it can render the failure instead.
pub fn discover_availability() -> Vec<AgentAvailability> {
    ALL.iter().map(|adapter| adapter.availability()).collect()
}

/// Discover every built-in provider in catalog order, failing the sweep
/// when any provider's absence would be a guess (F-SET-17).
///
/// One error fails the whole sweep — mirroring the Swift registry, which
/// shows a single banner over the previous rows rather than per-row
/// partial results.
pub fn try_discover_availability() -> Result<Vec<AgentAvailability>, DiscoveryError> {
    let path = std::env::var_os("PATH").ok_or(DiscoveryError::PathUnset)?;
    try_discover_availability_in(&path)
}

/// [`try_discover_availability`] against an explicit PATH value — the
/// testable seam, like [`find_executable_in_path`].
pub fn try_discover_availability_in(
    path: &OsStr,
) -> Result<Vec<AgentAvailability>, DiscoveryError> {
    ALL.iter()
        .map(|adapter| {
            Ok(AgentAvailability {
                id: adapter.id(),
                display_name: adapter.display_name(),
                executable: find_executable_in_path_checked(adapter.executable_name(), path)?,
            })
        })
        .collect()
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
    fn acp_program_differs_by_adapter_with_an_honest_none() {
        // Claude and Codex ship ACP servers and their programs are exact
        // and distinct. OpenCode, Pi and Oh-My-Pi have no ACP server — the
        // honest answer is `None`, never another agent's program.
        assert_eq!(
            ClaudeCodeAdapter.acp_program(),
            Some(AcpProgram::new(
                "npx",
                &["-y", "@agentclientprotocol/claude-agent-acp@latest"]
            ))
        );
        assert_eq!(
            CodexAdapter.acp_program(),
            Some(AcpProgram::new(
                "npx",
                &["-y", "@agentclientprotocol/codex-acp@latest"]
            ))
        );
        let terminal_only: [&dyn AgentAdapter; 3] = [&OpenCodeAdapter, &PiAdapter, &OhMyPiAdapter];
        for adapter in terminal_only {
            assert_eq!(
                adapter.acp_program(),
                None,
                "{} has no ACP server; inventing a program name would fail at spawn",
                adapter.id()
            );
        }
    }

    #[test]
    fn availability_surface_reports_acp_support_without_a_fallback() {
        // The picker consumes `AgentAvailability`; it must be able to tell
        // an ACP-backed agent from a terminal-only one, and a `None`
        // adapter must be labelled as such — never silently pointed at
        // Claude's server.
        let codex = CodexAdapter.availability();
        assert!(codex.acp_program().is_some());
        assert_eq!(codex.acp_status_label(), "ACP chat available");

        let opencode = OpenCodeAdapter.availability();
        assert_eq!(opencode.acp_program(), None);
        assert_eq!(
            opencode.acp_status_label(),
            "No ACP server",
            "the surface says there is no ACP server rather than falling back"
        );
    }

    /// F-AGENT-OPENCODE-03: the noninteractive summarizer command is
    /// `opencode run --pure '<prompt>'`, with the prompt shell-quoted the
    /// same way the Swift original quotes it.
    #[test]
    fn opencode_summarizer_command_is_run_pure_with_a_quoted_prompt() {
        assert_eq!(
            OpenCodeAdapter.summarizer_command("summarize this"),
            Some("opencode run --pure 'summarize this'".to_string())
        );
        // The Swift test's own edge case: an embedded single quote must
        // survive the shell round trip.
        assert_eq!(
            OpenCodeAdapter.summarizer_command("it's a test"),
            Some("opencode run --pure 'it'\\''s a test'".to_string())
        );
    }

    /// F-AGENT-OMP-03: the noninteractive summarizer flags are
    /// `--print --no-tools`. The Swift original spells the program `omp`,
    /// but this port launches adapters by `executable_name()` — the binary
    /// the distribution actually ships is `oh-my-pi`, with no `omp` alias
    /// (settled for the launch/resume commands; the summarizer follows the
    /// same discipline rather than generating a command no PATH can
    /// resolve).
    #[test]
    fn omp_summarizer_command_uses_the_distribution_binary_name() {
        assert_eq!(
            OhMyPiAdapter.summarizer_command("summarize this"),
            Some("oh-my-pi --print --no-tools 'summarize this'".to_string())
        );
        let command = OhMyPiAdapter.summarizer_command("x").expect("has a summarizer");
        assert!(
            command.starts_with(OhMyPiAdapter.executable_name()),
            "the summarizer must spawn the executable name, not the adapter id: {command}"
        );
    }

    /// The other three adapters' summarizers are their own inventory rows
    /// and are not yet ported; until they are, the honest answer is `None`
    /// — never a guessed argv for a CLI whose contract nobody checked.
    #[test]
    fn unported_summarizers_answer_none_rather_than_guessing() {
        assert_eq!(ClaudeCodeAdapter.summarizer_command("p"), None);
        assert_eq!(CodexAdapter.summarizer_command("p"), None);
        assert_eq!(PiAdapter.summarizer_command("p"), None);
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
