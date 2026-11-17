//! The WebSocket MCP server Claude Code's `/ide` connects to.
//!
//! One acceptor thread polls a non-blocking listener, so `shutdown` never waits on a blocked
//! `accept`. Each connection gets its own thread whose loop reads with a short socket timeout
//! and, between reads, writes what the app queued and expires diagnostics deadlines: a slow
//! app never blocks a request of the same or another connection.

use std::collections::HashMap;
use std::io;
use std::net::{Shutdown, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tungstenite::handshake::server::{ErrorResponse, Request, Response};
use tungstenite::http::{HeaderValue, StatusCode};
use tungstenite::protocol::WebSocketConfig;
use tungstenite::{HandshakeError, Message, WebSocket, accept_hdr_with_config};

use crate::mcp::{self, FileDiagnostics, Notification};
use crate::pending::Pending;
use crate::socket::WouldBlockOnTimeout;
use crate::untrusted::{ReadOutcome, read_regular_file};
use crate::uri::from_file_uri;

/// How long the app has to answer a `getDiagnostics` before Claude gets an error (spec §10.5).
pub const DIAGNOSTICS_DEADLINE: Duration = Duration::from_millis(400);

const AUTH_HEADER: &str = "x-claude-code-ide-authorization";
/// How often the acceptor and every connection loop look for work.
const POLL: Duration = Duration::from_millis(50);
/// A client that has not finished its handshake by then is dropped.
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);
const WRITE_TIMEOUT: Duration = Duration::from_secs(10);
/// Largest frame and message accepted (an `openDiff` carries a whole file).
const MAX_MESSAGE_BYTES: usize = 32 * 1024 * 1024;
/// The most of a file Sirio reads to draw its diff (the editor's own cap, 1 MiB).
const MAX_DIFF_FILE_BYTES: u64 = 1_048_576;

// Process-wide, so an id the app still holds from an earlier `Bridge` (the switch went off and
// on) never names anything in a later one.
static NEXT_CONN: AtomicU64 = AtomicU64::new(1);
static NEXT_DIAG: AtomicU64 = AtomicU64::new(1);
static NEXT_DIFF: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ConnId(pub u64);

/// One pending `getDiagnostics`, opaque to the app: hand it back to `answer_diagnostics`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DiagId {
    conn: ConnId,
    n: u64,
}

/// One proposal tab's `openDiff`, opaque to the app.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DiffId {
    n: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffAnswer {
    /// `FILE_SAVED`, then the final text.
    Saved(String),
    /// `DIFF_REJECTED`.
    Rejected,
}

/// The `openDiff` it names is no longer waiting (answered, closed, or its connection gone).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Gone;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BridgeEvent {
    /// Claude announced itself with `ide_connected`; `pid` is Claude's own.
    Connected { conn: ConnId, pid: u32 },
    /// A connection that passed the handshake is gone, announced or not (the app ignores ids it
    /// does not know). Its pending `openDiff` calls are dropped with it.
    Disconnected { conn: ConnId },
    /// Claude asked for diagnostics: of `path`, or of every file when `None`.
    Diagnostics { id: DiagId, conn: ConnId, path: Option<PathBuf> },
    /// Claude proposes an edit and waits. Answer it with [`Bridge::answer_diff`] on a user
    /// action only. `old_path` and `new_path` are as Claude sent them (untrusted); `original`
    /// was read, and `canonical` computed, on the connection's thread. `replaces` is the
    /// proposal this one took the place of (same connection and `tab_name`).
    OpenDiff {
        id: DiffId,
        conn: ConnId,
        old_path: PathBuf,
        canonical: Option<PathBuf>,
        original: ReadOutcome,
        new_path: PathBuf,
        contents: String,
        tab_name: String,
        replaces: Option<DiffId>,
    },
    /// Claude closed one proposal (`close_tab`); it was answered with an error that carries no
    /// decision, so close the tab without asking.
    DiffWithdrawn { id: DiffId },
    /// Claude closed every proposal of the connection at once (`closeAllDiffTabs`): one event,
    /// so what the user wrote on them can be delivered together.
    DiffsWithdrawn { ids: Vec<DiffId> },
}

#[derive(Debug, Clone, Copy)]
pub struct BridgeOptions {
    pub diagnostics_deadline: Duration,
}

impl Default for BridgeOptions {
    fn default() -> Self {
        Self { diagnostics_deadline: DIAGNOSTICS_DEADLINE }
    }
}

enum Command {
    Notify(Notification),
    /// A finished result frame for a call the app answered.
    Reply(String),
    Diagnostics(DiagId, Vec<FileDiagnostics>),
}

struct Shared {
    token: String,
    options: BridgeOptions,
    on_event: Box<dyn Fn(BridgeEvent) + Send + Sync>,
    stop: AtomicBool,
    diffs: Mutex<Pending>,
    conns: Mutex<HashMap<ConnId, Sender<Command>>>,
    /// A clone of every accepted socket, so `shutdown` can unblock a read, send or flush.
    streams: Mutex<HashMap<ConnId, TcpStream>>,
}

impl Shared {
    fn send_to(&self, conn: ConnId, command: Command) -> bool {
        let conns = self.conns.lock().unwrap_or_else(|p| p.into_inner());
        conns.get(&conn).is_some_and(|tx| tx.send(command).is_ok())
    }

    fn diffs(&self) -> std::sync::MutexGuard<'_, Pending> {
        self.diffs.lock().unwrap_or_else(|p| p.into_inner())
    }
}

/// The running server. Dropping it asks every thread to stop; `shutdown` also waits for them.
pub struct Bridge {
    shared: Arc<Shared>,
    port: u16,
    acceptor: Option<JoinHandle<()>>,
}

impl Bridge {
    /// Serve on `listener` (see [`crate::bind`]). `on_event` runs on the server's threads and
    /// must not block.
    pub fn start(
        listener: TcpListener,
        token: String,
        options: BridgeOptions,
        on_event: impl Fn(BridgeEvent) + Send + Sync + 'static,
    ) -> io::Result<Bridge> {
        let port = listener.local_addr()?.port();
        listener.set_nonblocking(true)?;
        let shared = Arc::new(Shared {
            token,
            options,
            on_event: Box::new(on_event),
            stop: AtomicBool::new(false),
            diffs: Mutex::new(Pending::default()),
            conns: Mutex::new(HashMap::new()),
            streams: Mutex::new(HashMap::new()),
        });
        let acceptor = {
            let shared = Arc::clone(&shared);
            thread::Builder::new()
                .name("sirio-ide-accept".into())
                .spawn(move || accept_loop(listener, &shared))?
        };
        Ok(Bridge { shared, port, acceptor: Some(acceptor) })
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    /// Queue `note` for `conn`; a connection that is gone is ignored.
    pub fn notify(&self, conn: ConnId, note: Notification) {
        self.shared.send_to(conn, Command::Notify(note));
    }

    /// Answer an `openDiff`, once. An id that was answered, closed, replaced, or whose
    /// connection is gone is `Gone`. Only a user action may call this: nothing in the server
    /// answers an `openDiff` with a decision on its own.
    pub fn answer_diff(&self, id: &DiffId, answer: DiffAnswer) -> Result<(), Gone> {
        let answered = self.shared.diffs().answer(id).ok_or(Gone)?;
        let result = match &answer {
            DiffAnswer::Saved(text) => mcp::saved_result(text),
            DiffAnswer::Rejected => mcp::rejected_result(&answered.tab_name),
        };
        let frame = mcp::result(&answered.rpc_id, result);
        if self.shared.send_to(answered.conn, Command::Reply(frame)) { Ok(()) } else { Err(Gone) }
    }

    /// Answer a `getDiagnostics`; one that already timed out, or whose connection is gone, is
    /// dropped.
    pub fn answer_diagnostics(&self, id: &DiagId, files: Vec<FileDiagnostics>) {
        let _ = self.shared.send_to(id.conn, Command::Diagnostics(id.clone(), files));
    }

    /// Close every connection and the listening port, and wait for the threads.
    pub fn shutdown(mut self) {
        self.shared.stop.store(true, Ordering::SeqCst);
        if let Some(acceptor) = self.acceptor.take() {
            let _ = acceptor.join();
        }
    }
}

impl Drop for Bridge {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::SeqCst);
    }
}

fn accept_loop(listener: TcpListener, shared: &Arc<Shared>) {
    let mut connections: Vec<JoinHandle<()>> = Vec::new();
    while !shared.stop.load(Ordering::SeqCst) {
        match listener.accept() {
            Ok((stream, _)) => {
                let conn = ConnId(NEXT_CONN.fetch_add(1, Ordering::SeqCst));
                let Ok(clone) = stream.try_clone() else { continue };
                shared.streams.lock().unwrap_or_else(|p| p.into_inner()).insert(conn, clone);
                let shared = Arc::clone(shared);
                if let Ok(handle) = thread::Builder::new()
                    .name("sirio-ide-conn".into())
                    .spawn(move || serve(&shared, stream, conn))
                {
                    connections.push(handle);
                }
            }
            Err(_) => thread::sleep(POLL),
        }
        connections.retain(|h| !h.is_finished());
    }
    // Free the port first, then cut every socket: a connection stuck in a handshake, a send or
    // the closing flush must not hold shutdown up.
    drop(listener);
    for stream in shared.streams.lock().unwrap_or_else(|p| p.into_inner()).values() {
        let _ = stream.shutdown(Shutdown::Both);
    }
    for handle in connections {
        let _ = handle.join();
    }
}

/// Equality that does not stop at the first differing byte.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

fn refuse(status: StatusCode) -> ErrorResponse {
    let mut response = ErrorResponse::new(Some(status.to_string()));
    *response.status_mut() = status;
    response
}

/// Whether the client offered the `mcp` subprotocol.
fn offers_mcp(request: &Request) -> bool {
    request
        .headers()
        .get_all("sec-websocket-protocol")
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(','))
        .any(|p| p.trim() == "mcp")
}

fn handshake(shared: &Shared, stream: TcpStream) -> Option<Ws> {
    stream.set_nonblocking(false).ok()?;
    stream.set_nodelay(true).ok()?;
    stream.set_read_timeout(Some(POLL)).ok()?;
    stream.set_write_timeout(Some(WRITE_TIMEOUT)).ok()?;
    let config = WebSocketConfig::default()
        .max_message_size(Some(MAX_MESSAGE_BYTES))
        .max_frame_size(Some(MAX_MESSAGE_BYTES));
    // The callback signature is fixed by tungstenite: its Err is a whole HTTP response.
    #[allow(clippy::result_large_err)]
    let check = |request: &Request, mut response: Response| -> Result<Response, ErrorResponse> {
        let authorised = request
            .headers()
            .get(AUTH_HEADER)
            .is_some_and(|v| constant_time_eq(v.as_bytes(), shared.token.as_bytes()));
        if !authorised {
            return Err(refuse(StatusCode::UNAUTHORIZED));
        }
        // Node's `ws` fails a handshake that drops a subprotocol it asked for. No extension
        // (permessage-deflate) is negotiated.
        if offers_mcp(request) {
            response
                .headers_mut()
                .insert("sec-websocket-protocol", HeaderValue::from_static("mcp"));
        }
        Ok(response)
    };
    // Under the short read timeout, so `stop` and the overall deadline are looked at between
    // attempts; the timeout is per read, not per handshake.
    let deadline = Instant::now() + HANDSHAKE_TIMEOUT;
    let mut attempt = accept_hdr_with_config(WouldBlockOnTimeout(stream), check, Some(config));
    let ws = loop {
        match attempt {
            Ok(ws) => break ws,
            Err(HandshakeError::Interrupted(mid)) => {
                if shared.stop.load(Ordering::SeqCst) || Instant::now() >= deadline {
                    return None;
                }
                attempt = mid.handshake();
            }
            Err(HandshakeError::Failure(_)) => return None,
        }
    };
    ws.get_ref().0.set_read_timeout(Some(POLL)).ok()?;
    Some(ws)
}

/// A connection's WebSocket: expired socket timeouts read as `WouldBlock` on every platform.
type Ws = WebSocket<WouldBlockOnTimeout<TcpStream>>;

struct PendingDiagnostics {
    id: DiagId,
    /// The request's `uri`, echoed as the answer's `uri`.
    uri: Option<String>,
    rpc_id: Value,
    deadline: Instant,
}

/// The socket is unusable; the connection ends.
struct Dead;

struct Conn<'a> {
    shared: &'a Shared,
    id: ConnId,
    ws: Ws,
    pending: Vec<PendingDiagnostics>,
    announced: bool,
}

fn serve(shared: &Shared, stream: TcpStream, id: ConnId) {
    let Some(ws) = handshake(shared, stream) else {
        shared.streams.lock().unwrap_or_else(|p| p.into_inner()).remove(&id);
        return;
    };
    let (tx, rx) = mpsc::channel();
    shared.conns.lock().unwrap_or_else(|p| p.into_inner()).insert(id, tx);
    let mut conn = Conn { shared, id, ws, pending: Vec::new(), announced: false };
    let _ = conn.run(&rx);
    // From here no `tx.send` can succeed, so `answer_diff` on an ending connection is `Gone`.
    drop(rx);
    shared.conns.lock().unwrap_or_else(|p| p.into_inner()).remove(&id);
    // Nothing is owed to anyone any more: no frame is attempted for a proposal still waiting.
    shared.diffs().drop_conn(id);
    // The clone stays in `streams` until the close and flush are done, so a shutdown sweep can
    // still cut a peer that stopped reading.
    let _ = conn.ws.close(None);
    let _ = conn.ws.flush();
    shared.streams.lock().unwrap_or_else(|p| p.into_inner()).remove(&id);
    (shared.on_event)(BridgeEvent::Disconnected { conn: id });
}

impl Conn<'_> {
    fn run(&mut self, commands: &Receiver<Command>) -> Result<(), Dead> {
        while !self.shared.stop.load(Ordering::SeqCst) {
            while let Ok(command) = commands.try_recv() {
                self.apply(command)?;
            }
            self.expire()?;
            self.set_read_timeout();
            match self.ws.read() {
                Ok(Message::Text(text)) => self.handle(text.as_str())?,
                Ok(_) => {}
                Err(tungstenite::Error::Io(e))
                    if e.kind() == io::ErrorKind::WouldBlock => {}
                Err(_) => return Err(Dead),
            }
        }
        Ok(())
    }

    /// Wake for the nearest diagnostics deadline when it is sooner than the next poll, so the
    /// deadline error leaves no later than the deadline plus a millisecond.
    fn set_read_timeout(&self) {
        let now = Instant::now();
        let wait = self
            .pending
            .iter()
            .map(|p| p.deadline.saturating_duration_since(now))
            .fold(POLL, Duration::min)
            .max(Duration::from_millis(1));
        let _ = self.ws.get_ref().0.set_read_timeout(Some(wait));
    }

    fn send(&mut self, frame: String) -> Result<(), Dead> {
        self.ws.send(Message::text(frame)).map_err(|_| Dead)
    }

    fn apply(&mut self, command: Command) -> Result<(), Dead> {
        match command {
            Command::Notify(note) => self.send(mcp::notification(&note)),
            Command::Reply(frame) => self.send(frame),
            Command::Diagnostics(id, files) => {
                let Some(at) = self.pending.iter().position(|p| p.id == id) else {
                    return Ok(());
                };
                let pending = self.pending.remove(at);
                let result = mcp::diagnostics_result(&files, pending.uri.as_deref());
                self.send(mcp::result(&pending.rpc_id, result))
            }
        }
    }

    /// Past the deadline the answer is an error, never `[]`: an empty baseline would make
    /// pre-existing errors look caused by the edit.
    fn expire(&mut self) -> Result<(), Dead> {
        let now = Instant::now();
        let mut i = 0;
        while i < self.pending.len() {
            if self.pending[i].deadline <= now {
                let pending = self.pending.remove(i);
                self.send(mcp::error(
                    &pending.rpc_id,
                    mcp::SERVER_ERROR,
                    "diagnostics were not ready in time",
                ))?;
            } else {
                i += 1;
            }
        }
        Ok(())
    }

    fn handle(&mut self, text: &str) -> Result<(), Dead> {
        let Ok(message) = serde_json::from_str::<Value>(text) else {
            return self.send(mcp::error(&Value::Null, mcp::PARSE_ERROR, "parse error"));
        };
        let Some(object) = message.as_object() else {
            return self.send(mcp::error(&Value::Null, mcp::INVALID_REQUEST, "invalid request"));
        };
        let id = object.get("id").filter(|v| !v.is_null()).cloned();
        let Some(method) = object.get("method").and_then(Value::as_str) else {
            // A response to something we never asked is ignored; anything else is malformed.
            if object.contains_key("result") || object.contains_key("error") {
                return Ok(());
            }
            let id = id.unwrap_or(Value::Null);
            return self.send(mcp::error(&id, mcp::INVALID_REQUEST, "invalid request"));
        };
        let params = object.get("params").cloned().unwrap_or(Value::Null);
        match id {
            None => {
                self.notification(method, &params);
                Ok(())
            }
            Some(id) => self.request(&id, method, &params),
        }
    }

    fn notification(&mut self, method: &str, params: &Value) {
        if method != "ide_connected" || self.announced {
            return;
        }
        if let Some(pid) = params.get("pid").and_then(Value::as_u64).and_then(|p| u32::try_from(p).ok()) {
            self.announced = true;
            (self.shared.on_event)(BridgeEvent::Connected { conn: self.id, pid });
        }
    }

    fn request(&mut self, id: &Value, method: &str, params: &Value) -> Result<(), Dead> {
        let reply = match method {
            "initialize" => mcp::result(id, mcp::initialize_result(params)),
            "ping" => mcp::result(id, json!({})),
            "tools/list" => mcp::result(id, mcp::tools_list_result()),
            "tools/call" => return self.call_tool(id, params),
            _ => mcp::error(id, mcp::METHOD_NOT_FOUND, "method not found"),
        };
        self.send(reply)
    }

    fn call_tool(&mut self, id: &Value, params: &Value) -> Result<(), Dead> {
        let name = params.get("name").and_then(Value::as_str).unwrap_or_default();
        match name {
            "getDiagnostics" => self.get_diagnostics(id, params.get("arguments")),
            "openDiff" => self.open_diff(id, params.get("arguments")),
            "close_tab" => self.close_tab(id, params.get("arguments")),
            "closeAllDiffTabs" => self.close_all_diff_tabs(id),
            _ => self.send(mcp::error(id, mcp::METHOD_NOT_FOUND, "tool not found")),
        }
    }

    fn get_diagnostics(&mut self, id: &Value, arguments: Option<&Value>) -> Result<(), Dead> {
        let (uri, path) = match arguments.and_then(|a| a.get("uri")) {
            None | Some(Value::Null) => (None, None),
            Some(Value::String(uri)) => match from_file_uri(uri) {
                Some(path) => (Some(uri.clone()), Some(path)),
                None => {
                    return self.send(mcp::error(id, mcp::INVALID_PARAMS, "uri must be a file:// URI"));
                }
            },
            Some(_) => return self.send(mcp::error(id, mcp::INVALID_PARAMS, "uri must be a string")),
        };
        let diag = DiagId { conn: self.id, n: NEXT_DIAG.fetch_add(1, Ordering::SeqCst) };
        self.pending.push(PendingDiagnostics {
            id: diag.clone(),
            uri,
            rpc_id: id.clone(),
            deadline: Instant::now() + self.shared.options.diagnostics_deadline,
        });
        (self.shared.on_event)(BridgeEvent::Diagnostics { id: diag, conn: self.id, path });
        Ok(())
    }

    /// Register the proposal and tell the app; the call stays unanswered until the app answers
    /// it (or Claude closes or replaces it). The file is read here, on this connection's thread.
    fn open_diff(&mut self, id: &Value, arguments: Option<&Value>) -> Result<(), Dead> {
        let text = |key: &str| arguments.and_then(|a| a.get(key)).and_then(Value::as_str);
        let (Some(old), Some(new), Some(contents), Some(tab_name)) = (
            text("old_file_path"),
            text("new_file_path"),
            text("new_file_contents"),
            text("tab_name"),
        ) else {
            return self.send(mcp::error(
                id,
                mcp::INVALID_PARAMS,
                "openDiff needs old_file_path, new_file_path, new_file_contents and tab_name",
            ));
        };
        // A relative path would be read from Sirio's own working directory.
        if !Path::new(old).is_absolute() || !Path::new(new).is_absolute() {
            return self.send(mcp::error(id, mcp::INVALID_PARAMS, "openDiff paths must be absolute"));
        }
        let old_path = Path::new(old).to_path_buf();
        let original = read_regular_file(&old_path, MAX_DIFF_FILE_BYTES);
        let canonical = match original {
            ReadOutcome::Missing => None,
            _ => std::fs::canonicalize(&old_path).ok(),
        };
        let diff = DiffId { n: NEXT_DIFF.fetch_add(1, Ordering::SeqCst) };
        let replaced = self.shared.diffs().insert(diff.clone(), self.id, tab_name.to_string(), id.clone());
        let replaces = replaced.as_ref().map(|w| w.id.clone());
        (self.shared.on_event)(BridgeEvent::OpenDiff {
            id: diff,
            conn: self.id,
            old_path,
            canonical,
            original,
            new_path: Path::new(new).to_path_buf(),
            contents: contents.to_string(),
            tab_name: tab_name.to_string(),
            replaces,
        });
        match replaced {
            Some(w) => self.send(withdrawn_error(&w.rpc_id)),
            None => Ok(()),
        }
    }

    fn close_tab(&mut self, id: &Value, arguments: Option<&Value>) -> Result<(), Dead> {
        let Some(tab_name) = arguments.and_then(|a| a.get("tab_name")).and_then(Value::as_str) else {
            return self.send(mcp::error(id, mcp::INVALID_PARAMS, "close_tab needs tab_name"));
        };
        let withdrawn = self.shared.diffs().close_tab(self.id, tab_name);
        if let Some(w) = withdrawn {
            (self.shared.on_event)(BridgeEvent::DiffWithdrawn { id: w.id });
            self.send(withdrawn_error(&w.rpc_id))?;
        }
        // Claude also closes tabs it has already been answered for: nothing pending is fine.
        self.send(mcp::result(id, mcp::text_result("TAB_CLOSED")))
    }

    fn close_all_diff_tabs(&mut self, id: &Value) -> Result<(), Dead> {
        let mut withdrawn = self.shared.diffs().close_all(self.id);
        // The order they were opened in, so what the user wrote on them reads in that order.
        withdrawn.sort_by_key(|w| w.id.n);
        let count = withdrawn.len();
        if !withdrawn.is_empty() {
            let ids = withdrawn.iter().map(|w| w.id.clone()).collect();
            (self.shared.on_event)(BridgeEvent::DiffsWithdrawn { ids });
        }
        for w in withdrawn {
            self.send(withdrawn_error(&w.rpc_id))?;
        }
        self.send(mcp::result(id, mcp::text_result(&format!("CLOSED_{count}_DIFF_TABS"))))
    }
}

/// What a pending `openDiff` gets when it is closed or replaced: an error, which Claude reads as
/// neither an accept nor a reject.
fn withdrawn_error(rpc_id: &Value) -> String {
    mcp::error(rpc_id, mcp::SERVER_ERROR, "the diff was closed before the user decided")
}
