//! The app's own panes as the control socket sees them.
//!
//! Terminal sessions opened through `panel.*` are owned by the host
//! (`sirio_host_client`, spec SP2 §5), so this crate no longer spawns a PTY.
//! What remains here is the app's side of the contract: the layout and the
//! state of the panes the app renders (`set_external`/`set_external_state`),
//! their scrollback on demand through a source the app supplies, and the
//! canonical-directory cache those calls use. A pane a `LocalLink` terminal
//! owns is served from here; a host session is served by `host_panels`.

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

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
        }
    }
}

impl std::error::Error for PaneError {}

/// The app's pane, as the registry holds it.
struct ExternalPane {
    working_directory: PathBuf,
    info: PaneInfo,
    state: PaneStateSnapshot,
    scrollback_source: Option<ScrollbackSource>,
}

/// Thread-safe source of truth for the app's own panes, as seen by the
/// control socket.
pub struct PaneRegistry {
    external: Mutex<HashMap<String, ExternalPane>>,
    canonical_directories: Mutex<HashMap<PathBuf, PathBuf>>,
    closed: Mutex<HashSet<String>>,
}

impl Default for PaneRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl PaneRegistry {
    pub fn new() -> Self {
        Self {
            external: Mutex::new(HashMap::new()),
            canonical_directories: Mutex::new(HashMap::new()),
            closed: Mutex::new(HashSet::new()),
        }
    }

    /// Replaces the live application-pane snapshot for one worktree. The GPUI
    /// workspace owns those panes and their PTYs; this registry owns the
    /// control-socket view so `panel.list` can report them.
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
    /// source for on-demand scrollback; this crate never recomputes them.
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
            closed.extend(removed);
            for id in current_ids {
                closed.remove(&id);
            }
        }
        Ok(())
    }

    /// Every pane the app renders, sorted by id.
    pub fn list(&self) -> Result<Vec<PaneInfo>, PaneError> {
        let mut panes: Vec<_> = self
            .external
            .lock()
            .map_err(|_| PaneError::Io("external pane registry lock poisoned".to_string()))?
            .values()
            .map(|pane| pane.info.clone())
            .collect();
        panes.sort_by(|left, right| left.id.cmp(&right.id));
        Ok(panes)
    }

    /// The app's panes of one worktree, sorted by id.
    pub fn list_for(&self, working_directory: &Path) -> Result<Vec<PaneInfo>, PaneError> {
        let working_directory = self.lookup_directory(working_directory)?;
        let mut panes: Vec<_> = self
            .external
            .lock()
            .map_err(|_| PaneError::Io("external pane registry lock poisoned".to_string()))?
            .values()
            .filter(|pane| pane.working_directory == working_directory)
            .map(|pane| pane.info.clone())
            .collect();
        panes.sort_by(|left, right| left.id.cmp(&right.id));
        Ok(panes)
    }

    /// Whether `pane_id` is one of the app's own panes (a `LocalLink` tab).
    pub fn is_external(&self, pane_id: &str) -> bool {
        self.external
            .lock()
            .map(|external| external.contains_key(pane_id))
            .unwrap_or(false)
    }

    pub fn read(&self, pane_id: &str) -> Result<Vec<u8>, PaneError> {
        self.scrollback(pane_id, None)
    }

    /// Returns the state snapshot currently held by the pane surface.
    pub fn state(&self, pane_id: &str) -> Result<PaneStateSnapshot, PaneError> {
        let external = self
            .external
            .lock()
            .map_err(|_| PaneError::Io("external pane registry lock poisoned".to_string()))?;
        match external.get(pane_id) {
            Some(pane) => Ok(pane.state.clone()),
            None => Err(self.missing_pane_error(pane_id)),
        }
    }

    /// The last `max_bytes` of an app pane's scrollback, or all of it.
    pub fn scrollback(
        &self,
        pane_id: &str,
        max_bytes: Option<usize>,
    ) -> Result<Vec<u8>, PaneError> {
        let source = {
            let external = self
                .external
                .lock()
                .map_err(|_| PaneError::Io("external pane registry lock poisoned".to_string()))?;
            match external.get(pane_id) {
                Some(pane) => pane.scrollback_source.clone(),
                None => return Err(self.missing_pane_error(pane_id)),
            }
        };
        let mut bytes = source.map_or_else(Vec::new, |source| source());
        if let Some(max_bytes) = max_bytes
            && bytes.len() > max_bytes
        {
            bytes.drain(..bytes.len() - max_bytes);
        }
        Ok(bytes)
    }

    /// Forgets the app's panes of one worktree; the next layout sync
    /// republishes whichever still exist.
    pub fn shutdown_for(&self, working_directory: impl AsRef<Path>) -> Result<(), PaneError> {
        let requested = working_directory.as_ref();
        let canonical = requested.canonicalize().ok();
        let mut external = self
            .external
            .lock()
            .map_err(|_| PaneError::Io("external pane registry lock poisoned".to_string()))?;
        let ids: Vec<String> = external
            .iter()
            .filter(|(_, pane)| {
                pane.working_directory == requested
                    || canonical
                        .as_ref()
                        .is_some_and(|directory| pane.working_directory == *directory)
            })
            .map(|(id, _)| id.clone())
            .collect();
        for id in &ids {
            external.remove(id);
        }
        drop(external);
        if let Ok(mut closed) = self.closed.lock() {
            closed.extend(ids);
        }
        Ok(())
    }

    /// Forgets every app pane. The app calls this when it quits.
    pub fn shutdown(&self) {
        if let Ok(mut external) = self.external.lock() {
            external.clear();
        }
    }

    /// The error for an id this registry does not hold: closed or unknown.
    pub fn missing_pane_error(&self, pane_id: &str) -> PaneError {
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
