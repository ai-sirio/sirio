//! One client: a reader thread decodes SRH1 frames and dispatches them; a
//! writer thread drains one outbound channel of encoded frames — responses,
//! events and, from SP2, Data — onto the stream. Nothing polls: a frame for
//! the client is written the moment it is queued. Every failure closes this
//! connection, never the host.

use std::io::{ErrorKind, Read, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use sirio_host_protocol::frame::{FrameDecoder, FrameError, FrameKind, encode};
use sirio_host_protocol::messages::{
    ErrorCode, Hello as HelloParams, HelloReply, Principal, ProtocolVersion, Refused,
    RefusedReason, Request, Response, SubscribeParams, Subscribed, Transport, Welcome,
    method,
};
use sirio_host_protocol::version::{Negotiation, PROTOCOL_MINOR, negotiate};
use sirio_ipc::{LocalStream, LocalStreamExt};

use crate::core::HostCore;
use crate::log::Log;
use crate::router::{ConnState, Router};

/// A peer must complete `host.hello` within this (spec §7.1.4).
pub const HANDSHAKE_DEADLINE: Duration = Duration::from_secs(5);
/// A single frame write that blocks this long means the peer stopped
/// reading: the connection is closed (its streams detach).
pub const WRITE_DEADLINE: Duration = Duration::from_secs(10);
/// How often a blocked read wakes to notice the host leaving.
const READ_WAKE: Duration = Duration::from_millis(250);

enum OutboundMessage {
    Frame(Vec<u8>),
    Close,
}

/// The writer's queue. Cloneable; a send after the connection closed is a
/// no-op the caller may ignore.
#[derive(Clone)]
pub struct Outbound {
    tx: mpsc::Sender<OutboundMessage>,
    closed: Arc<AtomicBool>,
}

impl Outbound {
    pub fn frame(&self, kind: FrameKind, payload: &[u8]) -> bool {
        if self.closed.load(Ordering::SeqCst) {
            return false;
        }
        match encode(kind, payload) {
            Ok(frame) => self.tx.send(OutboundMessage::Frame(frame)).is_ok(),
            Err(_) => false,
        }
    }

    pub fn json<T: serde::Serialize>(&self, kind: FrameKind, value: &T) -> bool {
        serde_json::to_vec(value).is_ok_and(|payload| self.frame(kind, &payload))
    }

    fn finish(&self) {
        if !self.closed.swap(true, Ordering::SeqCst) {
            let _ = self.tx.send(OutboundMessage::Close);
        }
    }
}

pub fn serve(mut stream: LocalStream, core: Arc<HostCore>, router: Arc<Router>, log: Arc<Log>) {
    stream.restore_blocking();
    if !core.client_connected() {
        log.line("client.refused", "draining");
        return;
    }
    log.line("client.connected", "");
    let closed = Arc::new(AtomicBool::new(false));
    let (tx, rx) = mpsc::channel::<OutboundMessage>();
    let outbound = Outbound {
        tx,
        closed: Arc::clone(&closed),
    };
    let writer = match stream.try_clone_stream() {
        Ok(mut w) => {
            let _ = w.set_write_timeout(Some(WRITE_DEADLINE));
            let closed = Arc::clone(&closed);
            std::thread::spawn(move || {
                while let Ok(message) = rx.recv() {
                    match message {
                        OutboundMessage::Frame(frame) => {
                            if w.write_all(&frame).and_then(|_| w.flush()).is_err() {
                                break;
                            }
                        }
                        OutboundMessage::Close => break,
                    }
                }
                closed.store(true, Ordering::SeqCst);
                w.abort();
            })
        }
        Err(_) => {
            log.line("client.clone_failed", "");
            core.client_disconnected();
            return;
        }
    };
    let _ = stream.set_read_timeout(Some(READ_WAKE));
    read_loop(&mut stream, &core, &router, &log, &outbound, &closed);
    // A subscription can retain a clone of this sender in HostCore. Close
    // explicitly so those clones cannot keep the writer thread alive.
    outbound.finish();
    drop(outbound);
    let _ = writer.join();
    core.client_disconnected();
    log.line("client.disconnected", "");
}

fn read_loop(
    stream: &mut LocalStream,
    core: &Arc<HostCore>,
    router: &Arc<Router>,
    log: &Log,
    outbound: &Outbound,
    closed: &AtomicBool,
) {
    let principal = Principal {
        transport: Transport::Local,
    };
    let mut conn = router.connection(outbound.clone());
    let mut decoder = FrameDecoder::new();
    let mut welcomed = false;
    let started = Instant::now();
    let mut buf = [0u8; 64 * 1024];
    loop {
        if closed.load(Ordering::SeqCst) || core.shutdown.load(Ordering::SeqCst) {
            return;
        }
        if !welcomed && started.elapsed() > HANDSHAKE_DEADLINE {
            log.line("client.handshake_timeout", "");
            return;
        }
        match stream.read(&mut buf) {
            Ok(0) => return,
            Ok(n) => decoder.push(&buf[..n]),
            Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => continue,
            Err(_) => return,
        }
        loop {
            let frame = match decoder.next_frame() {
                Ok(Some(frame)) => frame,
                Ok(None) => break,
                Err(error) => {
                    log.line("client.frame_error", frame_error_kind(&error));
                    if matches!(error, FrameError::BadMagic(_)) {
                        stream.abort();
                    } else {
                        send_frame_error(outbound, &error.to_string());
                    }
                    return;
                }
            };
            match frame.kind {
                FrameKind::Request => {}
                // Task 6: `FrameKind::Data if welcomed && conn.terminal()` → conn.data(&frame.payload),
                // returning on Err with send_frame_error.
                _ => {
                    log.line("client.frame_error", "unexpected_kind");
                    send_frame_error(outbound, "only request frames are accepted from a client");
                    return;
                }
            }
            let request: Request = match serde_json::from_slice(&frame.payload) {
                Ok(r) => r,
                Err(e) => {
                    outbound.json(
                        FrameKind::Response,
                        &Response::err(0, ErrorCode::InvalidParams, e.to_string()),
                    );
                    continue;
                }
            };
            if !welcomed {
                match hello(core, router, &request) {
                    Hello::Welcomed(response) => {
                        welcomed = true;
                        outbound.json(FrameKind::Response, &response);
                    }
                    Hello::Close(response) => {
                        outbound.json(FrameKind::Response, &response);
                        return;
                    }
                    Hello::Invalid(response) => {
                        outbound.json(FrameKind::Response, &response);
                    }
                }
                continue;
            }
            let response = if request.method == method::SUBSCRIBE {
                subscribe(router, &principal, &request, &mut conn)
            } else {
                conn.handle(&principal, &request)
            };
            outbound.json(FrameKind::Response, &response);
        }
    }
}

enum Hello {
    Welcomed(Response),
    Close(Response),
    Invalid(Response),
}

fn hello(core: &HostCore, router: &Router, request: &Request) -> Hello {
    if request.method != method::HELLO {
        return Hello::Close(Response::err(
            request.id,
            ErrorCode::HandshakeRequired,
            "send host.hello first",
        ));
    }
    let hello = match serde_json::from_value::<HelloParams>(request.params.clone()) {
        Ok(hello) => hello,
        Err(e) => {
            return Hello::Invalid(Response::err(
                request.id,
                ErrorCode::InvalidParams,
                e.to_string(),
            ));
        }
    };
    match negotiate(core.major, &hello.majors) {
        Negotiation::Accept { .. } => Hello::Welcomed(Response::ok(
            request.id,
            HelloReply::Welcome(Welcome {
                host_version: core.version.clone(),
                protocol: ProtocolVersion {
                    major: core.major,
                    minor: PROTOCOL_MINOR,
                },
                capabilities: router.capabilities(),
                host_id: core.host_id.clone(),
                generation: core.generation.clone(),
            }),
        )),
        Negotiation::Refuse { host_major } => Hello::Close(Response::ok(
            request.id,
            HelloReply::Refused(Refused {
                reason: RefusedReason::NoCommonMajor,
                host_major,
            }),
        )),
    }
}

fn subscribe(
    router: &Router,
    principal: &Principal,
    request: &Request,
    conn: &mut ConnState<'_>,
) -> Response {
    match serde_json::from_value::<SubscribeParams>(request.params.clone()) {
        Err(e) => Response::err(request.id, ErrorCode::InvalidParams, e.to_string()),
        Ok(params) => match router.subscribe(principal, &params.topic, conn.outbound.clone()) {
            Ok(subscription) => Response::ok(request.id, Subscribed { subscription }),
            Err(error) => Response {
                id: request.id,
                result: None,
                error: Some(error),
            },
        },
    }
}

/// What a refused frame is called in the log: the kind of the error and
/// nothing the client sent (spec §5.8).
fn frame_error_kind(error: &FrameError) -> &'static str {
    match error {
        FrameError::BadMagic(_) => "bad_magic",
        FrameError::UnknownKind(_) => "unknown_kind",
        FrameError::NonZeroFlags(_) => "nonzero_flags",
        FrameError::NonZeroReserved(_) => "nonzero_reserved",
        FrameError::TooLarge { .. } => "too_large",
    }
}

/// One `frame_error` response, best effort: if it cannot be written the
/// connection closes anyway.
fn send_frame_error(outbound: &Outbound, message: &str) {
    outbound.json(
        FrameKind::Response,
        &Response::err(0, ErrorCode::FrameError, message),
    );
}
