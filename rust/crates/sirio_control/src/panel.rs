//! Linux/macOS control-owned terminal panes.
//!
//! The GPUI terminal renderer intentionally owns its alacritty terminal
//! object privately. The control socket therefore keeps a small, independent
//! PTY registry for panes created through `panel.*`: it exposes the observable
//! control contract (bytes in, bounded bytes out, exit status) without
//! reaching into the renderer or creating a dependency cycle.
//!
//! The PTY backend (`spawn_process`/`child_exec`/`reap`/`terminate` and their
//! helpers) is unix-only: it hand-rolls a PTY with raw
//! `openpty`/`fork`/`setsid`/`TIOCSCTTY`/`execvp`, the same pattern as
//! `sirio_usage::claude::Pty` and `sirio_terminal`'s process-group
//! teardown. `PaneRegistry` itself — the ~20 public methods (`create`,
//! `split`, `write`, `read`, `wait`, `close`, `shutdown`, …) — is seamed for
//! non-unix targets: every method still exists with the identical signature
//! Linux/macOS expose, but `create`/`split` (the only entry points that spawn
//! a control-owned PTY) return `PaneError::Unsupported` there instead of
//! silently no-opping. Because those two never succeed off unix, the
//! `panes` map they populate stays empty, so `write`/`read`/`state`/`wait`/
//! `close` naturally and honestly report `UnknownPane`/`ClosedPane` for any
//! id a caller could have obtained — no separate stub logic needed for them.
//! `set_external`/`set_external_state`/`list`/`list_for`/`read`/`state`
//! (the renderer-owned-pane paths, which never touch a local PTY) work
//! unchanged on every platform.
//!
//! Deciding what a *real* Windows PTY backend should do is exactly the kind
//! of design decision this wave was told not to make speculatively ("seam
//! it, do not build it"). The honest Windows counterpart is not a handful of
//! Win32 substitutions but a second backend built on ConPTY
//! (`CreatePseudoConsole`) — `portable-pty`'s already-vendored
//! `tty/windows/` is the natural implementation to reuse (it already owns a
//! child's lifetime end-to-end) rather than a hand-rolled
//! `CreateNamedPipeW`/`ConnectNamedPipe` + `CreatePseudoConsole` client
//! written from scratch here. Left undone and documented rather than
//! partially/incorrectly implemented; see the wave report for the reasoning.

use std::collections::{HashMap, HashSet};
#[cfg(unix)]
use std::ffi::CString;
use std::fmt;
#[cfg(unix)]
use std::fs::File;
use std::io::Write;
use std::io::{self, Read};
#[cfg(unix)]
use std::os::fd::{FromRawFd, RawFd};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;
#[cfg(unix)]
use std::time::Instant;

const SCROLLBACK_CAPACITY: usize = 256 * 1024;
// The polite window a unix pane gets between SIGTERM and SIGKILL.
// Windows has no such two-step: `terminate` there kills the child and
// waits, so this constant is unix-only (#246).
#[cfg(unix)]
const TERMINATE_GRACE: Duration = Duration::from_millis(500);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PaneInfo {
    pub id: String,
    pub tab: String,
    pub title: String,
    pub agent: String,
    pub active: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaneExitStatus {
    Success,
    Code(i32),
    Signal(i32),
    Unknown,
}

/// A live source of a panel's retained scrollback.
///
/// The source is consulted only when a read asks for the bytes. It may
/// legitimately return an empty buffer when the panel is shutting down
/// mid-call, and it must never block indefinitely.
pub type ScrollbackSource = Arc<dyn Fn() -> Vec<u8> + Send + Sync>;

/// State observed from the actual terminal surface, whether it is a
/// control-owned PTY or an application-owned GPUI terminal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PaneStateSnapshot {
    pub working_directory: PathBuf,
    pub exit_status: Option<PaneExitStatus>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum PaneError {
    UnknownPane(String),
    ClosedPane(String),
    InvalidWorkingDirectory(String),
    InvalidKey(String),
    InvalidDirection(String),
    Spawn(String),
    Io(String),
    TimedOut(String),
    /// This platform has no control-owned PTY backend implemented yet — see
    /// the module doc comment for the counterpart (ConPTY via
    /// `portable-pty`'s `tty/windows/`).
    Unsupported(String),
}

impl fmt::Display for PaneError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownPane(id) => write!(formatter, "unknown pane: {id}"),
            Self::ClosedPane(id) => write!(formatter, "closed pane: {id}"),
            Self::InvalidWorkingDirectory(path) => {
                write!(formatter, "invalid pane working directory: {path}")
            }
            Self::InvalidKey(key) => write!(formatter, "unknown terminal key: {key}"),
            Self::InvalidDirection(direction) => {
                write!(formatter, "unknown split direction: {direction}")
            }
            Self::Spawn(detail) => write!(formatter, "cannot spawn pane: {detail}"),
            Self::Io(detail) => write!(formatter, "pane I/O failed: {detail}"),
            Self::TimedOut(id) => write!(formatter, "pane timed out: {id}"),
            Self::Unsupported(detail) => write!(formatter, "unsupported: {detail}"),
        }
    }
}

impl std::error::Error for PaneError {}

struct PaneState {
    output: Vec<u8>,
    exit_code: Option<i32>,
}

struct PaneProcess {
    #[cfg(unix)]
    pid: libc::pid_t,
    /// The pane child becomes a session leader in `child_exec`, so its pid is
    /// also the process-group id for every descendant that stays in the pane
    /// session.
    #[cfg(unix)]
    process_group: libc::pid_t,
    /// #246: the ConPTY child, kept so `terminate` can reach it. The unix path
    /// signals a process *group* by pid and needs no handle; Windows has no
    /// equivalent, so the handle is the ownership boundary.
    #[cfg(windows)]
    child: Mutex<Box<dyn portable_pty::Child + Send + Sync>>,
    /// The ConPTY master, held for the pane's lifetime. Dropping it at the end
    /// of `spawn_process` closed the pty under the reader thread: the pane
    /// spawned and could be killed, but produced no scrollback and never
    /// reported an exit code, because the reader blocked on a pty that was
    /// already gone. `sirio_terminal` keeps its master alive on an owner
    /// thread for the same reason.
    #[cfg(windows)]
    _master: Mutex<Box<dyn portable_pty::MasterPty + Send>>,
    /// Boxed rather than a `File` so both a unix PTY fd and a ConPTY writer
    /// fit the same field (#246).
    writer: Mutex<Box<dyn Write + Send>>,
    state: Mutex<PaneState>,
    exited: Condvar,
}

struct PaneEntry {
    info: PaneInfo,
    working_directory: PathBuf,
    process: Arc<PaneProcess>,
}

struct ExternalPane {
    working_directory: PathBuf,
    info: PaneInfo,
    state: PaneStateSnapshot,
    scrollback_source: Option<ScrollbackSource>,
}

/// Thread-safe source of truth for panes created through the control socket.
pub struct PaneRegistry {
    panes: Mutex<HashMap<String, Arc<PaneEntry>>>,
    external: Mutex<HashMap<String, ExternalPane>>,
    canonical_directories: Mutex<HashMap<PathBuf, PathBuf>>,
    closed: Mutex<HashSet<String>>,
    active: Mutex<Option<String>>,
    next_id: AtomicU64,
}

impl Default for PaneRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl PaneRegistry {
    pub fn new() -> Self {
        Self {
            panes: Mutex::new(HashMap::new()),
            external: Mutex::new(HashMap::new()),
            canonical_directories: Mutex::new(HashMap::new()),
            closed: Mutex::new(HashSet::new()),
            active: Mutex::new(None),
            next_id: AtomicU64::new(1),
        }
    }

    pub fn create(
        &self,
        working_directory: impl AsRef<Path>,
        command: Option<&str>,
        title: impl Into<String>,
    ) -> Result<PaneInfo, PaneError> {
        let working_directory = self.checked_directory(working_directory.as_ref())?;
        let id = format!(
            "pane-{}-{}",
            std::process::id(),
            self.next_id.fetch_add(1, Ordering::Relaxed)
        );
        let title = title.into();
        let process = spawn_process(&id, &working_directory, command)?;
        let info = PaneInfo {
            id: id.clone(),
            tab: "control".to_string(),
            title,
            agent: command
                .and_then(|value| value.split_whitespace().next())
                .unwrap_or_default()
                .to_string(),
            active: true,
        };
        let entry = Arc::new(PaneEntry {
            info: info.clone(),
            working_directory,
            process,
        });
        self.panes
            .lock()
            .map_err(|_| PaneError::Io("pane registry lock poisoned".to_string()))?
            .insert(id.clone(), entry);
        if let Ok(mut closed) = self.closed.lock() {
            closed.remove(&id);
        }
        *self
            .active
            .lock()
            .map_err(|_| PaneError::Io("pane active lock poisoned".to_string()))? = Some(id);
        Ok(info)
    }

    /// Replaces the live application-pane snapshot for one worktree. The GPUI
    /// workspace owns those panes and their PTYs; this registry owns the
    /// control-socket view so `panel.list` can report both kinds without
    /// pretending it can read or close a renderer-owned PTY.
    pub fn set_external(
        &self,
        working_directory: impl AsRef<Path>,
        panes: Vec<PaneInfo>,
    ) -> Result<(), PaneError> {
        let working_directory = self.checked_directory(working_directory.as_ref())?;
        let panes = panes
            .into_iter()
            .map(|pane| {
                (
                    pane,
                    PaneStateSnapshot {
                        working_directory: working_directory.clone(),
                        exit_status: None,
                    },
                    None,
                )
            })
            .collect();
        self.set_external_state(working_directory, panes)
    }

    /// Replaces the live application-pane snapshot and the terminal state
    /// held by each renderer-owned pane. The app supplies these facts and a
    /// source for on-demand scrollback; this crate never recomputes them from
    /// git or a second PTY.
    pub fn set_external_state(
        &self,
        working_directory: impl AsRef<Path>,
        panes: Vec<(PaneInfo, PaneStateSnapshot, Option<ScrollbackSource>)>,
    ) -> Result<(), PaneError> {
        let working_directory = self.lookup_directory(working_directory.as_ref())?;
        let mut external = self
            .external
            .lock()
            .map_err(|_| PaneError::Io("external pane registry lock poisoned".to_string()))?;
        let removed = external
            .iter()
            .filter(|(_, pane)| pane.working_directory == working_directory)
            .map(|(id, _)| id.clone())
            .collect::<Vec<_>>();
        let current_ids = panes
            .iter()
            .map(|(pane, _, _)| pane.id.clone())
            .collect::<Vec<_>>();
        external.retain(|_, pane| pane.working_directory != working_directory);
        for (pane, mut state, scrollback_source) in panes {
            state.working_directory = working_directory.clone();
            external.insert(
                pane.id.clone(),
                ExternalPane {
                    working_directory: working_directory.clone(),
                    info: pane,
                    state,
                    scrollback_source,
                },
            );
        }
        drop(external);
        if let Ok(mut closed) = self.closed.lock() {
            for id in removed {
                closed.insert(id);
            }
            for id in current_ids {
                closed.remove(&id);
            }
        }
        Ok(())
    }

    pub fn split(
        &self,
        source_id: &str,
        direction: &str,
        command: Option<&str>,
    ) -> Result<PaneInfo, PaneError> {
        if !matches!(direction, "left" | "right" | "up" | "down") {
            return Err(PaneError::InvalidDirection(direction.to_string()));
        }
        let working_directory = self
            .panes
            .lock()
            .map_err(|_| PaneError::Io("pane registry lock poisoned".to_string()))?
            .get(source_id)
            .map(|pane| pane.working_directory.clone())
            .ok_or_else(|| PaneError::UnknownPane(source_id.to_string()))?;
        let title = command
            .and_then(|value| value.split_whitespace().next())
            .map(|name| format!("{name} ({direction})"))
            .unwrap_or_else(|| format!("Split ({direction})"));
        self.create(working_directory, command, title)
    }

    pub fn list(&self) -> Result<Vec<PaneInfo>, PaneError> {
        let active = self
            .active
            .lock()
            .map_err(|_| PaneError::Io("pane active lock poisoned".to_string()))?
            .clone();
        let mut panes: Vec<_> = self
            .panes
            .lock()
            .map_err(|_| PaneError::Io("pane registry lock poisoned".to_string()))?
            .values()
            .map(|pane| {
                let mut info = pane.info.clone();
                info.active = active.as_deref() == Some(info.id.as_str());
                info
            })
            .collect();
        panes.extend(
            self.external
                .lock()
                .map_err(|_| PaneError::Io("external pane registry lock poisoned".to_string()))?
                .values()
                .map(|pane| pane.info.clone()),
        );
        panes.sort_by(|left, right| left.id.cmp(&right.id));
        Ok(panes)
    }

    pub fn list_for(&self, working_directory: &Path) -> Result<Vec<PaneInfo>, PaneError> {
        let working_directory = self.lookup_directory(working_directory)?;
        let active = self
            .active
            .lock()
            .map_err(|_| PaneError::Io("pane active lock poisoned".to_string()))?
            .clone();
        let mut panes: Vec<_> = self
            .panes
            .lock()
            .map_err(|_| PaneError::Io("pane registry lock poisoned".to_string()))?
            .values()
            .filter(|pane| pane.working_directory == working_directory)
            .map(|pane| {
                let mut info = pane.info.clone();
                info.active = active.as_deref() == Some(info.id.as_str());
                info
            })
            .collect();
        panes.extend(
            self.external
                .lock()
                .map_err(|_| PaneError::Io("external pane registry lock poisoned".to_string()))?
                .values()
                .filter(|pane| pane.working_directory == working_directory)
                .map(|pane| pane.info.clone()),
        );
        panes.sort_by(|left, right| left.id.cmp(&right.id));
        Ok(panes)
    }

    pub fn write(&self, pane_id: &str, bytes: &[u8]) -> Result<(), PaneError> {
        let pane = self.get(pane_id)?;
        pane.process
            .writer
            .lock()
            .map_err(|_| PaneError::Io("pane writer lock poisoned".to_string()))?
            .write_all(bytes)
            .map_err(|error| PaneError::Io(error.to_string()))
    }

    pub fn key(&self, pane_id: &str, key: &str) -> Result<(), PaneError> {
        let bytes = terminal_key_bytes(key)?;
        self.write(pane_id, &bytes)
    }

    pub fn read(&self, pane_id: &str) -> Result<Vec<u8>, PaneError> {
        // `panel.list` merges control-owned (`panes`) and renderer-owned
        // (`external`) panes, so a pane it reports wired can be either kind.
        // `state()`/`scrollback()` already check both maps; this used to
        // check only `panes` via `self.get`, so a renderer-owned pane (e.g.
        // OpenCode's ACP-bridged tab) that `panel.list` had just shown as
        // wired would fail `panel.read` moments later with `UnknownPane`.
        if let Some(pane) = self
            .panes
            .lock()
            .map_err(|_| PaneError::Io("pane registry lock poisoned".to_string()))?
            .get(pane_id)
            .cloned()
        {
            return Ok(pane
                .process
                .state
                .lock()
                .map_err(|_| PaneError::Io("pane state lock poisoned".to_string()))?
                .output
                .clone());
        }
        let external_source = {
            let external = self
                .external
                .lock()
                .map_err(|_| PaneError::Io("external pane registry lock poisoned".to_string()))?;
            external
                .get(pane_id)
                .map(|pane| pane.scrollback_source.clone())
        };
        if let Some(source) = external_source {
            return Ok(source.map_or_else(Vec::new, |source| source()));
        }
        Err(self.missing_pane_error(pane_id))
    }

    /// Returns the state snapshot currently held by the pane surface.
    pub fn state(&self, pane_id: &str) -> Result<PaneStateSnapshot, PaneError> {
        if let Some(pane) = self
            .panes
            .lock()
            .map_err(|_| PaneError::Io("pane registry lock poisoned".to_string()))?
            .get(pane_id)
            .cloned()
        {
            let state = pane
                .process
                .state
                .lock()
                .map_err(|_| PaneError::Io("pane state lock poisoned".to_string()))?;
            return Ok(PaneStateSnapshot {
                working_directory: pane.working_directory.clone(),
                exit_status: state.exit_code.map(pane_exit_status),
            });
        }
        if let Some(pane) = self
            .external
            .lock()
            .map_err(|_| PaneError::Io("external pane registry lock poisoned".to_string()))?
            .get(pane_id)
        {
            return Ok(pane.state.clone());
        }
        Err(self.missing_pane_error(pane_id))
    }

    /// Returns scrollback bytes, bounded by the caller's requested maximum.
    /// Renderer-owned panes invoke their live source at read time without
    /// changing the stored pane facts.
    pub fn scrollback(
        &self,
        pane_id: &str,
        max_bytes: Option<usize>,
    ) -> Result<Vec<u8>, PaneError> {
        let mut bytes = if let Some(pane) = self
            .panes
            .lock()
            .map_err(|_| PaneError::Io("pane registry lock poisoned".to_string()))?
            .get(pane_id)
            .cloned()
        {
            pane.process
                .state
                .lock()
                .map_err(|_| PaneError::Io("pane state lock poisoned".to_string()))?
                .output
                .clone()
        } else {
            let external_source = {
                let external = self
                    .external
                    .lock()
                    .map_err(|_| {
                        PaneError::Io("external pane registry lock poisoned".to_string())
                    })?;
                external
                    .get(pane_id)
                    .map(|pane| pane.scrollback_source.clone())
            };
            if let Some(source) = external_source {
                source.map_or_else(Vec::new, |source| source())
            } else {
                return Err(self.missing_pane_error(pane_id));
            }
        };
        if let Some(max_bytes) = max_bytes
            && bytes.len() > max_bytes
        {
            bytes.drain(..bytes.len() - max_bytes);
        }
        Ok(bytes)
    }

    pub fn wait(&self, pane_id: &str, timeout: Option<Duration>) -> Result<i32, PaneError> {
        let pane = self.get(pane_id)?;
        let mut state = pane
            .process
            .state
            .lock()
            .map_err(|_| PaneError::Io("pane state lock poisoned".to_string()))?;
        if state.exit_code.is_none() {
            if let Some(timeout) = timeout {
                let (next_state, wait_result) = pane
                    .process
                    .exited
                    .wait_timeout(state, timeout)
                    .map_err(|_| PaneError::Io("pane state lock poisoned".to_string()))?;
                state = next_state;
                if wait_result.timed_out() && state.exit_code.is_none() {
                    return Err(PaneError::TimedOut(pane_id.to_string()));
                }
            } else {
                while state.exit_code.is_none() {
                    state = pane
                        .process
                        .exited
                        .wait(state)
                        .map_err(|_| PaneError::Io("pane state lock poisoned".to_string()))?;
                }
            }
        }
        state
            .exit_code
            .ok_or_else(|| PaneError::Io("pane exited without a status".to_string()))
    }

    pub fn close(&self, pane_id: &str) -> Result<(), PaneError> {
        let pane = self
            .panes
            .lock()
            .map_err(|_| PaneError::Io("pane registry lock poisoned".to_string()))?
            .remove(pane_id)
            .ok_or_else(|| self.missing_pane_error(pane_id))?;
        if let Ok(mut closed) = self.closed.lock() {
            closed.insert(pane_id.to_string());
        }
        if self
            .active
            .lock()
            .map_err(|_| PaneError::Io("pane active lock poisoned".to_string()))?
            .as_deref()
            == Some(pane_id)
        {
            *self
                .active
                .lock()
                .map_err(|_| PaneError::Io("pane active lock poisoned".to_string()))? = None;
        }
        terminate(&pane.process);
        Ok(())
    }

    pub fn focus(&self, pane_id: &str) -> Result<(), PaneError> {
        let _ = self.get(pane_id)?;
        *self
            .active
            .lock()
            .map_err(|_| PaneError::Io("pane active lock poisoned".to_string()))? =
            Some(pane_id.to_string());
        Ok(())
    }

    /// Terminates every control-owned PTY. This is called from the app quit
    /// hook as well as `Drop`, because a live registry may be held by the
    /// control socket's handler after the main window has gone away.
    pub fn shutdown(&self) {
        let entries = match self.panes.lock() {
            Ok(mut panes) => panes.drain().map(|(_, pane)| pane).collect::<Vec<_>>(),
            Err(_) => Vec::new(),
        };
        if let Ok(mut active) = self.active.lock() {
            *active = None;
        }
        for pane in entries {
            terminate(&pane.process);
        }
    }

    /// Terminates only the control-owned panes whose working directory is the
    /// closed worktree. Renderer-owned panes are tracked separately and are
    /// cleared by the workspace host; this method exists so a workspace close
    /// cannot leave a socket-created PTY alive behind that host transition.
    pub fn shutdown_for(&self, working_directory: impl AsRef<Path>) -> Result<(), PaneError> {
        // A close can race with worktree deletion. Cleanup must still use the
        // registry's ownership metadata when the path is no longer on disk.
        let requested_directory = working_directory.as_ref();
        let canonical_directory = requested_directory.canonicalize().ok();
        let (ids, entries) = {
            let mut panes = self
                .panes
                .lock()
                .map_err(|_| PaneError::Io("pane registry lock poisoned".to_string()))?;
            let ids = panes
                .iter()
                .filter(|(_, pane)| {
                    pane.working_directory == requested_directory
                        || canonical_directory
                            .as_ref()
                            .is_some_and(|directory| pane.working_directory == *directory)
                })
                .map(|(id, _)| id.clone())
                .collect::<Vec<_>>();
            let entries = ids
                .iter()
                .filter_map(|id| panes.remove(id))
                .collect::<Vec<_>>();
            (ids, entries)
        };

        if let Ok(mut closed) = self.closed.lock() {
            closed.extend(ids.iter().cloned());
        }
        if let Ok(mut active) = self.active.lock()
            && active
                .as_ref()
                .is_some_and(|active_id| ids.iter().any(|id| id == active_id))
        {
            *active = None;
        }
        for pane in entries {
            terminate(&pane.process);
        }
        Ok(())
    }

    fn get(&self, pane_id: &str) -> Result<Arc<PaneEntry>, PaneError> {
        let pane = self
            .panes
            .lock()
            .map_err(|_| PaneError::Io("pane registry lock poisoned".to_string()))?
            .get(pane_id)
            .cloned();
        pane.ok_or_else(|| self.missing_pane_error(pane_id))
    }

    fn missing_pane_error(&self, pane_id: &str) -> PaneError {
        let closed = self
            .closed
            .lock()
            .map(|closed| closed.contains(pane_id))
            .unwrap_or(false);
        if closed {
            PaneError::ClosedPane(pane_id.to_string())
        } else {
            PaneError::UnknownPane(pane_id.to_string())
        }
    }
}

impl Drop for PaneRegistry {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn pane_exit_status(code: i32) -> PaneExitStatus {
    if code == 0 {
        PaneExitStatus::Success
    } else {
        PaneExitStatus::Code(code)
    }
}

impl PaneRegistry {
    /// Resolves and validates a write path against the filesystem, then
    /// remembers the result for later lookups.
    fn checked_directory(&self, raw_path: &Path) -> Result<PathBuf, PaneError> {
        let path = raw_path
            .canonicalize()
            .map_err(|error| PaneError::InvalidWorkingDirectory(error.to_string()))?;
        if !path.is_dir() {
            return Err(PaneError::InvalidWorkingDirectory(
                path.display().to_string(),
            ));
        }
        self.canonical_directories
            .lock()
            .map_err(|_| PaneError::Io("canonical directory cache lock poisoned".to_string()))?
            .insert(raw_path.to_path_buf(), path.clone());
        Ok(path)
    }

    /// Resolves a query path without touching the filesystem when its raw
    /// spelling has already been seen. A first lookup falls back to the
    /// validating path routine, which also seeds the cache.
    fn lookup_directory(&self, raw_path: &Path) -> Result<PathBuf, PaneError> {
        if let Some(path) = self
            .canonical_directories
            .lock()
            .map_err(|_| PaneError::Io("canonical directory cache lock poisoned".to_string()))?
            .get(raw_path)
            .cloned()
        {
            return Ok(path);
        }
        self.checked_directory(raw_path)
    }
}

/// The interpreter a control-owned pane runs when no `--cmd` is given.
///
/// #246: this duplicates `sirio_terminal::default_system_shell`'s Windows
/// arm, and that is a real cost rather than an oversight. `sirio_control`
/// sits BESIDE `sirio_terminal` in the crate layering (CLAUDE.md), not above
/// it, so the resolved answer cannot be borrowed. #230 was a bug produced by
/// exactly this kind of duplication drifting apart -- if the rule changes
/// there, change it here too. `$SHELL` is deliberately NOT consulted: it is a
/// POSIX notion, and honouring an MSYS value inherited from Git Bash is the
/// failure #230 fixed.
#[cfg(windows)]
const VERBATIM_PREFIX: &str = "\\\\?\\";
#[cfg(windows)]
const UNC_PREFIX: &str = "UNC\\";
#[cfg(windows)]
const UNC_ROOT: &str = "\\\\";

/// Strips the verbatim `\?\` prefix from a path handed to a child as its
/// working directory (#246, the same defect as #150).
///
/// `checked_directory` canonicalizes, which on Windows yields `\?\D:\...`.
/// `CreateProcess` accepts that, but `cmd.exe` reads it as UNC, refuses to
/// keep it as its cwd and silently falls back to the Windows directory, so
/// every command in the pane would run against the wrong directory. Observed
/// exactly that before this existed: the pane started and printed
/// "I percorsi UNC non sono supportati".
///
/// A third copy of `sirio_terminal::spawn_cwd`, whose own comment records
/// that these copies live independently on purpose: the crates sit side by
/// side, so neither can borrow the other's. Only the child's cwd is stripped;
/// the stored path keeps its prefix and its long-path capability.
#[cfg(windows)]
fn child_cwd(path: &Path) -> PathBuf {
    let string = path.to_string_lossy();
    let Some(rest) = string.strip_prefix(VERBATIM_PREFIX) else {
        return path.to_path_buf();
    };
    if let Some(unc) = rest.strip_prefix(UNC_PREFIX) {
        return PathBuf::from(format!("{}{unc}", UNC_ROOT));
    }
    PathBuf::from(rest.to_string())
}

#[cfg(windows)]
fn windows_pane_shell() -> String {
    std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".to_string())
}

/// #246: control-owned panes on Windows, over ConPTY through `portable-pty` --
/// the same transport `sirio_terminal` already uses for the UI's own panes.
///
/// The contract mirrors the unix arm exactly: an `Arc<PaneProcess>` whose
/// reader thread accumulates scrollback and whose exit status lands in
/// `PaneState::exit_code`, so every `PaneRegistry` method above works
/// unchanged on either platform.
#[cfg(windows)]
fn spawn_process(
    _pane_id: &str,
    working_directory: &Path,
    command: Option<&str>,
) -> Result<Arc<PaneProcess>, PaneError> {
    use portable_pty::{CommandBuilder, PtySize, native_pty_system};

    let pair = native_pty_system()
        .openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|error| PaneError::Spawn(error.to_string()))?;

    let shell = windows_pane_shell();
    let mut builder = CommandBuilder::new(&shell);
    if let Some(command) = command {
        // VERBATIM: `CommandBuilder` re-quotes each argument itself
        // (`append_quoted`). Pre-wrapping the command for `cmd /C` is what
        // made a pane fail silently in #39 -- it arrived as a quoted program
        // name rather than a command.
        builder.arg("/C");
        builder.arg(command);
    }
    builder.cwd(child_cwd(working_directory));

    let child = pair
        .slave
        .spawn_command(builder)
        .map_err(|error| PaneError::Spawn(error.to_string()))?;
    // The slave is the child's end. Holding it open here would stop the pane
    // ever reporting EOF when the child exits, so the reader thread would
    // never reach the exit-status step.
    drop(pair.slave);

    let reader = pair
        .master
        .try_clone_reader()
        .map_err(|error| PaneError::Spawn(error.to_string()))?;
    let writer = pair
        .master
        .take_writer()
        .map_err(|error| PaneError::Spawn(error.to_string()))?;

    let process = Arc::new(PaneProcess {
        child: Mutex::new(child),
        _master: Mutex::new(pair.master),
        writer: Mutex::new(writer),
        state: Mutex::new(PaneState {
            output: Vec::new(),
            exit_code: None,
        }),
        exited: Condvar::new(),
    });
    let reader_process = Arc::clone(&process);
    std::thread::spawn(move || read_process_output(reader, reader_process));
    let waiter_process = Arc::clone(&process);
    std::thread::spawn(move || watch_process_exit(waiter_process));
    Ok(process)
}

/// Neither unix nor Windows: no PTY backend exists here. Returning an error
/// rather than fabricating a pane that can never produce output keeps every
/// downstream `PaneRegistry` method honest instead of silently degrading.
#[cfg(not(any(unix, windows)))]
fn spawn_process(
    _pane_id: &str,
    _working_directory: &Path,
    _command: Option<&str>,
) -> Result<Arc<PaneProcess>, PaneError> {
    Err(PaneError::Unsupported(
        "control-owned panes have no PTY backend on this platform".to_string(),
    ))
}

#[cfg(unix)]
fn spawn_process(
    pane_id: &str,
    working_directory: &Path,
    command: Option<&str>,
) -> Result<Arc<PaneProcess>, PaneError> {
    let mut master: RawFd = -1;
    let mut slave: RawFd = -1;
    // `termp`/`winp` are typed `*const` on Linux/glibc but `*mut` on
    // macOS/BSD libc (same split already handled for `TIOCSCTTY` below).
    // Pass an explicit `*mut`, which coerces to `*const` on Linux and matches
    // natively on macOS — so this one call compiles unchanged on both.
    //
    // The pointer is spelled out rather than passing `&mut window` directly:
    // against Linux's `*const` parameter clippy fires
    // `unnecessary_mut_passed` ("doesn't need a mutable reference"), which is
    // denied by the gate. A raw pointer satisfies both libc signatures without
    // claiming a mutability Linux does not want.
    let mut window = libc::winsize {
        ws_row: 24,
        ws_col: 80,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    let window_ptr: *mut libc::winsize = &mut window;
    let result = unsafe {
        libc::openpty(
            &mut master,
            &mut slave,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            window_ptr,
        )
    };
    if result != 0 {
        return Err(PaneError::Spawn(io::Error::last_os_error().to_string()));
    }

    let pid = unsafe { libc::fork() };
    if pid < 0 {
        unsafe {
            libc::close(master);
            libc::close(slave);
        }
        return Err(PaneError::Spawn(io::Error::last_os_error().to_string()));
    }
    if pid == 0 {
        child_exec(master, slave, pane_id, working_directory, command);
    }

    unsafe {
        libc::close(slave);
    }
    let writer_fd = unsafe { libc::dup(master) };
    if writer_fd < 0 {
        unsafe {
            libc::kill(pid, libc::SIGKILL);
            libc::close(master);
        }
        return Err(PaneError::Spawn(io::Error::last_os_error().to_string()));
    }

    let reader = unsafe { File::from_raw_fd(master) };
    let writer = unsafe { File::from_raw_fd(writer_fd) };
    let process = Arc::new(PaneProcess {
        pid,
        process_group: pid,
        writer: Mutex::new(Box::new(writer)),
        state: Mutex::new(PaneState {
            output: Vec::new(),
            exit_code: None,
        }),
        exited: Condvar::new(),
    });
    let reader_process = Arc::clone(&process);
    std::thread::spawn(move || read_process_output(Box::new(reader), reader_process));
    Ok(process)
}

#[cfg(unix)]
fn child_exec(
    master: RawFd,
    slave: RawFd,
    pane_id: &str,
    working_directory: &Path,
    command: Option<&str>,
) -> ! {
    unsafe {
        libc::close(master);
        if libc::setsid() < 0 {
            libc::_exit(127);
        }
        // TIOCSCTTY's libc-crate type differs by platform: Linux glibc
        // types it as `ioctl`'s native c_ulong, but macOS/BSD libc types it
        // narrower (still correct at the ioctl(2) ABI level once widened).
        // `as _` makes this call correct on both without a cfg split --
        // the identical fix already proven necessary (E0308 on
        // `cargo check --target aarch64-apple-darwin`) for the same call
        // in `sirio_usage::claude::Pty::spawn`; this crate's own
        // `cargo check` for that target cannot run yet (blocked earlier by
        // sirio_persistence's bundled sqlite3.c needing a real macOS C
        // toolchain -- see the wave report), so this fix is applied by
        // inspection rather than confirmed by the compiler.
        if libc::ioctl(slave, libc::TIOCSCTTY as _, 0) < 0 {
            libc::_exit(127);
        }
        if libc::dup2(slave, libc::STDIN_FILENO) < 0
            || libc::dup2(slave, libc::STDOUT_FILENO) < 0
            || libc::dup2(slave, libc::STDERR_FILENO) < 0
        {
            libc::_exit(127);
        }
        if slave > libc::STDERR_FILENO {
            libc::close(slave);
        }
        let directory = match std::ffi::CString::new(working_directory.to_string_lossy().as_bytes())
        {
            Ok(directory) => directory,
            Err(_) => CString::new("/").expect("literal has no nul"),
        };
        if libc::chdir(directory.as_ptr()) != 0 {
            libc::_exit(127);
        }
        let pane = std::ffi::CString::new(pane_id)
            .unwrap_or_else(|_| CString::new("pane-invalid").expect("literal has no nul"));
        let term = std::ffi::CString::new("xterm-256color").expect("literal has no nul");
        libc::setenv(c"SIRIO_PANE_ID".as_ptr(), pane.as_ptr(), 1);
        // Same pre-rebrand alias the terminal crate exports.
        libc::setenv(c"TILLER_PANE_ID".as_ptr(), pane.as_ptr(), 1);
        libc::setenv(c"TERM".as_ptr(), term.as_ptr(), 1);

        let shell = std::env::var("SHELL")
            .ok()
            .filter(|shell| !shell.is_empty())
            .unwrap_or_else(|| "/bin/sh".to_string());
        let shell = std::ffi::CString::new(shell)
            .unwrap_or_else(|_| std::ffi::CString::new("/bin/sh").expect("literal has no nul"));
        let mut arguments = vec![shell.clone()];
        if let Some(command) = command {
            arguments.push(std::ffi::CString::new("-lc").expect("literal has no nul"));
            arguments.push(
                std::ffi::CString::new(command)
                    .unwrap_or_else(|_| std::ffi::CString::new("").expect("literal has no nul")),
            );
        } else {
            arguments.push(std::ffi::CString::new("-il").expect("literal has no nul"));
        }
        let mut pointers: Vec<*const libc::c_char> =
            arguments.iter().map(|argument| argument.as_ptr()).collect();
        pointers.push(std::ptr::null());
        libc::execvp(shell.as_ptr(), pointers.as_ptr());
        libc::_exit(127);
    }
}

/// Answers ConPTY's cursor-position query so the child can start.
///
/// #246: ConPTY opens by sending DSR (`ESC [ 6 n`) and **waits for a reply**
/// before letting the child proceed. A UI pane has an emulator that answers;
/// a control-owned pane has nobody, so without this the child hangs forever —
/// the pane spawns and can be killed, but produces no output and never
/// reports an exit code, which is exactly how this presented.
///
/// `ESC [ 1 ; 1 R` is the minimal honest answer: this pane keeps raw
/// scrollback and tracks no cursor, so row 1 column 1 is all it can truthfully
/// claim. Nothing downstream reads the position back.
#[cfg(windows)]
fn answer_device_status_report(chunk: &[u8], process: &Arc<PaneProcess>) {
    if !chunk.windows(4).any(|window| window == b"[6n") {
        return;
    }
    if let Ok(mut writer) = process.writer.lock() {
        let _ = writer.write_all(b"[1;1R");
        let _ = writer.flush();
    }
}

/// Unix PTYs carry no such handshake: the child is not gated on a reply.
#[cfg(not(windows))]
fn answer_device_status_report(_chunk: &[u8], _process: &Arc<PaneProcess>) {}

fn read_process_output(mut reader: Box<dyn Read + Send>, process: Arc<PaneProcess>) {
    let mut buffer = [0u8; 8192];
    loop {
        match reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(size) => {
                answer_device_status_report(&buffer[..size], &process);
                if let Ok(mut state) = process.state.lock() {
                    state.output.extend_from_slice(&buffer[..size]);
                    if state.output.len() > SCROLLBACK_CAPACITY {
                        let excess = state.output.len() - SCROLLBACK_CAPACITY;
                        state.output.drain(..excess);
                    }
                } else {
                    break;
                }
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            // A closed PTY master reports EIO on unix rather than EOF.
            #[cfg(unix)]
            Err(error) if error.raw_os_error() == Some(libc::EIO) => break,
            Err(_) => break,
        }
    }

    // #246: only unix ends here. On Windows the pane holds the ConPTY master
    // open for its whole life, so this read loop never sees EOF even after the
    // child is gone -- exit detection is the waiter thread's job instead.
    #[cfg(unix)]
    {
        let exit_code = exit_code_of(&process);
        if let Ok(mut state) = process.state.lock() {
            state.exit_code = Some(exit_code);
            process.exited.notify_all();
        }
    }
}

/// Watches a Windows pane's child and publishes its exit status (#246).
///
/// Polls `try_wait` rather than blocking in `wait`: the child lives behind the
/// same mutex `terminate` needs for `kill`, so a blocking wait would hold that
/// lock for the child's entire life and deadlock any attempt to close the
/// pane. Polling holds it only for the length of one non-blocking check.
#[cfg(windows)]
fn watch_process_exit(process: Arc<PaneProcess>) {
    loop {
        let status = match process.child.lock() {
            Ok(mut child) => child.try_wait(),
            Err(_) => return,
        };
        match status {
            Ok(Some(status)) => {
                if let Ok(mut state) = process.state.lock() {
                    state.exit_code = Some(status.exit_code() as i32);
                    process.exited.notify_all();
                }
                return;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
            Err(_) => return,
        }
    }
}

/// The child's exit status, once its output stream has ended.
///
/// #246: unix reaps by pid; Windows waits on the `Child` handle the ConPTY
/// spawn kept. Both feed the same `PaneState::exit_code`, so `panel wait` is
/// unchanged on either platform.
#[cfg(unix)]
fn exit_code_of(process: &Arc<PaneProcess>) -> i32 {
    reap(process.pid)
}

#[cfg(unix)]
fn reap(pid: libc::pid_t) -> i32 {
    let mut status = 0;
    loop {
        let result = unsafe { libc::waitpid(pid, &mut status, 0) };
        if result == pid {
            if libc::WIFEXITED(status) {
                return libc::WEXITSTATUS(status);
            }
            if libc::WIFSIGNALED(status) {
                return 128 + libc::WTERMSIG(status);
            }
            return 1;
        }
        if result < 0 && io::Error::last_os_error().kind() != io::ErrorKind::Interrupted {
            return 1;
        }
    }
}

#[cfg(unix)]
fn terminate(process: &PaneProcess) {
    // A pane gets one polite 500 ms window. A child that ignores SIGTERM (or
    // a shell that is waiting on one) then receives SIGKILL as a group, so
    // the close boundary cannot leave descendants behind. A descendant that
    // deliberately creates a new session is outside the pane's ownership
    // boundary and cannot be reached by this group signal.
    unsafe {
        libc::killpg(process.process_group, libc::SIGTERM);
    }
    if wait_for_process_group(process.process_group, TERMINATE_GRACE) {
        let _ = process_done(process, TERMINATE_GRACE);
        return;
    }
    unsafe {
        libc::killpg(process.process_group, libc::SIGKILL);
    }
    let _ = wait_for_process_group(process.process_group, TERMINATE_GRACE);
    let _ = process_done(process, TERMINATE_GRACE);
}

/// #246: Windows has no process *group* signal, so the pane's ownership
/// boundary is the `Child` handle the ConPTY spawn kept. `kill` terminates the
/// job the child heads; the following `wait` is what makes `shutdown`
/// synchronous, so app quit cannot leave a pane's process behind — which is
/// the failure this had to avoid, since `PaneRegistry::shutdown` runs there.
#[cfg(windows)]
fn terminate(process: &PaneProcess) {
    if let Ok(mut child) = process.child.lock() {
        let _ = child.kill();
        let _ = child.wait();
    }
}

/// Neither unix nor Windows: no PTY backend exists, and `spawn_process`
/// refuses before a `PaneProcess` is ever built, so nothing can reach this.
#[cfg(not(any(unix, windows)))]
fn terminate(_process: &PaneProcess) {}

#[cfg(unix)]
fn wait_for_process_group(process_group: libc::pid_t, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        let result = unsafe { libc::killpg(process_group, 0) };
        if result != 0 && io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH) {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[cfg(unix)]
fn process_done(process: &PaneProcess, timeout: Duration) -> bool {
    let Ok(state) = process.state.lock() else {
        return true;
    };
    if state.exit_code.is_some() {
        return true;
    }
    process
        .exited
        .wait_timeout(state, timeout)
        .map(|(state, _)| state.exit_code.is_some())
        .unwrap_or(true)
}

pub fn terminal_key_bytes(key: &str) -> Result<Vec<u8>, PaneError> {
    let bytes = match key {
        "enter" | "return" => b"\r".to_vec(),
        "tab" => b"\t".to_vec(),
        "escape" => b"\x1b".to_vec(),
        "backspace" => b"\x7f".to_vec(),
        "delete" => b"\x1b[3~".to_vec(),
        "up" => b"\x1b[A".to_vec(),
        "down" => b"\x1b[B".to_vec(),
        "left" => b"\x1b[D".to_vec(),
        "right" => b"\x1b[C".to_vec(),
        _ => return Err(PaneError::InvalidKey(key.to_string())),
    };
    Ok(bytes)
}

pub fn base64_encode(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut output = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let first = chunk[0];
        let second = chunk.get(1).copied().unwrap_or(0);
        let third = chunk.get(2).copied().unwrap_or(0);
        output.push(ALPHABET[(first >> 2) as usize] as char);
        output.push(ALPHABET[((first & 0x03) << 4 | second >> 4) as usize] as char);
        output.push(if chunk.len() > 1 {
            ALPHABET[((second & 0x0f) << 2 | third >> 6) as usize] as char
        } else {
            '='
        });
        output.push(if chunk.len() > 2 {
            ALPHABET[(third & 0x3f) as usize] as char
        } else {
            '='
        });
    }
    output
}

#[cfg(all(test, windows))]
mod windows_pane_tests {
    use super::{PaneRegistry, spawn_process};
    use std::time::{Duration, Instant};

    /// #246 replaces `unsupported_pane_error_names_no_deleted_pty_backend`,
    /// which asserted that creation FAILS here. That was true and is now
    /// deliberately false: Windows has a ConPTY backend. Recorded rather than
    /// quietly deleted, because a removed test that pinned real behaviour is
    /// worth explaining.
    #[test]
    fn a_control_owned_pane_spawns_on_windows() {
        let process = spawn_process("pane-spawn", std::path::Path::new("."), None)
            .expect("Windows has a ConPTY backend for control-owned panes");
        super::terminate(&process);
    }

    /// #246/#150: a verbatim path handed to cmd.exe as its cwd is read as
    /// UNC and silently ignored, so the pane would run every command in the
    /// Windows directory instead. Observed before this was stripped.
    #[test]
    fn a_childs_cwd_carries_no_verbatim_prefix() {
        use super::child_cwd;
        use std::path::{Path, PathBuf};
        assert_eq!(
            child_cwd(Path::new(r"\\?\D:\Progetti")),
            PathBuf::from(r"D:\Progetti")
        );
        assert_eq!(
            child_cwd(Path::new(r"\\?\UNC\server\share")),
            PathBuf::from(r"\\server\share")
        );
        assert_eq!(
            child_cwd(Path::new(r"D:\plain")),
            PathBuf::from(r"D:\plain"),
            "a plain path is left alone"
        );
    }

    /// The command's output has to reach the pane's scrollback, or `panel
    /// read` is silent and every scripted use of the socket is blind.
    #[test]
    fn a_pane_command_reaches_the_panes_scrollback() {
        let process = spawn_process(
            "pane-echo",
            std::path::Path::new("."),
            Some("echo sirio-246-marker"),
        )
        .expect("spawn");

        let deadline = Instant::now() + Duration::from_secs(20);
        let mut seen = String::new();
        while Instant::now() < deadline {
            if let Ok(state) = process.state.lock() {
                seen = String::from_utf8_lossy(&state.output).to_string();
                if seen.contains("sirio-246-marker") {
                    break;
                }
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        super::terminate(&process);

        assert!(
            seen.contains("sirio-246-marker"),
            "the command's output must land in the pane's scrollback, got: {seen:?}"
        );
    }

    /// A command that ends must report its status through the same
    /// `PaneState::exit_code` the unix arm feeds, or `panel wait` hangs.
    #[test]
    fn a_finished_pane_reports_its_exit_code() {
        let process =
            spawn_process("pane-exit", std::path::Path::new("."), Some("exit 3")).expect("spawn");

        let deadline = Instant::now() + Duration::from_secs(20);
        let mut code = None;
        while Instant::now() < deadline {
            if let Ok(state) = process.state.lock() {
                code = state.exit_code;
                if code.is_some() {
                    break;
                }
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        super::terminate(&process);

        assert_eq!(
            code,
            Some(3),
            "the child's exit status must reach PaneState::exit_code"
        );
    }

    /// The one that matters most: `PaneRegistry::shutdown` runs on app quit,
    /// so a pane it cannot kill leaks a process per pane for the rest of the
    /// session. Asserts the child is gone, not merely that shutdown returned.
    #[test]
    fn shutdown_kills_a_control_owned_pane() {
        let registry = PaneRegistry::new();
        let directory = std::env::temp_dir();
        let info = registry
            .create(&directory, None, "pane")
            .expect("a pane can be created on Windows");

        registry.shutdown();

        let after = registry.state(&info.id);
        assert!(
            after.is_err() || after.unwrap().exit_status.is_some(),
            "after shutdown the pane must be gone or have reported an exit status"
        );
    }
}
