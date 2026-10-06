//! Transport-neutral dispatch (spec §6.3): a principal and a request in, a
//! response out. Nothing here knows about sockets or pipes.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, Weak, mpsc};
use std::time::Duration;

use sirio_host_protocol::data::{DataHeader, Phase, decode_data, encode_data, MAX_DATA_BYTES};
use sirio_host_protocol::frame::FrameKind;
use sirio_host_protocol::messages::*;
use sirio_host_protocol::term::*;
use sirio_host_protocol::version::PROTOCOL_MINOR;
use sirio_term_session::fanout::{Attachment, AttachmentId, Chunk};
use sirio_term_session::Format;

use crate::connection::Outbound;
use crate::core::HostCore;
use crate::sessions::{CreateError, Entry, base64_decode, base64_encode, wait_status};

pub struct Router {
    core: Arc<HostCore>,
}

pub struct ConnState {
    router: Arc<Router>,
    pub(crate) outbound: Outbound,
    terminal: bool,
    streams: Arc<Mutex<BTreeMap<u32, StreamState>>>,
    next_stream: u32,
    pending_start: Vec<(u32, mpsc::SyncSender<()>)>,
}

struct StreamState {
    session_id: String,
    attachment_id: AttachmentId,
    entry: Arc<Entry>,
    forwarder: std::thread::JoinHandle<()>,
    stop: Arc<AtomicBool>,
}

impl Router {
    pub fn new(core: Arc<HostCore>) -> Self {
        Self { core }
    }

    pub fn handle(&self, _principal: &Principal, request: &Request) -> Response {
        let core = &self.core;
        match request.method.as_str() {
            method::PING => Response::ok(request.id, serde_json::json!({})),
            method::INFO => Response::ok(
                request.id,
                HostInfo {
                    version: core.version.clone(),
                    pid: std::process::id(),
                    mode: core.mode.clone(),
                    sessions: core.sessions(),
                    clients: core.clients(),
                    uptime_s: core.started.elapsed().as_secs(),
                    generation: core.generation.clone(),
                    protocol: ProtocolVersion {
                        major: core.major,
                        minor: PROTOCOL_MINOR,
                    },
                },
            ),
            method::SHUTDOWN => {
                if let Some(stale) = self.stale(request) {
                    return stale;
                }
                let params: ShutdownParams = match serde_json::from_value(params_or_empty(request))
                {
                    Ok(p) => p,
                    Err(e) => {
                        return Response::err(request.id, ErrorCode::InvalidParams, e.to_string());
                    }
                };
                match core.try_shutdown(params.force) {
                    Ok(()) => Response::ok(request.id, serde_json::json!({})),
                    Err(live) => Response::err(
                        request.id,
                        ErrorCode::SessionsLive,
                        format!("{live} session(s) live; shutdown needs force"),
                    ),
                }
            }
            method::DEBUG_HOLD_SESSION if cfg!(debug_assertions) => {
                if let Some(stale) = self.stale(request) {
                    return stale;
                }
                match serde_json::from_value::<HoldSessionParams>(params_or_empty(request)) {
                    Ok(p) if core.hold_session(p.held) => {
                        Response::ok(request.id, serde_json::json!({}))
                    }
                    Ok(_) => {
                        Response::err(request.id, ErrorCode::Internal, "host is shutting down")
                    }
                    Err(e) => Response::err(request.id, ErrorCode::InvalidParams, e.to_string()),
                }
            }
            method::HELLO => {
                Response::err(request.id, ErrorCode::InvalidParams, "already welcomed")
            }
            other => Response::err(
                request.id,
                ErrorCode::UnknownMethod,
                format!("unknown method {other}"),
            ),
        }
    }

    pub fn connection(self: &Arc<Self>, outbound: Outbound) -> ConnState {
        ConnState {
            router: Arc::clone(self),
            outbound,
            terminal: false,
            streams: Arc::new(Mutex::new(BTreeMap::new())),
            next_stream: 1,
            pending_start: Vec::new(),
        }
    }

    pub fn capabilities(&self) -> Vec<String> {
        vec![capability::HOST_V1.into(), capability::TERMINAL.into()]
    }

    pub fn subscribe(
        &self,
        _principal: &Principal,
        topic_name: &str,
        sink: Outbound,
    ) -> Result<u64, ErrorBody> {
        match topic_name {
            topic::HOST_STATE => Ok(self.core.subscribe(sink)),
            topic::TERM_SESSIONS => Ok(self.core.sessions.subscribe(sink)),
            _ => Err(ErrorBody {
                code: ErrorCode::InvalidParams,
                message: format!("unknown topic {topic_name}"),
            }),
        }
    }

    /// An effect carries the generation it was welcomed with (spec §6.2).
    fn stale(&self, request: &Request) -> Option<Response> {
        (request.generation.as_deref() != Some(self.core.generation.as_str())).then(|| {
            Response::err(
                request.id,
                ErrorCode::StaleGeneration,
                "request belongs to another host generation",
            )
        })
    }
}

impl ConnState {
    pub fn handle(&mut self, principal: &Principal, request: &Request) -> Response {
        match request.method.as_str() {
            method::TERM_CREATE => self.create(request),
            method::TERM_ATTACH => self.attach(request),
            method::TERM_DETACH => self.detach(request),
            method::TERM_RESIZE => self.resize(request),
            method::TERM_READ => self.read(request),
            method::TERM_KEY => self.key(request),
            method::TERM_WRITE => self.write(request),
            method::TERM_FEED => self.feed(request),
            method::TERM_WAIT => self.wait(request),
            method::TERM_CLOSE => self.close(request),
            method::TERM_LIST => Response::ok(
                request.id,
                SessionList {
                    sessions: self.router.core.sessions.list(),
                },
            ),
            _ => self.router.handle(principal, request),
        }
    }

    pub fn terminal(&self) -> bool {
        self.terminal
    }

    pub fn welcomed_terminal(&mut self) {
        self.terminal = true;
    }

    /// Start any stream forwarder only after its `Attached` response was
    /// queued, preserving response-before-Data ordering on the writer.
    pub fn after_response(&mut self, sent: bool) {
        let pending = std::mem::take(&mut self.pending_start);
        for (stream, start) in pending {
            if sent {
                let _ = start.send(());
            } else {
                drop(start);
                self.detach_stream(stream);
            }
        }
    }

    pub fn data(&mut self, payload: &[u8]) -> Result<(), String> {
        if !self.terminal {
            return Err("terminal capability was not welcomed".into());
        }
        let (header, bytes) = decode_data(payload).map_err(|error| error.to_string())?;
        if header.phase != Phase::Input {
            return Err("client Data must use the input phase".into());
        }
        if header.offset != 0 {
            return Err("input Data offset must be zero".into());
        }
        let streams = self.streams.lock().unwrap_or_else(|p| p.into_inner());
        let stream = streams
            .get(&header.stream)
            .ok_or_else(|| "client Data names a stream it does not hold".to_string())?;
        if stream.entry.session.wait(Duration::ZERO).is_some() {
            return Err(format!("session {} has exited", stream.session_id));
        }
        stream.entry.session.input(bytes.to_vec());
        Ok(())
    }

    fn create(&self, request: &Request) -> Response {
        if let Some(stale) = self.router.stale(request) {
            return stale;
        }
        let params = match serde_json::from_value::<CreateParams>(params_or_empty(request)) {
            Ok(params) => params,
            Err(error) => return Response::err(request.id, ErrorCode::InvalidParams, error.to_string()),
        };
        let env_fallback = std::env::vars().collect::<BTreeMap<_, _>>();
        match self.router.core.sessions.create(params, &env_fallback) {
            Ok(created) => Response::ok(request.id, created),
            Err(CreateError::Duplicate | CreateError::Malformed) => {
                Response::err(request.id, ErrorCode::InvalidParams, "invalid or duplicate session id")
            }
            Err(CreateError::InvalidCwd(reason)) => {
                Response::err(request.id, ErrorCode::InvalidCwd, reason)
            }
            Err(CreateError::Spawn(reason)) => {
                Response::err(request.id, ErrorCode::SpawnFailed, reason)
            }
            Err(CreateError::ShuttingDown) => {
                Response::err(request.id, ErrorCode::Internal, "host is shutting down")
            }
        }
    }

    fn attach(&mut self, request: &Request) -> Response {
        if let Some(stale) = self.router.stale(request) {
            return stale;
        }
        let params = match serde_json::from_value::<AttachParams>(params_or_empty(request)) {
            Ok(params) => params,
            Err(error) => return Response::err(request.id, ErrorCode::InvalidParams, error.to_string()),
        };
        let Some(entry) = self.router.core.sessions.get(&params.session_id) else {
            return unknown_session(request.id, &params.session_id);
        };
        let attached = entry.session.attach(params.resume_from);
        let mode = if attached.resumed {
            AttachMode::Resume
        } else {
            AttachMode::Snapshot
        };
        let (cols, rows) = attached.size;
        let attachment = attached.attachment;
        let stream = self.allocate_stream();
        let stop = Arc::new(AtomicBool::new(false));
        let (start_tx, start_rx) = mpsc::sync_channel(0);
        let streams = Arc::downgrade(&self.streams);
        let forward_entry = Arc::clone(&entry);
        let forward_stop = Arc::clone(&stop);
        let outbound = self.outbound.clone();
        let attachment_id = attachment.id;
        let forwarder = std::thread::spawn(move || {
            if start_rx.recv().is_ok() {
                forward_attachment(
                    stream,
                    attachment,
                    attachment_id,
                    forward_entry,
                    outbound,
                    forward_stop,
                    streams,
                );
            }
        });
        self.streams
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .insert(
                stream,
                StreamState {
                    session_id: params.session_id,
                    attachment_id,
                    entry,
                    forwarder,
                    stop,
                },
            );
        self.pending_start.push((stream, start_tx));
        Response::ok(
            request.id,
            Attached {
                stream,
                size: TermSize { cols, rows },
                mode,
            },
        )
    }

    fn detach(&mut self, request: &Request) -> Response {
        if let Some(stale) = self.router.stale(request) {
            return stale;
        }
        let params = match serde_json::from_value::<DetachParams>(params_or_empty(request)) {
            Ok(params) => params,
            Err(error) => return Response::err(request.id, ErrorCode::InvalidParams, error.to_string()),
        };
        if !self.detach_stream(params.stream) {
            return Response::err(request.id, ErrorCode::InvalidParams, "unknown stream");
        }
        Response::ok(request.id, serde_json::json!({}))
    }

    fn resize(&self, request: &Request) -> Response {
        if let Some(stale) = self.router.stale(request) {
            return stale;
        }
        let params = match serde_json::from_value::<ResizeParams>(params_or_empty(request)) {
            Ok(params) => params,
            Err(error) => return Response::err(request.id, ErrorCode::InvalidParams, error.to_string()),
        };
        let Some(entry) = self.router.core.sessions.get(&params.session_id) else {
            return unknown_session(request.id, &params.session_id);
        };
        entry.session.resize(params.cols, params.rows);
        Response::ok(request.id, serde_json::json!({}))
    }

    fn read(&self, request: &Request) -> Response {
        let params = match serde_json::from_value::<ReadParams>(params_or_empty(request)) {
            Ok(params) => params,
            Err(error) => return Response::err(request.id, ErrorCode::InvalidParams, error.to_string()),
        };
        let Some(entry) = self.router.core.sessions.get(&params.session_id) else {
            return unknown_session(request.id, &params.session_id);
        };
        let format = params.format.unwrap_or(match &entry.origin {
            Origin::Control => ReadFormat::Raw,
            Origin::App | Origin::Other => ReadFormat::Text,
        });
        let session_format = match format {
            ReadFormat::Raw => Format::Raw,
            ReadFormat::Text | ReadFormat::Other => Format::Text,
        };
        let max_bytes = params
            .max_bytes
            .map(|max_bytes| usize::try_from(max_bytes).unwrap_or(usize::MAX));
        let bytes = entry.session.read(session_format, max_bytes);
        let data = match format {
            ReadFormat::Raw => base64_encode(&bytes),
            ReadFormat::Text | ReadFormat::Other => String::from_utf8_lossy(&bytes).into_owned(),
        };
        Response::ok(request.id, ReadResult { format, data })
    }

    fn key(&self, request: &Request) -> Response {
        if let Some(stale) = self.router.stale(request) {
            return stale;
        }
        let params = match serde_json::from_value::<KeyParams>(params_or_empty(request)) {
            Ok(params) => params,
            Err(error) => return Response::err(request.id, ErrorCode::InvalidParams, error.to_string()),
        };
        let Some(entry) = self.router.core.sessions.get(&params.session_id) else {
            return unknown_session(request.id, &params.session_id);
        };
        if entry.session.wait(Duration::ZERO).is_some() {
            return session_exited(request.id, &params.session_id);
        }
        match entry.session.key(&params.key) {
            Ok(()) => Response::ok(request.id, serde_json::json!({})),
            Err(reason) if entry.session.wait(Duration::ZERO).is_some() => {
                let _ = reason;
                session_exited(request.id, &params.session_id)
            }
            Err(reason) => Response::err(request.id, ErrorCode::InvalidParams, reason),
        }
    }

    fn write(&self, request: &Request) -> Response {
        self.input_bytes(request, true)
    }

    fn feed(&self, request: &Request) -> Response {
        self.input_bytes(request, false)
    }

    fn input_bytes(&self, request: &Request, to_pty: bool) -> Response {
        if let Some(stale) = self.router.stale(request) {
            return stale;
        }
        let params = match serde_json::from_value::<BytesParams>(params_or_empty(request)) {
            Ok(params) => params,
            Err(error) => return Response::err(request.id, ErrorCode::InvalidParams, error.to_string()),
        };
        let Some(entry) = self.router.core.sessions.get(&params.session_id) else {
            return unknown_session(request.id, &params.session_id);
        };
        let Some(bytes) = base64_decode(&params.data) else {
            return Response::err(request.id, ErrorCode::InvalidParams, "data is not base64");
        };
        if to_pty && entry.session.wait(Duration::ZERO).is_some() {
            return session_exited(request.id, &params.session_id);
        }
        if to_pty {
            entry.session.input(bytes);
        } else {
            entry.session.feed(bytes);
        }
        Response::ok(request.id, serde_json::json!({}))
    }

    fn wait(&self, request: &Request) -> Response {
        let params = match serde_json::from_value::<WaitParams>(params_or_empty(request)) {
            Ok(params) => params,
            Err(error) => return Response::err(request.id, ErrorCode::InvalidParams, error.to_string()),
        };
        let Some(entry) = self.router.core.sessions.get(&params.session_id) else {
            return unknown_session(request.id, &params.session_id);
        };
        let timeout = Duration::from_millis(params.timeout_ms.min(60_000));
        Response::ok(
            request.id,
            WaitResult {
                exit: wait_status(entry.session.wait(timeout)),
            },
        )
    }

    fn close(&self, request: &Request) -> Response {
        if let Some(stale) = self.router.stale(request) {
            return stale;
        }
        let params = match serde_json::from_value::<SessionParams>(params_or_empty(request)) {
            Ok(params) => params,
            Err(error) => return Response::err(request.id, ErrorCode::InvalidParams, error.to_string()),
        };
        if self.router.core.sessions.close(&params.session_id) {
            Response::ok(request.id, serde_json::json!({}))
        } else {
            unknown_session(request.id, &params.session_id)
        }
    }

    fn allocate_stream(&mut self) -> u32 {
        loop {
            let stream = self.next_stream;
            self.next_stream = self.next_stream.wrapping_add(1);
            if self.next_stream == 0 {
                self.next_stream = 1;
            }
            if stream != 0
                && !self
                    .streams
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .contains_key(&stream)
            {
                return stream;
            }
        }
    }

    fn detach_stream(&mut self, stream: u32) -> bool {
        let state = self
            .streams
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .remove(&stream);
        let Some(state) = state else {
            return false;
        };
        state.stop.store(true, Ordering::SeqCst);
        state.entry.session.detach(state.attachment_id);
        let _ = state.forwarder.join();
        true
    }
}

impl Drop for ConnState {
    fn drop(&mut self) {
        self.pending_start.clear();
        let streams = std::mem::take(
            &mut *self.streams.lock().unwrap_or_else(|p| p.into_inner()),
        );
        for stream in streams.values() {
            stream.stop.store(true, Ordering::SeqCst);
            stream.entry.session.detach(stream.attachment_id);
        }
        for stream in streams.into_values() {
            let _ = stream.forwarder.join();
        }
    }
}

fn forward_attachment(
    stream: u32,
    attachment: Attachment,
    attachment_id: AttachmentId,
    entry: Arc<Entry>,
    outbound: Outbound,
    stop: Arc<AtomicBool>,
    streams: Weak<Mutex<BTreeMap<u32, StreamState>>>,
) {
    loop {
        if stop.load(Ordering::SeqCst) {
            break;
        }
        let chunk = match attachment.receiver.recv() {
            Ok(chunk) => chunk,
            Err(_) => break,
        };
        let keep_going = match chunk {
            Chunk::Snapshot(bytes) => send_pieces(
                &outbound,
                stream,
                Phase::Snapshot,
                0,
                &bytes,
                &attachment,
            ),
            Chunk::Output { offset, bytes } => {
                send_pieces(&outbound, stream, Phase::Output, offset, &bytes, &attachment)
            }
            Chunk::Resync => outbound.json(
                FrameKind::Event,
                &Event {
                    subscription: 0,
                    seq: 0,
                    payload: serde_json::json!(SessionEvent::Resync { stream }),
                },
            ),
            Chunk::Lagged => {
                let _ = outbound.json(
                    FrameKind::Event,
                    &Event {
                        subscription: 0,
                        seq: 0,
                        payload: serde_json::json!(SessionEvent::Lagged { stream }),
                    },
                );
                false
            }
        };
        if !keep_going {
            break;
        }
    }
    entry.session.detach(attachment_id);
    if let Some(streams) = streams.upgrade() {
        streams
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .remove(&stream);
    }
}

fn send_pieces(
    outbound: &Outbound,
    stream: u32,
    phase: Phase,
    mut offset: u64,
    bytes: &[u8],
    attachment: &Attachment,
) -> bool {
    let mut pieces = bytes.chunks(MAX_DATA_BYTES);
    if bytes.is_empty() {
        return send_data_piece(outbound, stream, phase, offset, &[], attachment);
    }
    while let Some(piece) = pieces.next() {
        if !send_data_piece(outbound, stream, phase, offset, piece, attachment) {
            return false;
        }
        if phase == Phase::Output {
            offset = offset.saturating_add(piece.len() as u64);
        }
    }
    true
}

fn send_data_piece(
    outbound: &Outbound,
    stream: u32,
    phase: Phase,
    offset: u64,
    bytes: &[u8],
    attachment: &Attachment,
) -> bool {
    let payload = match encode_data(&DataHeader { stream, phase, offset }, bytes) {
        Ok(payload) => payload,
        Err(_) => return false,
    };
    let sent = outbound.frame(FrameKind::Data, &payload);
    attachment.consumed(bytes.len());
    sent
}

fn unknown_session(id: u64, session_id: &str) -> Response {
    Response::err(
        id,
        ErrorCode::UnknownSession,
        format!("unknown session {session_id}"),
    )
}

fn session_exited(id: u64, session_id: &str) -> Response {
    Response::err(
        id,
        ErrorCode::SessionExited,
        format!("session {session_id} has exited"),
    )
}

fn params_or_empty(request: &Request) -> serde_json::Value {
    if request.params.is_null() {
        serde_json::json!({})
    } else {
        request.params.clone()
    }
}
