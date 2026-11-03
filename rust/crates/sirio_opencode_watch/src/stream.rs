//! The service's HTTP API as far as Sirio reads it, and what its events say.
//!
//! Every failure is a kind name (`&'static str`); nothing the service sent, and no
//! credential, is ever put into an error or a log. The paths, auth and event names are
//! the ones measured against the real v2 service (live-facts L12 to L19):
//!
//! - `GET /api/info`, to confirm the registered service is the one that answers;
//! - `GET /api/event`, the SSE stream: `data: {json}` frames and `: heartbeat` comments,
//!   a `server.connected` frame first, nothing replayed;
//! - `GET /api/session?directory=`, `GET /api/session/active` and
//!   `GET /api/session/{id}/permission|form`, the state before the stream began.

use std::collections::{BTreeMap, HashSet};
use std::io::{BufRead, Read};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::time::Duration;

use serde_json::Value;
use ureq::Agent;

use crate::endpoint::Endpoint;
use crate::{Msg, OcSession, OcState};

/// How long a connect, or the headers of a reply, may take.
const CALL_TIMEOUT: Duration = Duration::from_secs(2);
/// A longest line of the stream (a frame is one line): more is not the service.
const MAX_LINE: u64 = 1 << 20;
/// Sessions asked of one directory.
const SESSION_PAGE: &str = "100";

/// Kind names of a failed call.
pub(crate) const UNREACHABLE: &str = "unreachable";
pub(crate) const UNAUTHORIZED: &str = "unauthorized";
pub(crate) const MISMATCH: &str = "mismatch";
pub(crate) const UNEXPECTED_REPLY: &str = "unexpected_reply";
pub(crate) const STREAM_LOST: &str = "stream_lost";
pub(crate) const STREAM_CLOSED: &str = "stream_closed";
pub(crate) const UNEXPECTED_STREAM: &str = "unexpected_stream";
pub(crate) const STALLED: &str = "stalled";

/// One endpoint's calls. Both agents ignore the proxy variables (the service is on this
/// machine), follow no redirect (the registration's URL was checked for loopback, a
/// `Location` is not) and ask for no compression (a stream must not be buffered by a decoder).
pub(crate) struct Client {
    calls: Agent,
    stream: Agent,
    base: String,
    authorization: Option<String>,
    pid: u32,
}

impl Client {
    pub(crate) fn new(endpoint: &Endpoint) -> Client {
        let calls = Agent::config_builder()
            .proxy(None)
            .max_redirects(0)
            .timeout_global(Some(CALL_TIMEOUT))
            .build()
            .new_agent();
        // No body timeout: a stream is idle between events. The watch loop owns the
        // liveness check (the heartbeat) and the stop flag.
        let stream = Agent::config_builder()
            .proxy(None)
            .max_redirects(0)
            .timeout_connect(Some(CALL_TIMEOUT))
            .timeout_recv_response(Some(CALL_TIMEOUT))
            .build()
            .new_agent();
        Client {
            calls,
            stream,
            base: endpoint.base_url.clone(),
            authorization: endpoint
                .auth
                .as_ref()
                .map(|(user, password)| format!("Basic {}", base64(&format!("{user}:{password}")))),
            pid: endpoint.pid,
        }
    }

    fn get(
        &self,
        agent: &Agent,
        path: &str,
        query: &[(&str, &str)],
    ) -> Result<ureq::Body, &'static str> {
        let mut request = agent
            .get(format!("{}{path}", self.base))
            .header("Accept-Encoding", "identity");
        if let Some(authorization) = &self.authorization {
            request = request.header("Authorization", authorization);
        }
        for (key, value) in query {
            request = request.query(key, value);
        }
        match request.call() {
            Ok(response) => Ok(response.into_body()),
            Err(ureq::Error::StatusCode(401 | 403)) => Err(UNAUTHORIZED),
            Err(ureq::Error::StatusCode(_)) => Err(UNEXPECTED_REPLY),
            Err(_) => Err(UNREACHABLE),
        }
    }

    fn json(&self, path: &str, query: &[(&str, &str)]) -> Result<Value, &'static str> {
        let mut body = self.get(&self.calls, path, query)?;
        let text = body.read_to_string().map_err(|_| UNREACHABLE)?;
        serde_json::from_str(&text).map_err(|_| UNEXPECTED_REPLY)
    }

    /// The registered service is the one that answers: its `/api/info` names the pid the
    /// registration file names. A restarted service has a new pid, and a stale file
    /// pointing at a port something else now holds does not match.
    pub(crate) fn confirm(&self) -> Result<(), &'static str> {
        let info = self.json("/api/info", &[])?;
        match info.get("pid").and_then(Value::as_u64) {
            Some(pid) if pid == u64::from(self.pid) => Ok(()),
            Some(_) => Err(MISMATCH),
            None => Err(UNEXPECTED_REPLY),
        }
    }

    /// Opens the event stream and reads it on its own thread: whole frames go to `inbox`
    /// tagged with `connection`, then one `End` when it ends. The thread ends with the
    /// stream, or at the first line after `abandoned` is set (a heartbeat comes about
    /// every ten seconds), which closes the socket.
    pub(crate) fn open_stream(
        &self,
        connection: u64,
        inbox: Sender<Msg>,
        abandoned: Arc<AtomicBool>,
    ) -> Result<(), &'static str> {
        let body = self.get(&self.stream, "/api/event", &[])?;
        std::thread::Builder::new()
            .name("opencode-stream".into())
            .spawn(move || read_frames(body.into_reader(), connection, &inbox, &abandoned))
            .map_err(|_| UNREACHABLE)?;
        Ok(())
    }

    /// The sessions whose directory is exactly `directory`, newest first.
    pub(crate) fn sessions_in(&self, directory: &Path) -> Result<Vec<Value>, &'static str> {
        let directory = directory.to_string_lossy();
        let reply = self.json(
            "/api/session",
            &[
                ("directory", &directory),
                ("order", "desc"),
                ("limit", SESSION_PAGE),
            ],
        )?;
        data_array(&reply).ok_or(UNEXPECTED_REPLY)
    }

    /// The ids of the sessions whose execution is running.
    pub(crate) fn active(&self) -> Result<HashSet<String>, &'static str> {
        let reply = self.json("/api/session/active", &[])?;
        let data = reply
            .get("data")
            .and_then(Value::as_object)
            .ok_or(UNEXPECTED_REPLY)?;
        Ok(data.keys().cloned().collect())
    }

    /// Whether the session waits on a permission or a question. A lookup that fails (the
    /// service does not answer that path) says the session is not waiting: it is no
    /// reason to treat the whole connection as down.
    pub(crate) fn waiting(&self, session: &str) -> bool {
        ["permission", "form"].iter().any(|what| {
            self.json(&format!("/api/session/{session}/{what}"), &[])
                .ok()
                .and_then(|reply| data_array(&reply))
                .is_some_and(|asks| !asks.is_empty())
        })
    }
}

/// `{"data":[..]}`, or a bare array.
fn data_array(reply: &Value) -> Option<Vec<Value>> {
    match reply {
        Value::Array(rows) => Some(rows.clone()),
        other => other.get("data")?.as_array().cloned(),
    }
}

/// Reads the stream line by line and hands over each frame. Ends quietly once `abandoned`
/// is set: the watch has moved on and nobody listens.
fn read_frames(reader: impl Read, connection: u64, inbox: &Sender<Msg>, abandoned: &AtomicBool) {
    let mut reader = std::io::BufReader::new(reader);
    let mut data = String::new();
    // A line longer than `MAX_LINE` was skipped: the frame it belonged to is lost.
    let mut broken = false;
    let end = loop {
        let mut line = String::new();
        match reader.by_ref().take(MAX_LINE).read_line(&mut line) {
            Ok(0) => break STREAM_CLOSED,
            Ok(_) if !line.ends_with('\n') => {
                // Skipped up to its end; a stream that ends inside it is just ended.
                match skip_line(&mut reader) {
                    Ok(true) => {
                        data.clear();
                        broken = true;
                        continue;
                    }
                    Ok(false) => break STREAM_CLOSED,
                    Err(_) => break STREAM_LOST,
                }
            }
            Ok(_) => {}
            Err(_) => break STREAM_LOST,
        }
        if abandoned.load(Ordering::SeqCst) {
            return;
        }
        let line = line.trim_end_matches(['\r', '\n']);
        let sent = if line.is_empty() {
            if std::mem::take(&mut broken) {
                data.clear();
                continue;
            }
            if data.is_empty() {
                continue;
            }
            inbox.send(Msg::Frame(connection, std::mem::take(&mut data)))
        } else if broken {
            continue;
        } else if line.starts_with(':') {
            inbox.send(Msg::Beat(connection))
        } else if let Some(value) = line.strip_prefix("data:") {
            if !data.is_empty() {
                data.push('\n');
            }
            data.push_str(value.strip_prefix(' ').unwrap_or(value));
            continue;
        } else {
            // `event:`, `id:` and `retry:` are not used by this service.
            continue;
        };
        if sent.is_err() {
            return;
        }
    };
    let _ = inbox.send(Msg::End(connection, end));
}

/// Discards the rest of a line. False when the stream ends first.
fn skip_line(reader: &mut impl BufRead) -> std::io::Result<bool> {
    loop {
        let buffer = reader.fill_buf()?;
        if buffer.is_empty() {
            return Ok(false);
        }
        match buffer.iter().position(|byte| *byte == b'\n') {
            Some(at) => {
                reader.consume(at + 1);
                return Ok(true);
            }
            None => {
                let all = buffer.len();
                reader.consume(all);
            }
        }
    }
}

/// Base64 (standard alphabet, padded), for the Basic credentials.
fn base64(text: &str) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in text.as_bytes().chunks(3) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |n, (i, byte)| n | u32::from(*byte) << (16 - 8 * i));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(ALPHABET[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// The sessions the watch knows, in the directories it was asked to watch.
#[derive(Default)]
pub(crate) struct Table {
    sessions: BTreeMap<String, OcSession>,
}

impl Table {
    pub(crate) fn snapshot(&self) -> Vec<OcSession> {
        self.sessions.values().cloned().collect()
    }

    /// Forgets the sessions of directories no longer watched.
    pub(crate) fn retain_directories(&mut self, watched: &[PathBuf]) {
        self.sessions
            .retain(|_, session| watched.contains(&session.directory));
    }

    /// A session read from the service's list. One already known keeps the state its
    /// events gave it: they are newer than the list.
    pub(crate) fn seed(&mut self, row: &Value, active: bool, waiting: bool) {
        let Some(id) = row.get("id").and_then(Value::as_str) else {
            return;
        };
        let Some(directory) = row
            .pointer("/location/directory")
            .and_then(Value::as_str)
            .map(canonical)
        else {
            return;
        };
        let title = row.get("title").and_then(Value::as_str).unwrap_or_default();
        let created = row
            .pointer("/time/created")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let updated = row
            .pointer("/time/updated")
            .and_then(Value::as_u64)
            .unwrap_or(created);
        let state = if waiting {
            OcState::Waiting
        } else if active {
            OcState::Busy
        } else {
            OcState::Idle
        };
        match self.sessions.get_mut(id) {
            Some(known) => {
                known.title = title.to_string();
            }
            None => {
                self.sessions.insert(
                    id.to_string(),
                    OcSession {
                        id: id.to_string(),
                        directory,
                        title: title.to_string(),
                        created_ms: created,
                        state,
                        at_ms: updated,
                    },
                );
            }
        }
    }

    /// One frame of the stream. Returns whether a session the watch knows changed.
    /// An event it does not know, or about a session it does not watch, changes nothing.
    pub(crate) fn apply(&mut self, frame: &Value, watched: &[PathBuf]) -> bool {
        let Some(kind) = frame.get("type").and_then(Value::as_str) else {
            return false;
        };
        let created = frame.get("created").and_then(Value::as_u64).unwrap_or(0);
        let data = frame.get("data").unwrap_or(&Value::Null);
        let session = data
            .get("sessionID")
            .or_else(|| data.pointer("/form/sessionID"))
            .and_then(Value::as_str);
        let state = match kind {
            "session.created" => return self.created(frame, data, session, created, watched),
            "session.renamed" => return self.renamed(session, data),
            "session.deleted" => {
                return session.is_some_and(|id| self.sessions.remove(id).is_some());
            }
            "session.execution.started" => OcState::Busy,
            "session.execution.succeeded" | "session.execution.interrupted" => OcState::Idle,
            "session.execution.failed" => OcState::Error,
            "permission.asked" | "form.created" => OcState::Waiting,
            "permission.replied" | "permission.rejected" | "form.replied" | "form.cancelled" => {
                OcState::Busy
            }
            _ => return false,
        };
        let Some(known) = session.and_then(|id| self.sessions.get_mut(id)) else {
            return false;
        };
        known.state = state;
        known.at_ms = created;
        true
    }

    fn created(
        &mut self,
        frame: &Value,
        data: &Value,
        session: Option<&str>,
        created: u64,
        watched: &[PathBuf],
    ) -> bool {
        let Some(id) = session else {
            return false;
        };
        let Some(directory) = data
            .pointer("/location/directory")
            .or_else(|| frame.pointer("/location/directory"))
            .and_then(Value::as_str)
            .map(canonical)
        else {
            return false;
        };
        if !watched.contains(&directory) {
            return false;
        }
        // Already read from the list (it was created while the watch seeded): its events
        // that follow carry the rest.
        if self.sessions.contains_key(id) {
            return false;
        }
        self.sessions.insert(
            id.to_string(),
            OcSession {
                id: id.to_string(),
                directory,
                title: String::new(),
                created_ms: created,
                state: OcState::Idle,
                at_ms: created,
            },
        );
        true
    }

    fn renamed(&mut self, session: Option<&str>, data: &Value) -> bool {
        let (Some(id), Some(title)) = (session, data.get("title").and_then(Value::as_str)) else {
            return false;
        };
        match self.sessions.get_mut(id) {
            Some(known) if known.title != title => {
                known.title = title.to_string();
                true
            }
            _ => false,
        }
    }
}

/// A directory as the comparison sees it: resolved when it exists, as given otherwise.
pub(crate) fn canonical(directory: impl AsRef<Path>) -> PathBuf {
    let directory = directory.as_ref();
    std::fs::canonicalize(directory).unwrap_or_else(|_| directory.to_path_buf())
}
