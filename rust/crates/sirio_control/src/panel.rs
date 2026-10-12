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
    /// Whether the pane's session outlives the app (a host session does, an
    /// in-process fallback does not). Answered as `survives` by `panel.state`.
    pub survives: bool,
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

/// A terminal session the host owns and the control socket created (or
/// found), as the registry holds it so the sidebar's status and `panel.list`
/// see it. Layer D does not walk these (the pre-SP2 status quo), so no pid.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ControlSession {
    pub info: PaneInfo,
    pub working_directory: PathBuf,
}

/// A pane in the registry: an app tab (`control == false`) or a control-owned
/// host session (`control == true`).
struct ExternalPane {
    working_directory: PathBuf,
    info: PaneInfo,
    state: PaneStateSnapshot,
    scrollback_source: Option<ScrollbackSource>,
    control: bool,
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
                        survives: false,
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
            .filter(|(_, pane)| !pane.control && pane.working_directory == working_directory)
            .map(|(id, _)| id.clone())
            .collect::<Vec<_>>();
        let current_ids = panes
            .iter()
            .map(|(pane, _, _)| pane.id.clone())
            .collect::<Vec<_>>();
        // Control sessions are not the layout's to replace: only the tabs'
        // entries of this worktree are republished.
        external.retain(|_, pane| pane.control || pane.working_directory != working_directory);
        for (pane, mut state, scrollback_source) in panes {
            state.working_directory = working_directory.clone();
            external.insert(
                pane.id.clone(),
                ExternalPane {
                    working_directory: working_directory.clone(),
                    info: pane,
                    state,
                    scrollback_source,
                    control: false,
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

    /// Replaces the registered control sessions. Every listed session is
    /// published with its cwd; any registered control session not
    /// listed leaves the registry (it has closed or left the host).
    pub fn set_control_sessions(&self, sessions: Vec<ControlSession>) -> Result<(), PaneError> {
        let resolved: Vec<(ControlSession, PathBuf)> = sessions
            .into_iter()
            .filter_map(|session| {
                let directory = self.lookup_directory(&session.working_directory).ok()?;
                Some((session, directory))
            })
            .collect();
        let mut external = self
            .external
            .lock()
            .map_err(|_| PaneError::Io("external pane registry lock poisoned".to_string()))?;
        external.retain(|_, pane| !pane.control);
        for (session, directory) in resolved {
            let state = PaneStateSnapshot {
                working_directory: directory.clone(),
                survives: false,
                exit_status: None,
            };
            external.insert(
                session.info.id.clone(),
                ExternalPane {
                    working_directory: directory,
                    info: session.info,
                    state,
                    scrollback_source: None,
                    control: true,
                },
            );
        }
        Ok(())
    }

    /// The registered control sessions, sorted by id.
    pub fn control_sessions(&self) -> Vec<PaneInfo> {
        let mut panes: Vec<PaneInfo> = self
            .external
            .lock()
            .map(|external| {
                external
                    .values()
                    .filter(|pane| pane.control)
                    .map(|pane| pane.info.clone())
                    .collect()
            })
            .unwrap_or_default();
        panes.sort_by(|left, right| left.id.cmp(&right.id));
        panes
    }

    /// Whether `pane_id` is one of the app's own tabs (a `LocalLink` pane).
    /// A registered control session is not an app pane.
    pub fn is_app_pane(&self, pane_id: &str) -> bool {
        self.external
            .lock()
            .map(|external| external.get(pane_id).is_some_and(|pane| !pane.control))
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

    /// Forgets the panes of one worktree: its tabs and its control sessions.
    /// Returns the ids of the control sessions it forgot, which the caller
    /// must end on the host (spec §4.2: a closed worktree's sessions end).
    /// Makes no host call itself: the caller runs it off the GPUI thread.
    pub fn shutdown_for(
        &self,
        working_directory: impl AsRef<Path>,
        worktrees: &[PathBuf],
    ) -> Result<Vec<String>, PaneError> {
        let requested = working_directory.as_ref();
        let canonical = requested.canonicalize().ok();
        // Every worktree the app knows, the one closing included: a control
        // session belongs to the deepest worktree that contains its cwd.
        let mut candidates = worktrees.to_vec();
        if !candidates.iter().any(|worktree| worktree == requested) {
            candidates.push(requested.to_path_buf());
        }
        let mut external = self
            .external
            .lock()
            .map_err(|_| PaneError::Io("external pane registry lock poisoned".to_string()))?;
        let forgotten: Vec<(String, bool)> = external
            .iter()
            .filter(|(_, pane)| {
                if pane.control {
                    owning_worktree(&pane.working_directory, &candidates)
                        .is_some_and(|owner| owner.as_path() == requested)
                } else {
                    pane.working_directory == requested
                        || canonical
                            .as_ref()
                            .is_some_and(|directory| pane.working_directory == *directory)
                }
            })
            .map(|(id, pane)| (id.clone(), pane.control))
            .collect();
        for (id, _) in &forgotten {
            external.remove(id);
        }
        drop(external);
        if let Ok(mut closed) = self.closed.lock() {
            closed.extend(forgotten.iter().map(|(id, _)| id.clone()));
        }
        Ok(forgotten
            .into_iter()
            .filter(|(_, control)| *control)
            .map(|(id, _)| id)
            .collect())
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

/// The worktree that owns a session whose working directory is `cwd`: the
/// deepest of `worktrees` that contains it, component-wise, so `/a/wt` does
/// not contain `/a/wt2` and a nested worktree keeps its own sessions (the
/// orphan-adoption rule, spec §4.2). Paths are compared canonicalised when
/// they exist.
pub fn owning_worktree<'a>(cwd: &Path, worktrees: &'a [PathBuf]) -> Option<&'a PathBuf> {
    let cwd = canonical_or_raw(cwd);
    worktrees
        .iter()
        .filter(|worktree| cwd.starts_with(canonical_or_raw(worktree)))
        .max_by_key(|worktree| canonical_or_raw(worktree).components().count())
}

fn canonical_or_raw(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

#[cfg(test)]
mod containment_tests {
    use super::*;

    /// A scratch tree under `$TMPDIR`: wt, wt2, wt/sub, wt/nested, wt/nested/deep.
    fn tree(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("sirio-contain-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for dir in ["wt", "wt2", "wt/sub", "wt/nested/deep"] {
            std::fs::create_dir_all(root.join(dir)).expect("scratch directory");
        }
        root
    }

    #[test]
    fn a_subdirectory_of_a_worktree_belongs_to_it() {
        let root = tree("sub");
        let worktrees = vec![root.join("wt")];
        assert_eq!(owning_worktree(&root.join("wt/sub"), &worktrees), Some(&root.join("wt")));
    }

    #[test]
    fn a_sibling_whose_name_extends_a_worktree_does_not_belong_to_it() {
        let root = tree("sibling");
        let worktrees = vec![root.join("wt")];
        assert_eq!(owning_worktree(&root.join("wt2"), &worktrees), None);
    }

    #[test]
    fn a_nested_worktree_takes_the_deepest_containing_one() {
        let root = tree("nested");
        let worktrees = vec![root.join("wt"), root.join("wt/nested")];
        assert_eq!(
            owning_worktree(&root.join("wt/nested/deep"), &worktrees),
            Some(&root.join("wt/nested"))
        );
    }

    fn pane(id: &str) -> PaneInfo {
        PaneInfo {
            id: id.to_string(),
            tab: "tab".to_string(),
            title: "title".to_string(),
            agent: String::new(),
            active: false,
        }
    }

    /// Closing a worktree returns only the control sessions it contains: a
    /// subdirectory session ends with its own worktree and survives a sibling's
    /// close, and an app pane of the closed worktree is forgotten but never
    /// returned for the host to end.
    #[test]
    fn closing_a_worktree_ends_only_the_control_sessions_it_contains() {
        let root = tree("control-only");
        let (wt, wt2) = (root.join("wt"), root.join("wt2"));
        let worktrees = vec![wt.clone(), wt2.clone()];
        let registry = PaneRegistry::new();
        registry.set_external(&wt, vec![pane("app-tab")]).expect("app tab");
        registry
            .set_control_sessions(vec![ControlSession {
                info: pane("pane-c000000000001"),
                working_directory: root.join("wt/sub"),
            }])
            .expect("control session");

        let sibling_close = registry
            .shutdown_for(&wt2, &worktrees)
            .expect("close sibling");
        assert!(
            sibling_close.is_empty(),
            "a sibling's close must not end wt's control session: {sibling_close:?}"
        );

        let own_close = registry.shutdown_for(&wt, &worktrees).expect("close wt");
        assert_eq!(own_close, vec!["pane-c000000000001".to_string()]);
        assert!(
            registry.list().expect("list").is_empty(),
            "the closed worktree's app tab is forgotten too"
        );
    }
}
