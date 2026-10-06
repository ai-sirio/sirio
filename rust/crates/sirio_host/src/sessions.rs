//! Every terminal session the host owns, by id (SP2 spec §3). The table is the
//! one place a session is created, found, counted and closed; the router and
//! the connections only hold ids.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::mpsc;
use std::sync::{Arc, Mutex, Weak};
use std::time::{SystemTime, UNIX_EPOCH};

use sirio_host_protocol::term::*;
use sirio_term_session::{Event, Exit, Session, SpawnError, SpawnSpec};

use crate::connection::Outbound;
use crate::core::{HostCore, TopicSinks};
use crate::log::Log;

pub struct Entry {
    pub session: Session,
    pub origin: Origin,
    pub parent: Option<String>,
    pub cwd: String,
    pub created_at: u64,
}

pub struct SessionTable {
    entries: Mutex<BTreeMap<String, Arc<Entry>>>,
    next_control: Mutex<u64>,
    /// The `term.sessions` subscribers, fed by one pump thread per session.
    pub(crate) topic: TopicSinks,
    core: Weak<HostCore>,
    log: Arc<Log>,
}

#[derive(Debug)]
pub enum CreateError {
    Duplicate,
    Malformed,
    InvalidCwd(String),
    Spawn(String),
    ShuttingDown,
}

impl SessionTable {
    pub(crate) fn new(topic: TopicSinks, core: Weak<HostCore>, log: Arc<Log>) -> Self {
        Self {
            entries: Mutex::new(BTreeMap::new()),
            next_control: Mutex::new(0),
            topic,
            core,
            log,
        }
    }

    /// Live sessions only — an exited session does not keep a host up
    /// (SP2 spec §4.3).
    pub fn live(&self) -> u32 {
        let entries: Vec<_> = self
            .entries
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .values()
            .cloned()
            .collect();
        entries
            .iter()
            .filter(|entry| entry.session.wait(std::time::Duration::ZERO).is_none())
            .count()
            .min(u32::MAX as usize) as u32
    }

    pub fn get(&self, id: &str) -> Option<Arc<Entry>> {
        self.entries
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .get(id)
            .cloned()
    }

    pub fn list(&self) -> Vec<SessionSummary> {
        let entries: Vec<_> = self
            .entries
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .iter()
            .map(|(id, entry)| (id.clone(), Arc::clone(entry)))
            .collect();
        entries
            .iter()
            .map(|(id, entry)| {
                let info = entry.session.info();
                SessionSummary {
                    session_id: id.clone(),
                    pid: info.pid,
                    cwd: entry.cwd.clone(),
                    title: info.title,
                    size: TermSize {
                        cols: info.cols,
                        rows: info.rows,
                    },
                    exit: info.exit.map(exit_status),
                    foreground: Foreground {
                        running: info.foreground,
                    },
                    origin: entry.origin.clone(),
                    parent: entry.parent.clone(),
                    clients: info.attachments.min(u32::MAX as usize) as u32,
                    created_at: entry.created_at,
                }
            })
            .collect()
    }

    pub fn create(
        &self,
        params: CreateParams,
        env_fallback: &BTreeMap<String, String>,
    ) -> Result<Created, CreateError> {
        let core = self.core.upgrade().ok_or(CreateError::ShuttingDown)?;
        let _lifecycle = core.lifecycle();
        if core.shutdown.load(Ordering::SeqCst) {
            return Err(CreateError::ShuttingDown);
        }

        let entries = self.entries.lock().unwrap_or_else(|p| p.into_inner());
        let session_id = match params.session_id {
            Some(id) if !is_valid_session_id(&id) => return Err(CreateError::Malformed),
            Some(id) if entries.contains_key(&id) => return Err(CreateError::Duplicate),
            Some(id) => id,
            None => {
                let mut next = self.next_control.lock().unwrap_or_else(|p| p.into_inner());
                loop {
                    *next = next.wrapping_add(1);
                    let token = random_token();
                    let candidate = format!("pane-c{}", &token[..12]);
                    if !entries.contains_key(&candidate) {
                        break candidate;
                    }
                }
            }
        };
        drop(entries);

        let prelude = match params.prelude.as_deref() {
            Some(value) => b64_decode(value)
                .ok_or_else(|| CreateError::Spawn("prelude is not base64".into()))?,
            None => Vec::new(),
        };
        let program = match params.command {
            TermCommand::Shell => None,
            TermCommand::Program { program, args } => Some((program, args)),
        };
        let mut env = env_fallback.clone();
        env.extend(params.env);
        let (events_tx, events_rx) = mpsc::channel();
        let session = Session::spawn(
            SpawnSpec {
                id: session_id.clone(),
                cwd: PathBuf::from(&params.cwd),
                program,
                env,
                cols: params.size.cols,
                rows: params.size.rows,
                prelude,
            },
            events_tx,
        )
        .map_err(|error| match error {
            SpawnError::InvalidCwd(kind) => CreateError::InvalidCwd(kind),
            SpawnError::Spawn(reason) => CreateError::Spawn(reason),
        })?;
        let info = session.info();
        if let Some(kind) = info.tree_kill_error.as_deref() {
            self.log
                .line("session.no_job", &format!("{session_id} {kind}"));
        }
        let created_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_secs())
            .unwrap_or(0);
        let entry = Arc::new(Entry {
            session,
            origin: params.origin,
            parent: params.parent,
            cwd: params.cwd,
            created_at,
        });
        let pid = info.pid;
        let mut entries = self.entries.lock().unwrap_or_else(|p| p.into_inner());
        entries.insert(session_id.clone(), Arc::clone(&entry));
        drop(entries);

        self.topic.publish(serde_json::json!(SessionEvent::Created {
            session_id: session_id.clone(),
        }));
        core.sessions_changed();
        pump_events(
            session_id.clone(),
            events_rx,
            Arc::clone(&entry),
            self.topic.clone(),
            Arc::downgrade(&core),
            Arc::clone(&self.log),
        );
        Ok(Created { session_id, pid })
    }

    /// Removes and terminates. `false` for an unknown id.
    pub fn close(&self, id: &str) -> bool {
        if let Some(core) = self.core.upgrade() {
            let _lifecycle = core.lifecycle();
            self.close_one(id, &core)
        } else {
            self.close_one_without_core(id)
        }
    }

    /// `host.shutdown {force}`: terminate every session.
    /// The host calls this while holding `HostCore::lifecycle`, before it
    /// sets the shutdown flag.
    pub fn close_all(&self) {
        let entries = std::mem::take(&mut *self.entries.lock().unwrap_or_else(|p| p.into_inner()));
        if entries.is_empty() {
            return;
        }
        for (id, entry) in entries {
            entry.session.close();
            self.topic
                .publish(serde_json::json!(SessionEvent::Closed { session_id: id }));
        }
        if let Some(core) = self.core.upgrade() {
            core.sessions_changed();
        }
    }

    pub(crate) fn subscribe(&self, sink: Outbound) -> u64 {
        self.topic.subscribe(sink, None)
    }

    fn close_one(&self, id: &str, core: &HostCore) -> bool {
        let Some(entry) = self.remove(id) else {
            return false;
        };
        entry.session.close();
        self.topic.publish(serde_json::json!(SessionEvent::Closed {
            session_id: id.to_string(),
        }));
        core.sessions_changed();
        true
    }

    fn close_one_without_core(&self, id: &str) -> bool {
        let Some(entry) = self.remove(id) else {
            return false;
        };
        entry.session.close();
        self.topic.publish(serde_json::json!(SessionEvent::Closed {
            session_id: id.to_string(),
        }));
        true
    }

    fn remove(&self, id: &str) -> Option<Arc<Entry>> {
        self.entries
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .remove(id)
    }
}

fn pump_events(
    session_id: String,
    events: mpsc::Receiver<Event>,
    entry: Arc<Entry>,
    topic: TopicSinks,
    core: Weak<HostCore>,
    log: Arc<Log>,
) {
    std::thread::spawn(move || {
        for event in events {
            let payload = match event {
                Event::Title(title) => SessionEvent::Title {
                    session_id: session_id.clone(),
                    title,
                },
                Event::Foreground(running) => SessionEvent::Foreground {
                    session_id: session_id.clone(),
                    running,
                },
                Event::Exited(exit) => {
                    let exit = entry
                        .session
                        .wait(std::time::Duration::from_secs(5))
                        .unwrap_or(exit);
                    log.line("session.exited", &session_id);
                    if let Some(core) = core.upgrade() {
                        core.sessions_changed();
                    }
                    SessionEvent::Exited {
                        session_id: session_id.clone(),
                        status: exit_status(exit),
                    }
                }
            };
            topic.publish(serde_json::json!(payload));
        }
    });
}

pub(crate) fn exit_status(exit: Exit) -> ExitStatus {
    match exit {
        Exit::Success => ExitStatus::Success,
        Exit::Code(code) => ExitStatus::Code { code },
        Exit::Unknown => ExitStatus::Unknown,
    }
}

pub(crate) fn wait_status(exit: Option<Exit>) -> Option<ExitStatus> {
    exit.map(exit_status)
}

/// A host-local token used for generated pane ids and host generations.
pub(crate) fn random_token() -> String {
    use std::hash::{BuildHasher, Hasher};
    let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
    hasher.write_u32(std::process::id());
    hasher.write_u128(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0),
    );
    let first = hasher.finish();
    let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
    hasher.write_u64(first);
    format!("{first:016x}{:016x}", hasher.finish())
}
