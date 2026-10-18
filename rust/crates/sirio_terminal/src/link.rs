//! What a terminal pane's owner thread talks to instead of a PTY.
//!
//! A [`SessionLink`] is the view side of a terminal session: bytes in, live
//! [`LinkChunk`]s out, in order. Two engines implement it — [`LocalLink`],
//! the in-process fallback (SP2 spec §1, §10), and [`HostLink`], a session
//! on the host — and the owner thread cannot tell them apart.

use std::collections::BTreeMap;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc,
    Mutex,
};
use std::time::{Duration, Instant};

use sirio_host_client::connection::CallError;
use sirio_host_client::term::TermClient;
use sirio_host_client::{StreamItem, StreamReceiver};
use sirio_host_protocol::messages::ErrorCode;
use sirio_host_protocol::term::{
    CreateParams, ExitStatus, Origin, SessionEvent, TermCommand, TermSize, b64_encode,
};
use sirio_term_session::{
    Event, Exit, Session, SpawnSpec,
    fanout::{Attachment, Chunk},
};

use crate::{TerminalExitStatus, TerminalShell};

/// What a terminal pane's owner thread talks to instead of a PTY.
pub trait SessionLink: Send + 'static {
    fn session_id(&self) -> &str;
    fn shell_pid(&self) -> Option<u32>;
    fn input(&self, bytes: Vec<u8>);
    fn feed(&self, bytes: Vec<u8>);
    fn resize(&self, cols: u16, rows: u16);
    /// Bytes for the replica, in order. `Snapshot` resets the replica first.
    fn chunks(&self) -> &mpsc::Receiver<LinkChunk>;
    /// End the session (tab, worktree or project closed; End all sessions).
    fn end(&self);
    /// Stop watching; the session lives on (Quit, eviction, Drop). For a
    /// `LocalLink` this ends it too: an in-process session cannot outlive the app.
    fn detach(&self);
    fn survives_app(&self) -> bool;
}

pub enum LinkChunk {
    Snapshot { cols: u16, rows: u16, bytes: Vec<u8> },
    Output(Vec<u8>),
    Title(String),
    Foreground(bool),
    Exited(TerminalExitStatus),
    /// The host is gone or the stream could not be re-established.
    Lost,
}

/// Opens links. The app installs a `HostSessions` factory once the host is
/// known; until then `open` answers `Pending`, and without any factory (tests,
/// a build with no host) every pane gets a `LocalLink`.
pub trait SessionFactory: Send + Sync + 'static {
    fn open(&self, request: OpenRequest) -> OpenOutcome;

    /// The view that asked for `session_id` no longer wants it: it was ended
    /// or dropped while its open was still in flight. The factory applies
    /// the intent when the open finishes, so nothing is left attached or
    /// created for a tab that is gone (SP2 review finding 12). The default
    /// does nothing.
    fn abandon(&self, _session_id: &str, _intent: Abandon) {}
}

/// What a view wants done with a session its open has not yet delivered:
/// `End` closes it (an owned tab closed while connecting), `Detach` only
/// stops the view from attaching to it. `End` wins over `Detach`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Abandon {
    Detach,
    End,
}

impl Abandon {
    /// The stronger of two intents.
    pub fn merge(self, other: Abandon) -> Abandon {
        if self == Abandon::End || other == Abandon::End {
            Abandon::End
        } else {
            Abandon::Detach
        }
    }
}

#[derive(Clone, Debug)]
pub struct OpenRequest {
    pub session_id: String,
    pub cwd: std::path::PathBuf,
    pub shell: TerminalShell,
    pub cols: u16,
    pub rows: u16,
    /// Fed before the child's first byte when the session is created here
    /// (a restored tab's scrollback); ignored when an existing session is
    /// attached.
    pub prelude: Vec<u8>,
    /// A restart re-opens under the same id: the factory must close the old
    /// session synchronously before re-creating, or the open attaches to the
    /// still-listed dying session (Task 9 review).
    pub recreate: bool,
    /// Attach only (an attached-not-owned tab, Show session): a session the
    /// host no longer lists fails the open as *Session lost* instead of being
    /// created again as a new shell.
    pub attach_only: bool,
    /// A restore: a persisted tab carrying its `session_id`. Only a restore
    /// (and an attach-only open) may attach to a session the host already
    /// lists; every other open is a create (fix batch 1, finding 11).
    pub restore: bool,
    /// Every worktree the app knows, for a restore's ownership check (fix batch
    /// 5, N3): a listed session of another known worktree is not this tab's.
    /// Empty when nothing is known, which lets every listed session through.
    pub known_worktrees: std::sync::Arc<[std::path::PathBuf]>,
    /// Variables this terminal adds over the user environment (`for_child()`),
    /// such as `SIRIO_WORKTREE_ID` for a terminal of a worktree. A host session
    /// is created with them in its environment too.
    pub extra_env: BTreeMap<String, String>,
}

pub enum OpenOutcome {
    Linked(Box<dyn SessionLink>),
    /// The host is still being found; ask again on `SessionFactoryReady`.
    Pending,
    /// The host open failed. A restored or fresh terminal shows the reason
    /// with Retry and never falls back to a local shell: the session may be
    /// live on a host that is only slow or unverifiable (spec §8, finding 7).
    Failed(String),
    /// No host is available (absent, or without the `terminal` capability):
    /// the only case that falls back to `LocalLink` (spec §8).
    NoHost(String),
    /// The session id is already held by another session on the host, and
    /// this open would create a session under it (a fresh open, or a restore
    /// whose listed session belongs to another worktree). Nothing is attached
    /// or closed: the owner gives the pane a fresh id and opens it again
    /// (fix batch 5, N5 and #11).
    Taken(String),
}

/// Why a host open produced no link. `Taken` is the only failure its owner
/// answers by changing the pane's id; every other one is shown as it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostOpenError {
    Taken(String),
    Failed(String),
}

impl From<String> for HostOpenError {
    fn from(message: String) -> Self {
        Self::Failed(message)
    }
}

impl std::fmt::Display for HostOpenError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Taken(message) | Self::Failed(message) => formatter.write_str(message),
        }
    }
}

#[derive(Clone)]
pub struct SessionFactoryGlobal(pub Option<Arc<dyn SessionFactory>>);
impl gpui::Global for SessionFactoryGlobal {}

/// What a host open does, decided before any host call (spec §5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OpenPlan {
    /// The host lists the session: attach to it.
    Attach,
    /// Not listed, owned open: create it, then attach.
    Create,
    /// Not listed, attach-only: the session is lost. Never created.
    Lost,
    /// Listed, but this open is a fresh create: the id belongs to another
    /// session, so this open must not attach to it.
    Duplicate,
}

/// The open decision. Attach-only opens attach to a listed session and lose
/// a missing one. Restores attach to a listed session and create a missing
/// one. Every other open is a create: a listed id is a duplicate, never an
/// attach (fix batch 1, finding 11).
fn open_plan(listed: bool, attach_only: bool, restore: bool) -> OpenPlan {
    match (listed, attach_only, restore) {
        (true, true, _) => OpenPlan::Attach,
        (false, true, _) => OpenPlan::Lost,
        (true, false, true) => OpenPlan::Attach,
        (false, false, true) => OpenPlan::Create,
        (true, false, false) => OpenPlan::Duplicate,
        (false, false, false) => OpenPlan::Create,
    }
}

/// The in-process engine: the temporary fallback (SP2 spec §1, §10).
pub struct LocalLink {
    session_id: String,
    shell_pid: Option<u32>,
    incoming: mpsc::Receiver<LinkChunk>,
    session: Arc<Session>,
    done: Arc<AtomicBool>,
}

impl LocalLink {
    pub fn open(request: &OpenRequest) -> Result<LocalLink, String> {
        // An attached tab never starts a shell of its own (spec §5): with no
        // host session to attach to, there is nothing to show.
        if request.attach_only {
            return Err(format!(
                "Session lost: {} is no longer on the host",
                request.session_id
            ));
        }
        let program = match &request.shell {
            TerminalShell::System => None,
            TerminalShell::WithArguments { program, args } => {
                Some((program.clone(), args.clone()))
            }
        };
        // The user's environment, not the launcher's: an AppImage launch would
        // otherwise hand its mount paths to the shell. `Session::spawn` exports
        // the pane id (`SIRIO_PANE_ID`, `TILLER_PANE_ID`) from `spec.id` on top.
        let mut env = sirio_env::for_child();
        env.extend(request.extra_env.clone());
        let (events_tx, events_rx) = mpsc::channel();
        let session = Session::spawn(
            SpawnSpec {
                id: request.session_id.clone(),
                cwd: request.cwd.clone(),
                program,
                env,
                cols: request.cols,
                rows: request.rows,
                prelude: request.prelude.clone(),
            },
            events_tx,
        )
        .map_err(|error| error.to_string())?;
        let session = Arc::new(session);
        let attach = session.attach(None);
        let size = attach.size;
        let attachment = attach.attachment;
        let (chunks_tx, chunks_rx) = mpsc::channel();
        let done = Arc::new(AtomicBool::new(false));
        let shell_pid = Some(session.info().pid);
        std::thread::Builder::new()
            .name(format!("sirio-terminal-link-{}", request.session_id))
            .spawn({
                let session = Arc::clone(&session);
                let done = Arc::clone(&done);
                move || {
                    forward_local(session, attachment, events_rx, chunks_tx, size, done);
                }
            })
            .map_err(|error| error.to_string())?;
        Ok(LocalLink {
            session_id: request.session_id.clone(),
            shell_pid,
            incoming: chunks_rx,
            session,
            done,
        })
    }
}

/// Turns one attachment and the session events into [`LinkChunk`]s.
///
/// Every forwarded chunk releases its queue charge at once (`consumed`): this
/// thread drains the attachment continuously, so it is the view's
/// back-pressure — a view that stops draining these lets this channel grow,
/// and a session that outruns even this thread lags the attachment, which is
/// re-attached below. `size` is the session's size from the `Attach` that
/// produced `attachment`, refreshed on every re-attach.
fn forward_local(
    session: Arc<Session>,
    mut attachment: Attachment,
    events_rx: mpsc::Receiver<Event>,
    chunks_tx: mpsc::Sender<LinkChunk>,
    mut size: (u16, u16),
    done: Arc<AtomicBool>,
) {
    let mut next_offset = 0u64;
    let reattach = |attachment: &mut Attachment, size: &mut (u16, u16), next_offset: u64| {
        let fresh = session.attach(Some(next_offset));
        *size = fresh.size;
        *attachment = fresh.attachment;
    };
    loop {
        if done.load(Ordering::Acquire) {
            return;
        }
        match attachment.receiver.recv_timeout(Duration::from_millis(4)) {
            Ok(chunk) => {
                match chunk {
                    Chunk::Snapshot(bytes) => {
                        let charged = bytes.len();
                        if chunks_tx
                            .send(LinkChunk::Snapshot {
                                cols: size.0,
                                rows: size.1,
                                bytes,
                            })
                            .is_err()
                        {
                            return;
                        }
                        attachment.consumed(charged);
                    }
                    Chunk::Output { offset, bytes } => {
                        next_offset = offset + bytes.len() as u64;
                        let charged = bytes.len();
                        // The engine follows every snapshot (and resync) with
                        // an empty Output carrying the journal head: forward it
                        // like any other chunk — the owner thread treats an
                        // empty Output as a no-op without a wakeup.
                        if chunks_tx.send(LinkChunk::Output(bytes)).is_err() {
                            return;
                        }
                        attachment.consumed(charged);
                    }
                    // The alternate screen was left; a fresh Snapshot follows
                    // on this same attachment.
                    Chunk::Resync => {}
                    Chunk::Lagged => {
                        reattach(&mut attachment, &mut size, next_offset);
                    }
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                if done.load(Ordering::Acquire) {
                    return;
                }
                reattach(&mut attachment, &mut size, next_offset);
            }
        }
        while let Ok(event) = events_rx.try_recv() {
            let chunk = match event {
                Event::Title(title) => LinkChunk::Title(title),
                Event::Foreground(running) => LinkChunk::Foreground(running),
                Event::Exited(exit) => LinkChunk::Exited(session_exit_status(&exit)),
            };
            if chunks_tx.send(chunk).is_err() {
                return;
            }
        }
    }
}

impl SessionLink for LocalLink {
    fn session_id(&self) -> &str {
        &self.session_id
    }

    fn shell_pid(&self) -> Option<u32> {
        self.shell_pid
    }

    fn input(&self, bytes: Vec<u8>) {
        let _ = self.session.input(bytes);
    }

    fn feed(&self, bytes: Vec<u8>) {
        self.session.feed(bytes);
    }

    fn resize(&self, cols: u16, rows: u16) {
        self.session.resize(cols, rows);
    }

    fn chunks(&self) -> &mpsc::Receiver<LinkChunk> {
        &self.incoming
    }

    fn end(&self) {
        self.done.store(true, Ordering::Release);
        self.session.close();
    }

    fn detach(&self) {
        self.done.store(true, Ordering::Release);
        self.session.close();
    }

    fn survives_app(&self) -> bool {
        false
    }
}

impl Drop for LocalLink {
    fn drop(&mut self) {
        self.done.store(true, Ordering::Release);
        self.session.close();
    }
}

/// A session on the host.
pub struct HostLink {
    session_id: String,
    shell_pid: Option<u32>,
    incoming: mpsc::Receiver<LinkChunk>,
    control: Arc<Mutex<HostControl>>,
    /// The owner thread hands resize and feed to `sender_thread` and never
    /// waits on the host for them (SP2 review finding 5).
    outbound: mpsc::Sender<Outbound>,
    latest_size: Arc<Mutex<Option<(u16, u16)>>>,
    /// The factory's reconnect closure: re-opens a `Mux` to the host's
    /// endpoint. Cloned into the forwarder at spawn; kept here so the link
    /// owns its own re-establishment path.
    #[allow(dead_code)]
    reconnect: ReconnectFn,
    done: Arc<AtomicBool>,
}

type ReconnectFn = Arc<dyn Fn() -> Option<TermClient> + Send + Sync>;

/// A request the owner thread leaves for the link's sender thread.
enum Outbound {
    Feed(Vec<u8>),
    /// Latest wins: the size itself travels in `HostLink::latest_size`, so
    /// several queued resizes send one call, at the newest size.
    Resize,
}

/// Sends the link's feeds and resizes, in order, each through the client
/// current at send time (a reconnect swaps it). A call that waits on a stopped
/// host blocks this thread only. Ends when the link is dropped.
fn sender_thread(
    session_id: String,
    control: Arc<Mutex<HostControl>>,
    outbound: mpsc::Receiver<Outbound>,
    latest_size: Arc<Mutex<Option<(u16, u16)>>>,
) {
    while let Ok(message) = outbound.recv() {
        let client = {
            let control = control
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            control.client.clone()
        };
        match message {
            Outbound::Feed(bytes) => {
                if let Err(error) = client.feed(&session_id, &bytes) {
                    eprintln!("[sirio] host link {session_id}: feed failed: {error}");
                }
            }
            Outbound::Resize => {
                let size = latest_size
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .take();
                if let Some((cols, rows)) = size {
                    if let Err(error) = client.resize(&session_id, cols, rows) {
                        eprintln!("[sirio] host link {session_id}: resize failed: {error}");
                    }
                }
            }
        }
    }
}

struct HostControl {
    client: TermClient,
    stream: u32,
}

impl HostLink {
    /// Attach to `session_id` if the host lists it (any state), else create it
    /// with `origin: app`, the app's environment and `prelude`, then attach.
    pub fn attach_or_create(
        client: TermClient,
        request: &OpenRequest,
    ) -> Result<HostLink, HostOpenError> {
        Self::attach_or_create_with_reconnect(client, request, Arc::new(|| None))
    }

    /// [`HostLink::attach_or_create`] with the reconnect closure the factory
    /// hands over: re-opening a `Mux` to the host's endpoint, used to
    /// re-establish the stream after the connection drops.
    pub fn attach_or_create_with_reconnect(
        client: TermClient,
        request: &OpenRequest,
        reconnect: ReconnectFn,
    ) -> Result<HostLink, HostOpenError> {
        // Subscribed before the listing and the attach: an exit after this
        // point is an event, and one before it is in the listing (finding 9).
        let events_rx = client.events().map_err(|error| error.to_string())?;
        let listed = client.list().map_err(|error| error.to_string())?;
        let listed_summary = listed
            .sessions
            .iter()
            .find(|summary| summary.session_id == request.session_id);
        let listed_pid = listed_summary.map(|summary| summary.pid);
        let listed_exit = listed_summary.and_then(|summary| summary.exit.clone());
        let plan = open_plan(listed_pid.is_some(), request.attach_only, request.restore);
        if plan == OpenPlan::Lost {
            return Err(HostOpenError::Failed(format!(
                "Session lost: {} is no longer on the host",
                request.session_id
            )));
        }
        if plan == OpenPlan::Duplicate {
            return Err(HostOpenError::Taken(format!(
                "{} is already a session on the host; a new terminal does not attach to it",
                request.session_id
            )));
        }
        // A restore attaches only to a session its own worktree holds (fix batch
        // 5, N3). A listed session of another known worktree is a stale id for
        // this tab: the pane takes a fresh id and creates its own session.
        if plan == OpenPlan::Attach
            && !request.attach_only
            && let Some(summary) = listed_summary
            && !restore_may_attach(
                std::path::Path::new(&summary.cwd),
                &request.cwd,
                &request.known_worktrees,
            )
        {
            return Err(HostOpenError::Taken(format!(
                "{} is held by another worktree's session on the host",
                request.session_id
            )));
        }
        let created_pid = if plan == OpenPlan::Create {
            // The session starts in the user's environment. A host without
            // `term.env_exact` only extends its own, so it is sent the blanks
            // that empty what the app had and the user lacks (`host_create_env`).
            let exact = client.env_exact();
            let params = CreateParams {
                session_id: Some(request.session_id.clone()),
                cwd: request.cwd.to_string_lossy().into_owned(),
                command: term_command(&request.shell),
                env: sirio_env::host_create_env(
                    exact,
                    &sirio_env::for_child(),
                    &request.extra_env,
                    &sirio_env::absent_from_user(),
                ),
                size: TermSize {
                    cols: request.cols,
                    rows: request.rows,
                },
                origin: Origin::App,
                parent: None,
                prelude: if request.prelude.is_empty() {
                    None
                } else {
                    Some(b64_encode(&request.prelude))
                },
                env_exact: exact,
            };
            match client.create(&params) {
                Ok(created) => Some(created.pid),
                // A racing create answered `invalid_params` for a duplicate
                // id: a restore attaches to the session that now holds it. A
                // fresh open never does (fix batch 1, finding 11).
                Err(CallError::Remote(body))
                    if request.restore
                        && body.code == ErrorCode::InvalidParams
                        && body.message.contains("duplicate") =>
                {
                    None
                }
                // A fresh open that lost the race to another create of its id
                // is refused the same way as a listed one (taken, not attached).
                Err(CallError::Remote(body))
                    if body.code == ErrorCode::InvalidParams
                        && body.message.contains("duplicate") =>
                {
                    return Err(HostOpenError::Taken(body.message.clone()));
                }
                Err(error) => return Err(error.to_string().into()),
            }
        } else {
            None
        };
        let (attached, stream_rx) = client
            .attach(&request.session_id, None)
            .map_err(|error| error.to_string())?;
        // A racing create (`invalid_params` duplicate → attach) leaves
        // `shell_pid` None; take the pid from `term.list` after attaching.
        let shell_pid = created_pid.or(listed_pid).or_else(|| {
            client
                .list()
                .ok()
                .and_then(|listed| {
                    listed
                        .sessions
                        .iter()
                        .find(|summary| summary.session_id == request.session_id)
                        .map(|summary| summary.pid)
                })
        });
        let size = attached.size;
        // A session that exited before this attach shows its exit from its
        // listing, after the snapshot (finding 9).
        let exit_at_attach = match plan {
            OpenPlan::Attach => listed_exit,
            OpenPlan::Create | OpenPlan::Duplicate | OpenPlan::Lost => None,
        };
        let stream = attached.stream;
        let (chunks_tx, chunks_rx) = mpsc::channel();
        let done = Arc::new(AtomicBool::new(false));
        let control = Arc::new(Mutex::new(HostControl { client, stream }));
        let session_id = request.session_id.clone();
        let (outbound_tx, outbound_rx) = mpsc::channel();
        let latest_size = Arc::new(Mutex::new(None));
        std::thread::Builder::new()
            .name(format!("sirio-terminal-host-link-{session_id}"))
            .spawn({
                let control = Arc::clone(&control);
                let done = Arc::clone(&done);
                let reconnect = Arc::clone(&reconnect);
                move || {
                    forward_host(
                        session_id,
                        control,
                        reconnect,
                        stream_rx,
                        events_rx,
                        chunks_tx,
                        size,
                        done,
                        exit_at_attach,
                    );
                }
            })
            .map_err(|error| error.to_string())?;
        std::thread::Builder::new()
            .name(format!("sirio-terminal-host-send-{}", request.session_id))
            .spawn({
                let session_id = request.session_id.clone();
                let control = Arc::clone(&control);
                let latest_size = Arc::clone(&latest_size);
                move || sender_thread(session_id, control, outbound_rx, latest_size)
            })
            .map_err(|error| error.to_string())?;
        Ok(HostLink {
            session_id: request.session_id.clone(),
            shell_pid,
            incoming: chunks_rx,
            control,
            outbound: outbound_tx,
            latest_size,
            reconnect,
            done,
        })
    }
}

/// How long a dropped connection is retried before the link reports `Lost`,
/// and how often a fresh client is attempted in between.
const HOST_RECONNECT_GRACE: Duration = Duration::from_secs(10);
const HOST_RECONNECT_INTERVAL: Duration = Duration::from_millis(500);

/// Reads one stream and the session events into [`LinkChunk`]s, remembering
/// the next offset so a re-attach can resume where the bytes stopped.
///
/// `Lagged` (this stream fell too far behind) re-attaches for a snapshot
/// transparently on the same connection. `Closed` (the connection is gone)
/// retries the attach against a fresh client every 500 ms for 10 s; `Lost`
/// goes out only when the host no longer lists the id or no host answers —
/// never on the dropped connection alone (SP2 spec §8). A session the host
/// closed (`SessionEvent::Closed`, End All) is an exit, not a loss: its exit
/// is sent and the forwarder ends without reconnecting (finding 6).
#[allow(clippy::too_many_arguments)]
fn forward_host(
    session_id: String,
    control: Arc<Mutex<HostControl>>,
    reconnect: ReconnectFn,
    mut stream_rx: StreamReceiver,
    mut events_rx: mpsc::Receiver<SessionEvent>,
    chunks_tx: mpsc::Sender<LinkChunk>,
    mut size: TermSize,
    done: Arc<AtomicBool>,
    exit_at_attach: Option<ExitStatus>,
) {
    let mut next_offset = 0u64;
    // Whether this session's exit has been sent: once, from whichever of the
    // listing, an `Exited` event or a `Closed` event comes first (finding 9).
    let mut exited = false;
    // The exit the listing reported at attach: sent after the first snapshot,
    // so the replica is drawn before the view learns the session is over.
    let mut exit_pending = exit_at_attach.as_ref().map(host_exit_status);
    // Re-attach on the live connection (after `Lagged`): a snapshot, no
    // resume offset. Returns false when the view is gone.
    let reattach_snapshot = |control: &Arc<Mutex<HostControl>>,
                             stream_rx: &mut StreamReceiver,
                             size: &mut TermSize| {
        let attached = {
            let control = control.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            control.client.attach(&session_id, None)
        };
        match attached {
            Ok((attached, fresh_rx)) => {
                control
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .stream = attached.stream;
                *stream_rx = fresh_rx;
                *size = attached.size;
                true
            }
            Err(_) => false,
        }
    };
    loop {
        if done.load(Ordering::Acquire) {
            return;
        }
        match stream_rx.recv_timeout(Duration::from_millis(50)) {
            Ok(item) => match item {
                StreamItem::Snapshot(bytes) => {
                    if chunks_tx
                        .send(LinkChunk::Snapshot {
                            cols: size.cols,
                            rows: size.rows,
                            bytes,
                        })
                        .is_err()
                    {
                        return;
                    }
                    if let Some(status) = exit_pending.take()
                        && !exited
                    {
                        exited = true;
                        if chunks_tx.send(LinkChunk::Exited(status)).is_err() {
                            return;
                        }
                    }
                }
                StreamItem::Output { offset, bytes } => {
                    next_offset = offset + bytes.len() as u64;
                    if chunks_tx.send(LinkChunk::Output(bytes)).is_err() {
                        return;
                    }
                }
                // The host is about to send a fresh snapshot on this same
                // stream; nothing to do until it arrives.
                StreamItem::Resync => {}
                StreamItem::Lagged => {
                    if !reattach_snapshot(&control, &mut stream_rx, &mut size) && !reconnect_until_attached(
                        &session_id,
                        &control,
                        &reconnect,
                        &mut stream_rx,
                        &mut events_rx,
                        &chunks_tx,
                        &mut size,
                        Some(next_offset),
                        &done,
                        &mut exited,
                    ) {
                        return;
                    }
                }
                StreamItem::Closed => {
                    // The events that came before the connection went may
                    // still be queued: a session the host closed is an exit
                    // (finding 6), so they are read before any reconnect.
                    if matches!(
                        forward_session_events(&session_id, &events_rx, &chunks_tx, &mut exited),
                        Events::Ended | Events::ViewGone
                    ) {
                        return;
                    }
                    if !reconnect_until_attached(
                        &session_id,
                        &control,
                        &reconnect,
                        &mut stream_rx,
                        &mut events_rx,
                        &chunks_tx,
                        &mut size,
                        Some(next_offset),
                        &done,
                        &mut exited,
                    ) {
                        return;
                    }
                }
            },
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                if matches!(
                    forward_session_events(&session_id, &events_rx, &chunks_tx, &mut exited),
                    Events::Ended | Events::ViewGone
                ) {
                    return;
                }
                if !reconnect_until_attached(
                    &session_id,
                    &control,
                    &reconnect,
                    &mut stream_rx,
                    &mut events_rx,
                    &chunks_tx,
                    &mut size,
                    Some(next_offset),
                    &done,
                    &mut exited,
                ) {
                    return;
                }
            }
        }
        match forward_session_events(&session_id, &events_rx, &chunks_tx, &mut exited) {
            Events::Open => {}
            Events::Ended | Events::ViewGone => return,
            // The events subscription lives on the same connection as the
            // stream: when it is gone the connection is too.
            Events::Disconnected => {
                if !reconnect_until_attached(
                    &session_id,
                    &control,
                    &reconnect,
                    &mut stream_rx,
                    &mut events_rx,
                    &chunks_tx,
                    &mut size,
                    Some(next_offset),
                    &done,
                    &mut exited,
                ) {
                    return;
                }
            }
        }
    }
}

/// What the session's queued events left the forwarder with.
#[derive(Debug, PartialEq, Eq)]
enum Events {
    /// Drained; the connection is up.
    Open,
    /// The events subscription is gone: the connection is too.
    Disconnected,
    /// The host closed the session: its exit went out, and nothing follows.
    Ended,
    /// The view is gone.
    ViewGone,
}

/// Sends the session's queued events, in order. `exited` records that the
/// session's exit has gone out, so a later event does not send it again.
fn forward_session_events(
    session_id: &str,
    events_rx: &mpsc::Receiver<SessionEvent>,
    chunks_tx: &mpsc::Sender<LinkChunk>,
    exited: &mut bool,
) -> Events {
    loop {
        let event = match events_rx.try_recv() {
            Ok(event) => event,
            Err(mpsc::TryRecvError::Empty) => return Events::Open,
            Err(mpsc::TryRecvError::Disconnected) => return Events::Disconnected,
        };
        let chunk = match event {
            SessionEvent::Title {
                session_id: id,
                title,
            } if id == session_id => LinkChunk::Title(title),
            SessionEvent::Foreground {
                session_id: id,
                running,
            } if id == session_id => LinkChunk::Foreground(running),
            SessionEvent::Exited {
                session_id: id,
                status,
            } if id == session_id => {
                if *exited {
                    continue;
                }
                *exited = true;
                LinkChunk::Exited(host_exit_status(&status))
            }
            SessionEvent::Closed { session_id: id } if id == session_id => {
                if !*exited {
                    *exited = true;
                    if chunks_tx
                        .send(LinkChunk::Exited(TerminalExitStatus::Unknown))
                        .is_err()
                    {
                        return Events::ViewGone;
                    }
                }
                return Events::Ended;
            }
            _ => continue,
        };
        if chunks_tx.send(chunk).is_err() {
            return Events::ViewGone;
        }
    }
}

/// Whether the session's queued events end it: its close or its exit has
/// gone out, and nothing follows on the session.
fn session_ended(
    session_id: &str,
    events_rx: &mpsc::Receiver<SessionEvent>,
    chunks_tx: &mpsc::Sender<LinkChunk>,
    exited: &mut bool,
) -> bool {
    matches!(
        forward_session_events(session_id, events_rx, chunks_tx, exited),
        Events::Ended | Events::ViewGone
    )
}

/// Re-establish the stream against a fresh client after the connection
/// dropped. Every 500 ms for 10 s: no fresh client, or a host that no longer
/// lists the id, ends in `Lost` (sent exactly once); `true` means the stream,
/// events and size were replaced and the loop continues.
#[allow(clippy::too_many_arguments)]
fn reconnect_until_attached(
    session_id: &str,
    control: &Arc<Mutex<HostControl>>,
    reconnect: &ReconnectFn,
    stream_rx: &mut StreamReceiver,
    events_rx: &mut mpsc::Receiver<SessionEvent>,
    chunks_tx: &mpsc::Sender<LinkChunk>,
    size: &mut TermSize,
    resume_from: Option<u64>,
    done: &Arc<AtomicBool>,
    exited: &mut bool,
) -> bool {
    let deadline = Instant::now() + HOST_RECONNECT_GRACE;
    loop {
        if done.load(Ordering::Acquire) {
            return false;
        }
        // A `Closed` event can still be on its way through the events relay
        // after the connection ended: the session is then an exit, not a loss
        // (finding 6), so the queue is read on every turn.
        if session_ended(session_id, events_rx, chunks_tx, exited) {
            return false;
        }
        if Instant::now() >= deadline {
            let _ = chunks_tx.send(LinkChunk::Lost);
            return false;
        }
        std::thread::sleep(HOST_RECONNECT_INTERVAL);
        if done.load(Ordering::Acquire) {
            return false;
        }
        let Some(fresh) = reconnect() else {
            continue;
        };
        match fresh.list() {
            Ok(listed)
                if !listed
                    .sessions
                    .iter()
                    .any(|summary| summary.session_id == session_id) =>
            {
                if session_ended(session_id, events_rx, chunks_tx, exited) {
                    return false;
                }
                let _ = chunks_tx.send(LinkChunk::Lost);
                return false;
            }
            Ok(_) => {}
            Err(_) => continue,
        }
        let attached = fresh.attach(session_id, resume_from);
        match attached {
            Ok((attached, fresh_rx)) => match fresh.events() {
                Ok(fresh_events) => {
                    if done.load(Ordering::Acquire) {
                        let _ = fresh.detach(attached.stream);
                        return false;
                    }
                    {
                        // The new client owns the re-established stream from
                        // here: every later input, resize, end and detach must
                        // travel on its connection.
                        let mut control = control
                            .lock()
                            .unwrap_or_else(|poisoned| poisoned.into_inner());
                        control.stream = attached.stream;
                        control.client = fresh;
                    }
                    *stream_rx = fresh_rx;
                    *events_rx = fresh_events;
                    *size = attached.size;
                    return true;
                }
                Err(_) => continue,
            },
            Err(_) => continue,
        }
    }
}

impl SessionLink for HostLink {
    fn session_id(&self) -> &str {
        &self.session_id
    }

    fn shell_pid(&self) -> Option<u32> {
        self.shell_pid
    }

    fn input(&self, bytes: Vec<u8>) {
        let control = self.control.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let _ = control.client.input(control.stream, &bytes);
    }

    fn feed(&self, bytes: Vec<u8>) {
        let _ = self.outbound.send(Outbound::Feed(bytes));
    }

    fn resize(&self, cols: u16, rows: u16) {
        *self
            .latest_size
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some((cols, rows));
        let _ = self.outbound.send(Outbound::Resize);
    }

    fn chunks(&self) -> &mpsc::Receiver<LinkChunk> {
        &self.incoming
    }

    fn end(&self) {
        self.done.store(true, Ordering::Release);
        let control = self.control.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let _ = control.client.close(&self.session_id);
    }

    fn detach(&self) {
        self.done.store(true, Ordering::Release);
        let control = self.control.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let _ = control.client.detach(control.stream);
    }

    fn survives_app(&self) -> bool {
        true
    }
}

impl Drop for HostLink {
    fn drop(&mut self) {
        self.done.store(true, Ordering::Release);
    }
}

fn session_exit_status(exit: &Exit) -> TerminalExitStatus {
    match exit {
        Exit::Success => TerminalExitStatus::Success,
        Exit::Code(code) => TerminalExitStatus::Code(*code),
        Exit::Unknown => TerminalExitStatus::Unknown,
    }
}

fn host_exit_status(status: &ExitStatus) -> TerminalExitStatus {
    match status {
        ExitStatus::Success => TerminalExitStatus::Success,
        ExitStatus::Code { code } => TerminalExitStatus::Code(*code),
        ExitStatus::Unknown | ExitStatus::Other => TerminalExitStatus::Unknown,
    }
}

fn term_command(shell: &TerminalShell) -> TermCommand {
    match shell {
        TerminalShell::System => TermCommand::Shell,
        TerminalShell::WithArguments { program, args } => TermCommand::Program {
            program: program.clone(),
            args: args.clone(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pty_fixture_shell;
    use std::time::Duration;

    fn test_directory(name: &str) -> std::path::PathBuf {
        let directory = std::env::temp_dir().join(format!(
            "sirio-link-test-{name}-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&directory).unwrap();
        directory
    }

    fn open_request(session_id: &str, cwd: std::path::PathBuf, shell: TerminalShell) -> OpenRequest {
        OpenRequest {
            session_id: session_id.to_string(),
            cwd,
            shell,
            cols: 80,
            rows: 24,
            prelude: Vec::new(),
            recreate: false,
            attach_only: false,
            restore: false,
            known_worktrees: std::sync::Arc::from(Vec::new()),
            extra_env: BTreeMap::new(),
        }
    }

    fn next_chunk(link: &LocalLink, timeout: Duration) -> LinkChunk {
        link.chunks()
            .recv_timeout(timeout)
            .expect("the link must deliver a chunk")
    }

    /// A child that exits with `code` on every platform: `/bin/sh` on unix,
    /// `python3` (which the PTY fixture already requires) elsewhere.
    fn exit_shell(code: i32) -> TerminalShell {
        #[cfg(unix)]
        {
            TerminalShell::WithArguments {
                program: "/bin/sh".to_string(),
                args: vec!["-c".to_string(), format!("exit {code}")],
            }
        }
        #[cfg(not(unix))]
        {
            TerminalShell::WithArguments {
                program: "python3".to_string(),
                args: vec![
                    "-c".to_string(),
                    format!("import sys; sys.exit({code})"),
                ],
            }
        }
    }

    #[test]
    fn local_link_snapshot_carries_the_session_size() {
        let directory = test_directory("snapshot-size");
        let mut request = open_request(
            "pane-9101",
            directory.clone(),
            pty_fixture_shell("exec sleep 60", &["sleep", "inf"]),
        );
        request.cols = 100;
        request.rows = 30;
        let link = LocalLink::open(&request).expect("open the in-process session");
        assert_eq!(link.session_id(), "pane-9101");
        assert!(link.shell_pid().is_some_and(|pid| pid != 0));
        assert!(!link.survives_app());
        match next_chunk(&link, Duration::from_secs(10)) {
            LinkChunk::Snapshot { cols, rows, .. } => {
                assert_eq!((cols, rows), (100, 30));
            }
            other => panic!("the first chunk must be a snapshot, got {}", chunk_name(&other)),
        }
        link.end();
        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn local_link_echoes_input_back_as_output() {
        let directory = test_directory("echo");
        let request = open_request(
            "pane-9102",
            directory.clone(),
            pty_fixture_shell("exec cat", &["cat"]),
        );
        let link = LocalLink::open(&request).expect("open the in-process session");
        link.input(b"LINK_ECHO_PROBE\n".to_vec());
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        let mut seen = Vec::new();
        while std::time::Instant::now() < deadline {
            match link.chunks().recv_timeout(Duration::from_millis(200)) {
                Ok(LinkChunk::Output(bytes)) => {
                    seen.extend_from_slice(&bytes);
                    if seen.windows(15).any(|w| w == b"LINK_ECHO_PROBE") {
                        link.end();
                        let _ = std::fs::remove_dir_all(directory);
                        return;
                    }
                }
                Ok(_) => {}
                Err(_) => {}
            }
        }
        panic!(
            "the input never came back as output: {:?}",
            String::from_utf8_lossy(&seen)
        );
    }

    #[test]
    fn local_link_reports_exit_status() {
        let directory = test_directory("exit");
        let request = open_request("pane-9103", directory.clone(), exit_shell(3));
        let link = LocalLink::open(&request).expect("open the in-process session");
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while std::time::Instant::now() < deadline {
            match link.chunks().recv_timeout(Duration::from_millis(200)) {
                Ok(LinkChunk::Exited(status)) => {
                    assert_eq!(status, TerminalExitStatus::Code(3));
                    link.end();
                    let _ = std::fs::remove_dir_all(directory);
                    return;
                }
                Ok(_) => {}
                Err(_) => {}
            }
        }
        panic!("the session exit never arrived as a link chunk");
    }

    #[test]
    fn local_link_forwards_the_session_title() {
        let directory = test_directory("title");
        let request = open_request(
            "pane-9104",
            directory.clone(),
            pty_fixture_shell(
                "printf '\\033]0;LINK_TITLE\\007'; exec sleep 60",
                &[
                    "print",
                    "\\033]0;LINK_TITLE\\007",
                    "--expand",
                    "--sleep",
                    "60",
                ],
            ),
        );
        let link = LocalLink::open(&request).expect("open the in-process session");
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while std::time::Instant::now() < deadline {
            match link.chunks().recv_timeout(Duration::from_millis(200)) {
                Ok(LinkChunk::Title(title)) if title == "LINK_TITLE" => {
                    link.end();
                    let _ = std::fs::remove_dir_all(directory);
                    return;
                }
                Ok(_) => {}
                Err(_) => {}
            }
        }
        panic!("the OSC title never arrived as a link chunk");
    }

    #[test]
    fn local_link_end_closes_the_session() {
        let directory = test_directory("end");
        let request = open_request(
            "pane-9105",
            directory.clone(),
            pty_fixture_shell("exec sleep 60", &["sleep", "inf"]),
        );
        let link = LocalLink::open(&request).expect("open the in-process session");
        // Drain the opening snapshot so the channel is quiet before the end.
        let _ = link.chunks().recv_timeout(Duration::from_secs(10));
        link.end();
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while std::time::Instant::now() < deadline {
            match link.chunks().recv_timeout(Duration::from_millis(200)) {
                Ok(_) => {}
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    let _ = std::fs::remove_dir_all(directory);
                    return;
                }
            }
        }
        panic!("ending the link never closed the chunk channel");
    }

    #[test]
    fn local_link_open_refuses_a_missing_directory() {
        let missing = std::env::temp_dir().join(format!(
            "sirio-link-test-missing-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&missing);
        let request = open_request(
            "pane-9106",
            missing,
            TerminalShell::System,
        );
        assert!(
            LocalLink::open(&request).is_err(),
            "a missing cwd must fail the open like the old spawn did"
        );
    }

    fn chunk_name(chunk: &LinkChunk) -> &'static str {
        match chunk {
            LinkChunk::Snapshot { .. } => "Snapshot",
            LinkChunk::Output(_) => "Output",
            LinkChunk::Title(_) => "Title",
            LinkChunk::Foreground(_) => "Foreground",
            LinkChunk::Exited(_) => "Exited",
            LinkChunk::Lost => "Lost",
        }
    }

    /// Unix: a local session starts in the user environment. `env` prints what
    /// the session was given: no name the user environment lacks reaches it,
    /// and the request's own variables do.
    #[cfg(unix)]
    #[test]
    fn local_link_session_gets_the_user_environment() {
        // The launcher's variable: in this process, absent from the user
        // environment installed below.
        // SAFETY: a test process of its own under nextest; nothing else reads
        // the environment concurrently.
        unsafe { std::env::set_var("SIRIO_LINK_TEST_LAUNCHER_ONLY", "1") };
        sirio_env::install(BTreeMap::from([(
            "PATH".to_string(),
            "/usr/bin:/bin".to_string(),
        )]));
        let absent = sirio_env::absent_from_user();
        assert!(
            absent.iter().any(|name| name == "SIRIO_LINK_TEST_LAUNCHER_ONLY"),
            "the fixture must hold a name the user environment lacks"
        );
        let directory = test_directory("user-environment");
        let mut request = open_request(
            "pane-9104",
            directory.clone(),
            TerminalShell::WithArguments {
                program: "/usr/bin/env".to_string(),
                args: Vec::new(),
            },
        );
        request.extra_env =
            BTreeMap::from([("SIRIO_WORKTREE_ID".to_string(), "wt-link".to_string())]);
        let link = LocalLink::open(&request).expect("open the in-process session");
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        let mut seen = Vec::new();
        let needle = "SIRIO_WORKTREE_ID=wt-link";
        while std::time::Instant::now() < deadline
            && !String::from_utf8_lossy(&seen).contains(needle)
        {
            // `env` can finish before the link attaches, and then its output
            // arrives only in the attach's snapshot.
            match link.chunks().recv_timeout(Duration::from_millis(200)) {
                Ok(LinkChunk::Output(bytes) | LinkChunk::Snapshot { bytes, .. }) => {
                    seen.extend_from_slice(&bytes);
                }
                _ => {}
            }
        }
        link.end();
        let _ = std::fs::remove_dir_all(directory);
        let text = String::from_utf8_lossy(&seen).into_owned();
        assert!(
            text.contains(needle),
            "the request's variable must reach the session: {text:?}"
        );
        // The names the session sets itself (`Session::spawn`), and `SHELL`,
        // which `portable-pty` sets whenever the environment lacks it.
        const SESSION_OWNED: [&str; 6] = [
            "TERM",
            "COLORTERM",
            "PI_FORCE_IMAGE_PROTOCOL",
            "SIRIO_PANE_ID",
            "TILLER_PANE_ID",
            "SHELL",
        ];
        for name in absent
            .iter()
            .filter(|name| !SESSION_OWNED.contains(&name.as_str()))
        {
            assert!(
                !text
                    .lines()
                    .any(|line| line.trim_start().starts_with(&format!("{name}="))),
                "{name} is absent from the user environment yet reached the session"
            );
        }
    }
}

#[cfg(test)]
mod attach_only_tests {
    use super::*;

    /// Every way a host open can go, decided before any host call: an
    /// attach-only open whose session the host does not list is `Lost` and
    /// is never created (spec §5); an owned open of a missing session
    /// creates it; a listed session is attached whichever kind of open.
    #[test]
    fn an_attach_only_open_of_a_missing_session_is_lost_and_never_created() {
        assert_eq!(open_plan(false, true, false), OpenPlan::Lost);
        assert_eq!(open_plan(false, false, false), OpenPlan::Create);
        assert_eq!(open_plan(true, true, false), OpenPlan::Attach);
        assert_eq!(open_plan(true, false, true), OpenPlan::Attach);
    }

    /// Fix batch 1, finding 11: a fresh open never attaches to a session that
    /// already has its id, whoever created it, and a restore of a session
    /// the host does not list creates it.
    #[test]
    fn a_fresh_open_of_a_listed_id_is_a_duplicate_and_never_an_attach() {
        assert_eq!(open_plan(true, false, false), OpenPlan::Duplicate);
        assert_eq!(open_plan(false, false, true), OpenPlan::Create);
    }

    /// An attach-only open never starts a local shell: with no host to attach
    /// to it fails, and nothing is spawned under the session's id.
    #[test]
    fn a_local_link_refuses_an_attach_only_open() {
        let request = OpenRequest {
            session_id: "pane-c000000000000".to_string(),
            cwd: std::env::temp_dir(),
            shell: TerminalShell::System,
            cols: 80,
            rows: 24,
            prelude: Vec::new(),
            recreate: false,
            attach_only: true,
            restore: false,
            known_worktrees: std::sync::Arc::from(Vec::new()),
            extra_env: BTreeMap::new(),
        };
        let refused = LocalLink::open(&request);
        assert!(
            matches!(&refused, Err(message) if message.starts_with("Session lost")),
            "an attach-only open must not start a shell: {:?}",
            refused.as_ref().err()
        );
    }
}

/// The worktree that holds `cwd`: the deepest one whose path contains it, in
/// canonical form where the path exists (the app's own ownership rule).
fn deepest_owner<'a>(cwd: &std::path::Path, worktrees: &'a [std::path::PathBuf]) -> Option<&'a std::path::PathBuf> {
    let cwd = std::fs::canonicalize(cwd).unwrap_or_else(|_| cwd.to_path_buf());
    let mut best: Option<(&std::path::PathBuf, usize)> = None;
    for worktree in worktrees {
        let canonical = std::fs::canonicalize(worktree).unwrap_or_else(|_| worktree.clone());
        if cwd.starts_with(&canonical) {
            let depth = canonical.components().count();
            if best.is_none_or(|(_, best_depth)| depth > best_depth) {
                best = Some((worktree, depth));
            }
        }
    }
    best.map(|(worktree, _)| worktree)
}

/// Whether a restore may attach to a session the host lists under the tab's
/// id, whose cwd is `session_cwd` (SP2 fix batch 5, N3). It may when no known
/// worktree holds that cwd, or when the worktree holding it is the one holding
/// the tab's own `tab_cwd`. Otherwise the id is stale for this tab.
pub fn restore_may_attach(
    session_cwd: &std::path::Path,
    tab_cwd: &std::path::Path,
    worktrees: &[std::path::PathBuf],
) -> bool {
    match deepest_owner(session_cwd, worktrees) {
        None => true,
        Some(owner) => deepest_owner(tab_cwd, worktrees) == Some(owner),
    }
}

#[cfg(test)]
mod restore_claim_tests {
    use super::*;
    use std::path::PathBuf;

    fn p(path: &str) -> PathBuf {
        PathBuf::from(path)
    }

    #[test]
    fn a_restore_attaches_a_session_of_its_own_worktree() {
        let worktrees = [p("/r/a"), p("/r/b")];
        assert!(restore_may_attach(&p("/r/a"), &p("/r/a"), &worktrees));
    }

    #[test]
    fn a_restore_does_not_attach_a_session_of_another_worktree() {
        let worktrees = [p("/r/a"), p("/r/b")];
        assert!(!restore_may_attach(&p("/r/b/src"), &p("/r/a"), &worktrees));
    }

    #[test]
    fn a_restore_in_a_subdirectory_attaches_its_worktrees_session() {
        let worktrees = [p("/r/a"), p("/r/b")];
        assert!(restore_may_attach(&p("/r/a/x"), &p("/r/a/sub/y"), &worktrees));
    }

    #[test]
    fn a_restore_attaches_a_session_no_known_worktree_holds() {
        let worktrees = [p("/r/a")];
        assert!(restore_may_attach(&p("/elsewhere"), &p("/r/a"), &worktrees));
        assert!(restore_may_attach(&p("/r/a"), &p("/r/a"), &[]));
    }
}
