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
//! `tiller_usage::claude::Pty` and `tiller_terminal`'s process-group
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
//! (`CreatePseudoConsole`) — `alacritty_terminal`'s already-vendored
//! `tty/windows/` is the natural implementation to reuse (it already owns a
//! child's lifetime end-to-end) rather than a hand-rolled
//! `CreateNamedPipeW`/`ConnectNamedPipe` + `CreatePseudoConsole` client
//! written from scratch here. Left undone and documented rather than
//! partially/incorrectly implemented; see the wave report for the reasoning.

use std::collections::{HashMap, HashSet};
#[cfg(unix)]
use std::ffi::CString;
use std::fmt;
use std::fs::File;
#[cfg(unix)]
use std::io::{self, Read};
use std::io::Write;
#[cfg(unix)]
use std::os::fd::{FromRawFd, RawFd};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
#[cfg(unix)]
use std::time::Instant;
use std::time::Duration;

#[cfg(unix)]
const SCROLLBACK_CAPACITY: usize = 256 * 1024;
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

/// State observed from the actual terminal surface, whether it is a
/// control-owned PTY or an application-owned GPUI terminal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PaneStateSnapshot {
    pub working_directory: PathBuf,
    pub scrollback: Vec<u8>,
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
    /// `alacritty_terminal`'s `tty/windows/`).
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
    writer: Mutex<File>,
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
}

/// Thread-safe source of truth for panes created through the control socket.
pub struct PaneRegistry {
    panes: Mutex<HashMap<String, Arc<PaneEntry>>>,
    external: Mutex<HashMap<String, ExternalPane>>,
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
        let working_directory = checked_directory(working_directory.as_ref())?;
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
        let working_directory = checked_directory(working_directory.as_ref())?;
        let panes = panes
            .into_iter()
            .map(|pane| {
                (
                    pane,
                    PaneStateSnapshot {
                        working_directory: working_directory.clone(),
                        scrollback: Vec::new(),
                        exit_status: None,
                    },
                )
            })
            .collect();
        self.set_external_state(working_directory, panes)
    }

    /// Replaces the live application-pane snapshot and the terminal state
    /// held by each renderer-owned pane. The app supplies these snapshots;
    /// this crate never recomputes them from git or a second PTY.
    pub fn set_external_state(
        &self,
        working_directory: impl AsRef<Path>,
        panes: Vec<(PaneInfo, PaneStateSnapshot)>,
    ) -> Result<(), PaneError> {
        let working_directory = checked_directory(working_directory.as_ref())?;
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
            .map(|(pane, _)| pane.id.clone())
            .collect::<Vec<_>>();
        external.retain(|_, pane| pane.working_directory != working_directory);
        for (pane, mut state) in panes {
            state.working_directory = working_directory.clone();
            external.insert(
                pane.id.clone(),
                ExternalPane {
                    working_directory: working_directory.clone(),
                    info: pane,
                    state,
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
        let working_directory = checked_directory(working_directory)?;
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
        if let Some(pane) = self
            .external
            .lock()
            .map_err(|_| PaneError::Io("external pane registry lock poisoned".to_string()))?
            .get(pane_id)
        {
            return Ok(pane.state.scrollback.clone());
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
                scrollback: state.output.clone(),
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

    /// Returns the renderer-owned scrollback bytes, bounded by the caller's
    /// requested maximum without changing the stored surface state.
    pub fn scrollback(
        &self,
        pane_id: &str,
        max_bytes: Option<usize>,
    ) -> Result<Vec<u8>, PaneError> {
        let mut bytes = self.state(pane_id)?.scrollback;
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

fn checked_directory(path: &Path) -> Result<PathBuf, PaneError> {
    let path = path
        .canonicalize()
        .map_err(|error| PaneError::InvalidWorkingDirectory(error.to_string()))?;
    if !path.is_dir() {
        return Err(PaneError::InvalidWorkingDirectory(
            path.display().to_string(),
        ));
    }
    Ok(path)
}

/// No control-owned PTY backend exists on this platform yet (see the module
/// doc comment). `create`/`split` are the only callers, so returning an
/// error here — rather than fabricating a pane that can never produce
/// output — is what keeps every downstream `PaneRegistry` method honest
/// instead of silently degrading.
#[cfg(not(unix))]
fn spawn_process(
    _pane_id: &str,
    _working_directory: &Path,
    _command: Option<&str>,
) -> Result<Arc<PaneProcess>, PaneError> {
    Err(PaneError::Unsupported(
        "control-owned panes have no PTY backend on this platform yet \
         (Windows counterpart: ConPTY via alacritty_terminal's tty/windows/)"
            .to_string(),
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
    // `null_mut()`/`&mut window` coerce to either mutability at the call
    // site, so this one call compiles unchanged on both.
    let mut window = libc::winsize {
        ws_row: 24,
        ws_col: 80,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    let result = unsafe {
        libc::openpty(
            &mut master,
            &mut slave,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut window,
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
        writer: Mutex::new(writer),
        state: Mutex::new(PaneState {
            output: Vec::new(),
            exit_code: None,
        }),
        exited: Condvar::new(),
    });
    let reader_process = Arc::clone(&process);
    std::thread::spawn(move || read_process_output(reader, reader_process));
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
        // in `tiller_usage::claude::Pty::spawn`; this crate's own
        // `cargo check` for that target cannot run yet (blocked earlier by
        // tiller_persistence's bundled sqlite3.c needing a real macOS C
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

#[cfg(unix)]
fn read_process_output(mut reader: File, process: Arc<PaneProcess>) {
    let mut buffer = [0u8; 8192];
    loop {
        match reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(size) => {
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
            Err(error) if error.raw_os_error() == Some(libc::EIO) => break,
            Err(_) => break,
        }
    }

    let exit_code = reap(process.pid);
    if let Ok(mut state) = process.state.lock() {
        state.exit_code = Some(exit_code);
        process.exited.notify_all();
    }
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

/// Unreachable in practice: `spawn_process` on this platform always returns
/// `Err` before a `PaneEntry`/`PaneProcess` is ever constructed, so no live
/// pane can reach this. Kept so `close`/`shutdown`/`shutdown_for` — portable,
/// platform-generic `PaneRegistry` methods — don't need a `cfg` split of
/// their own just to call it.
#[cfg(not(unix))]
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
