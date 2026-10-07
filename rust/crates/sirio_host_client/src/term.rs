//! Typed client methods for the host's terminal-session capability.

use std::sync::Arc;
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

use serde::de::DeserializeOwned;
use serde_json::Value;
use sirio_host_protocol::messages::{method, topic};
use sirio_host_protocol::term::{
    AttachParams, Attached, BytesParams, CreateParams, Created, DetachParams, KeyParams,
    ReadFormat, ReadParams, ReadResult, ResizeParams, SessionEvent, SessionList, SessionParams,
    WaitParams, WaitResult, b64_encode,
};

use crate::connection::CallError;
use crate::mux::{CALL_TIMEOUT, Mux, StreamReceiver, stream_channel};

#[derive(Clone)]
pub struct TermClient {
    mux: Arc<Mux>,
}

impl TermClient {
    pub fn new(mux: Arc<Mux>) -> Option<Self> {
        mux.has_terminal().then_some(Self { mux })
    }

    pub fn create(&self, params: &CreateParams) -> Result<Created, CallError> {
        let value = serde_json::to_value(params).map_err(decode_error)?;
        self.call_typed(method::TERM_CREATE, value, true, CALL_TIMEOUT)
    }

    pub fn attach(
        &self,
        session_id: &str,
        resume_from: Option<u64>,
    ) -> Result<(Attached, StreamReceiver), CallError> {
        let (sink, stream) = stream_channel();
        let params = AttachParams {
            session_id: session_id.to_string(),
            resume_from,
        };
        let value = serde_json::to_value(params).map_err(decode_error)?;
        let result = self
            .mux
            .call_attach(method::TERM_ATTACH, value, CALL_TIMEOUT, sink)?;
        Ok((decode(result)?, stream))
    }

    pub fn detach(&self, stream: u32) -> Result<(), CallError> {
        let params = serde_json::to_value(DetachParams { stream }).map_err(decode_error)?;
        self.call_unit(method::TERM_DETACH, params, true, CALL_TIMEOUT)?;
        self.mux.forget_stream(stream);
        Ok(())
    }

    pub fn input(&self, stream: u32, bytes: &[u8]) -> Result<(), CallError> {
        self.mux.send_data(stream, bytes)
    }

    pub fn resize(&self, session_id: &str, cols: u16, rows: u16) -> Result<(), CallError> {
        self.call_unit(
            method::TERM_RESIZE,
            serde_json::to_value(ResizeParams {
                session_id: session_id.to_string(),
                cols,
                rows,
            })
            .map_err(decode_error)?,
            true,
            CALL_TIMEOUT,
        )
    }

    pub fn read(
        &self,
        session_id: &str,
        format: Option<ReadFormat>,
        max_bytes: Option<u64>,
    ) -> Result<ReadResult, CallError> {
        self.call_typed(
            method::TERM_READ,
            serde_json::to_value(ReadParams {
                session_id: session_id.to_string(),
                format,
                max_bytes,
            })
            .map_err(decode_error)?,
            false,
            CALL_TIMEOUT,
        )
    }

    pub fn key(&self, session_id: &str, key: &str) -> Result<(), CallError> {
        self.call_unit(
            method::TERM_KEY,
            serde_json::to_value(KeyParams {
                session_id: session_id.to_string(),
                key: key.to_string(),
            })
            .map_err(decode_error)?,
            true,
            CALL_TIMEOUT,
        )
    }

    pub fn write(&self, session_id: &str, bytes: &[u8]) -> Result<(), CallError> {
        self.write_bytes(method::TERM_WRITE, session_id, bytes)
    }

    pub fn feed(&self, session_id: &str, bytes: &[u8]) -> Result<(), CallError> {
        self.write_bytes(method::TERM_FEED, session_id, bytes)
    }

    pub fn wait(&self, session_id: &str, timeout: Duration) -> Result<WaitResult, CallError> {
        let timeout_ms = timeout.as_millis().min(u64::MAX as u128) as u64;
        self.call_typed(
            method::TERM_WAIT,
            serde_json::to_value(WaitParams {
                session_id: session_id.to_string(),
                timeout_ms,
            })
            .map_err(decode_error)?,
            false,
            timeout.saturating_add(CALL_TIMEOUT),
        )
    }

    pub fn close(&self, session_id: &str) -> Result<(), CallError> {
        self.call_unit(
            method::TERM_CLOSE,
            serde_json::to_value(SessionParams {
                session_id: session_id.to_string(),
            })
            .map_err(decode_error)?,
            true,
            CALL_TIMEOUT,
        )
    }

    pub fn list(&self) -> Result<SessionList, CallError> {
        self.call_typed(
            method::TERM_LIST,
            serde_json::json!({}),
            false,
            CALL_TIMEOUT,
        )
    }

    /// Ends every session and the host (`host.shutdown {force: true}`).
    pub fn shutdown_force(&self) -> Result<(), CallError> {
        self.mux
            .call(
                method::SHUTDOWN,
                serde_json::json!({"force": true}),
                true,
                CALL_TIMEOUT,
            )
            .map(|_| ())
    }

    pub fn events(&self) -> Result<Receiver<SessionEvent>, CallError> {
        let events = self.mux.subscribe(topic::TERM_SESSIONS)?;
        let (session_tx, session_rx) = mpsc::channel();
        std::thread::Builder::new()
            .name("sirio-host-terminal-events".into())
            .spawn(move || {
                for event in events {
                    if let Ok(event) = serde_json::from_value::<SessionEvent>(event.payload)
                        && session_tx.send(event).is_err()
                    {
                        break;
                    }
                }
            })
            .map_err(CallError::Io)?;
        Ok(session_rx)
    }

    fn write_bytes(&self, method: &str, session_id: &str, bytes: &[u8]) -> Result<(), CallError> {
        let params = BytesParams {
            session_id: session_id.to_string(),
            data: b64_encode(bytes),
        };
        self.call_unit(
            method,
            serde_json::to_value(params).map_err(decode_error)?,
            true,
            CALL_TIMEOUT,
        )
    }

    fn call_typed<T: DeserializeOwned>(
        &self,
        method: &str,
        params: Value,
        effect: bool,
        timeout: Duration,
    ) -> Result<T, CallError> {
        decode(self.mux.call(method, params, effect, timeout)?)
    }

    fn call_unit(
        &self,
        method: &str,
        params: Value,
        effect: bool,
        timeout: Duration,
    ) -> Result<(), CallError> {
        self.mux.call(method, params, effect, timeout).map(|_| ())
    }
}

fn decode<T: DeserializeOwned>(value: Value) -> Result<T, CallError> {
    serde_json::from_value(value).map_err(decode_error)
}

fn decode_error(error: serde_json::Error) -> CallError {
    CallError::Decode(error.to_string())
}
