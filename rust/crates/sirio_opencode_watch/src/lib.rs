//! OpenCode's session status, read from the background service its TUI talks to.
//!
//! OpenCode v2 reports nothing in the terminal (no OSC 7501), but every session's work
//! happens in one per-user service with an HTTP API and an event stream. This crate finds
//! that service (`endpoint`), keeps one connection to it on a thread of its own (`Watch`,
//! the wire in `stream`), and links its sessions to terminal panes with certainty or not
//! at all (`link`). It decides nothing about what a status means to the app: it hands
//! over, for every session it knows, the state the service last reported and when.
//!
//! It degrades rather than fails. No registration, a refused connection, a stream that
//! ends or goes quiet, a reply it does not understand: each is a kind name logged once
//! per run (`[opencode-watch] <kind>`, never content) and a `WatchEvent::Down`; the watch
//! then reconnects, re-reading the registration each time, because a restarted service has
//! a new port, password and pid. Event names it does not know are ignored.
//!
//! Nothing here touches the network on the caller's thread: `Watch::start` does a
//! `canonicalize` per directory (the filesystem) and spawns the thread, and
//! `set_directories` only queues a message.

mod endpoint;
mod link;
mod stream;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

pub use endpoint::{Endpoint, EndpointError, discover};
pub use link::{OcPane, TITLED, link};

use stream::{Client, Table};

/// What a session is doing, as the service last said.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OcState {
    /// An execution is running.
    Busy,
    /// An execution waits on a permission or a question.
    Waiting,
    /// No execution is running.
    Idle,
    /// The last execution failed.
    Error,
}

/// One server session in a watched directory.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OcSession {
    pub id: String,
    /// The directory the session was started in, canonical.
    pub directory: PathBuf,
    /// Empty until the service has named it.
    pub title: String,
    pub created_ms: u64,
    pub state: OcState,
    /// The service's own time of the event that set `state` (a session read from the
    /// list: its last update), in milliseconds.
    pub at_ms: u64,
}

/// What the watch tells its owner.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WatchEvent {
    /// Every session the watch knows, whole: the connection is up and this is the state.
    Sessions(Vec<OcSession>),
    /// The connection is down or was never there; every earlier session is unknown. A kind
    /// name, never content. Sent once per failure kind until the connection is up again.
    Down(&'static str),
}

/// A message to the watch's thread: from its owner, or from the stream's reader.
pub(crate) enum Msg {
    Directories(Vec<PathBuf>),
    Stop,
    /// One `data:` frame of connection `.0`.
    Frame(u64, String),
    /// A comment line (the heartbeat) of connection `.0`.
    Beat(u64),
    /// Connection `.0` ended, for a reason of this kind.
    End(u64, &'static str),
}

/// First wait before reconnecting; it doubles to [`RETRY_MAX`].
const RETRY_MIN: Duration = Duration::from_secs(2);
const RETRY_MAX: Duration = Duration::from_secs(60);
/// How long the first frame of a stream may take.
const FIRST_FRAME: Duration = Duration::from_secs(5);
/// A stream that sends nothing, not even a heartbeat (about every ten seconds), for this
/// long is dead.
const QUIET: Duration = Duration::from_secs(35);
/// The longest the thread waits on its inbox before looking at the clock.
const TICK: Duration = Duration::from_secs(2);

/// The failure kinds logged so far in this process: each is logged once per run of the
/// app, however many watches it starts.
static LOGGED: Mutex<Vec<&'static str>> = Mutex::new(Vec::new());

fn log_once(kind: &'static str) {
    let Ok(mut logged) = LOGGED.lock() else {
        return;
    };
    if !logged.contains(&kind) {
        logged.push(kind);
        eprintln!("[opencode-watch] {kind}");
    }
}

/// Sets the flag when it goes out of scope: the connection that holds it was left, so its
/// reader has nobody to read for.
struct Abandon(Arc<AtomicBool>);

impl Drop for Abandon {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

/// The thread that keeps the connection. Dropping it asks the thread to end; `stop` waits.
pub struct Watch {
    inbox: Sender<Msg>,
    stopping: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Watch {
    /// Starts watching on a thread of its own. `locator` finds the endpoint and is called
    /// again on every (re)connect; `sink` receives what the watch has to say, on the
    /// watch's thread; `directories` are the folders whose sessions are wanted (read from
    /// the service's list at connect, and followed through its events after).
    pub fn start(
        locator: impl Fn() -> Result<Endpoint, EndpointError> + Send + 'static,
        sink: impl Fn(WatchEvent) + Send + 'static,
        directories: Vec<PathBuf>,
    ) -> Watch {
        let (inbox, messages) = mpsc::channel();
        let stopping = Arc::new(AtomicBool::new(false));
        let run = Run {
            locator: Box::new(locator),
            sink: Box::new(sink),
            messages,
            inbox: inbox.clone(),
            stopping: stopping.clone(),
            directories: directories.iter().map(stream::canonical).collect(),
            down: None,
            connection: 0,
        };
        let thread = std::thread::Builder::new()
            .name("opencode-watch".into())
            .spawn(move || run.run())
            .inspect_err(|_| log_once("spawn_failed"))
            .ok();
        Watch {
            inbox,
            stopping,
            thread,
        }
    }

    /// Replaces the folders whose sessions are wanted. The thread reads the new ones from
    /// the service's list and forgets the others.
    pub fn set_directories(&self, directories: Vec<PathBuf>) {
        let _ = self.inbox.send(Msg::Directories(directories));
    }

    /// Ends the thread and waits for it: it looks at its inbox at least every two seconds,
    /// and a call it is in the middle of is bounded by the call's own two-second timeout.
    /// The reader of an open stream outlives it by at most one heartbeat.
    pub fn stop(mut self) {
        self.signal();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }

    fn signal(&self) {
        self.stopping.store(true, Ordering::SeqCst);
        let _ = self.inbox.send(Msg::Stop);
    }
}

impl Drop for Watch {
    fn drop(&mut self) {
        if self.thread.is_some() {
            self.signal();
        }
    }
}

/// How one connection attempt ended.
enum Outcome {
    Stopped,
    /// Lost, for a reason of this kind; `up` when it had been serving before that.
    Lost { kind: &'static str, up: bool },
}

struct Run {
    locator: Box<dyn Fn() -> Result<Endpoint, EndpointError> + Send>,
    sink: Box<dyn Fn(WatchEvent) + Send>,
    messages: Receiver<Msg>,
    inbox: Sender<Msg>,
    stopping: Arc<AtomicBool>,
    directories: Vec<PathBuf>,
    /// The kind of the last `Down` sent, until the connection is up again.
    down: Option<&'static str>,
    connection: u64,
}

impl Run {
    fn run(mut self) {
        let mut backoff = RETRY_MIN;
        loop {
            if self.stopping.load(Ordering::SeqCst) {
                return;
            }
            match self.connect() {
                Outcome::Stopped => return,
                Outcome::Lost { kind, up } => {
                    // Stopped while the call was in flight: nobody wants to hear of it.
                    if self.stopping.load(Ordering::SeqCst) {
                        return;
                    }
                    if up {
                        backoff = RETRY_MIN;
                    }
                    self.report_down(kind);
                    if !self.pause(backoff) {
                        return;
                    }
                    backoff = (backoff * 2).min(RETRY_MAX);
                }
            }
        }
    }

    /// Logs a failure kind once per run and tells the owner once per kind.
    fn report_down(&mut self, kind: &'static str) {
        log_once(kind);
        if self.down != Some(kind) {
            self.down = Some(kind);
            (self.sink)(WatchEvent::Down(kind));
        }
    }

    /// Waits `wait` before the next attempt, still taking messages. False when stopped.
    fn pause(&mut self, wait: Duration) -> bool {
        let deadline = Instant::now() + wait;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return true;
            }
            match self.messages.recv_timeout(left.min(TICK)) {
                Ok(Msg::Stop) | Err(RecvTimeoutError::Disconnected) => return false,
                Ok(Msg::Directories(directories)) => self.set_directories(directories),
                // A late word from the connection that was lost.
                Ok(_) | Err(RecvTimeoutError::Timeout) => {}
            }
            if self.stopping.load(Ordering::SeqCst) {
                return false;
            }
        }
    }

    fn set_directories(&mut self, directories: Vec<PathBuf>) {
        self.directories = directories.iter().map(stream::canonical).collect();
    }

    /// One connection: find the service, confirm it, subscribe, read what was before the
    /// subscription, then follow the stream until it ends or the watch is stopped.
    fn connect(&mut self) -> Outcome {
        let lost = |kind| Outcome::Lost { kind, up: false };
        let endpoint = match (self.locator)() {
            Ok(endpoint) => endpoint,
            Err(error) => return lost(error.kind()),
        };
        let client = Client::new(&endpoint);
        if let Err(kind) = client.confirm() {
            return lost(kind);
        }
        // Subscribe first, read the list after: an event between the two waits in the
        // inbox and is applied once the list is in.
        self.connection += 1;
        let connection = self.connection;
        // Set on every way out of this function: the reader of an abandoned stream ends at
        // its next line instead of keeping the socket open for the life of the app.
        let abandoned = Arc::new(AtomicBool::new(false));
        let _abandon = Abandon(abandoned.clone());
        if let Err(kind) = client.open_stream(connection, self.inbox.clone(), abandoned) {
            return lost(kind);
        }
        let mut heard = Instant::now();
        let first_by = heard + FIRST_FRAME;
        loop {
            match self.messages.recv_timeout(TICK) {
                Ok(Msg::Stop) | Err(RecvTimeoutError::Disconnected) => return Outcome::Stopped,
                Ok(Msg::Directories(directories)) => self.set_directories(directories),
                Ok(Msg::Frame(c, text)) if c == connection => {
                    let first = serde_json::from_str::<serde_json::Value>(&text).ok();
                    let kind = first.as_ref().and_then(|f| f.get("type")?.as_str());
                    if kind == Some("server.connected") {
                        heard = Instant::now();
                        break;
                    }
                    return lost(stream::UNEXPECTED_STREAM);
                }
                Ok(Msg::Beat(c)) if c == connection => {}
                Ok(Msg::End(c, kind)) if c == connection => return lost(kind),
                Ok(_) => {}
                Err(RecvTimeoutError::Timeout) => {
                    if Instant::now() > first_by {
                        return lost(stream::STALLED);
                    }
                }
            }
            if self.stopping.load(Ordering::SeqCst) {
                return Outcome::Stopped;
            }
        }

        let mut table = Table::default();
        let mut seeded: Vec<PathBuf> = Vec::new();
        let mut up = false;
        let mut announce = true;
        loop {
            if self.stopping.load(Ordering::SeqCst) {
                return Outcome::Stopped;
            }
            // What was before the subscription, for each folder not read yet.
            let fresh: Vec<PathBuf> = self
                .directories
                .iter()
                .filter(|directory| !seeded.contains(directory))
                .cloned()
                .collect();
            if !fresh.is_empty() {
                if let Err(kind) = seed(&client, &mut table, &fresh) {
                    return Outcome::Lost { kind, up };
                }
                seeded.extend(fresh);
                announce = true;
            }
            if seeded.iter().any(|directory| !self.directories.contains(directory)) {
                seeded.retain(|directory| self.directories.contains(directory));
                table.retain_directories(&self.directories);
                announce = true;
            }
            if self.stopping.load(Ordering::SeqCst) {
                return Outcome::Stopped;
            }
            if announce {
                announce = false;
                up = true;
                self.down = None;
                (self.sink)(WatchEvent::Sessions(table.snapshot()));
            }
            match self.messages.recv_timeout(TICK) {
                Ok(Msg::Stop) | Err(RecvTimeoutError::Disconnected) => return Outcome::Stopped,
                Ok(Msg::Directories(directories)) => self.set_directories(directories),
                Ok(Msg::Frame(c, text)) if c == connection => {
                    heard = Instant::now();
                    // A frame that is not JSON is not an event this watch knows.
                    if let Ok(frame) = serde_json::from_str::<serde_json::Value>(&text)
                        && table.apply(&frame, &self.directories)
                    {
                        announce = true;
                    }
                }
                Ok(Msg::Beat(c)) if c == connection => heard = Instant::now(),
                Ok(Msg::End(c, kind)) if c == connection => return Outcome::Lost { kind, up },
                // A late word from a connection that was lost earlier.
                Ok(_) => {}
                Err(RecvTimeoutError::Timeout) => {
                    if heard.elapsed() > QUIET {
                        return Outcome::Lost {
                            kind: stream::STALLED,
                            up,
                        };
                    }
                }
            }
        }
    }
}

/// Reads the sessions of `directories` from the service's list into `table`, with what
/// is running and what waits on an answer.
fn seed(client: &Client, table: &mut Table, directories: &[PathBuf]) -> Result<(), &'static str> {
    let active = client.active()?;
    for directory in directories {
        for row in client.sessions_in(directory)? {
            let id = row.get("id").and_then(serde_json::Value::as_str).unwrap_or_default();
            let running = active.contains(id);
            let waiting = running && client.waiting(id);
            table.seed(&row, running, waiting);
        }
    }
    Ok(())
}
