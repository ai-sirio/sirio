//! Transport-neutral dispatch (spec §6.3): a principal and a request in, a
//! response out. Nothing here knows about sockets or pipes.

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::sync::mpsc::Sender;

use sirio_host_protocol::messages::*;
use sirio_host_protocol::version::PROTOCOL_MINOR;

use crate::core::HostCore;

pub struct Router {
    core: Arc<HostCore>,
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
                if core.sessions() > 0 && !params.force {
                    return Response::err(
                        request.id,
                        ErrorCode::SessionsLive,
                        format!("{} session(s) live; shutdown needs force", core.sessions()),
                    );
                }
                core.shutdown.store(true, Ordering::SeqCst);
                core.broadcast();
                Response::ok(request.id, serde_json::json!({}))
            }
            method::DEBUG_HOLD_SESSION if cfg!(debug_assertions) => {
                if let Some(stale) = self.stale(request) {
                    return stale;
                }
                match serde_json::from_value::<HoldSessionParams>(params_or_empty(request)) {
                    Ok(p) => {
                        core.hold_session(p.held);
                        Response::ok(request.id, serde_json::json!({}))
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

    pub fn subscribe(
        &self,
        _principal: &Principal,
        topic_name: &str,
        sink: Sender<Event>,
    ) -> Result<u64, ErrorBody> {
        if topic_name == topic::HOST_STATE {
            Ok(self.core.subscribe(sink))
        } else {
            Err(ErrorBody {
                code: ErrorCode::InvalidParams,
                message: format!("unknown topic {topic_name}"),
            })
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

fn params_or_empty(request: &Request) -> serde_json::Value {
    if request.params.is_null() {
        serde_json::json!({})
    } else {
        request.params.clone()
    }
}
