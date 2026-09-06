//! The adapter interface and the fixed catalog of supported agent CLIs.

use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

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
/// not on whether Sirio ships an adapter for the provider.
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

    /// The documented command that installs this provider's CLI, or `None`
    /// when no such command is known here (F-SET-18). Deliberately absent
    /// rather than guessed for an adapter this table has no entry for —
    /// the same "never fabricate a command" rule `agent_skill_install_command`
    /// and the sirioctl card's "Bundled binary" row already follow: a
    /// button that ran a made-up install line would be worse than no
    /// button at all.
    pub fn install_command(&self) -> Option<&'static str> {
        match self.id {
            "claude" => Some("npm install -g @anthropic-ai/claude-code"),
            "codex" => Some("npm install -g @openai/codex"),
            "opencode" => Some("npm install -g opencode-ai@latest"),
            _ => None,
        }
    }
}

/// A program invocation that speaks the Agent Client Protocol.
///
/// This is deliberately a static description, not a shell line: it is the
/// ACP counterpart of [`AgentAdapter::command`], which remains the
/// terminal/PTY command for launching the CLI in a pane. Only an adapter
/// whose own binary serves ACP answers with one of these; everything else
/// resolves through `sirio_registry`.
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

/// One supported agent CLI, ported from `SirioAgents.AgentAdapter`.
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
    /// `sirioctl notify`). False → Sirio watches the pane's exit code
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
        sirioctl_path: &str,
    ) -> Result<(), PrepareError>;

    /// Full shell command to run inside the pane, which the pane executes
    /// with its cwd already set to the worktree.
    fn command(&self, worktree_path: &str, pane_id: &str, sirioctl_path: &str) -> String;

    /// Full shell command that relaunches the agent resuming a previously
    /// captured native session, or `None` when the agent cannot resume by
    /// reference. `session_ref` comes from the agent session table.
    fn resume_command(
        &self,
        worktree_path: &str,
        pane_id: &str,
        sirioctl_path: &str,
        session_ref: &str,
    ) -> Option<String>;

    /// The ACP server this adapter's own CLI serves, as a subcommand of a
    /// binary the user already installed — or `None`.
    ///
    /// This claim covers only what the local binary does, so it stays true
    /// without a network round-trip. Everything reachable through a
    /// separate package resolves through `sirio_registry`, not here.
    ///
    /// The default is `None`: an adapter answers here only once its
    /// subcommand has been observed answering an ACP `initialize`.
    fn builtin_acp(&self) -> Option<AcpProgram> {
        None
    }

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

    /// The Sirio-authored skill markdown this adapter installs into the
    /// worktree during `prepare` (F-AGENT-SAFE-01), or `None` when this
    /// port installs no skill for it yet. Mirrors the `skillMarkdown`
    /// argument `AgentAdapter.prepare` takes in the Swift app — there it is
    /// supplied by the caller (a bundled resource); here each adapter that
    /// has ported the install carries its own copy so `prepare` alone is
    /// enough to exercise it.
    fn skill_markdown(&self) -> Option<&'static str> {
        None
    }

    /// Installs this agent's Sirio hooks in the user's own configuration —
    /// the one place `prepare` must never touch — so that agents the user
    /// starts by hand, in any directory, still report to Sirio. Only the
    /// explicit Settings → Install Hooks action calls this; it is never
    /// part of launching a pane.
    ///
    /// One file serves every pane, so the hooks carry no pane id: sirioctl
    /// resolves the pane from the `SIRIO_PANE_ID` each Sirio pane exports,
    /// and stays silent outside one. `environment` supplies the agent's
    /// own config-location overrides (`CLAUDE_CONFIG_DIR`, `CODEX_HOME`, …)
    /// so the hooks land where the agent actually reads.
    ///
    /// The default answers honestly for an agent with no user-global hook
    /// mechanism rather than writing a file nothing reads.
    fn install_global_hooks(
        &self,
        _home: &Path,
        _environment: &BTreeMap<String, String>,
        _sirioctl_path: &str,
    ) -> Result<GlobalHookInstall, PrepareError> {
        Ok(GlobalHookInstall::NotSupported)
    }
}

/// What [`AgentAdapter::install_global_hooks`] did for one agent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GlobalHookInstall {
    /// The user-global file the hooks now live in.
    Written(PathBuf),
    /// The agent has no user-global place for a hook (Pi has no hook
    /// mechanism; omp takes hooks only through `--hook <file>` on its
    /// command line).
    NotSupported,
}

/// One agent's outcome from [`install_global_hooks`], with the line the
/// Settings card shows for it.
#[derive(Debug)]
pub struct GlobalHookReport {
    pub display_name: &'static str,
    pub outcome: Result<GlobalHookInstall, PrepareError>,
}

impl GlobalHookReport {
    /// `<agent>: <file>`, `<agent>: no user-global hook mechanism`, or
    /// `<agent>: failed — <error>`.
    pub fn summary_line(&self) -> String {
        match &self.outcome {
            Ok(GlobalHookInstall::Written(path)) => {
                format!("{}: {}", self.display_name, path.display())
            }
            Ok(GlobalHookInstall::NotSupported) => {
                format!("{}: no user-global hook mechanism", self.display_name)
            }
            Err(error) => format!("{}: failed — {error}", self.display_name),
        }
    }
}

/// Settings → Install Hooks: runs [`AgentAdapter::install_global_hooks`]
/// for every adapter in display order and reports each one. A failure in
/// one agent does not stop the others — the user reads all five outcomes.
pub fn install_global_hooks(
    home: &Path,
    environment: &BTreeMap<String, String>,
    sirioctl_path: &str,
) -> Vec<GlobalHookReport> {
    ALL.iter()
        .map(|adapter| GlobalHookReport {
            display_name: adapter.display_name(),
            outcome: adapter.install_global_hooks(home, environment, sirioctl_path),
        })
        .collect()
}

/// Writes `contents` to `path` atomically: a unique temp file in the same
/// directory is renamed over the target, so a crash or a concurrent writer
/// can never leave a torn config file.
pub(crate) fn write_atomic(path: &Path, contents: &[u8]) -> Result<(), PrepareError> {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let tmp = path.with_file_name(format!("{name}.tmp-{}-{unique}", std::process::id()));
    std::fs::write(&tmp, contents)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

/// Installs `markdown` as `agent_id`'s Sirio skill file inside
/// `worktree_path`, refusing to overwrite a file that exists there but
/// carries no Sirio managed-file marker — ported from
/// `SirioSkillProvisioner.install` in the Swift app (F-AGENT-SAFE-01).
pub fn install_skill(
    markdown: &str,
    agent_id: &str,
    worktree_path: &str,
) -> Result<(), PrepareError> {
    if !markdown.contains(SKILL_MANAGED_MARKER) {
        return Err(PrepareError::MissingSkillMarker);
    }
    let relative = match agent_id {
        "claude" => ".claude/skills/sirio/SKILL.md",
        "codex" | "opencode" | "pi" | "omp" => ".agents/skills/sirio/SKILL.md",
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

/// The exact sentinel every skill file Sirio writes carries in its
/// content — matches `SirioSkillProvisioner.marker` verbatim.
pub const SKILL_MANAGED_MARKER: &str =
    "<!-- Machine-managed by Sirio. Do not edit this installed copy. -->";

/// Stable prefix shared by every marker sentence Sirio has ever written.
/// Recognizing an existing file by this prefix, rather than the exact
/// current [`SKILL_MANAGED_MARKER`] string, lets the wording evolve across
/// releases without locking out worktrees a previous build provisioned.
const SKILL_MANAGED_PREFIX: &str = "<!-- Machine-managed by Sirio";

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
    // A name that already carries a path is probed as-is, never searched on
    // PATH. unix knows one separator; Windows accepts `\` and `/` both, and
    // an absolute path starts with a drive letter (`C:\...`) that
    // `Path::is_absolute` already recognizes — the extra backslash check
    // catches the relative-but-pathlike form (`foo\bar`) that would
    // otherwise be misread as a bare name and joined onto PATH directories.
    let verbatim = candidate.is_absolute()
        || program.contains('/')
        || (cfg!(windows) && program.contains('\\'));

    // Every variant in probe order, dir-major when searching PATH. On
    // Windows those are the PATHEXT suffixes, mirroring how cmd.exe resolves
    // a bare name — see [`executable_candidates`] for why the bare name is
    // deliberately not among them. (`SearchPath` is a different mechanism and
    // no model for this: it appends one caller-supplied extension and never
    // reads PATHEXT.) unix has no such convention, so the enumeration is
    // exactly one element there and the behavior is unchanged.
    let variants = executable_candidates(program);

    let mut first_error: Option<DiscoveryError> = None;
    let mut probe = |candidate: PathBuf| match probe_executable(&candidate) {
        Ok(true) => Ok(Some(on_disk_spelling(candidate))),
        Ok(false) => Ok(None),
        Err(error) => {
            if first_error.is_none() {
                first_error = Some(probe_error(candidate, error));
            }
            Ok(None)
        }
    };

    if verbatim {
        for variant in &variants {
            if let Some(hit) = probe(PathBuf::from(variant.as_str()))? {
                return Ok(Some(hit));
            }
        }
    } else {
        for directory in std::env::split_paths(path) {
            for variant in &variants {
                if let Some(hit) = probe(directory.join(variant))? {
                    return Ok(Some(hit));
                }
            }
        }
    }

    match first_error {
        Some(error) => Err(error),
        None => Ok(None),
    }
}

/// The real on-disk spelling of a just-found candidate.
///
/// On Windows a case-insensitive probe can hit a file whose name is written
/// differently than the candidate that found it: `PATHEXT` spells its
/// extensions in uppercase, so `demo-agent.CMD` matches a fixture named
/// `demo-agent.cmd`. The candidate's spelling must not leak into the result
/// — the returned path is a user-visible value (settings screen, hook
/// config files), not an opaque spawn handle, so it must carry the name the
/// filesystem actually stored. Only the file-name component is fixed: PATH
/// directories keep their given spelling, exactly as they always have.
fn on_disk_spelling(candidate: PathBuf) -> PathBuf {
    #[cfg(windows)]
    {
        let Some(parent) = candidate.parent() else {
            return candidate;
        };
        let Some(wanted) = candidate.file_name() else {
            return candidate;
        };
        let Ok(entries) = std::fs::read_dir(parent) else {
            return candidate;
        };
        for entry in entries.flatten() {
            let name = entry.file_name();
            if name.eq_ignore_ascii_case(wanted) {
                return parent.join(name);
            }
        }
        candidate
    }

    #[cfg(not(windows))]
    {
        // On case-sensitive filesystems a probe hit and the disk entry are
        // the same string by construction, so the candidate passes through
        // untouched — this is the identity the unix arms relied on before.
        candidate
    }
}

/// The `PATHEXT` extension list, normalized to dotted, non-empty entries in
/// the order the variable spells them. When the variable is unset the
/// documented default stands in; entries are trimmed and dotted the way
/// cmd.exe tolerates them being written.
#[cfg(windows)]
fn pathext_entries() -> Vec<String> {
    let pathext = std::env::var("PATHEXT").unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".to_string());
    pathext
        .split(';')
        .map(str::trim)
        .filter(|extension| !extension.is_empty())
        .map(|extension| {
            if extension.starts_with('.') {
                extension.to_string()
            } else {
                format!(".{extension}")
            }
        })
        .collect()
}

/// Whether `program` already ends in one of the `PATHEXT` extensions, and is
/// therefore an explicit command name rather than a bare one. The comparison
/// is case-insensitive because Windows spells `PATHEXT` in uppercase while
/// the artifacts on disk are lowercase (`sirioctl.exe`).
#[cfg(windows)]
fn has_pathext_extension(program: &str) -> bool {
    let lowered = program.to_ascii_lowercase();
    pathext_entries()
        .iter()
        .any(|extension| lowered.ends_with(&extension.to_ascii_lowercase()))
}

/// Candidate file names for `program`, in probe order.
///
/// On Windows the bare name is deliberately **not** a candidate. `PATHEXT` is
/// the OS's own answer to "which suffixes make this a command?", and cmd.exe
/// resolves a bare name by appending those extensions — it never executes an
/// extensionless file. Probing the bare name anyway is not a harmless extra
/// try: npm lays down three files for a Node-hosted CLI (`pi`, `pi.cmd`,
/// `pi.ps1`), and the extensionless one is a `#!/bin/sh` shim for Git Bash.
/// A search that accepts it returns a path that fails at spawn time, which
/// is strictly worse than reporting the agent missing — the user sees the
/// failure when they open a tab instead of on the settings screen. `pi` and
/// `omp` are the entries seen in this shape so far, but that is a fact about
/// how they were installed, not about those two ids: every catalog CLI
/// arrives with the same triple when installed through npm, so the guard
/// belongs to the search itself rather than to a list of names.
///
/// A name that already carries a `PATHEXT` extension is explicit and passes
/// through untouched, so `sirioctl.exe` is never suffixed into
/// `sirioctl.exe.COM`.
///
/// On unix the bare name is the only candidate — there an extensionless file
/// with the execute bit set genuinely is a command — so this helper adds
/// nothing and the search semantics are unchanged.
fn executable_candidates(program: &str) -> Vec<String> {
    #[cfg(windows)]
    {
        if has_pathext_extension(program) {
            return vec![program.to_string()];
        }
        pathext_entries()
            .into_iter()
            .map(|extension| format!("{program}{extension}"))
            .collect()
    }

    #[cfg(not(windows))]
    {
        vec![program.to_string()]
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
        #[cfg(windows)]
        // Windows reports ERROR_INVALID_NAME from a malformed PATH entry as
        // InvalidFilename. That candidate is unusable, not an unknowable
        // filesystem state, so it must not poison the whole registry sweep.
        Err(error) if error.kind() == ErrorKind::InvalidFilename => return Ok(false),
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

/// The fixed list of adapters shipped with Sirio, in display order —
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
    fn only_agents_that_serve_acp_from_their_own_binary_claim_it() {
        // The claim is deliberately narrow: it means "this CLI, already on
        // the user's machine, answers ACP on a subcommand". Claude, Codex
        // and Pi reach ACP through separate packages, so they claim
        // nothing here.
        assert_eq!(
            OpenCodeAdapter.builtin_acp(),
            Some(AcpProgram::new("opencode", &["acp"])),
            "verified live 2026-08-23: opencode 1.18.21 answers initialize"
        );
        for adapter in [
            &ClaudeCodeAdapter as &dyn AgentAdapter,
            &CodexAdapter,
            &PiAdapter,
        ] {
            assert_eq!(
                adapter.builtin_acp(),
                None,
                "{} reaches ACP through a package, not a subcommand",
                adapter.id()
            );
        }
    }

    #[test]
    fn oh_my_pi_claims_nothing_until_a_real_omp_is_verified() {
        // Ship gate, not a note. The retired Swift app launched `omp acp`
        // (AgentLaunchSpec.swift:73-75), but no omp has been available to
        // confirm it against, and asserting an unverified capability is the
        // exact defect this work exists to remove.
        assert_eq!(OhMyPiAdapter.builtin_acp(), None);
    }

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
    fn no_adapter_names_a_package_it_would_have_to_download() {
        // The launch path must not contain a package name at all: pinning
        // and installing are the registry's job, and a literal here is how
        // the previous version ended up fetching from the network before a
        // chat could say anything.
        for adapter in ALL {
            if let Some(program) = adapter.builtin_acp() {
                assert_ne!(
                    program.program,
                    "npx",
                    "{} still launches npx",
                    adapter.id()
                );
                assert!(
                    !program.args.iter().any(|arg| arg.contains('@')),
                    "{} still names a package version",
                    adapter.id()
                );
            }
        }
    }

    /// F-AGENT-OPENCODE-03: the noninteractive summarizer command is
    /// `opencode run --pure '<prompt>'`, with the prompt shell-quoted the
    /// same way the Swift original quotes it.
    #[cfg(not(windows))]
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

    /// Windows form of the same command: the prompt rides one cmd.exe
    /// double-quoted token (`""` doubling for embedded quotes), the only
    /// quoting cmd understands.
    #[cfg(windows)]
    #[test]
    fn opencode_summarizer_command_is_run_pure_with_a_quoted_prompt() {
        assert_eq!(
            OpenCodeAdapter.summarizer_command("summarize this"),
            Some("opencode run --pure \"summarize this\"".to_string())
        );
        // A prompt with an embedded double quote: cmd collapses the doubled
        // pair back to one before the child parses the token.
        assert_eq!(
            OpenCodeAdapter.summarizer_command("say \"hi\""),
            Some("opencode run --pure \"say \"\"hi\"\"\"".to_string())
        );
    }

    /// F-AGENT-OMP-03: the noninteractive summarizer flags are
    /// `--print --no-tools`, and the program is `omp` — the name the
    /// Swift original always used and the `bin` that
    /// `@oh-my-pi/pi-coding-agent` ships. An earlier revision of this
    /// doc comment asserted the distribution has "no `omp` alias"; that
    /// was read off an unrelated npm package of the same name, and it is
    /// false. See `OhMyPiAdapter::executable_name` for the collision and
    /// the live evidence.
    #[cfg(not(windows))]
    #[test]
    fn omp_summarizer_command_uses_the_distribution_binary_name() {
        assert_eq!(
            OhMyPiAdapter.summarizer_command("summarize this"),
            Some("omp --print --no-tools 'summarize this'".to_string())
        );
        let command = OhMyPiAdapter
            .summarizer_command("x")
            .expect("has a summarizer");
        assert!(
            command.starts_with(OhMyPiAdapter.executable_name()),
            "the summarizer must spawn the executable name, not the adapter id: {command}"
        );
    }

    /// The Windows prompt form of the same summarizer: one cmd.exe
    /// double-quoted token.
    #[cfg(windows)]
    #[test]
    fn omp_summarizer_command_uses_the_distribution_binary_name() {
        assert_eq!(
            OhMyPiAdapter.summarizer_command("summarize this"),
            Some("omp --print --no-tools \"summarize this\"".to_string())
        );
        let command = OhMyPiAdapter
            .summarizer_command("x")
            .expect("has a summarizer");
        assert!(
            command.starts_with(OhMyPiAdapter.executable_name()),
            "the summarizer must spawn the executable name, not the adapter id: {command}"
        );
    }

    /// I1-autoname: Claude Code, Codex and Pi's summarizer invocations,
    /// ported from `ClaudeCodeAdapter.swift`/`CodexAdapter.swift`/
    /// `PiAdapter.swift`'s `summarizerCommand` — matching the Swift
    /// reference's exact argv, not a guess. The default chat tab
    /// (`agent_id: None`, default `summarizer_agent: Claude`) depends on
    /// this returning `Some` rather than falling through to the trait
    /// default.
    #[cfg(not(windows))]
    #[test]
    fn claude_codex_pi_summarizer_commands_match_the_swift_reference() {
        assert_eq!(
            ClaudeCodeAdapter.summarizer_command("p"),
            Some("claude -p 'p'".to_string())
        );
        assert_eq!(
            CodexAdapter.summarizer_command("p"),
            Some("codex exec --output-last-message /dev/stdout 'p'".to_string())
        );
        assert_eq!(
            PiAdapter.summarizer_command("p"),
            Some("pi --print --no-tools 'p'".to_string())
        );
    }

    #[test]
    fn every_adapter_supports_resuming() {
        for adapter in ALL {
            let resume =
                adapter.resume_command("/wt", "pane-1", "/usr/local/bin/sirioctl", "sess-42");
            assert!(
                resume.is_some(),
                "{} must support resuming by session reference",
                adapter.id()
            );
        }
    }
}
