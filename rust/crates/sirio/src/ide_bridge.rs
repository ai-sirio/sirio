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
//! The same module decides which terminal pane each connection belongs to (`resolve`: the
//! connecting pid's ancestry against the shell pids of the panes Sirio holds, off the UI thread),
//! coalesces the editor's selection per connection (`mark_selection`), and keeps the order in
//! which the user last used each worktree's agents (`AgentFocus`), which an at-mention follows.
//!
//! Nothing is logged here that carries the token, a path or a selection.

use std::collections::{HashMap, HashSet};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::mpsc;
use std::thread::JoinHandle;
use std::time::Duration;

use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
use gpui::{App, BorrowAppContext as _, Context, Entity, EntityId, Global, WeakEntity};
use serde_json::{Value, json};
use sirio_ide::{Bridge, BridgeEvent, BridgeOptions, Coalescer, ConnId, LockBody, Lockfile};
use sirio_ui::chat::Chat;
use sirio_ui::file_view::FileView;

use crate::session::{ProjectCatalog, SessionStore};
use crate::{SirioWorkspace, TabContent, paths_name_the_same_document};

/// The port `CLAUDE_CODE_SSE_PORT` is given in every new terminal; 0 while the bridge is off.
pub(crate) static SSE_PORT: AtomicU16 = AtomicU16::new(0);

const IDE_NAME: &str = if cfg!(debug_assertions) {
    "Sirio (debug)"
} else {
    "Sirio"
};

/// How long a quit or a switch waits for the lockfile's writer to remove the file.
const REMOVE_DEADLINE: Duration = Duration::from_secs(2);

/// How many toasts the state keeps.
const TOASTS_KEPT: usize = 8;

/// The shown copy of the toast an at-mention with no agent to take it leaves (spec 4.5).
pub(crate) const NO_AGENT_TOAST: &str = "No Claude terminal or chat in this worktree";

/// The terminal pane a connection's Claude runs in, as the number `select_pane` takes (the
/// control socket spells it `pane-<n>`). Resolved from the connecting pid's ancestry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct OwnerPane {
    pub(crate) pane_id: usize,
}

/// A terminal Sirio holds a view of, mounted in a tab or parked in the pane cache.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LivePane {
    pub(crate) pane_id: usize,
    pub(crate) worktree: PathBuf,
    /// The OS pid of the pane's shell, once its session has one.
    pub(crate) shell_pid: Option<u32>,
}

/// What an agent terminal or chat of a worktree the user can have used last.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AgentTarget {
    Chat(EntityId),
    Terminal(usize),
}

/// The agents of each worktree in the order the user focused them, most recent first. Every
/// terminal pane and chat is recorded; whether one can take an at-mention (a chat, or a pane that
/// owns a live connection) is checked only when sending.
#[derive(Default)]
pub(crate) struct AgentFocus {
    by_worktree: HashMap<PathBuf, Vec<AgentTarget>>,
}

impl AgentFocus {
    pub(crate) fn record(&mut self, worktree: &Path, target: AgentTarget) {
        let targets = self.by_worktree.entry(worktree.to_path_buf()).or_default();
        targets.retain(|known| *known != target);
        targets.insert(0, target);
    }

    /// The worktree's targets, most recent first; `same` says whether two spellings name one
    /// worktree.
    pub(crate) fn targets(&self, worktree: &Path, same: impl Fn(&Path, &Path) -> bool) -> Vec<AgentTarget> {
        self.by_worktree
            .iter()
            .find(|(known, _)| known.as_path() == worktree)
            .or_else(|| self.by_worktree.iter().find(|(known, _)| same(known, worktree)))
            .map(|(_, targets)| targets.clone())
            .unwrap_or_default()
    }
}

/// A pane's shell, as the walk looks for it.
#[derive(Debug, Clone)]
pub(crate) struct PaneShell {
    pub(crate) pane_id: usize,
    pub(crate) shell_pid: u32,
    pub(crate) worktree: PathBuf,
}

/// What resolving one connection found.
#[derive(Debug, Clone)]
pub(crate) struct Resolved {
    pub(crate) conn: ConnId,
    /// The pane whose shell is an ancestor of the connecting pid, with that pane's worktree.
    pub(crate) owner: Option<(usize, PathBuf)>,
    /// With no owner: the deepest worktree holding the process's cwd, where the platform reads it.
    pub(crate) cwd_worktree: Option<PathBuf>,
}

/// Resolves `connections` (connection, connecting pid) against `panes`. Blocking: reads the
/// process table (Windows: one snapshot for the whole call) and canonicalises paths, so it runs
/// on the background executor.
pub(crate) fn resolve(
    connections: &[(ConnId, u32)],
    panes: &[PaneShell],
    worktrees: &[PathBuf],
) -> Vec<Resolved> {
    #[cfg(windows)]
    let table = sirio_activity::process::parent_table().unwrap_or_default();
    #[cfg(windows)]
    let mut parent = |pid: u32| table.get(&pid).copied();
    #[cfg(not(windows))]
    let mut parent = |pid: u32| sirio_activity::process::parent_pid(pid);
    let keys: Vec<(usize, u32)> = panes
        .iter()
        .filter(|pane| pane.shell_pid != 0)
        .map(|pane| (pane.pane_id, pane.shell_pid))
        .collect();
    connections
        .iter()
        .map(|&(conn, pid)| {
            let owner = sirio_ide::resolve_owner(pid, &keys, &mut parent).and_then(|pane_id| {
                let pane = panes.iter().find(|pane| pane.pane_id == pane_id)?;
                Some((pane_id, pane.worktree.clone()))
            });
            let cwd_worktree = if owner.is_some() {
                None
            } else {
                sirio_activity::process::process_cwd(pid).and_then(|cwd| deepest_worktree(&cwd, worktrees))
            };
            Resolved {
                conn,
                owner,
                cwd_worktree,
            }
        })
        .collect()
}

/// The deepest of `worktrees` that holds `directory`, compared canonically.
fn deepest_worktree(directory: &Path, worktrees: &[PathBuf]) -> Option<PathBuf> {
    let directory = directory.canonicalize().unwrap_or_else(|_| directory.to_path_buf());
    worktrees
        .iter()
        .filter(|root| {
            let root = root.canonicalize().unwrap_or_else(|_| (*root).clone());
            directory.starts_with(root)
        })
        .max_by_key(|root| root.as_os_str().len())
        .cloned()
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
        // Debug builds only: the E2E widens the window in which a start is in flight (the
        // listener is bound and held through this sleep), so a switch flapped meanwhile is seen.
        #[cfg(debug_assertions)]
        if let Some(delay) = std::env::var("SIRIO_DEBUG_IDE_START_DELAY_MS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
        {
            std::thread::sleep(Duration::from_millis(delay));
        }
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
    /// A start is on the background executor. No second one is handed out while it runs: its
    /// listener holds the saved port, and a second bind would find it taken and move the port.
    starting: bool,
    /// Bumped by every switch off, so a start that finishes after the switch moved on is
    /// dropped (and, if the switch is on again, started afresh).
    generation: u64,
    /// The port saved last, tried first by the next bind.
    saved_port: Option<u16>,
    token: String,
    events_tx: UnboundedSender<BridgeEvent>,
    events_rx: Option<UnboundedReceiver<BridgeEvent>>,
    connections: HashMap<ConnId, Connection>,
    folders: Vec<PathBuf>,
    /// Per connection, the file view whose selection waits for its coalescing timer.
    selection: Coalescer<ConnId, WeakEntity<FileView>>,
    /// What the commands said to the user, newest last; content-free copy only.
    toasts: Vec<String>,
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
        starting: false,
        generation: 0,
        saved_port,
        token,
        events_tx,
        events_rx: Some(events_rx),
        connections: HashMap::new(),
        folders,
        selection: Coalescer::default(),
        toasts: Vec::new(),
    };
    let mut next = if enabled { bridge.begin_start() } else { None };
    while let Some(job) = next {
        next = bridge.finish_start(job.run(), db);
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
        if self.token.is_empty() {
            eprintln!("[ide] the bridge has no token and stays off");
            return None;
        }
        if self.running.is_some() || self.starting {
            return None;
        }
        self.starting = true;
        Some(StartJob {
            saved_port: self.saved_port,
            token: self.token.clone(),
            folders: self.folders.clone(),
            events: self.events_tx.clone(),
            generation: self.generation,
        })
    }

    /// Installs what a start produced. When the switch moved on while it ran, the listener it
    /// bound is closed again (its lockfile first) and, if the switch is on again, the job for a
    /// fresh start is returned: it binds the saved port once this one has let go of it.
    #[must_use]
    pub(crate) fn finish_start(&mut self, started: Started, db: &SessionStore) -> Option<StartJob> {
        let Started {
            generation,
            outcome,
        } = started;
        self.starting = false;
        let running = match outcome {
            Ok(running) => running,
            Err(error) => {
                eprintln!("[ide] the bridge did not start: {}", error.kind());
                return None;
            }
        };
        if !self.enabled || generation != self.generation || self.running.is_some() {
            running.lock.remove();
            running.bridge.shutdown();
            return if self.enabled { self.begin_start() } else { None };
        }
        self.saved_port = Some(running.port);
        db.save_ide_bridge_endpoint(running.port, &self.token);
        SSE_PORT.store(running.port, Ordering::SeqCst);
        // The catalog may have changed while the lockfile was being written.
        running.lock.rewrite(lock_body(&self.token, &self.folders));
        self.running = Some(running);
        None
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
        self.selection.clear();
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
                self.selection.forget(&conn);
            }
            BridgeEvent::Diagnostics { .. }
            | BridgeEvent::OpenDiff { .. }
            | BridgeEvent::DiffWithdrawn { .. } => {}
        }
    }

    /// The running listener, to send a notification through.
    pub(crate) fn bridge(&self) -> Option<&Bridge> {
        self.running.as_ref().map(|running| &running.bridge)
    }

    /// Connections with no owner, with the pid each connected with.
    pub(crate) fn ownerless(&self) -> Vec<(ConnId, u32)> {
        let mut found: Vec<(ConnId, u32)> = self
            .connections
            .iter()
            .filter(|(_, connection)| connection.owner.is_none())
            .map(|(conn, connection)| (*conn, connection.pid))
            .collect();
        found.sort_by_key(|(conn, _)| conn.0);
        found
    }

    /// A pane closed while its Claude was connected leaves the connection ownerless: owners that
    /// are not among `live` (by pane id and the connection's worktree) are dropped.
    pub(crate) fn drop_dead_owners(&mut self, live: &[LivePane]) {
        for connection in self.connections.values_mut() {
            let alive = |owner: &OwnerPane| {
                live.iter().any(|pane| {
                    pane.pane_id == owner.pane_id && Some(&pane.worktree) == connection.worktree.as_ref()
                })
            };
            if connection.owner.as_ref().is_some_and(|owner| !alive(owner)) {
                connection.owner = None;
            }
        }
    }

    /// Takes what resolving found, for the connections still ownerless.
    pub(crate) fn apply_resolutions(&mut self, found: Vec<Resolved>) {
        for resolved in found {
            let Some(connection) = self.connections.get_mut(&resolved.conn) else {
                continue;
            };
            if connection.owner.is_some() {
                continue;
            }
            match resolved.owner {
                Some((pane_id, worktree)) => {
                    connection.owner = Some(OwnerPane { pane_id });
                    connection.worktree = Some(worktree);
                }
                None => connection.worktree = resolved.cwd_worktree.or(connection.worktree.take()),
            }
        }
    }

    /// The connections owned by a live pane of `worktree`, with the pane. `same` says whether
    /// two spellings name one worktree.
    pub(crate) fn owned_in(
        &self,
        worktree: &Path,
        live: &[LivePane],
        same: impl Fn(&Path, &Path) -> bool,
    ) -> Vec<(ConnId, usize)> {
        let mut found: Vec<(ConnId, usize)> = self
            .connections
            .iter()
            .filter_map(|(conn, connection)| {
                let owner = connection.owner.as_ref()?;
                let home = connection.worktree.as_deref()?;
                let alive = live
                    .iter()
                    .any(|pane| pane.pane_id == owner.pane_id && pane.worktree == home);
                (alive && same(home, worktree)).then_some((*conn, owner.pane_id))
            })
            .collect();
        found.sort_by_key(|(conn, _)| conn.0);
        found
    }

    /// Notes that `view`'s selection is the one to send to `conn`. Returns the delay of the send
    /// to schedule, when none is waiting for that connection yet.
    pub(crate) fn mark_selection(&mut self, conn: ConnId, view: WeakEntity<FileView>) -> Option<Duration> {
        self.selection.change(conn, view)
    }

    /// `conn`'s send is due: the view that changed last.
    pub(crate) fn take_selection(&mut self, conn: ConnId) -> Option<WeakEntity<FileView>> {
        self.selection.fire(&conn)
    }

    /// Records the copy of a toast the app showed.
    pub(crate) fn push_toast(&mut self, message: &str) {
        self.toasts.push(message.to_string());
        if self.toasts.len() > TOASTS_KEPT {
            self.toasts.remove(0);
        }
    }

    /// `surface.ide.state`: what the bridge holds, with no token, no selection and no text of a
    /// proposal; a path appears only as a worktree root. `live` is every terminal Sirio holds: an
    /// owner whose pane is not among them is shown as none, as it is for every action.
    pub(crate) fn state_json(&self, live: &[LivePane]) -> String {
        let mut connections: Vec<(&ConnId, &Connection)> = self.connections.iter().collect();
        connections.sort_by_key(|(conn, _)| conn.0);
        let connections: Vec<Value> = connections
            .into_iter()
            .map(|(conn, connection)| {
                let owner = connection.owner.as_ref().filter(|owner| {
                    live.iter().any(|pane| {
                        pane.pane_id == owner.pane_id && Some(&pane.worktree) == connection.worktree.as_ref()
                    })
                });
                json!({
                    "conn": conn.0,
                    "pid": connection.pid,
                    "owner_pane": owner.map(|owner| format!("pane-{}", owner.pane_id)),
                    "worktree": connection
                        .worktree
                        .as_deref()
                        .map(sirio_project::display_absolute_path),
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
            "toasts": self.toasts,
            "last_delivery": null,
        })
        .to_string()
    }
}

/// The workspace's side of the bridge: everything here reads the tabs and the pane cache that only
/// the workspace holds. `main.rs` calls in at the focus sites, the file view's events and the
/// delivery loop.
impl SirioWorkspace {
    /// Every terminal Sirio holds a view of: the ones mounted in tabs, and the ones parked in
    /// the pane cache for a worktree that is not selected. A pane whose session has no pid yet
    /// carries none.
    pub(crate) fn ide_live_panes(&self, cx: &App) -> Vec<LivePane> {
        let mut panes = Vec::new();
        let mut mounted: HashSet<gpui::EntityId> = HashSet::new();
        for tab in &self.tabs {
            let worktree = self.tab_worktree_path(tab.id);
            tab.panes.for_each(&mut |pane_id, content| {
                if let Some(view) = content.terminal() {
                    mounted.insert(view.entity_id());
                    panes.push(LivePane {
                        pane_id,
                        worktree: worktree.clone(),
                        shell_pid: view.read(cx).shell_pid(),
                    });
                }
            });
        }
        for cached in self.terminal_pane_cache.iter() {
            let Some(pane_id) = cached
                .content_id
                .strip_prefix("terminal-")
                .and_then(|id| id.parse::<usize>().ok())
            else {
                continue;
            };
            if mounted.contains(&cached.controller.entity_id()) {
                continue;
            }
            panes.push(LivePane {
                pane_id,
                worktree: PathBuf::from(&cached.worktree_id),
                shell_pid: cached.controller.read(cx).shell_pid(),
            });
        }
        panes
    }

    /// Runs `then` once every ownerless connection has been resolved against the terminals Sirio
    /// holds now (spec 2026-11-15, 10.3): at once when there is none, else after the resolution
    /// ran on the background executor. Every action that depends on who owns a connection goes
    /// through here.
    fn ide_when_resolved(
        &mut self,
        cx: &mut Context<Self>,
        then: impl FnOnce(&mut Self, &mut Context<Self>) + 'static,
    ) {
        if !cx.has_global::<IdeBridge>() {
            then(self, cx);
            return;
        }
        let live = self.ide_live_panes(cx);
        let pending = cx.update_global(|bridge: &mut IdeBridge, _| {
            bridge.drop_dead_owners(&live);
            bridge.ownerless()
        });
        if pending.is_empty() {
            then(self, cx);
            return;
        }
        let panes: Vec<PaneShell> = live
            .iter()
            .filter_map(|pane| {
                Some(PaneShell {
                    pane_id: pane.pane_id,
                    shell_pid: pane.shell_pid?,
                    worktree: pane.worktree.clone(),
                })
            })
            .collect();
        let worktrees = self.ide_workspace_folders();
        cx.spawn(async move |this, cx| {
            let found = cx
                .background_executor()
                .spawn(async move { resolve(&pending, &panes, &worktrees) })
                .await;
            let _ = this.update(cx, |workspace, cx| {
                workspace.ide_apply_resolutions(found, cx);
                then(workspace, cx);
            });
        })
        .detach();
    }

    /// Takes what a resolution found. An owner that is the focused pane right now becomes the
    /// one used last: a terminal made by the user is the active tab and nothing recorded it.
    fn ide_apply_resolutions(&mut self, found: Vec<Resolved>, cx: &mut Context<Self>) {
        if !cx.has_global::<IdeBridge>() {
            return;
        }
        let owned: Vec<(usize, PathBuf)> =
            found.iter().filter_map(|resolved| resolved.owner.clone()).collect();
        cx.update_global(|bridge: &mut IdeBridge, _| bridge.apply_resolutions(found));
        for (pane_id, worktree) in owned {
            let focused = self.tabs.get(self.active_tab).is_some_and(|tab| {
                tab.focused_pane == pane_id && self.tab_worktree_path(tab.id) == worktree
            });
            if focused {
                self.agent_focus.record(&worktree, AgentTarget::Terminal(pane_id));
            }
        }
    }

    /// A Claude connected, or a worktree's terminals attached: resolve what is still ownerless.
    /// The terminals of a worktree just selected have no session pid for a moment (a restored
    /// one attaches to its host session first), so a connection still ownerless afterwards is
    /// tried again every 500 ms, ten times at most, newest chain only. The actions (an
    /// at-mention, a selection) do not depend on this: they resolve for themselves.
    pub(crate) fn ide_resolve_ownerless(&mut self, cx: &mut Context<Self>) {
        self.ide_resolve_generation += 1;
        self.ide_resolve_retry(self.ide_resolve_generation, 0, cx);
    }

    fn ide_resolve_retry(&mut self, generation: u64, attempt: u32, cx: &mut Context<Self>) {
        const RETRIES: u32 = 10;
        if generation != self.ide_resolve_generation {
            return;
        }
        self.ide_when_resolved(cx, move |_, cx| {
            let left = cx.has_global::<IdeBridge>() && !cx.global::<IdeBridge>().ownerless().is_empty();
            cx.notify();
            if left && attempt < RETRIES {
                cx.spawn(async move |this, cx| {
                    cx.background_executor().timer(Duration::from_millis(500)).await;
                    let _ = this.update(cx, |workspace, cx| {
                        workspace.ide_resolve_retry(generation, attempt + 1, cx);
                    });
                })
                .detach();
            }
        });
    }

    /// The selection of `view` moved or was focused: tell each connection, at most once per
    /// 100 ms and always ending on the latest selection, from the view that changed, whatever
    /// tab is active (a file in the other half stays visible while a terminal is active).
    pub(crate) fn ide_selection_changed(&mut self, view: &Entity<FileView>, cx: &mut Context<Self>) {
        // A drag changes the selection on every mouse move: nobody connected, nothing to do.
        if !cx.has_global::<IdeBridge>() {
            return;
        }
        // Only the connections this file can concern: those owned in its worktree, and the
        // ownerless ones (resolved at the send, and filtered after). One owned elsewhere keeps
        // the view it is waiting to send.
        let path = view.read(cx).path().to_path_buf();
        let Some(worktree) = self.worktree_root_for(&path) else {
            return;
        };
        let live = self.ide_live_panes(cx);
        let bridge = cx.global::<IdeBridge>();
        let mut conns: Vec<sirio_ide::ConnId> = bridge
            .owned_in(&worktree, &live, paths_name_the_same_document)
            .into_iter()
            .map(|(conn, _)| conn)
            .collect();
        conns.extend(bridge.ownerless().into_iter().map(|(conn, _)| conn));
        for conn in conns {
            let delay = cx.update_global(|bridge: &mut IdeBridge, _| {
                bridge.mark_selection(conn, view.downgrade())
            });
            if let Some(delay) = delay {
                cx.spawn(async move |this, cx| {
                    cx.background_executor().timer(delay).await;
                    let _ = this.update(cx, |workspace, cx| workspace.ide_selection_due(conn, cx));
                })
                .detach();
            }
        }
    }

    /// A connection's coalescing timer fired: resolve owners, then send the view's selection as
    /// it is now. A view that is gone sends nothing.
    fn ide_selection_due(&mut self, conn: sirio_ide::ConnId, cx: &mut Context<Self>) {
        let Some(view) = cx
            .update_global(|bridge: &mut IdeBridge, _| bridge.take_selection(conn))
            .and_then(|weak| weak.upgrade())
        else {
            return;
        };
        self.ide_when_resolved(cx, move |workspace, cx| {
            workspace.ide_send_selection(conn, &view, cx);
        });
    }

    fn ide_send_selection(&mut self, conn: sirio_ide::ConnId, view: &Entity<FileView>, cx: &mut Context<Self>) {
        if !cx.has_global::<IdeBridge>() {
            return;
        }
        let path = view.read(cx).path().to_path_buf();
        let Some(worktree) = self.worktree_root_for(&path) else {
            return;
        };
        let live = self.ide_live_panes(cx);
        let bridge = cx.global::<IdeBridge>();
        if !bridge
            .owned_in(&worktree, &live, paths_name_the_same_document)
            .iter()
            .any(|(owned, _)| *owned == conn)
        {
            return;
        }
        let Some(selection) = view.read(cx).ide_selection(cx) else {
            return;
        };
        let position = |(line, character): (u32, u32)| sirio_ide::Pos16 { line, character };
        if let Some(running) = bridge.bridge() {
            running.notify(
                conn,
                sirio_ide::Notification::SelectionChanged {
                    path: selection.path,
                    text: selection.text,
                    start: position(selection.start),
                    end: position(selection.end),
                },
            );
        }
    }

    /// "Add to Agent Thread": resolve owners, then deliver.
    pub(crate) fn send_selection_to_agent(
        &mut self,
        path: &Path,
        lines: (usize, usize),
        text: &str,
        language: &'static str,
        cx: &mut Context<Self>,
    ) {
        let (path, text) = (path.to_path_buf(), text.to_owned());
        self.ide_when_resolved(cx, move |workspace, cx| {
            workspace.deliver_at_mention(&path, lines, &text, language, cx);
        });
    }

    /// The agent of the file's worktree an at-mention goes to: the first of those the user
    /// focused, most recent first, that can take it (a chat, or a pane that owns a live
    /// connection); else the owned terminal that connected last; else a chat; else a toast.
    fn deliver_at_mention(
        &mut self,
        path: &Path,
        lines: (usize, usize),
        text: &str,
        language: &'static str,
        cx: &mut Context<Self>,
    ) {
        let worktree = self.worktree_root_for(path);
        let candidates = worktree
            .as_deref()
            .map(|root| self.agent_focus.targets(root, paths_name_the_same_document))
            .unwrap_or_default();
        let live = self.ide_live_panes(cx);
        let owned = match (&worktree, cx.has_global::<IdeBridge>()) {
            (Some(root), true) => cx
                .global::<IdeBridge>()
                .owned_in(root, &live, paths_name_the_same_document),
            _ => Vec::new(),
        };
        let chats = worktree.as_deref().map(|root| self.chats_in(root)).unwrap_or_default();
        let mut chosen: Option<Result<Entity<Chat>, (sirio_ide::ConnId, usize)>> = None;
        for target in candidates {
            match target {
                AgentTarget::Chat(id) => {
                    if let Some(chat) = chats.iter().find(|chat| chat.entity_id() == id) {
                        chosen = Some(Ok(chat.clone()));
                    }
                }
                AgentTarget::Terminal(pane) => {
                    if let Some(found) = owned.iter().find(|(_, owner)| *owner == pane) {
                        chosen = Some(Err(*found));
                    }
                }
            }
            if chosen.is_some() {
                break;
            }
        }
        // Nothing focused qualifies (after a relaunch nothing has been focused): the owned
        // terminal that connected last, and only then a chat.
        if chosen.is_none() {
            chosen = owned.last().map(|found| Err(*found));
        }
        if chosen.is_none() {
            chosen = self.active_chat_for(path).map(Ok);
        }
        match chosen {
            Some(Ok(chat)) => {
                let label = self
                    .repo_relative(path)
                    .unwrap_or_else(|| path.display().to_string());
                let payload =
                    sirio_ui::file_context_menu::agent_payload(&label, lines, text, language);
                chat.update(cx, |chat, cx| chat.control_append(&payload, cx));
            }
            Some(Err((conn, pane))) => {
                let note = sirio_ide::Notification::AtMentioned {
                    path: path.to_path_buf(),
                    line_start: lines.0.saturating_sub(1) as u32,
                    line_end: lines.1.saturating_sub(1) as u32,
                };
                if let Some(running) = cx.global::<IdeBridge>().bridge() {
                    running.notify(conn, note);
                }
                let home = worktree.unwrap_or_default();
                let tab = self.tabs.iter().find(|tab| {
                    tab.panes.contains(pane) && paths_name_the_same_document(&self.tab_worktree_path(tab.id), &home)
                });
                self.pending_ide_focus = tab.map(|tab| (tab.id, pane));
                cx.notify();
            }
            None => {
                if cx.has_global::<IdeBridge>() {
                    cx.update_global(|bridge: &mut IdeBridge, _| {
                        bridge.push_toast(NO_AGENT_TOAST);
                    });
                }
                self.show_toast(NO_AGENT_TOAST, cx);
            }
        }
    }

    /// Records that the user focused `pane_id` of tab `tab_id`, when it is an agent's place: a
    /// terminal pane or a chat (spec 2026-11-15, 4.5). Whether it can take an at-mention is
    /// checked only when one is sent.
    pub(crate) fn record_agent_focus(&mut self, tab_id: usize, pane_id: usize) {
        let Some(tab) = self.tabs.iter().find(|tab| tab.id == tab_id) else {
            return;
        };
        let mut target = None;
        tab.panes.for_each(&mut |id, content| {
            if id == pane_id {
                target = match content {
                    TabContent::Chat(chat) => Some(AgentTarget::Chat(chat.entity_id())),
                    TabContent::Terminal { .. } => Some(AgentTarget::Terminal(pane_id)),
                    _ => None,
                };
            }
        });
        if let Some(target) = target {
            let worktree = self.tab_worktree_path(tab_id);
            self.agent_focus.record(&worktree, target);
        }
    }

    /// The tab at `index` became the active one: its agent pane is the one used last, and a file
    /// in it says its selection (a focus notifies nothing of its own).
    pub(crate) fn ide_tab_focused(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(tab) = self.tabs.get(index) else {
            return;
        };
        let (tab_id, focused) = (tab.id, tab.focused_pane);
        let mut file = None;
        tab.panes.for_each(&mut |_, content| {
            if let TabContent::File { view } = content
                && file.is_none()
            {
                file = Some(view.clone());
            }
        });
        self.record_agent_focus(tab_id, focused);
        if let Some(view) = file {
            self.ide_selection_changed(&view, cx);
        }
    }
}
