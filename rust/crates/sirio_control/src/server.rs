//! The NDJSON control server: accepts connections on a unix socket, reads
//! one JSON request per line, dispatches to the app's handler, and writes
//! one JSON response per line. Ported from
//! `SirioControl/ControlServer.swift`.
//!
//! The server knows nothing about the app: it carries the transport and the
//! protocol, and delegates every request to a [`ControlHandler`] the app
//! implements.
//!
//! Robustness properties, each covered by a test:
//! - one blocked or idle client never blocks others (one thread per
//!   connection);
//! - a client that disconnects mid-request is torn down without affecting
//!   the server;
//! - a request line larger than [`MAX_BUFFER_BYTES`] is rejected with a
//!   failure response and the connection closed, not buffered without
//!   limit;
//! - concurrent clients are all served;
//! - a stale socket file left by a crashed run is probed and replaced, so
//!   bind never fails forever; a *live* socket (another running Sirio) is
//!   never stolen.
//! - a malformed request line gets a failure response and the connection
//!   keeps serving.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use sirio_ipc::{BindError, LocalListener, LocalStreamExt};

use crate::protocol::{ControlRequest, ControlResponse, decode_request, encode_line};

/// Per-connection input buffer cap (1 MiB), matching the Swift server.
/// Prevents a local memory DoS from an unbounded request line.
pub const MAX_BUFFER_BYTES: usize = 1 << 20;

/// How the app answers a control request. Implementations must be cheap to
/// call and safe to invoke from any connection thread; the trait is
/// synchronous, so a handler that needs async work must resolve it
/// internally (respond from current state) rather than block.
pub trait ControlHandler: Send + Sync {
    fn handle(&self, request: &ControlRequest) -> ControlResponse;
}

/// A failure while starting the control server.
#[derive(Debug)]
pub enum ServerError {
    /// The socket path exceeds `sun_path` capacity.
    PathTooLong { path: PathBuf },
    /// A live server is already listening on the path (the probe connect
    /// succeeded), so the socket is not stolen.
    AlreadyRunning { path: PathBuf },
    /// Binding the unix socket failed.
    BindFailed { path: PathBuf, detail: String },
    /// Marking the socket 0600 failed.
    ChmodFailed { path: PathBuf, detail: String },
    /// The bound path failed its ownership/permissiveness sanity check.
    InsecureSocket { detail: String },
}

impl std::fmt::Display for ServerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ServerError::PathTooLong { path } => write!(
                f,
                "socket path too long ({} bytes, max {}): {}",
                path.as_os_str().as_encoded_bytes().len(),
                sirio_ipc::MAX_SOCKET_PATH_LENGTH,
                path.display()
            ),
            ServerError::AlreadyRunning { path } => {
                write!(f, "a Sirio is already listening at {}", path.display())
            }
            ServerError::BindFailed { path, detail } => {
                write!(f, "bind {}: {detail}", path.display())
            }
            ServerError::ChmodFailed { path, detail } => {
                write!(f, "chmod 0o600 {}: {detail}", path.display())
            }
            ServerError::InsecureSocket { detail } => write!(f, "insecure socket: {detail}"),
        }
    }
}

impl std::error::Error for ServerError {}

impl From<BindError> for ServerError {
    fn from(error: BindError) -> Self {
        match error {
            BindError::PathTooLong { path } => ServerError::PathTooLong { path },
            BindError::AlreadyRunning { path } => ServerError::AlreadyRunning { path },
            BindError::BindFailed { path, detail } => ServerError::BindFailed { path, detail },
            BindError::ChmodFailed { path, detail } => ServerError::ChmodFailed { path, detail },
            BindError::InsecureSocket { detail } => ServerError::InsecureSocket { detail },
        }
    }
}

/// The control server. Bind it with [`ControlServer::start`], which spawns
/// the accept loop; drop or [`ControlServer::stop`] to shut it down.
pub struct ControlServer {
    socket_path: PathBuf,
    handler: Arc<dyn ControlHandler>,
    shutdown: Arc<AtomicBool>,
    /// True once this server bound the socket. Only a server that actually
    /// owns the socket file may remove it on stop — a failed start must not
    /// unlink a live sibling's socket.
    started: AtomicBool,
    accept_thread: Mutex<Option<std::thread::JoinHandle<()>>>,
}

impl ControlServer {
    /// Creates a server for `socket_path` that dispatches to `handler`.
    pub fn new(socket_path: impl Into<PathBuf>, handler: Arc<dyn ControlHandler>) -> Self {
        Self {
            socket_path: socket_path.into(),
            handler,
            shutdown: Arc::new(AtomicBool::new(false)),
            started: AtomicBool::new(false),
            accept_thread: Mutex::new(None),
        }
    }

    /// The socket path this server serves.
    pub fn socket_path(&self) -> &Path {
        &self.socket_path
    }

    /// Binds the local endpoint and starts accepting connections.
    ///
    /// The transport — a unix domain socket, or a named pipe on Windows —
    /// and its layers of defence (a private bind with no permissive window,
    /// the post-bind check, per-connection peer-identity checks) live in
    /// [`sirio_ipc::LocalListener`]; the wire protocol and the framing code
    /// below are shared verbatim by both.
    pub fn start(&self) -> Result<(), ServerError> {
        let listener = LocalListener::bind(&self.socket_path)?;

        let handler = Arc::clone(&self.handler);
        let shutdown = Arc::clone(&self.shutdown);
        let handle = std::thread::spawn(move || {
            accept_loop(listener, handler, shutdown);
        });
        *self.accept_thread.lock().expect("accept thread mutex") = Some(handle);
        self.started.store(true, Ordering::SeqCst);
        Ok(())
    }

    /// Shuts the server down: stops the accept loop, closes the listener,
    /// and removes the socket file. Idempotent, and safe to call on a server
    /// whose [`start`](Self::start) failed — it never touches a socket file
    /// it does not own.
    ///
    /// On Windows there is nothing to remove: the pipe object dies with the
    /// process, so the best-effort unlink below simply finds no file.
    pub fn stop(&self) {
        if !self.started.swap(false, Ordering::SeqCst) {
            return;
        }
        self.shutdown.store(true, Ordering::SeqCst);
        if let Some(handle) = self
            .accept_thread
            .lock()
            .expect("accept thread mutex")
            .take()
        {
            let _ = handle.join();
        }
        let _ = fs::remove_file(&self.socket_path);
    }
}

impl Drop for ControlServer {
    fn drop(&mut self) {
        self.stop();
    }
}

/// How long a read that would block waits before looking again. Also the
/// cadence the local listener polls at, so shutdown is detected without
/// depending on any wake-up mechanism.
const POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(5);

/// Accepts connections until shutdown. Each connection gets its own thread,
/// so an idle or malicious client can never block another. Peer-identity
/// checks happen inside [`LocalListener::accept`], so every stream that
/// reaches here is already verified same-user.
fn accept_loop(
    mut listener: LocalListener,
    handler: Arc<dyn ControlHandler>,
    shutdown: Arc<AtomicBool>,
) {
    while let Some(stream) = listener.accept(&shutdown) {
        let handler = Arc::clone(&handler);
        std::thread::spawn(move || serve_connection(stream, handler));
    }
}

/// Serves one connection: read one request per line, dispatch, write one
/// response per line, in order. Never panics: every failure mode closes the
/// connection, never the server.
fn serve_connection<S: LocalStreamExt>(mut stream: S, handler: Arc<dyn ControlHandler>) {
    // On macOS (and other BSDs) the accepted socket inherits the listener's
    // O_NONBLOCK; restore blocking so reads wait for the client's data
    // instead of failing instantly with WouldBlock.
    stream.restore_blocking();

    let mut buffer: Vec<u8> = Vec::new();
    let mut chunk = [0u8; 64 * 1024];
    loop {
        // Pull complete lines out of the buffer first (a read may carry
        // several requests).
        while let Some(newline) = buffer.iter().position(|byte| *byte == b'\n') {
            if newline > MAX_BUFFER_BYTES {
                // Check the complete line before draining or dispatching it;
                // otherwise a single read can bypass the cap entirely.
                let response = ControlResponse::failure("?", "request line too large");
                let _ = write_line(&mut stream, &response);
                stream.abort();
                return;
            }
            let line: Vec<u8> = buffer.drain(..=newline).collect();
            let line = &line[..line.len() - 1];
            if line.is_empty() {
                continue; // blank lines are ignored, as in the Swift server
            }
            match decode_request(line) {
                Ok(request) => {
                    let response = handler.handle(&request);
                    if !write_line(&mut stream, &response) {
                        return;
                    }
                }
                Err(error) => {
                    // Malformed request: answer with an error and keep
                    // serving this connection.
                    let response =
                        ControlResponse::failure("?", format!("malformed request: {error}"));
                    if !write_line(&mut stream, &response) {
                        return;
                    }
                }
            }
        }

        if buffer.len() > MAX_BUFFER_BYTES {
            // A request line larger than any sane buffer: reject and close
            // the connection, exactly like the Swift server.
            let response = ControlResponse::failure("?", "request line too large");
            let _ = write_line(&mut stream, &response);
            stream.abort();
            return;
        }

        match stream.read(&mut chunk) {
            Ok(0) => return, // clean disconnect
            Ok(n) => buffer.extend_from_slice(&chunk[..n]),
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            // Defensive: the stream should be blocking now, but a WouldBlock
            // must never be mistaken for a disconnect.
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                std::thread::sleep(POLL_INTERVAL);
            }
            Err(_) => return, // disconnect or read error — tear down
        }
    }
}

/// Full-write loop with EINTR retry. Returns false on failure (client gone).
fn write_line<S: Write>(stream: &mut S, response: &ControlResponse) -> bool {
    let Ok(data) = encode_line(response) else {
        return false;
    };
    let mut offset = 0;
    while offset < data.len() {
        match stream.write(&data[offset..]) {
            Ok(0) => return false,
            Ok(n) => offset += n,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(_) => return false,
        }
    }
    true
}
