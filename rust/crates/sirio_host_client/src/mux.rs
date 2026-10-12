//! A multiplexed SRH1 client connection. One blocking reader dispatches
//! replies, subscriptions and terminal streams while callers share a writer.

use std::collections::HashMap;
use std::io::{self, ErrorKind, Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, RecvError, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::Value;
use sirio_host_protocol::data::{DataHeader, MAX_DATA_BYTES, Phase, decode_data, encode_data};
use sirio_host_protocol::frame::{FrameDecoder, FrameKind, encode};
use sirio_host_protocol::messages::{
    Event, Hello, HelloReply, Request, Response, Subscribed, Welcome, capability, method,
};
use sirio_host_protocol::term::{Attached, DetachParams, SessionEvent};
use sirio_host_protocol::version::PROTOCOL_MINOR;
use sirio_ipc::{LocalStream, LocalStreamExt};

use crate::connection::CallError;

const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);
pub(crate) const CALL_TIMEOUT: Duration = Duration::from_secs(5);
const WRITE_TIMEOUT: Duration = Duration::from_secs(10);
pub const STREAM_QUEUE_BYTES: usize = 4 << 20;

#[derive(Debug, PartialEq, Eq)]
pub enum StreamItem {
    Snapshot(Vec<u8>),
    Output { offset: u64, bytes: Vec<u8> },
    Resync,
    Lagged,
    Closed,
}

impl StreamItem {
    fn weight(&self) -> usize {
        match self {
            Self::Snapshot(bytes) | Self::Output { bytes, .. } => bytes.len(),
            Self::Resync | Self::Lagged | Self::Closed => 0,
        }
    }
}

/// Receives terminal stream items and releases their queued byte charge as
/// they leave the Mux queue.
pub struct StreamReceiver {
    receiver: Receiver<StreamItem>,
    queued: Arc<AtomicUsize>,
}

impl StreamReceiver {
    pub fn recv(&self) -> Result<StreamItem, RecvError> {
        self.receiver.recv().inspect(|item| self.consumed(item))
    }

    pub fn recv_timeout(&self, timeout: Duration) -> Result<StreamItem, RecvTimeoutError> {
        self.receiver
            .recv_timeout(timeout)
            .inspect(|item| self.consumed(item))
    }

    pub fn try_recv(&self) -> Result<StreamItem, mpsc::TryRecvError> {
        self.receiver.try_recv().inspect(|item| self.consumed(item))
    }

    fn consumed(&self, item: &StreamItem) {
        let bytes = item.weight();
        let _ = self.queued.fetch_update(Ordering::SeqCst, Ordering::SeqCst, |queued| {
            Some(queued.saturating_sub(bytes))
        });
    }
}

pub struct Mux {
    inner: Arc<MuxInner>,
}

struct MuxInner {
    writer: Arc<Mutex<LocalStream>>,
    shared: Arc<Shared>,
}

struct Shared {
    generation: String,
    terminal: bool,
    closed: AtomicBool,
    next_id: Arc<AtomicU64>,
    discarded_data_frames: AtomicU64,
    pending: Mutex<HashMap<u64, PendingCall>>,
    subscriptions: Mutex<HashMap<u64, Sender<Event>>>,
    streams: Mutex<HashMap<u32, Arc<Mutex<StreamSink>>>>,
    detach_tx: Sender<u32>,
}

struct PendingCall {
    response: Sender<Result<Value, CallError>>,
    attach: Option<StreamSink>,
    subscription: Option<Sender<Event>>,
    abandoned: bool,
}

pub(crate) struct StreamSink {
    sender: Sender<StreamItem>,
    queued: Arc<AtomicUsize>,
    snapshot: Vec<u8>,
    snapshot_started: bool,
}

impl Mux {
    pub fn open(
        endpoint: &Path,
        client_version: &str,
        majors: &[u32],
    ) -> Result<(Self, Welcome), CallError> {
        let mut read_stream = sirio_ipc::connect(endpoint).map_err(CallError::Io)?;
        LocalStreamExt::set_read_timeout(&read_stream, Some(Duration::from_millis(50)))
            .map_err(CallError::Io)?;
        let writer = read_stream.try_clone_stream().map_err(CallError::Io)?;
        LocalStreamExt::set_write_timeout(&writer, Some(WRITE_TIMEOUT)).map_err(CallError::Io)?;
        let writer = Arc::new(Mutex::new(writer));

        let params = serde_json::to_value(Hello {
            client_version: client_version.to_string(),
            majors: majors.to_vec(),
            minor: PROTOCOL_MINOR,
        })
        .map_err(|error| CallError::Decode(error.to_string()))?;
        let request = Request {
            id: 1,
            method: method::HELLO.to_string(),
            params,
            generation: None,
        };
        write_request(
            &mut writer.lock().unwrap_or_else(|poisoned| poisoned.into_inner()),
            &request,
        )?;
        let welcome = read_welcome(&mut read_stream)?;
        LocalStreamExt::set_read_timeout(&read_stream, None).map_err(CallError::Io)?;

        let (detach_tx, detach_rx) = mpsc::channel();
        let next_id = Arc::new(AtomicU64::new(2));
        let shared = Arc::new(Shared {
            generation: welcome.generation.clone(),
            terminal: has_terminal_capability(&welcome.capabilities),
            closed: AtomicBool::new(false),
            next_id: Arc::clone(&next_id),
            discarded_data_frames: AtomicU64::new(0),
            pending: Mutex::new(HashMap::new()),
            subscriptions: Mutex::new(HashMap::new()),
            streams: Mutex::new(HashMap::new()),
            detach_tx,
        });
        let detach_writer = Arc::clone(&writer);
        let detach_generation = welcome.generation.clone();
        let detach_next_id = Arc::clone(&next_id);
        if let Err(error) = std::thread::Builder::new()
            .name("sirio-host-client-detach".into())
            .spawn(move || {
                while let Ok(stream) = detach_rx.recv() {
                    let request = Request {
                        id: detach_next_id.fetch_add(1, Ordering::Relaxed),
                        method: method::TERM_DETACH.to_string(),
                        params: serde_json::to_value(DetachParams { stream })
                            .unwrap_or(Value::Null),
                        generation: Some(detach_generation.clone()),
                    };
                    let result = {
                        let mut writer = detach_writer
                            .lock()
                            .unwrap_or_else(|poisoned| poisoned.into_inner());
                        write_request(&mut writer, &request)
                    };
                    if result.is_err() {
                        break;
                    }
                }
            })
        {
            finish(&shared);
            return Err(CallError::Io(error));
        }
        let reader_shared = Arc::clone(&shared);
        if let Err(error) = std::thread::Builder::new()
            .name("sirio-host-client-reader".into())
            .spawn(move || reader_loop(read_stream, reader_shared))
        {
            finish(&shared);
            return Err(CallError::Io(error));
        }
        Ok((
            Self {
                inner: Arc::new(MuxInner {
                    writer,
                    shared,
                }),
            },
            welcome,
        ))
    }

    pub fn call(
        &self,
        method: &str,
        params: Value,
        effect: bool,
        timeout: Duration,
    ) -> Result<Value, CallError> {
        self.call_with(method, params, effect, timeout, None, None)
    }

    pub fn subscribe(&self, topic: &str) -> Result<Receiver<Event>, CallError> {
        let (event_tx, event_rx) = mpsc::channel();
        let params = serde_json::json!({ "topic": topic });
        self.call_with(
            method::SUBSCRIBE,
            params,
            false,
            CALL_TIMEOUT,
            None,
            Some(event_tx),
        )?;
        Ok(event_rx)
    }

    /// Send raw input bytes for an attached stream as one or more Data frames.
    pub fn send_data(&self, stream: u32, bytes: &[u8]) -> Result<(), CallError> {
        if !self.inner.shared.terminal {
            return Err(CallError::Decode(
                "host did not advertise the terminal capability".to_string(),
            ));
        }
        if self.inner.shared.closed.load(Ordering::SeqCst) {
            return Err(unexpected_eof());
        }
        let mut writer = self
            .inner
            .writer
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if bytes.is_empty() {
            let payload = encode_data(
                &DataHeader {
                    stream,
                    phase: Phase::Input,
                    offset: 0,
                },
                bytes,
            )
            .map_err(|error| CallError::Decode(error.to_string()))?;
            return write_frame(&mut writer, FrameKind::Data, &payload);
        }
        for chunk in bytes.chunks(MAX_DATA_BYTES) {
            let payload = encode_data(
                &DataHeader {
                    stream,
                    phase: Phase::Input,
                    offset: 0,
                },
                chunk,
            )
            .map_err(|error| CallError::Decode(error.to_string()))?;
            write_frame(&mut writer, FrameKind::Data, &payload)?;
        }
        Ok(())
    }

    pub fn generation(&self) -> &str {
        &self.inner.shared.generation
    }

    pub(crate) fn has_terminal(&self) -> bool {
        self.inner.shared.terminal
    }

    pub fn discarded_data_frames(&self) -> u64 {
        self.inner
            .shared
            .discarded_data_frames
            .load(Ordering::SeqCst)
    }

    pub fn closed(&self) -> bool {
        self.inner.shared.closed.load(Ordering::SeqCst)
    }

    pub(crate) fn call_attach(
        &self,
        method: &str,
        params: Value,
        timeout: Duration,
        sink: StreamSink,
    ) -> Result<Value, CallError> {
        self.call_with(method, params, true, timeout, Some(sink), None)
    }

    pub(crate) fn forget_stream(&self, stream: u32) {
        self.inner
            .shared
            .streams
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(&stream);
    }

    fn call_with(
        &self,
        method: &str,
        params: Value,
        effect: bool,
        timeout: Duration,
        attach: Option<StreamSink>,
        subscription: Option<Sender<Event>>,
    ) -> Result<Value, CallError> {
        if self.closed() {
            return Err(unexpected_eof());
        }
        let id = self.inner.shared.next_id.fetch_add(1, Ordering::Relaxed);
        let (response_tx, response_rx) = mpsc::channel();
        let request = Request {
            id,
            method: method.to_string(),
            params,
            generation: effect.then(|| self.generation().to_string()),
        };
        self.inner
            .shared
            .pending
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(
                id,
                PendingCall {
                    response: response_tx,
                    attach,
                    subscription,
                    abandoned: false,
                },
            );

        let write_result = {
            let mut writer = self
                .inner
                .writer
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            write_request(&mut writer, &request)
        };
        if let Err(error) = write_result {
            self.inner
                .shared
                .pending
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .remove(&id);
            return Err(error);
        }

        match response_rx.recv_timeout(timeout) {
            Ok(result) => result,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                let mut pending = self
                    .inner
                    .shared
                    .pending
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                if let Some(call) = pending.get_mut(&id) {
                    if call.attach.is_some() {
                        call.abandoned = true;
                        drop(response_rx);
                    } else {
                        pending.remove(&id);
                    }
                    Err(CallError::Timeout)
                } else {
                    match response_rx.try_recv() {
                        Ok(result) => result,
                        Err(mpsc::TryRecvError::Empty | mpsc::TryRecvError::Disconnected) => {
                            Err(CallError::Timeout)
                        }
                    }
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => Err(unexpected_eof()),
        }
    }
}

impl Drop for MuxInner {
    fn drop(&mut self) {
        self.writer
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .abort();
    }
}

fn write_request(writer: &mut LocalStream, request: &Request) -> Result<(), CallError> {
    let payload =
        serde_json::to_vec(request).map_err(|error| CallError::Decode(error.to_string()))?;
    write_frame(writer, FrameKind::Request, &payload)
}

fn write_frame(writer: &mut LocalStream, kind: FrameKind, payload: &[u8]) -> Result<(), CallError> {
    let frame = encode(kind, payload).map_err(CallError::Frame)?;
    writer
        .write_all(&frame)
        .and_then(|_| writer.flush())
        .map_err(CallError::Io)
}

fn read_welcome(stream: &mut LocalStream) -> Result<Welcome, CallError> {
    let deadline = Instant::now() + HANDSHAKE_TIMEOUT;
    let mut decoder = FrameDecoder::new();
    let mut buffer = [0u8; 16 * 1024];
    loop {
        if let Some(frame) = decoder.next_frame().map_err(CallError::Frame)? {
            if frame.kind != FrameKind::Response {
                continue;
            }
            let response: Response = serde_json::from_slice(&frame.payload)
                .map_err(|error| CallError::Decode(error.to_string()))?;
            if response.id != 1 {
                continue;
            }
            let value = response_value(response)?;
            let reply: HelloReply = serde_json::from_value(value)
                .map_err(|error| CallError::Decode(error.to_string()))?;
            return match reply {
                HelloReply::Welcome(welcome) => Ok(welcome),
                HelloReply::Refused(refused) => Err(CallError::Decode(format!(
                    "host refused protocol major {}",
                    refused.host_major
                ))),
            };
        }
        if Instant::now() >= deadline {
            return Err(CallError::Timeout);
        }
        match stream.read(&mut buffer) {
            Ok(0) => return Err(unexpected_eof()),
            Ok(read) => decoder.push(&buffer[..read]),
            Err(error) if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut | ErrorKind::Interrupted) => {}
            Err(error) => return Err(CallError::Io(error)),
        }
    }
}

fn reader_loop(mut stream: LocalStream, shared: Arc<Shared>) {
    let mut decoder = FrameDecoder::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        match stream.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => decoder.push(&buffer[..read]),
            Err(error) if error.kind() == ErrorKind::Interrupted => {}
            Err(_) => break,
        }
        loop {
            let frame = match decoder.next_frame() {
                Ok(Some(frame)) => frame,
                Ok(None) => break,
                Err(_) => {
                    finish(&shared);
                    return;
                }
            };
            let result = match frame.kind {
                FrameKind::Response => serde_json::from_slice::<Response>(&frame.payload)
                    .map(|response| dispatch_response(&shared, response))
                    .map_err(|_| ()),
                FrameKind::Event => serde_json::from_slice::<Event>(&frame.payload)
                    .map(|event| dispatch_event(&shared, event))
                    .map_err(|_| ()),
                FrameKind::Data => dispatch_data(&shared, &frame.payload),
                FrameKind::Request => Err(()),
            };
            if result.is_err() {
                finish(&shared);
                return;
            }
        }
    }
    finish(&shared);
}

fn dispatch_response(shared: &Shared, response: Response) {
    let mut pending_calls = shared
        .pending
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let pending = pending_calls.remove(&response.id);
    let Some(pending) = pending else { return };
    let value = response_value(response);
    let mut installed_stream = None;
    let mut installed_subscription = None;
    if let (Ok(value), Some(mut sink)) = (&value, pending.attach) {
        if let Ok(attached) = serde_json::from_value::<Attached>(value.clone()) {
            let stream = attached.stream;
            if pending.abandoned {
                request_detach(shared, stream);
            } else {
                shared
                    .streams
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .insert(stream, Arc::new(Mutex::new(sink)));
                installed_stream = Some(stream);
            }
        } else {
            // The slot has no stream id to detach if the host refused attach.
            sink.snapshot.clear();
        }
    }
    if let (Ok(value), Some(sender)) = (&value, pending.subscription)
        && let Ok(subscribed) = serde_json::from_value::<Subscribed>(value.clone())
    {
        shared
            .subscriptions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(subscribed.subscription, sender);
        installed_subscription = Some(subscribed.subscription);
    }
    if pending.response.send(value).is_err() {
        if let Some(stream) = installed_stream {
            shared
                .streams
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .remove(&stream);
            request_detach(shared, stream);
        }
        if let Some(subscription) = installed_subscription {
            shared
                .subscriptions
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .remove(&subscription);
        }
    }
}

fn request_detach(shared: &Shared, stream: u32) {
    let _ = shared.detach_tx.send(stream);
}

fn has_terminal_capability(capabilities: &[String]) -> bool {
    capabilities
        .iter()
        .any(|name| name == capability::TERMINAL)
}

fn response_value(response: Response) -> Result<Value, CallError> {
    match (response.result, response.error) {
        (_, Some(error)) => Err(CallError::Remote(error)),
        (Some(result), None) => Ok(result),
        (None, None) => Ok(Value::Null),
    }
}

fn dispatch_event(shared: &Shared, event: Event) {
    if event.subscription == 0 {
        let Ok(session_event) = serde_json::from_value::<SessionEvent>(event.payload) else {
            return;
        };
        match session_event {
            SessionEvent::Lagged { stream } => {
                let _ = stream_event(shared, stream, StreamItem::Lagged, true);
            }
            SessionEvent::Resync { stream } => {
                let _ = stream_event(shared, stream, StreamItem::Resync, false);
            }
            _ => {}
        }
        return;
    }
    let sender = shared
        .subscriptions
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .get(&event.subscription)
        .cloned();
    if let Some(sender) = sender
        && sender.send(event.clone()).is_err()
    {
        shared
            .subscriptions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(&event.subscription);
    }
}

fn dispatch_data(shared: &Shared, payload: &[u8]) -> Result<(), ()> {
    let (header, bytes) = decode_data(payload).map_err(|_| ())?;
    match header.phase {
        Phase::Snapshot => {
            let sink = {
                shared
                    .streams
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .get(&header.stream)
                    .cloned()
            };
            let Some(sink) = sink else {
                shared.discarded_data_frames.fetch_add(1, Ordering::SeqCst);
                return Ok(());
            };
            let mut sink = sink.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            if !reserve_stream_bytes(&sink.queued, bytes.len()) {
                drop_snapshot_charge(&mut sink);
                let _ = sink.sender.send(StreamItem::Lagged);
                drop(sink);
                remove_stream(shared, header.stream);
                request_detach(shared, header.stream);
                return Ok(());
            }
            sink.snapshot.extend_from_slice(bytes);
            sink.snapshot_started = true;
            Ok(())
        }
        Phase::Output => {
            if !stream_event(
                shared,
                header.stream,
                StreamItem::Output {
                    offset: header.offset,
                    bytes: bytes.to_vec(),
                },
                false,
            ) {
                shared.discarded_data_frames.fetch_add(1, Ordering::SeqCst);
            }
            Ok(())
        }
        Phase::Input => Err(()),
    }
}

/// Returns false only when the stream id is unknown. Queue overflow and a
/// dropped consumer are handled locally and never fail the connection reader.
fn stream_event(shared: &Shared, stream: u32, item: StreamItem, remove: bool) -> bool {
    let sink = {
        let mut streams = shared
            .streams
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if remove {
            streams.remove(&stream)
        } else {
            streams.get(&stream).cloned()
        }
    };
    let Some(sink) = sink else { return false };
    let mut sink = sink.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if sink.snapshot_started {
        let snapshot = std::mem::take(&mut sink.snapshot);
        let snapshot_len = snapshot.len();
        sink.snapshot_started = false;
        if sink.sender.send(StreamItem::Snapshot(snapshot)).is_err() {
            release_stream_bytes(&sink.queued, snapshot_len);
            drop(sink);
            remove_stream(shared, stream);
            request_detach(shared, stream);
            return true;
        }
    }
    match send_stream_item(&sink, item) {
        Ok(()) => true,
        Err(StreamSendError::Overflow) => {
            let _ = sink.sender.send(StreamItem::Lagged);
            drop(sink);
            remove_stream(shared, stream);
            request_detach(shared, stream);
            true
        }
        Err(StreamSendError::Disconnected) => {
            drop(sink);
            remove_stream(shared, stream);
            request_detach(shared, stream);
            true
        }
    }
}

enum StreamSendError {
    Overflow,
    Disconnected,
}

fn send_stream_item(sink: &StreamSink, item: StreamItem) -> Result<(), StreamSendError> {
    let weight = item.weight();
    if !reserve_stream_bytes(&sink.queued, weight) {
        return Err(StreamSendError::Overflow);
    }
    if sink.sender.send(item).is_err() {
        release_stream_bytes(&sink.queued, weight);
        return Err(StreamSendError::Disconnected);
    }
    Ok(())
}

fn reserve_stream_bytes(queued: &AtomicUsize, bytes: usize) -> bool {
    queued
        .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |current| {
            let total = current.saturating_add(bytes);
            (total <= STREAM_QUEUE_BYTES).then_some(total)
        })
        .is_ok()
}

fn release_stream_bytes(queued: &AtomicUsize, bytes: usize) {
    let _ = queued.fetch_update(Ordering::SeqCst, Ordering::SeqCst, |current| {
        Some(current.saturating_sub(bytes))
    });
}

fn drop_snapshot_charge(sink: &mut StreamSink) {
    release_stream_bytes(&sink.queued, sink.snapshot.len());
    sink.snapshot.clear();
    sink.snapshot_started = false;
}

fn remove_stream(shared: &Shared, stream: u32) {
    shared
        .streams
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .remove(&stream);
}

pub(crate) fn stream_channel() -> (StreamSink, StreamReceiver) {
    let (sender, receiver) = mpsc::channel();
    let queued = Arc::new(AtomicUsize::new(0));
    (
        StreamSink {
            sender,
            queued: Arc::clone(&queued),
            snapshot: Vec::new(),
            snapshot_started: false,
        },
        StreamReceiver { receiver, queued },
    )
}

fn finish(shared: &Shared) {
    if shared.closed.swap(true, Ordering::SeqCst) {
        return;
    }
    let pending = std::mem::take(
        &mut *shared
            .pending
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()),
    );
    for (_, pending) in pending {
        if let Some(attach) = pending.attach {
            let _ = attach.sender.send(StreamItem::Closed);
        }
        let _ = pending.response.send(Err(unexpected_eof()));
    }
    shared
        .subscriptions
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clear();
    let streams = std::mem::take(
        &mut *shared
            .streams
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()),
    );
    for (_, sink) in streams {
        let mut sink = sink.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        drop_snapshot_charge(&mut sink);
        let _ = sink.sender.send(StreamItem::Closed);
    }
}

fn unexpected_eof() -> CallError {
    CallError::Io(io::Error::new(
        ErrorKind::UnexpectedEof,
        "host closed the connection",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shared_with_stream(sink: StreamSink) -> (Arc<Shared>, Receiver<u32>) {
        let (detach_tx, detach_rx) = mpsc::channel();
        (
            Arc::new(Shared {
                generation: "generation".to_string(),
                terminal: true,
                closed: AtomicBool::new(false),
                next_id: Arc::new(AtomicU64::new(2)),
                discarded_data_frames: AtomicU64::new(0),
                pending: Mutex::new(HashMap::new()),
                subscriptions: Mutex::new(HashMap::new()),
                streams: Mutex::new(HashMap::from([(
                    7,
                    Arc::new(Mutex::new(sink)),
                )])),
                detach_tx,
            }),
            detach_rx,
        )
    }

    fn empty_shared() -> Shared {
        let (detach_tx, _detach_rx) = mpsc::channel();
        Shared {
            generation: "generation".to_string(),
            terminal: false,
            closed: AtomicBool::new(false),
            next_id: Arc::new(AtomicU64::new(2)),
            discarded_data_frames: AtomicU64::new(0),
            pending: Mutex::new(HashMap::new()),
            subscriptions: Mutex::new(HashMap::new()),
            streams: Mutex::new(HashMap::new()),
            detach_tx,
        }
    }

    #[test]
    fn unread_stream_does_not_block_dispatch() {
        let (sink, receiver) = stream_channel();
        let (shared, _detach_rx) = shared_with_stream(sink);
        for offset in 0..8 {
            assert!(stream_event(
                &shared,
                7,
                StreamItem::Output {
                    offset,
                    bytes: vec![b'x'],
                },
                false,
            ));
        }

        let (done_tx, done_rx) = mpsc::channel();
        let dispatch_shared = Arc::clone(&shared);
        let dispatch = std::thread::spawn(move || {
            let _ = stream_event(
                &dispatch_shared,
                7,
                StreamItem::Output {
                    offset: 8,
                    bytes: vec![b'y'],
                },
                false,
            );
            let _ = done_tx.send(());
        });
        let completed = done_rx.recv_timeout(Duration::from_millis(100)).is_ok();
        drop(receiver);
        dispatch.join().unwrap();
        assert!(completed, "dispatch must not wait for the stream consumer");
    }

    #[test]
    fn a_stream_is_lagged_when_unconsumed_bytes_exceed_four_mib() {
        let (sink, receiver) = stream_channel();
        let (shared, detach_rx) = shared_with_stream(sink);
        assert!(stream_event(
            &shared,
            7,
            StreamItem::Output {
                offset: 0,
                bytes: vec![b'x'; 4 << 20],
            },
            false,
        ));
        assert!(stream_event(
            &shared,
            7,
            StreamItem::Output {
                offset: 4 << 20,
                bytes: vec![b'y'],
            },
            false,
        ));

        let mut items = Vec::new();
        while let Ok(item) = receiver.try_recv() {
            items.push(item);
        }
        assert_eq!(items.last(), Some(&StreamItem::Lagged));
        assert_eq!(detach_rx.recv_timeout(Duration::from_secs(1)).unwrap(), 7);
        assert!(shared
            .streams
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get(&7)
            .is_none());
    }

    #[test]
    fn receiving_stream_bytes_releases_their_queue_charge() {
        let (sink, receiver) = stream_channel();
        let (shared, _detach_rx) = shared_with_stream(sink);
        assert!(stream_event(
            &shared,
            7,
            StreamItem::Output {
                offset: 0,
                bytes: vec![b'x'; STREAM_QUEUE_BYTES],
            },
            false,
        ));
        let _ = receiver.recv().unwrap();
        assert!(stream_event(
            &shared,
            7,
            StreamItem::Output {
                offset: STREAM_QUEUE_BYTES as u64,
                bytes: vec![b'y'],
            },
            false,
        ));
        assert!(matches!(receiver.recv().unwrap(), StreamItem::Output { bytes, .. } if bytes == b"y"));
    }

    #[test]
    fn data_for_an_unknown_stream_is_discarded() {
        let shared = empty_shared();
        let payload = encode_data(
            &DataHeader {
                stream: 77,
                phase: Phase::Output,
                offset: 0,
            },
            b"unknown",
        )
        .unwrap();
        assert!(dispatch_data(&shared, &payload).is_ok());
        assert!(!shared.closed.load(Ordering::SeqCst));
        assert_eq!(shared.discarded_data_frames.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn a_late_attached_response_for_a_timed_out_call_is_detached() {
        let (detach_tx, detach_rx) = mpsc::channel();
        let shared = Shared {
            generation: "generation".to_string(),
            terminal: true,
            closed: AtomicBool::new(false),
            next_id: Arc::new(AtomicU64::new(2)),
            pending: Mutex::new(HashMap::new()),
            subscriptions: Mutex::new(HashMap::new()),
            streams: Mutex::new(HashMap::new()),
            discarded_data_frames: AtomicU64::new(0),
            detach_tx,
        };
        let (response, response_rx) = mpsc::channel();
        drop(response_rx);
        let (sink, _stream) = stream_channel();
        shared
            .pending
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(
                9,
                PendingCall {
                    response,
                    attach: Some(sink),
                    subscription: None,
                    abandoned: true,
                },
            );

        dispatch_response(
            &shared,
            Response::ok(
                9,
                Attached {
                    stream: 13,
                    size: sirio_host_protocol::term::TermSize { cols: 80, rows: 24 },
                    mode: sirio_host_protocol::term::AttachMode::Snapshot,
                },
            ),
        );

        assert_eq!(detach_rx.recv_timeout(Duration::from_secs(1)).unwrap(), 13);
        assert!(shared
            .streams
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .is_empty());
    }

    #[test]
    fn terminal_capability_is_required_for_data_frames() {
        assert!(!has_terminal_capability(&[]));
        assert!(!has_terminal_capability(&["host.v1".to_string()]));
        assert!(has_terminal_capability(&["terminal".to_string()]));
    }
}
