//! The host's state. SP1 holds only counters; later sub-projects add
//! sessions behind the same lock.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Mutex, MutexGuard};
use std::time::Instant;

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

#[derive(Default)]
struct Counters {
    clients: u32,
    held_sessions: u32,
    next_subscription: u64,
    subscribers: Vec<(u64, u64, Sender<Event>)>, // (subscription, next seq, sink)
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
            state: Mutex::new(Counters::default()),
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

    pub fn client_connected(&self) {
        self.counters().clients += 1;
        self.broadcast();
    }

    pub fn client_disconnected(&self) {
        {
            let mut c = self.counters();
            c.clients = c.clients.saturating_sub(1);
        }
        self.broadcast();
    }

    pub fn hold_session(&self, held: bool) {
        {
            let mut c = self.counters();
            c.held_sessions = if held {
                c.held_sessions + 1
            } else {
                c.held_sessions.saturating_sub(1)
            };
        }
        self.broadcast();
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
    pub fn broadcast(&self) {
        let mut c = self.counters();
        let payload = self.snapshot(&c);
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
