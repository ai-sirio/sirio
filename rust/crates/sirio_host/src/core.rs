//! The host's state. SP1 holds only counters; later sub-projects add
//! sessions behind the same lock.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

use sirio_host_protocol::messages::{Event, HostMode, HostStateEvent};

pub struct HostCore {
    pub version: String,
    pub major: u32,
    pub mode: HostMode,
    pub generation: String,
    pub host_id: String,
    pub started: Instant,
    pub shutdown: AtomicBool,
    state: Mutex<Counters>,
}

struct Counters {
    clients: u32,
    held_sessions: u32,
    next_subscription: u64,
    subscribers: Vec<(u64, u64, Sender<Event>)>, // (subscription, next seq, sink)
    /// When a client last connected or left, or a session was last held or
    /// released — or the host started. The idle window (spec §5.5) runs from
    /// here, so a client that came and went between two samples of the idle
    /// watcher still restarts it.
    last_activity: Instant,
}

impl HostCore {
    pub fn new(
        version: String,
        major: u32,
        mode: HostMode,
        generation: String,
        host_id: String,
    ) -> Self {
        Self {
            version,
            major,
            mode,
            generation,
            host_id,
            started: Instant::now(),
            shutdown: AtomicBool::new(false),
            state: Mutex::new(Counters {
                clients: 0,
                held_sessions: 0,
                next_subscription: 0,
                subscribers: Vec::new(),
                last_activity: Instant::now(),
            }),
        }
    }

    fn counters(&self) -> MutexGuard<'_, Counters> {
        self.state.lock().unwrap_or_else(|p| p.into_inner())
    }

    pub fn clients(&self) -> u32 {
        self.counters().clients
    }

    pub fn sessions(&self) -> u32 {
        self.counters().held_sessions
    }

    /// Counts a client, unless the host has already decided to leave: a
    /// client accepted during the exit is never counted after the decision,
    /// and its connection closes at once.
    pub fn client_connected(&self) -> bool {
        let mut c = self.counters();
        if self.shutdown.load(Ordering::SeqCst) {
            return false;
        }
        c.clients += 1;
        c.last_activity = Instant::now();
        self.broadcast_locked(&mut c);
        true
    }

    pub fn client_disconnected(&self) {
        let mut c = self.counters();
        c.clients = c.clients.saturating_sub(1);
        c.last_activity = Instant::now();
        self.broadcast_locked(&mut c);
    }

    /// Holds or releases one session. Refuses to hold once the host is
    /// leaving, so a session cannot appear after `try_shutdown` or
    /// `try_begin_idle_exit` decided there were none.
    pub fn hold_session(&self, held: bool) -> bool {
        let mut c = self.counters();
        if held && self.shutdown.load(Ordering::SeqCst) {
            return false;
        }
        c.held_sessions = if held {
            c.held_sessions + 1
        } else {
            c.held_sessions.saturating_sub(1)
        };
        c.last_activity = Instant::now();
        self.broadcast_locked(&mut c);
        true
    }

    /// `host.shutdown`: the check for live sessions and the decision to leave
    /// are one critical section, so a session cannot be held in between.
    /// `Err` carries the number of live sessions that refused it.
    pub fn try_shutdown(&self, force: bool) -> Result<(), u32> {
        let mut c = self.counters();
        if c.held_sessions > 0 && !force {
            return Err(c.held_sessions);
        }
        self.shutdown.store(true, Ordering::SeqCst);
        self.broadcast_locked(&mut c);
        Ok(())
    }

    /// The idle watcher's decision (spec §5.5): no client, no session and
    /// nothing has happened for `grace`. Deciding and setting the flag happen
    /// in one critical section; `true` means this call decided to leave.
    pub fn try_begin_idle_exit(&self, grace: Duration) -> bool {
        let c = self.counters();
        if self.shutdown.load(Ordering::SeqCst)
            || c.clients > 0
            || c.held_sessions > 0
            || c.last_activity.elapsed() < grace
        {
            return false;
        }
        self.shutdown.store(true, Ordering::SeqCst);
        true
    }

    /// The state as it is under the lock the caller already holds. Reading it
    /// and handing it to the sinks inside one acquisition keeps what a
    /// subscriber sees in the order the state changed: a snapshot taken
    /// before the lock could reach a sink after a newer one.
    fn snapshot(&self, counters: &Counters) -> serde_json::Value {
        serde_json::to_value(HostStateEvent {
            clients: counters.clients,
            sessions: counters.held_sessions,
            mode: self.mode.clone(),
            draining: self.shutdown.load(Ordering::SeqCst),
        })
        .unwrap_or_default()
    }

    /// Registers a sink and immediately sends it the current state.
    pub fn subscribe(&self, sink: Sender<Event>) -> u64 {
        let mut c = self.counters();
        let payload = self.snapshot(&c);
        c.next_subscription += 1;
        let id = c.next_subscription;
        let _ = sink.send(Event {
            subscription: id,
            seq: 0,
            payload,
        });
        c.subscribers.push((id, 1, sink));
        id
    }

    /// Sends the current state to every subscriber; drops sinks whose
    /// connection is gone.
    fn broadcast_locked(&self, c: &mut Counters) {
        let payload = self.snapshot(c);
        c.subscribers.retain_mut(|(id, seq, sink)| {
            let sent = sink
                .send(Event {
                    subscription: *id,
                    seq: *seq,
                    payload: payload.clone(),
                })
                .is_ok();
            *seq += 1;
            sent
        });
    }
}
