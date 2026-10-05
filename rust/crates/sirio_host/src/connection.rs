//! One client: SRH1 frames in and out on a blocking stream, polled with a
//! short read timeout so host-initiated events are written between reads.
//! Every failure closes this connection, never the host.

use std::io::{ErrorKind, Read, Write};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::sync::mpsc;
use std::time::Duration;

use sirio_host_protocol::frame::{FrameDecoder, FrameKind, encode};
use sirio_host_protocol::messages::*;
use sirio_host_protocol::version::{Negotiation, PROTOCOL_MINOR, negotiate};
use sirio_ipc::{LocalStream, LocalStreamExt};

use crate::core::HostCore;
use crate::log::Log;
use crate::router::Router;

const POLL: Duration = Duration::from_millis(50);

pub fn serve(mut stream: LocalStream, core: Arc<HostCore>, router: Arc<Router>, log: Arc<Log>) {
    stream.restore_blocking();
    let _ = stream.set_read_timeout(Some(POLL));
    let principal = Principal {
        transport: Transport::Local,
    };
    let (events_tx, events_rx) = mpsc::channel::<Event>();
    let mut decoder = FrameDecoder::new();
    let mut welcomed = false;
    let mut buf = [0u8; 16 * 1024];
    core.client_connected();
    log.line("client.connected", "");
    'conn: loop {
        while let Ok(event) = events_rx.try_recv() {
            if !write(&mut stream, FrameKind::Event, &event) {
                break 'conn;
            }
        }
        // A host that is leaving closes its connections itself, once the
        // answer to whoever asked it to leave has been written (that answer
        // is written inside the loop below, before this check is reached).
        if core.shutdown.load(Ordering::SeqCst) {
            break;
        }
        match stream.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => decoder.push(&buf[..n]),
            Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => continue,
            Err(_) => break,
        }
        loop {
            let frame = match decoder.next_frame() {
                Ok(Some(frame)) => frame,
                Ok(None) => break,
                Err(error) => {
                    log.line("client.frame_error", &error.to_string());
                    stream.abort();
                    break 'conn;
                }
            };
            if frame.kind != FrameKind::Request {
                log.line("client.unexpected_kind", "");
                stream.abort();
                break 'conn;
            }
            let request: Request = match serde_json::from_slice(&frame.payload) {
                Ok(r) => r,
                Err(e) => {
                    let _ = write(
                        &mut stream,
                        FrameKind::Response,
                        &Response::err(0, ErrorCode::InvalidParams, e.to_string()),
                    );
                    continue;
                }
            };
            let response = if !welcomed {
                if request.method != method::HELLO {
                    let _ = write(
                        &mut stream,
                        FrameKind::Response,
                        &Response::err(
                            request.id,
                            ErrorCode::HandshakeRequired,
                            "send host.hello first",
                        ),
                    );
                    break 'conn;
                }
                match serde_json::from_value::<Hello>(request.params.clone()) {
                    Err(e) => Response::err(request.id, ErrorCode::InvalidParams, e.to_string()),
                    Ok(hello) => match negotiate(core.major, &hello.majors) {
                        Negotiation::Accept { .. } => {
                            welcomed = true;
                            Response::ok(
                                request.id,
                                HelloReply::Welcome(Welcome {
                                    host_version: core.version.clone(),
                                    protocol: ProtocolVersion {
                                        major: core.major,
                                        minor: PROTOCOL_MINOR,
                                    },
                                    capabilities: vec![capability::HOST_V1.to_string()],
                                    host_id: core.host_id.clone(),
                                    generation: core.generation.clone(),
                                }),
                            )
                        }
                        Negotiation::Refuse { host_major } => {
                            let _ = write(
                                &mut stream,
                                FrameKind::Response,
                                &Response::ok(
                                    request.id,
                                    HelloReply::Refused(Refused {
                                        reason: RefusedReason::NoCommonMajor,
                                        host_major,
                                    }),
                                ),
                            );
                            break 'conn;
                        }
                    },
                }
            } else if request.method == method::SUBSCRIBE {
                match serde_json::from_value::<SubscribeParams>(request.params.clone()) {
                    Err(e) => Response::err(request.id, ErrorCode::InvalidParams, e.to_string()),
                    Ok(p) => match router.subscribe(&principal, &p.topic, events_tx.clone()) {
                        Ok(subscription) => Response::ok(request.id, Subscribed { subscription }),
                        Err(error) => Response {
                            id: request.id,
                            result: None,
                            error: Some(error),
                        },
                    },
                }
            } else {
                router.handle(&principal, &request)
            };
            if !write(&mut stream, FrameKind::Response, &response) {
                break 'conn;
            }
        }
    }
    core.client_disconnected();
    log.line("client.disconnected", "");
}

fn write<T: serde::Serialize>(stream: &mut LocalStream, kind: FrameKind, value: &T) -> bool {
    let Ok(payload) = serde_json::to_vec(value) else {
        return false;
    };
    let Ok(frame) = encode(kind, &payload) else {
        return false;
    };
    stream
        .write_all(&frame)
        .and_then(|_| stream.flush())
        .is_ok()
}
