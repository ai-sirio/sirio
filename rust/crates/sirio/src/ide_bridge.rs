//! Sirio as Claude Code's IDE: the app's side of `sirio_ide` (spec 2026-11-15, section 4.1).
//!
//! `sirio_ide` serves the protocol on threads of its own. This module owns what the app decides:
//! whether the bridge runs (the setting `claude.ideBridge`), which port and token it serves
//! (kept in the settings table, so a restarted Sirio listens where the terminals it left behind
//! expect it), the lockfile Claude finds it by, and the connections it holds. The bridge starts
//! from `main()` before the first terminal exists (`start_at_launch`), never from the
//! workspace's constructor, so in-process UI tests bind no port and write no lockfile. Its
//! events reach the workspace through a channel the workspace drains
//! (`SirioWorkspace::start_ide_delivery`), the way `opencode.rs` hands over its watch's.
//!
//! Three rules shape the code:
//! - Nothing here blocks the UI thread on the network or the disk. `sirio_ide::bind` can wait on
//!   a pathological network, and a lockfile write fsyncs: both run at launch (before the event
//!   loop) or on the background executor (`StartJob::run`), and every lockfile write after the
//!   first goes through one writer thread that keeps only the newest content (`LockWriter`).
//! - A lockfile never names a port nobody listens on while Sirio is alive: it is removed before
//!   the listener is dropped (`IdeBridge::shutdown`).
//! - The port lives in [`SSE_PORT`], not in the control state, which `sync_control_state`
//!   rebuilds from the catalog; 0 means off. `worktree_environment_in` reads it for every new
//!   terminal.
//!
//! Nothing is logged here that carries the token, a path or a selection.

use std::collections::HashMap;
use std::io;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::mpsc;
use std::thread::JoinHandle;
use std::time::Duration;

use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
use gpui::Global;
use serde_json::{Value, json};
use sirio_ide::{Bridge, BridgeEvent, BridgeOptions, ConnId, LockBody, Lockfile};

use crate::session::{ProjectCatalog, SessionStore};

/// The port `CLAUDE_CODE_SSE_PORT` is given in every new terminal; 0 while the bridge is off.
pub(crate) static SSE_PORT: AtomicU16 = AtomicU16::new(0);

const IDE_NAME: &str = if cfg!(debug_assertions) {
    "Sirio (debug)"
} else {
    "Sirio"
};

/// How long a quit or a switch waits for the lockfile's writer to remove the file.
const REMOVE_DEADLINE: Duration = Duration::from_secs(2);

/// The terminal pane a connection's Claude runs in. Resolved from the connecting pid's
/// ancestry; until that walk exists every connection is ownerless.
#[allow(dead_code, reason = "constructed once the owner walk resolves a connection's pane")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OwnerPane {
    pub(crate) pane_id: String,
}

/// What the app holds about one connection that announced itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Connection {
    pub(crate) pid: u32,
    pub(crate) owner: Option<OwnerPane>,
    pub(crate) worktree: Option<PathBuf>,
}

enum LockMessage {
    Rewrite(LockBody),
    Remove(mpsc::Sender<()>),
}

/// The only writer of the lockfile after its first write. `Lockfile::rewrite` fsyncs, so it
/// runs on a thread of its own, and a rewrite queued behind another replaces it: the newest
/// content is always the last one written.
struct LockWriter {
    name: String,
    tx: mpsc::Sender<LockMessage>,
    _thread: JoinHandle<()>,
}

impl LockWriter {
    fn start(lockfile: Lockfile) -> io::Result<LockWriter> {
        let name = lockfile
            .path()
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let (tx, rx) = mpsc::channel::<LockMessage>();
        let thread = std::thread::Builder::new()
            .name("sirio-ide-lockfile".into())
            .spawn(move || Self::run(lockfile, &rx))?;
        Ok(LockWriter {
            name,
            tx,
            _thread: thread,
        })
    }

    fn run(lockfile: Lockfile, rx: &mpsc::Receiver<LockMessage>) {
        while let Ok(message) = rx.recv() {
            let mut newest = message;
            // Only the newest content matters; a removal behind it ends the file.
            while let Ok(next) = rx.try_recv() {
                newest = next;
            }
            match newest {
                LockMessage::Rewrite(body) => {
                    if let Err(error) = lockfile.rewrite(&body) {
                        eprintln!("[ide] the lockfile was not rewritten: {}", error.kind());
                    }
                }
                LockMessage::Remove(done) => {
                    let _ = lockfile.remove();
                    let _ = done.send(());
                    return;
                }
            }
        }
    }

    fn rewrite(&self, body: LockBody) {
        let _ = self.tx.send(LockMessage::Rewrite(body));
    }

    /// Removes the file, waiting for it (the file must be gone before the listener is).
    fn remove(self) {
        let (done, wait) = mpsc::channel();
        if self.tx.send(LockMessage::Remove(done)).is_ok() {
            let _ = wait.recv_timeout(REMOVE_DEADLINE);
        }
    }
}

/// A listener, its lockfile, and the port they share.
struct Running {
    bridge: Bridge,
    lock: LockWriter,
    port: u16,
}

/// The root of every worktree of every project, in the catalog's order and without repeats.
pub(crate) fn workspace_folders(catalog: &ProjectCatalog) -> Vec<PathBuf> {
    let mut folders: Vec<PathBuf> = Vec::new();
    for worktree in catalog
        .projects()
        .iter()
        .flat_map(|project| project.worktrees.iter())
    {
        if !folders.contains(&worktree.path) {
            folders.push(worktree.path.clone());
        }
    }
    folders
}

fn lockfile_dir() -> Option<PathBuf> {
    crate::user_home_dir().map(|home| home.join(".claude").join("ide"))
}

fn lock_body(token: &str, folders: &[PathBuf]) -> LockBody {
    LockBody {
        pid: std::process::id(),
        workspace_folders: folders.to_vec(),
        ide_name: IDE_NAME.to_string(),
        auth_token: token.to_string(),
    }
}

/// Everything a start needs, so it can run on any thread.
pub(crate) struct StartJob {
    saved_port: Option<u16>,
    token: String,
    folders: Vec<PathBuf>,
    events: UnboundedSender<BridgeEvent>,
    generation: u64,
}

impl StartJob {
    /// Binds, starts the server and writes the lockfile. Blocking: call it before the event
    /// loop or from the background executor.
    pub(crate) fn run(self) -> Started {
        let outcome = self.launch();
        Started {
            generation: self.generation,
            outcome,
        }
    }

    fn launch(&self) -> io::Result<Running> {
        let dir = lockfile_dir().ok_or_else(|| io::Error::other("no home directory"))?;
        let listener = sirio_ide::bind(self.saved_port)?;
        let events = self.events.clone();
        let bridge = Bridge::start(
            listener,
            self.token.clone(),
            BridgeOptions::default(),
            move |event| {
                let _ = events.unbounded_send(event);
            },
        )?;
        let port = bridge.port();
        let lockfile = match Lockfile::write(&dir, port, &lock_body(&self.token, &self.folders)) {
            Ok(lockfile) => lockfile,
            Err(error) => {
                bridge.shutdown();
                return Err(error);
            }
        };
        match LockWriter::start(lockfile) {
            Ok(lock) => Ok(Running { bridge, lock, port }),
            Err(error) => {
                // The file was written; without its writer it could not be removed.
                let _ = std::fs::remove_file(dir.join(format!("{port}.lock")));
                bridge.shutdown();
                Err(error)
            }
        }
    }
}

/// The outcome of a [`StartJob`], to hand back to [`IdeBridge::finish_start`].
pub(crate) struct Started {
    generation: u64,
    outcome: io::Result<Running>,
}

/// The bridge, as a gpui global. `main()` sets it before the window opens; the workspace
/// reads and changes it, and the control socket's debug verbs read it.
pub(crate) struct IdeBridge {
    enabled: bool,
    running: Option<Running>,
    /// The generation of the start now on the background executor, if any: a second request
    /// to switch on, in the same generation, waits for it.
    starting: Option<u64>,
    /// Bumped by every switch off, so a start that finishes after the switch moved on is
    /// dropped.
    generation: u64,
    /// The port saved last, tried first by the next bind.
    saved_port: Option<u16>,
    token: String,
    events_tx: UnboundedSender<BridgeEvent>,
    events_rx: Option<UnboundedReceiver<BridgeEvent>>,
    connections: HashMap<ConnId, Connection>,
    folders: Vec<PathBuf>,
}

impl Global for IdeBridge {}

/// Binds before any terminal is created or restored, so every one of them is born with the
/// port. On failure it logs one content-free line and stays off. Blocking by design: it runs
/// in `main()` before the event loop.
pub(crate) fn start_at_launch(
    db: &SessionStore,
    enabled: bool,
    folders: Vec<PathBuf>,
) -> IdeBridge {
    let (saved_port, saved_token) = db.load_ide_bridge_endpoint();
    let token = saved_token
        .filter(|token| !token.is_empty())
        .or_else(|| sirio_ide::new_token().ok())
        .unwrap_or_default();
    let (events_tx, events_rx) = unbounded();
    let mut bridge = IdeBridge {
        enabled,
        running: None,
        starting: None,
        generation: 0,
        saved_port,
        token,
        events_tx,
        events_rx: Some(events_rx),
        connections: HashMap::new(),
        folders,
    };
    if enabled && let Some(job) = bridge.begin_start() {
        let started = job.run();
        bridge.finish_start(started, db);
    }
    bridge
}

impl IdeBridge {
    /// The receiver of the bridge's events, handed out once to the workspace's delivery loop.
    pub(crate) fn take_events(&mut self) -> Option<UnboundedReceiver<BridgeEvent>> {
        self.events_rx.take()
    }

    /// Turns the switch on. Returns the job that binds and writes the lockfile, to run off the
    /// UI thread, unless a listener is already up or being started.
    pub(crate) fn begin_enable(&mut self, folders: Vec<PathBuf>) -> Option<StartJob> {
        self.enabled = true;
        self.folders = folders;
        self.begin_start()
    }

    fn begin_start(&mut self) -> Option<StartJob> {
        if self.running.is_some()
            || self.starting == Some(self.generation)
            || self.token.is_empty()
        {
            return None;
        }
        self.starting = Some(self.generation);
        Some(StartJob {
            saved_port: self.saved_port,
            token: self.token.clone(),
            folders: self.folders.clone(),
            events: self.events_tx.clone(),
            generation: self.generation,
        })
    }

    /// Installs what a start produced, unless the switch moved on while it ran (then the
    /// listener it bound is closed again, its lockfile first).
    pub(crate) fn finish_start(&mut self, started: Started, db: &SessionStore) {
        let Started {
            generation,
            outcome,
        } = started;
        if self.starting == Some(generation) {
            self.starting = None;
        }
        let running = match outcome {
            Ok(running) => running,
            Err(error) => {
                eprintln!("[ide] the bridge did not start: {}", error.kind());
                return;
            }
        };
        if !self.enabled || generation != self.generation || self.running.is_some() {
            running.lock.remove();
            running.bridge.shutdown();
            return;
        }
        self.saved_port = Some(running.port);
        db.save_ide_bridge_endpoint(running.port, &self.token);
        SSE_PORT.store(running.port, Ordering::SeqCst);
        // The catalog may have changed while the lockfile was being written.
        running.lock.rewrite(lock_body(&self.token, &self.folders));
        self.running = Some(running);
    }

    /// Turns the switch off: no new terminal gets the port, the lockfile goes, and only then
    /// the listener and every connection.
    pub(crate) fn disable(&mut self) {
        self.enabled = false;
        self.shutdown();
    }

    /// Ends what is running (quit, or the switch off). The lockfile is removed before the
    /// listener is dropped, so it never names a port nobody listens on.
    pub(crate) fn shutdown(&mut self) {
        self.generation += 1;
        SSE_PORT.store(0, Ordering::SeqCst);
        self.connections.clear();
        if let Some(running) = self.running.take() {
            running.lock.remove();
            running.bridge.shutdown();
        }
    }

    /// The root of every worktree in the sidebar changed: the lockfile says so.
    pub(crate) fn set_workspace_folders(&mut self, folders: Vec<PathBuf>) {
        if self.folders == folders {
            return;
        }
        self.folders = folders;
        if let Some(running) = &self.running {
            running
                .lock
                .rewrite(lock_body(&self.token, &self.folders));
        }
    }

    /// One event from the bridge's threads. Requests that need the UI (diagnostics, diffs) are
    /// answered by later tasks; until they land the bridge answers diagnostics with its own
    /// deadline error and an `openDiff` waits for its connection to close.
    pub(crate) fn on_event(&mut self, event: BridgeEvent) {
        match event {
            BridgeEvent::Connected { conn, pid } => {
                self.connections.insert(
                    conn,
                    Connection {
                        pid,
                        owner: None,
                        worktree: None,
                    },
                );
            }
            BridgeEvent::Disconnected { conn } => {
                self.connections.remove(&conn);
            }
            BridgeEvent::Diagnostics { .. }
            | BridgeEvent::OpenDiff { .. }
            | BridgeEvent::DiffWithdrawn { .. } => {}
        }
    }

    /// `surface.ide.state`: what the bridge holds, with no token, no selection and no text of a
    /// proposal; a path appears only as a worktree root.
    pub(crate) fn state_json(&self) -> String {
        let mut connections: Vec<(&ConnId, &Connection)> = self.connections.iter().collect();
        connections.sort_by_key(|(conn, _)| conn.0);
        let connections: Vec<Value> = connections
            .into_iter()
            .map(|(conn, connection)| {
                json!({
                    "conn": conn.0,
                    "pid": connection.pid,
                    "owner_pane": connection.owner.as_ref().map(|owner| owner.pane_id.as_str()),
                    "worktree": connection
                        .worktree
                        .as_ref()
                        .map(|worktree| worktree.to_string_lossy().into_owned()),
                })
            })
            .collect();
        json!({
            "enabled": self.enabled,
            "port": self.running.as_ref().map(|running| running.port),
            "lockfile": self.running.as_ref().map(|running| running.lock.name.as_str()),
            "connections": connections,
            "proposals": [],
            "confirmation": null,
            "toasts": [],
            "last_delivery": null,
        })
        .to_string()
    }
}
