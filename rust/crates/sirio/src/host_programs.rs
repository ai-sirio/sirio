//! The app's one listener on the host's `term.sessions` topic, for the tabs no terminal holds.
//!
//! A hand-typed agent's program and status reach the app through its terminal's link
//! (`sirio_terminal`'s `HostLink`). After a relaunch only the tab on screen has a terminal,
//! and a worktree whose terminals were dropped has none, so their turns would make no stop and
//! no unread mark. This thread reads the same events from one subscription of its own and
//! hands them to the GPUI thread, which applies a session's only when its pane is a tab's and no
//! terminal is bound to it (`SirioWorkspace::apply_host_program_messages`): a bound terminal's
//! link is the only route for its pane.
//!
//! The thread subscribes, then lists (so a change between the two arrives as an event), sends
//! every listed session in the re-seed's form, and forwards the events. It lists again after a
//! `Lagged` or `Resync`. When the subscription is lost it retries through the reconnect closure
//! the terminals' links use, at their interval and within their grace; that closure reaches the
//! same endpoint and never starts a host. After the grace it ends, and the next adoption of a
//! host starts a new one. It never ensures a host and never retries on a timer of its own.
//!
//! Nothing here is logged: a title, a program and a report are content.

use std::collections::HashMap;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU64, Ordering},
    mpsc,
};
use std::time::{Duration, Instant};

use futures::channel::mpsc::UnboundedSender;
use sirio_host_client::term::TermClient;
use sirio_host_protocol::term::SessionEvent;
use sirio_terminal::{
    HOST_RECONNECT_GRACE, HOST_RECONNECT_INTERVAL, HostSessionNote, HostSessionState,
    TerminalActivityEvent, host_session_note, summary_reseed_events,
};

/// What the host last said about each session, written on the listener's thread as each
/// event arrives, before the message is sent.
pub(crate) type SessionCache = Arc<Mutex<HashMap<String, HostSessionState>>>;

/// How a lost connection is replaced: the reconnect closure of the host's client.
pub(crate) type Reconnect = Arc<dyn Fn() -> Option<TermClient> + Send + Sync>;

/// What the listener sends to the GPUI thread. Each message names its listener's generation:
/// a superseded listener's messages are ignored.
pub(crate) enum Message {
    /// Every listed session, in the re-seed's form.
    Seed {
        generation: u64,
        sessions: Vec<(String, Vec<TerminalActivityEvent>)>,
    },
    /// One event of one session.
    Event {
        generation: u64,
        session_id: String,
        note: HostSessionNote,
    },
}

impl Message {
    pub(crate) fn generation(&self) -> u64 {
        match self {
            Message::Seed { generation, .. } | Message::Event { generation, .. } => *generation,
        }
    }
}

/// How often a waiting listener looks at whether a later one has superseded it.
const POLL: Duration = Duration::from_millis(250);

/// Starts the listener of `generation` on a thread of its own. It ends when `current` names
/// another generation, when the receiver is gone, or when the subscription is lost for longer
/// than the grace.
pub(crate) fn spawn(
    client: TermClient,
    reconnect: Option<Reconnect>,
    generation: u64,
    current: Arc<AtomicU64>,
    cache: SessionCache,
    tx: UnboundedSender<Message>,
) -> std::io::Result<()> {
    std::thread::Builder::new()
        .name("sirio-host-programs".into())
        .spawn(move || {
            run(client, reconnect, generation, &current, &cache, &tx);
        })
        .map(|_| ())
}

fn superseded(generation: u64, current: &AtomicU64) -> bool {
    current.load(Ordering::Acquire) != generation
}

/// Why one subscription ended.
enum Ended {
    /// Superseded, or nobody is listening any more.
    Stop,
    /// The connection is gone. `established` says whether the subscription ever delivered its
    /// seed, which is what restarts the grace.
    Lost { established: bool },
}

fn run(
    mut client: TermClient,
    reconnect: Option<Reconnect>,
    generation: u64,
    current: &AtomicU64,
    cache: &SessionCache,
    tx: &UnboundedSender<Message>,
) {
    let mut deadline: Option<Instant> = None;
    loop {
        match subscribed(&client, generation, current, cache, tx) {
            Ended::Stop => return,
            Ended::Lost { established } => {
                if established {
                    deadline = None;
                }
            }
        }
        let until = *deadline.get_or_insert_with(|| Instant::now() + HOST_RECONNECT_GRACE);
        client = loop {
            if superseded(generation, current) || Instant::now() >= until {
                return;
            }
            let mut waited = Duration::ZERO;
            while waited < HOST_RECONNECT_INTERVAL {
                let step = POLL.min(HOST_RECONNECT_INTERVAL - waited);
                std::thread::sleep(step);
                waited += step;
                if superseded(generation, current) {
                    return;
                }
            }
            if let Some(fresh) = reconnect.as_ref().and_then(|reconnect| reconnect()) {
                break fresh;
            }
        };
    }
}

/// One subscription: subscribe, list, seed, then forward events until the connection is lost.
fn subscribed(
    client: &TermClient,
    generation: u64,
    current: &AtomicU64,
    cache: &SessionCache,
    tx: &UnboundedSender<Message>,
) -> Ended {
    let Ok(events) = client.events() else {
        return Ended::Lost { established: false };
    };
    if !seed(client, generation, cache, tx) {
        return Ended::Lost { established: false };
    }
    loop {
        if superseded(generation, current) || tx.is_closed() {
            return Ended::Stop;
        }
        let event = match events.recv_timeout(POLL) {
            Ok(event) => event,
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Ended::Lost { established: true };
            }
        };
        if matches!(
            event,
            SessionEvent::Lagged { .. } | SessionEvent::Resync { .. }
        ) {
            if !seed(client, generation, cache, tx) {
                return Ended::Lost { established: true };
            }
            continue;
        }
        let Some((session_id, note)) = host_session_note(&event) else {
            continue;
        };
        let session_id = session_id.to_owned();
        {
            let mut cache = cache
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if matches!(event, SessionEvent::Closed { .. }) {
                cache.remove(&session_id);
            } else {
                cache.entry(session_id.clone()).or_default().apply(&event);
            }
        }
        let message = Message::Event {
            generation,
            session_id,
            note,
        };
        if tx.unbounded_send(message).is_err() {
            return Ended::Stop;
        }
    }
}

/// Lists the host's sessions, keeps them in the cache and sends each in the re-seed's form.
/// `false` when the listing failed or nobody is listening.
fn seed(
    client: &TermClient,
    generation: u64,
    cache: &SessionCache,
    tx: &UnboundedSender<Message>,
) -> bool {
    let Ok(listed) = client.list() else {
        return false;
    };
    {
        let mut cache = cache
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        cache.clear();
        for summary in &listed.sessions {
            cache.insert(
                summary.session_id.clone(),
                HostSessionState::from_summary(summary),
            );
        }
    }
    let sessions = listed
        .sessions
        .iter()
        .map(|summary| (summary.session_id.clone(), summary_reseed_events(summary)))
        .collect();
    tx.unbounded_send(Message::Seed {
        generation,
        sessions,
    })
    .is_ok()
}
