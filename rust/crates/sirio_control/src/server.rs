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
use std::io::{self, Read, Write};
#[cfg(unix)]
use std::net::Shutdown;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
#[cfg(unix)]
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use crate::protocol::{ControlRequest, ControlResponse, decode_request, encode_line};
#[cfg(windows)]
use crate::windows_pipe::{ListenerError, PipeListener, post_bind_sanity_check};

/// Per-connection input buffer cap (1 MiB), matching the Swift server.
/// Prevents a local memory DoS from an unbounded request line.
pub const MAX_BUFFER_BYTES: usize = 1 << 20;

/// Maximum bytes in `sockaddr_un.sun_path` including the null terminator on
/// Maximum encoded Unix-socket path length for the target platform.
#[cfg(target_os = "linux")]
const MAX_SOCKET_PATH_LENGTH: usize = 108;
#[cfg(not(target_os = "linux"))]
const MAX_SOCKET_PATH_LENGTH: usize = 104;

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
                "socket path too long ({} bytes, max {MAX_SOCKET_PATH_LENGTH}): {}",
                path.as_os_str().as_encoded_bytes().len(),
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

    /// Binds the socket and starts accepting connections.
    ///
    /// Unix transport: a unix domain socket
    /// (`std::os::unix::net::{UnixListener, UnixStream}`), defended in
    /// three layers — private temporary bind + atomic rename (no permissive
    /// window), the post-bind stat below, and per-connection peer-credential
    /// checks. See [`windows_pipe`](crate::windows_pipe) for how each layer
    /// maps onto the named-pipe transport on Windows.
    #[cfg(unix)]
    pub fn start(&self) -> Result<(), ServerError> {
        self.prepare_parent_directory()?;
        self.prepare_path()?;

        // The socket must never exist at the umask-derived mode: between a
        // plain bind and a post-hoc chmod, the file sits at 0755 on a
        // default macOS umask — a window an attacker can race and connect
        // through (measured at 488/500 startups, and the captured connection
        // stays authorized after start() returns). The socket is therefore
        // bound under a private temporary name, chmodded, and atomically
        // renamed into place: the final path never exists in a permissive
        // state, because before the rename it does not exist at all and
        // after it it is already 0600.
        let listener =
            Self::bind_private(&self.socket_path).map_err(|error| ServerError::BindFailed {
                path: self.socket_path.clone(),
                detail: error.to_string(),
            })?;

        // Belt and braces: re-assert 0600 on the final path and fail loudly
        // if it cannot be enforced.
        fs::set_permissions(&self.socket_path, fs::Permissions::from_mode(0o600)).map_err(
            |error| ServerError::ChmodFailed {
                path: self.socket_path.clone(),
                detail: error.to_string(),
            },
        )?;

        self.post_bind_sanity_check()?;

        let handler = Arc::clone(&self.handler);
        let shutdown = Arc::clone(&self.shutdown);
        let handle = std::thread::spawn(move || {
            accept_loop(listener, handler, shutdown);
        });
        *self.accept_thread.lock().expect("accept thread mutex") = Some(handle);
        self.started.store(true, Ordering::SeqCst);
        Ok(())
    }

    /// Named-pipe transport: the Windows counterpart of the unix sequence
    /// above, with every security layer mapped 1:1 (see
    /// [`crate::windows_pipe`] for the full rationale):
    ///
    /// - unix private-bind + chmod + rename → the DACL is supplied atomically
    ///   inside `CreateNamedPipeW`, so there is no permissive window at all;
    /// - unix post-bind stat → `post_bind_sanity_check`, parsing back the
    ///   created object's DACL and refusing anything but owner+SYSTEM;
    /// - unix stale/live takeover probe → `FILE_FLAG_FIRST_PIPE_INSTANCE`
    ///   fails the create if the name exists, then a client-connect probe
    ///   distinguishes a live sibling Sirio (`AlreadyRunning`) from a
    ///   hostile squatter (`BindFailed`) — there is no stale case, since a
    ///   pipe object dies with its owning process.
    ///
    /// The wire protocol and the framing code are shared verbatim with the
    /// unix arm; only this setup differs.
    #[cfg(windows)]
    pub fn start(&self) -> Result<(), ServerError> {
        let listener = match PipeListener::bind(&self.socket_path) {
            Ok(listener) => listener,
            Err(ListenerError::AlreadyRunning { name }) => {
                return Err(ServerError::AlreadyRunning {
                    path: PathBuf::from(name),
                });
            }
            Err(ListenerError::Bind { detail }) => {
                return Err(ServerError::BindFailed {
                    path: self.socket_path.clone(),
                    detail,
                });
            }
        };
        if let Err(detail) = post_bind_sanity_check(listener.instance_handle()) {
            return Err(ServerError::InsecureSocket { detail });
        }

        let handler = Arc::clone(&self.handler);
        let shutdown = Arc::clone(&self.shutdown);
        let handle = std::thread::spawn(move || {
            accept_loop_windows(listener, handler, shutdown);
        });
        *self.accept_thread.lock().expect("accept thread mutex") = Some(handle);
        self.started.store(true, Ordering::SeqCst);
        Ok(())
    }

    /// XDG runtime directories are normally provisioned by the login/session
    /// manager, but the state-directory fallback and explicit test/override
    /// paths may not exist yet. Create only the socket's leaf parent and keep
    /// it private before binding anything inside it.
    #[cfg(unix)]
    fn prepare_parent_directory(&self) -> Result<(), ServerError> {
        let parent = self
            .socket_path
            .parent()
            .ok_or_else(|| ServerError::BindFailed {
                path: self.socket_path.clone(),
                detail: "socket path has no parent directory".to_string(),
            })?;
        let parent_was_present = parent.is_dir();
        fs::create_dir_all(parent).map_err(|error| ServerError::BindFailed {
            path: self.socket_path.clone(),
            detail: format!("could not create socket directory: {error}"),
        })?;
        // Never chmod an existing directory supplied by TILLER_SOCKET (it
        // may be a shared directory such as /tmp). New leaf directories are
        // ours, so make those private before binding the socket.
        if !parent_was_present {
            fs::set_permissions(parent, fs::Permissions::from_mode(0o700)).map_err(|error| {
                ServerError::BindFailed {
                    path: self.socket_path.clone(),
                    detail: format!("could not secure socket directory: {error}"),
                }
            })?;
        }
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

    /// Removes a stale socket file, or refuses if a live server owns it.
    #[cfg(unix)]
    fn prepare_path(&self) -> Result<(), ServerError> {
        let path = &self.socket_path;
        if path.as_os_str().as_encoded_bytes().len() > MAX_SOCKET_PATH_LENGTH {
            return Err(ServerError::PathTooLong { path: path.clone() });
        }

        // Probe: if a server is alive on this path, connecting succeeds and
        // the socket must not be stolen. If the connect fails, the path is
        // stale (crashed run) or not a socket — unlink and take it over.
        match UnixStream::connect(path) {
            Ok(_) => Err(ServerError::AlreadyRunning { path: path.clone() }),
            Err(_) => {
                let _ = fs::remove_file(path);
                Ok(())
            }
        }
    }

    /// Binds a unix socket whose final path is 0600 from the first instant
    /// it exists.
    ///
    /// The socket is created under a unique temporary name in the same
    /// directory, chmodded to 0600 while still private, then `rename`d over
    /// the final path — rename is atomic, so the final path is either absent
    /// or already 0600, never the umask-derived mode. The temporary name is
    /// unpredictable (counter + timestamp) and is itself chmodded before
    /// anyone could discover it, so its brief existence at the umask-derived
    /// mode is unreachable. Unlike narrowing the process umask, nothing here
    /// is process-global: concurrent binds from other threads cannot
    /// interfere with each other.
    #[cfg(unix)]
    fn bind_private(path: &Path) -> io::Result<UnixListener> {
        static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(0);
        let nanos_hex = format!("{nanos:x}");

        let parent = path.parent().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "socket path has no parent directory",
            )
        })?;
        // Leave room in `sun_path` for the temporary file name and NUL
        // terminator, after the parent directory and slash.
        let room = MAX_SOCKET_PATH_LENGTH
            .saturating_sub(1)
            .saturating_sub(parent.as_os_str().as_encoded_bytes().len())
            .saturating_sub(1);

        for _ in 0..4 {
            let counter = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            // Prefer the entropied name (counter + timestamp hex); a deep
            // parent with little room left falls back to the bare counter.
            let entropied = format!(".t{counter}-{nanos_hex}");
            let bare = format!(".t{counter}");
            let suffix = if entropied.len() <= room {
                entropied
            } else if bare.len() <= room {
                bare
            } else {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "socket path too long for a temporary bind name",
                ));
            };
            let tmp_path = parent.join(suffix);
            match UnixListener::bind(&tmp_path) {
                Ok(listener) => {
                    // Make the temporary socket private before it could be
                    // discovered; a failure here means the socket would be
                    // permissive, so fail the whole bind.
                    fs::set_permissions(&tmp_path, fs::Permissions::from_mode(0o600))?;
                    if let Err(error) = fs::rename(&tmp_path, path) {
                        let _ = fs::remove_file(&tmp_path);
                        return Err(error);
                    }
                    return Ok(listener);
                }
                Err(error) if error.kind() == io::ErrorKind::AddrInUse => {
                    // A stale temporary from a crashed run; clear it and try
                    // the next name.
                    let _ = fs::remove_file(&tmp_path);
                }
                Err(error) => return Err(error),
            }
        }
        Err(io::Error::new(
            io::ErrorKind::AddrInUse,
            "could not find a free temporary socket name",
        ))
    }

    /// Verifies the bound path is a socket owned by the current user,
    /// mirroring the Swift server's post-bind sanity check.
    #[cfg(unix)]
    fn post_bind_sanity_check(&self) -> Result<(), ServerError> {
        use std::os::unix::fs::{FileTypeExt, MetadataExt};
        let metadata =
            fs::metadata(&self.socket_path).map_err(|error| ServerError::InsecureSocket {
                detail: format!("stat failed: {error}"),
            })?;
        if !metadata.file_type().is_socket() {
            return Err(ServerError::InsecureSocket {
                detail: "not a socket".to_string(),
            });
        }
        if metadata.uid() != unsafe { libc::getuid() } {
            return Err(ServerError::InsecureSocket {
                detail: "wrong owner".to_string(),
            });
        }
        Ok(())
    }
}

impl Drop for ControlServer {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Accepts connections until shutdown. Each connection gets its own thread,
/// so an idle or malicious client can never block another. The listener is
/// non-blocking and polled at [`POLL_INTERVAL`], so shutdown is detected
/// without depending on any wake-up mechanism.
const POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(5);

#[cfg(unix)]
fn accept_loop(
    listener: UnixListener,
    handler: Arc<dyn ControlHandler>,
    shutdown: Arc<AtomicBool>,
) {
    let _ = listener.set_nonblocking(true);
    while !shutdown.load(Ordering::SeqCst) {
        match listener.accept() {
            Ok((stream, _)) => {
                if !peer_is_owner(&stream) {
                    // A connection from a different uid — however it was
                    // obtained (a raced pre-chmod connect, a world-accessible
                    // stale socket, a malicious local process) — is refused
                    // regardless of what the file mode says now. This is the
                    // check that catches the bind/chmod race even if it were
                    // still present.
                    drop(stream);
                    continue;
                }
                let handler = Arc::clone(&handler);
                std::thread::spawn(move || serve_connection(stream, handler));
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                std::thread::sleep(POLL_INTERVAL);
            }
            Err(_) => {
                // Transient accept failure; keep serving unless shutting
                // down.
                std::thread::sleep(POLL_INTERVAL);
            }
        }
    }
}

/// Whether the peer of an accepted connection is the same user as this
/// process, via `getpeereid` (LOCAL_PEERCRED on macOS). A control socket
/// that can drive the whole protocol is a local privilege boundary; the
/// file mode only gates the connect, so the identity of whoever did
/// connect is verified independently of it.
#[cfg(any(
    target_os = "macos",
    target_os = "ios",
    target_os = "freebsd",
    target_os = "dragonfly",
    target_os = "openbsd",
    target_os = "netbsd"
))]
fn peer_is_owner(stream: &UnixStream) -> bool {
    use std::os::unix::io::AsRawFd;

    let mut euid: libc::uid_t = 0;
    let mut egid: libc::gid_t = 0;
    let rc = unsafe { libc::getpeereid(stream.as_raw_fd(), &mut euid, &mut egid) };
    // A failed getpeereid is treated as not-owner: refuse closed, never
    // accept on error.
    rc == 0 && euid == unsafe { libc::getuid() }
}

/// Platforms without LOCAL_PEERCRED fall back to the file-mode boundary
/// alone. Sirio targets macOS, where the check above applies.
#[cfg(target_os = "linux")]
fn peer_is_owner(stream: &UnixStream) -> bool {
    use std::os::unix::io::AsRawFd;

    let mut credentials = libc::ucred {
        pid: 0,
        uid: 0,
        gid: 0,
    };
    let mut length = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
    let result = unsafe {
        libc::getsockopt(
            stream.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            (&mut credentials as *mut libc::ucred).cast(),
            &mut length,
        )
    };
    result == 0 && credentials.uid == unsafe { libc::getuid() }
}

/// Any other unix (this arm previously also matched Windows by accident,
/// where `UnixStream` does not exist at all — `unix` is now required
/// explicitly so this is truly the unix catch-all, not a non-Linux one).
#[cfg(all(
    unix,
    not(any(
        target_os = "linux",
        target_os = "macos",
        target_os = "ios",
        target_os = "freebsd",
        target_os = "dragonfly",
        target_os = "openbsd",
        target_os = "netbsd"
    ))
))]
fn peer_is_owner(_stream: &UnixStream) -> bool {
    true
}

/// The transport-neutral view of one connected client — the seam both
/// platforms sit behind. The framing loop below is written exactly once for
/// unix (`UnixStream`) and Windows (named-pipe stream); only this trait and
/// the accept loops know which transport is underneath.
trait ControlStream: Read + Write {
    /// Restores blocking reads where the platform hands out non-blocking
    /// sockets (macOS accepted sockets inherit the listener's O_NONBLOCK).
    /// A no-op everywhere it does not apply.
    #[allow(unused_variables)]
    fn restore_blocking(&mut self) {}

    /// Hard-abort the conversation: the peer sees EOF/error immediately.
    /// Used when a request line violates the protocol cap.
    fn abort(&mut self);
}

#[cfg(unix)]
impl ControlStream for UnixStream {
    fn restore_blocking(&mut self) {
        let _ = self.set_nonblocking(false);
    }

    fn abort(&mut self) {
        let _ = self.shutdown(Shutdown::Both);
    }
}

#[cfg(windows)]
impl ControlStream for crate::windows_pipe::PipeStream {
    // Overlapped handles are born blocking-by-default here; nothing to do.
    // `abort` maps to DisconnectNamedPipe inside PipeStream itself.
    fn abort(&mut self) {
        crate::windows_pipe::PipeStream::abort(self)
    }
}

/// Accepts connections on the named pipe until shutdown. Peer-identity
/// checks happen inside [`PipeListener::accept`] (the impersonation-based
/// equivalent of the unix `peer_is_owner` gate below), so every stream that
/// reaches here is already verified same-user.
#[cfg(windows)]
fn accept_loop_windows(
    mut listener: PipeListener,
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
fn serve_connection<S: ControlStream>(mut stream: S, handler: Arc<dyn ControlHandler>) {
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
