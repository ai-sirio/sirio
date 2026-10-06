//! A multiplexed SRH1 client connection. One blocking reader dispatches
//! replies, subscriptions and terminal streams while callers share a writer.

use std::collections::HashMap;
use std::io::{self, ErrorKind, Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, SyncSender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::Value;
use sirio_host_protocol::data::{DataHeader, MAX_DATA_BYTES, Phase, decode_data, encode_data};
use sirio_host_protocol::frame::{FrameDecoder, FrameKind, encode};
use sirio_host_protocol::messages::{
    Event, Hello, HelloReply, Request, Response, Subscribed, Welcome, method,
};
use sirio_host_protocol::term::{Attached, SessionEvent};
use sirio_host_protocol::version::PROTOCOL_MINOR;
use sirio_ipc::{LocalStream, LocalStreamExt};

use crate::connection::CallError;

const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);
pub(crate) const CALL_TIMEOUT: Duration = Duration::from_secs(5);
const WRITE_TIMEOUT: Duration = Duration::from_secs(10);
const SNAPSHOT_QUIET_PERIOD: Duration = Duration::from_millis(50);
const SNAPSHOT_TIMER_TICK: Duration = Duration::from_millis(10);
pub(crate) const STREAM_QUEUE_DEPTH: usize = 4;

#[derive(Debug, PartialEq, Eq)]
pub enum StreamItem {
    Snapshot(Vec<u8>),
    Output { offset: u64, bytes: Vec<u8> },
    Resync,
    Lagged,
    Closed,
}

pub struct Mux {
    inner: Arc<MuxInner>,
}

struct MuxInner {
    writer: Mutex<LocalStream>,
    shared: Arc<Shared>,
    next_id: AtomicU64,
}

struct Shared {
    generation: String,
    closed: AtomicBool,
    pending: Mutex<HashMap<u64, PendingCall>>,
    subscriptions: Mutex<HashMap<u64, Sender<Event>>>,
    streams: Mutex<HashMap<u32, Arc<Mutex<StreamSink>>>>,
}

struct PendingCall {
    response: Sender<Result<Value, CallError>>,
    attach: Option<SyncSender<StreamItem>>,
    subscription: Option<Sender<Event>>,
}

struct StreamSink {
    sender: SyncSender<StreamItem>,
    snapshot: Vec<u8>,
    snapshot_started: bool,
    last_snapshot: Instant,
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
        let mut writer = read_stream.try_clone_stream().map_err(CallError::Io)?;
        LocalStreamExt::set_write_timeout(&writer, Some(WRITE_TIMEOUT)).map_err(CallError::Io)?;

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
        write_request(&mut writer, &request)?;
        let welcome = read_welcome(&mut read_stream)?;
        LocalStreamExt::set_read_timeout(&read_stream, None).map_err(CallError::Io)?;

        let shared = Arc::new(Shared {
            generation: welcome.generation.clone(),
            closed: AtomicBool::new(false),
            pending: Mutex::new(HashMap::new()),
            subscriptions: Mutex::new(HashMap::new()),
            streams: Mutex::new(HashMap::new()),
        });
        let reader_shared = Arc::clone(&shared);
        if let Err(error) = std::thread::Builder::new()
            .name("sirio-host-client-reader".into())
            .spawn(move || reader_loop(read_stream, reader_shared))
        {
            finish(&shared);
            return Err(CallError::Io(error));
        }
        let timer_shared = Arc::clone(&shared);
        if let Err(error) = std::thread::Builder::new()
            .name("sirio-host-client-snapshot-timer".into())
            .spawn(move || snapshot_timer(timer_shared))
        {
            finish(&shared);
            return Err(CallError::Io(error));
        }

        Ok((
            Self {
                inner: Arc::new(MuxInner {
                    writer: Mutex::new(writer),
                    shared,
                    next_id: AtomicU64::new(2),
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

    pub fn closed(&self) -> bool {
        self.inner.shared.closed.load(Ordering::SeqCst)
    }

    pub(crate) fn call_attach(
        &self,
        method: &str,
        params: Value,
        timeout: Duration,
        sink: SyncSender<StreamItem>,
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
        attach: Option<SyncSender<StreamItem>>,
        subscription: Option<Sender<Event>>,
    ) -> Result<Value, CallError> {
        if self.closed() {
            return Err(unexpected_eof());
        }
        let id = self.inner.next_id.fetch_add(1, Ordering::Relaxed);
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
                self.inner
                    .shared
                    .pending
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .remove(&id);
                Err(CallError::Timeout)
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => Err(unexpected_eof()),
        }
    }
}

impl Drop for MuxInner {
    fn drop(&mut self) {
        self.writer
            .get_mut()
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
            Err(error) if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
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

fn snapshot_timer(shared: Arc<Shared>) {
    while !shared.closed.load(Ordering::SeqCst) {
        std::thread::sleep(SNAPSHOT_TIMER_TICK);
        if !shared.closed.load(Ordering::SeqCst) {
            flush_idle_snapshots(&shared);
        }
    }
}

fn flush_idle_snapshots(shared: &Shared) {
    let sinks: Vec<_> = shared
        .streams
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .values()
        .cloned()
        .collect();
    for sink in sinks {
        let mut sink = sink.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if shared.closed.load(Ordering::SeqCst)
            || !sink.snapshot_started
            || sink.last_snapshot.elapsed() < SNAPSHOT_QUIET_PERIOD
        {
            continue;
        }
        let snapshot = std::mem::take(&mut sink.snapshot);
        sink.snapshot_started = false;
        if sink.sender.send(StreamItem::Snapshot(snapshot)).is_err() {
            return;
        }
    }
}

fn dispatch_response(shared: &Shared, response: Response) {
    let pending = shared
        .pending
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .remove(&response.id);
    let Some(pending) = pending else { return };
    let value = response_value(response);
    let mut installed_stream = None;
    let mut installed_subscription = None;
    if let (Ok(value), Some(sender)) = (&value, pending.attach)
        && let Ok(attached) = serde_json::from_value::<Attached>(value.clone())
    {
        let stream = attached.stream;
        shared
            .streams
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(
                stream,
                Arc::new(Mutex::new(StreamSink {
                    sender,
                    snapshot: Vec::new(),
                    snapshot_started: false,
                    last_snapshot: Instant::now(),
                })),
            );
        installed_stream = Some(stream);
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
            }
            .ok_or(())?;
            let mut sink = sink.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            sink.snapshot.extend_from_slice(bytes);
            sink.snapshot_started = true;
            sink.last_snapshot = Instant::now();
            Ok(())
        }
        Phase::Output => stream_event(
            shared,
            header.stream,
            StreamItem::Output {
                offset: header.offset,
                bytes: bytes.to_vec(),
            },
            false,
        ),
        Phase::Input => Err(()),
    }
}

fn stream_event(shared: &Shared, stream: u32, item: StreamItem, remove: bool) -> Result<(), ()> {
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
    let Some(sink) = sink else { return Err(()) };
    let mut sink = sink.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if sink.snapshot_started {
        let snapshot = std::mem::take(&mut sink.snapshot);
        sink.snapshot_started = false;
        sink.sender
            .send(StreamItem::Snapshot(snapshot))
            .map_err(|_| ())?;
    }
    sink.sender.send(item).map_err(|_| ())
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
            let _ = attach.send(StreamItem::Closed);
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
        sink.snapshot_started = false;
        sink.snapshot.clear();
        let _ = sink.sender.send(StreamItem::Closed);
    }
}

fn unexpected_eof() -> CallError {
    CallError::Io(io::Error::new(
        ErrorKind::UnexpectedEof,
        "host closed the connection",
    ))
}
