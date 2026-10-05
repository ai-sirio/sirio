//! A blocking client for SRH1 frames. One request in flight at a time;
//! events that arrive while waiting for a response are queued for
//! `next_event`.

use std::collections::VecDeque;
use std::io::{self, ErrorKind, Read, Write};
use std::path::Path;
use std::time::{Duration, Instant};

use serde_json::Value;
use sirio_host_protocol::frame::{FrameDecoder, FrameError, FrameKind, encode};
use sirio_host_protocol::messages::*;
use sirio_ipc::{LocalStream, LocalStreamExt};

#[derive(Debug)]
pub enum CallError {
    Io(io::Error),
    Frame(FrameError),
    Decode(String),
    Remote(ErrorBody),
    Timeout,
}

impl std::fmt::Display for CallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "{e}"),
            Self::Frame(e) => write!(f, "{e}"),
            Self::Decode(e) => write!(f, "undecodable reply: {e}"),
            Self::Remote(b) => write!(f, "{:?}: {}", b.code, b.message),
            Self::Timeout => write!(f, "timed out"),
        }
    }
}

impl std::error::Error for CallError {}

pub struct Connection {
    stream: LocalStream,
    decoder: FrameDecoder,
    next_id: u64,
    events: VecDeque<Event>,
    timeout: Duration,
}

impl Connection {
    /// `timeout` bounds each `call`; a read wakes every 50 ms to check it.
    pub fn open(endpoint: &Path, timeout: Duration) -> io::Result<Self> {
        let stream = sirio_ipc::connect(endpoint)?;
        LocalStreamExt::set_read_timeout(&stream, Some(Duration::from_millis(50)))?;
        Ok(Self {
            stream,
            decoder: FrameDecoder::new(),
            next_id: 1,
            events: VecDeque::new(),
            timeout,
        })
    }

    pub fn hello(&mut self, client_version: &str, majors: &[u32]) -> Result<HelloReply, CallError> {
        let params = serde_json::to_value(Hello {
            client_version: client_version.to_string(),
            majors: majors.to_vec(),
            minor: sirio_host_protocol::version::PROTOCOL_MINOR,
        })
        .map_err(|e| CallError::Decode(e.to_string()))?;
        let value = self.call(method::HELLO, params, None)?;
        serde_json::from_value(value).map_err(|e| CallError::Decode(e.to_string()))
    }

    pub fn call(
        &mut self,
        method: &str,
        params: Value,
        generation: Option<&str>,
    ) -> Result<Value, CallError> {
        let id = self.next_id;
        self.next_id += 1;
        let request = Request {
            id,
            method: method.to_string(),
            params,
            generation: generation.map(str::to_string),
        };
        let payload = serde_json::to_vec(&request).map_err(|e| CallError::Decode(e.to_string()))?;
        let frame = encode(FrameKind::Request, &payload).map_err(CallError::Frame)?;
        self.stream
            .write_all(&frame)
            .and_then(|_| self.stream.flush())
            .map_err(CallError::Io)?;
        let deadline = Instant::now() + self.timeout;
        loop {
            match self.read_frame(deadline)? {
                (FrameKind::Response, payload) => {
                    let response: Response = serde_json::from_slice(&payload)
                        .map_err(|e| CallError::Decode(e.to_string()))?;
                    if response.id != id {
                        continue;
                    }
                    return match (response.result, response.error) {
                        (_, Some(error)) => Err(CallError::Remote(error)),
                        (Some(result), None) => Ok(result),
                        (None, None) => Ok(Value::Null),
                    };
                }
                (FrameKind::Event, payload) => {
                    if let Ok(event) = serde_json::from_slice(&payload) {
                        self.events.push_back(event);
                    }
                }
                _ => return Err(CallError::Decode("unexpected frame kind".into())),
            }
        }
    }

    pub fn next_event(&mut self, timeout: Duration) -> Result<Option<Event>, CallError> {
        if let Some(event) = self.events.pop_front() {
            return Ok(Some(event));
        }
        match self.read_frame(Instant::now() + timeout) {
            Ok((FrameKind::Event, payload)) => serde_json::from_slice(&payload)
                .map(Some)
                .map_err(|e| CallError::Decode(e.to_string())),
            Ok(_) => Err(CallError::Decode("unexpected frame kind".into())),
            Err(CallError::Timeout) => Ok(None),
            Err(e) => Err(e),
        }
    }

    fn read_frame(&mut self, deadline: Instant) -> Result<(FrameKind, Vec<u8>), CallError> {
        let mut buf = [0u8; 16 * 1024];
        loop {
            if let Some(frame) = self.decoder.next_frame().map_err(CallError::Frame)? {
                return Ok((frame.kind, frame.payload));
            }
            if Instant::now() >= deadline {
                return Err(CallError::Timeout);
            }
            match self.stream.read(&mut buf) {
                Ok(0) => {
                    return Err(CallError::Io(io::Error::new(
                        ErrorKind::UnexpectedEof,
                        "host closed the connection",
                    )));
                }
                Ok(n) => self.decoder.push(&buf[..n]),
                Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
                Err(e) => return Err(CallError::Io(e)),
            }
        }
    }
}
