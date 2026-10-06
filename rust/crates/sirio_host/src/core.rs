//! The host's shared state: lifecycle counters and the sessions it owns.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, Weak};
use std::time::{Duration, Instant};

use sirio_host_protocol::frame::FrameKind;
use sirio_host_protocol::messages::{Event, HostMode, HostStateEvent};

use crate::connection::Outbound;
use crate::log::Log;
use crate::sessions::SessionTable;

/// Sinks for one event topic. Host state and terminal events each have their
/// own sink list, while subscription ids are shared across both topics.
#[derive(Clone)]
pub struct TopicSinks {
    subscribers: Arc<Mutex<Vec<(u64, u64, Outbound)>>>,
    next_subscription: Arc<AtomicU64>,
}

impl TopicSinks {
    fn new(next_subscription: Arc<AtomicU64>) -> Self {
        Self {
            subscribers: Arc::new(Mutex::new(Vec::new())),
            next_subscription,
        }
    }

    pub fn subscribe(&self, sink: Outbound, initial: Option<serde_json::Value>) -> u64 {
        let mut subscribers = self.subscribers.lock().unwrap_or_else(|p| p.into_inner());
        let id = self.next_subscription.fetch_add(1, Ordering::SeqCst) + 1;
        let mut seq = 0;
        if let Some(payload) = initial {
            if !sink.json(
                FrameKind::Event,
                &Event {
                    subscription: id,
                    seq,
                    payload,
                },
            ) {
                return id;
            }
            seq += 1;
        }
        subscribers.push((id, seq, sink));
        id
    }

    pub fn publish(&self, payload: serde_json::Value) {
        let mut subscribers = self.subscribers.lock().unwrap_or_else(|p| p.into_inner());
        subscribers.retain_mut(|(id, seq, sink)| {
            let sent = sink.json(
                FrameKind::Event,
                &Event {
                    subscription: *id,
                    seq: *seq,
                    payload: payload.clone(),
                },
            );
            *seq += 1;
            sent
        });
    }
}

pub struct HostCore {
    pub version: String,
    pub major: u32,
    pub mode: HostMode,
    pub generation: String,
    pub host_id: String,
    pub started: Instant,
    pub shutdown: AtomicBool,
    pub sessions: SessionTable,
    state: Mutex<Counters>,
    /// Session creation and the idle/shutdown decision share this gate.
    lifecycle: Mutex<()>,
    host_state: TopicSinks,
}

struct Counters {
    clients: u32,
    held_sessions: u32,
    /// When a client last connected or left, or a session was last held or
    /// released — or the host started. The idle window runs from here.
    last_activity: Instant,
}

impl HostCore {
    pub fn new(
        version: String,
        major: u32,
        mode: HostMode,
        generation: String,
        host_id: String,
        log: Arc<Log>,
    ) -> Arc<Self> {
        let next_subscription = Arc::new(AtomicU64::new(0));
        let host_state = TopicSinks::new(Arc::clone(&next_subscription));
        Arc::new_cyclic(|weak| Self {
            version,
            major,
            mode,
            generation,
            host_id,
            started: Instant::now(),
            shutdown: AtomicBool::new(false),
            sessions: SessionTable::new(
                TopicSinks::new(Arc::clone(&next_subscription)),
                Weak::clone(weak),
                Arc::clone(&log),
            ),
            state: Mutex::new(Counters {
                clients: 0,
                held_sessions: 0,
                last_activity: Instant::now(),
            }),
            lifecycle: Mutex::new(()),
            host_state,
        })
    }

    fn counters(&self) -> MutexGuard<'_, Counters> {
        self.state.lock().unwrap_or_else(|p| p.into_inner())
    }

    pub(crate) fn lifecycle(&self) -> MutexGuard<'_, ()> {
        self.lifecycle.lock().unwrap_or_else(|p| p.into_inner())
    }

    pub fn clients(&self) -> u32 {
        self.counters().clients
    }

    pub fn sessions(&self) -> u32 {
        self.counters()
            .held_sessions
            .saturating_add(self.sessions.live())
    }

    /// Counts a client, unless the host has already decided to leave.
    pub fn client_connected(&self) -> bool {
        let mut c = self.counters();
        if self.shutdown.load(Ordering::SeqCst) {
            return false;
        }
        c.clients += 1;
        c.last_activity = Instant::now();
        self.broadcast_locked(&c);
        true
    }

    pub fn client_disconnected(&self) {
        let mut c = self.counters();
        c.clients = c.clients.saturating_sub(1);
        c.last_activity = Instant::now();
        self.broadcast_locked(&c);
    }

    /// Holds or releases one debug session, refusing to hold once the host is
    /// leaving.
    pub fn hold_session(&self, held: bool) -> bool {
        let _lifecycle = self.lifecycle();
        let mut c = self.counters();
        if held && self.shutdown.load(Ordering::SeqCst) {
            return false;
        }
        c.held_sessions = if held {
            c.held_sessions.saturating_add(1)
        } else {
            c.held_sessions.saturating_sub(1)
        };
        c.last_activity = Instant::now();
        self.broadcast_locked(&c);
        true
    }

    /// The check for live sessions and the decision to leave are serialized
    /// with session creation. A forced shutdown closes sessions before it
    /// returns.
    pub fn try_shutdown(&self, force: bool) -> Result<(), u32> {
        let _lifecycle = self.lifecycle();
        let live = self.sessions();
        if live > 0 && !force {
            return Err(live);
        }
        if force {
            self.sessions.close_all();
        }
        self.shutdown.store(true, Ordering::SeqCst);
        let c = self.counters();
        self.broadcast_locked(&c);
        Ok(())
    }

    /// The idle watcher cannot race a new session into a host that has
    /// decided to leave.
    pub fn try_begin_idle_exit(&self, grace: Duration) -> bool {
        let _lifecycle = self.lifecycle();
        let c = self.counters();
        if self.shutdown.load(Ordering::SeqCst)
            || c.clients > 0
            || c.held_sessions > 0
            || self.sessions.live() > 0
            || c.last_activity.elapsed() < grace
        {
            return false;
        }
        self.shutdown.store(true, Ordering::SeqCst);
        self.broadcast_locked(&c);
        true
    }

    /// Session changes are activity and update `host.state` after the same
    /// count that `host.info` reports.
    pub(crate) fn sessions_changed(&self) {
        let mut c = self.counters();
        c.last_activity = Instant::now();
        self.broadcast_locked(&c);
    }

    fn snapshot(&self, counters: &Counters) -> serde_json::Value {
        serde_json::to_value(HostStateEvent {
            clients: counters.clients,
            sessions: counters.held_sessions.saturating_add(self.sessions.live()),
            mode: self.mode.clone(),
            draining: self.shutdown.load(Ordering::SeqCst),
        })
        .unwrap_or_default()
    }

    /// Registers a host-state sink and immediately sends its current state.
    pub fn subscribe(&self, sink: Outbound) -> u64 {
        let c = self.counters();
        self.host_state.subscribe(sink, Some(self.snapshot(&c)))
    }

    fn broadcast_locked(&self, counters: &Counters) {
        self.host_state.publish(self.snapshot(counters));
    }
}
