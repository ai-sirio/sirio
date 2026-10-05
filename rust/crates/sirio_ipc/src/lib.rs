//! The local transport shared by every Sirio process that talks to another on
//! the same machine: a unix-domain socket on macOS and Linux, a named pipe on
//! Windows, behind one small surface.
//!
//! - [`LocalListener::bind`] creates the endpoint private to the current user
//!   and checks it after the fact; [`LocalListener::accept`] hands back only
//!   connections whose peer is that same user.
//! - [`connect`] is the client side; [`LocalStream`] is what both sides hold.
//! - [`LocalStreamExt`] is what a framing loop needs of a stream beyond
//!   `Read + Write`, written once for both transports.
//! - [`process`] names a process by pid *and* start time, so a reused pid is
//!   not mistaken for the process it replaced.
//!
//! The crate carries no protocol: `sirio_control` speaks line-delimited JSON
//! over it, and the detached host speaks its own framing. Extracted from
//! `sirio_control` so both share one implementation of the part that is a
//! local privilege boundary.

use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::Duration;

pub mod process;

#[cfg(unix)]
mod unix;
// The Windows named-pipe transport. Compiled only on Windows; on unix the
// crate never sees it, keeping the unix build free of it.
#[cfg(windows)]
pub mod windows_pipe;

/// The capacity of `sockaddr_un.sun_path` on this platform — 108 on Linux,
/// 104 elsewhere. A socket path longer than this cannot be bound. Named
/// pipes have no such limit (the pipe name is derived and length-bounded by
/// construction), but the constant exists there too so callers can budget a
/// path uniformly.
#[cfg(unix)]
pub const MAX_SOCKET_PATH_LENGTH: usize = unix::MAX_SOCKET_PATH_LENGTH;
#[cfg(windows)]
pub const MAX_SOCKET_PATH_LENGTH: usize = 104;

/// A failure while binding the local endpoint.
#[derive(Debug)]
pub enum BindError {
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

impl std::fmt::Display for BindError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BindError::PathTooLong { path } => write!(
                f,
                "socket path too long ({} bytes, max {MAX_SOCKET_PATH_LENGTH}): {}",
                path.as_os_str().as_encoded_bytes().len(),
                path.display()
            ),
            BindError::AlreadyRunning { path } => {
                write!(f, "a Sirio is already listening at {}", path.display())
            }
            BindError::BindFailed { path, detail } => {
                write!(f, "bind {}: {detail}", path.display())
            }
            BindError::ChmodFailed { path, detail } => {
                write!(f, "chmod 0o600 {}: {detail}", path.display())
            }
            BindError::InsecureSocket { detail } => write!(f, "insecure socket: {detail}"),
        }
    }
}

impl std::error::Error for BindError {}

/// One connected end of the local transport — a unix stream socket, or a
/// named-pipe instance on Windows. Byte-stream semantics: ordered, reliable,
/// EOF on peer disconnect.
#[cfg(unix)]
pub type LocalStream = std::os::unix::net::UnixStream;
#[cfg(windows)]
pub type LocalStream = windows_pipe::PipeStream;

/// What a framing loop needs of a connected stream beyond `Read + Write` —
/// the seam both platforms sit behind, so the loop is written exactly once.
pub trait LocalStreamExt: Read + Write {
    /// Restores blocking reads where the platform hands out non-blocking
    /// sockets (macOS accepted sockets inherit the listener's O_NONBLOCK).
    /// A no-op everywhere it does not apply.
    fn restore_blocking(&mut self) {}

    /// Hard-abort the conversation: the peer sees EOF/error immediately.
    /// Used when a peer violates the protocol's size cap.
    fn abort(&mut self);

    /// Bounds how long a single read may block; `None` waits forever.
    fn set_read_timeout(&self, timeout: Option<Duration>) -> io::Result<()>;
}

#[cfg(unix)]
impl LocalStreamExt for std::os::unix::net::UnixStream {
    fn restore_blocking(&mut self) {
        let _ = self.set_nonblocking(false);
    }

    fn abort(&mut self) {
        let _ = self.shutdown(std::net::Shutdown::Both);
    }

    fn set_read_timeout(&self, timeout: Option<Duration>) -> io::Result<()> {
        // The inherent method, which a path call prefers over this one.
        std::os::unix::net::UnixStream::set_read_timeout(self, timeout)
    }
}

#[cfg(windows)]
impl LocalStreamExt for windows_pipe::PipeStream {
    // Overlapped handles are born blocking-by-default here; nothing to do.
    // `abort` maps to DisconnectNamedPipe inside PipeStream itself.
    fn abort(&mut self) {
        windows_pipe::PipeStream::abort(self)
    }

    fn set_read_timeout(&self, timeout: Option<Duration>) -> io::Result<()> {
        // The inherent method, which a path call prefers over this one.
        windows_pipe::PipeStream::set_read_timeout(self, timeout)
    }
}

/// The listening end of the local transport.
///
/// Unix transport: a unix domain socket
/// (`std::os::unix::net::{UnixListener, UnixStream}`), defended in three
/// layers — private temporary bind + atomic rename (no permissive window),
/// the post-bind stat, and per-connection peer-credential checks. See
/// `windows_pipe` for how each layer maps onto the named-pipe transport on
/// Windows.
pub struct LocalListener {
    #[cfg(unix)]
    inner: std::os::unix::net::UnixListener,
    #[cfg(windows)]
    inner: windows_pipe::PipeListener,
}

impl LocalListener {
    /// Binds the endpoint at `path`, private to the current user, and checks
    /// it after the fact.
    #[cfg(unix)]
    pub fn bind(path: &Path) -> Result<Self, BindError> {
        Ok(Self {
            inner: unix::bind(path)?,
        })
    }

    /// Named-pipe transport: the Windows counterpart of the unix sequence,
    /// with every security layer mapped 1:1 (see [`windows_pipe`] for the
    /// full rationale):
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
    #[cfg(windows)]
    pub fn bind(path: &Path) -> Result<Self, BindError> {
        use windows_pipe::{ListenerError, PipeListener, post_bind_sanity_check};

        let listener = match PipeListener::bind(path) {
            Ok(listener) => listener,
            Err(ListenerError::AlreadyRunning { name }) => {
                return Err(BindError::AlreadyRunning {
                    path: PathBuf::from(name),
                });
            }
            Err(ListenerError::Bind { detail }) => {
                return Err(BindError::BindFailed {
                    path: path.to_path_buf(),
                    detail,
                });
            }
        };
        if let Err(detail) = post_bind_sanity_check(listener.instance_handle()) {
            return Err(BindError::InsecureSocket { detail });
        }
        Ok(Self { inner: listener })
    }

    /// Waits for the next client and hands it back; `None` once `shutdown` is
    /// set. Only a peer running as the current user is ever returned — a
    /// connection from anyone else is refused and the wait continues.
    ///
    /// The listener polls rather than blocks, so shutdown is noticed within a
    /// few milliseconds without any wake-up mechanism.
    ///
    /// On macOS and the BSDs an accepted socket inherits the listener's
    /// `O_NONBLOCK`, so the returned stream may be non-blocking; call
    /// [`LocalStreamExt::restore_blocking`] before reading from it.
    #[cfg(unix)]
    pub fn accept(&mut self, shutdown: &AtomicBool) -> Option<LocalStream> {
        unix::accept(&self.inner, shutdown)
    }

    /// Peer-identity checks happen inside `PipeListener::accept` (the
    /// impersonation-based equivalent of the unix `peer_is_owner` gate), so
    /// every stream returned is already verified same-user.
    #[cfg(windows)]
    pub fn accept(&mut self, shutdown: &AtomicBool) -> Option<LocalStream> {
        self.inner.accept(shutdown)
    }
}

/// Connects a client stream to the endpoint at `path`.
#[cfg(unix)]
pub fn connect(path: &Path) -> io::Result<LocalStream> {
    std::os::unix::net::UnixStream::connect(path)
}

/// The Windows twin keeps the unix signature by flattening the richer
/// connect error. `open_client` distinguishes a refused foreign-owned pipe
/// from ordinary connect noise, but this helper exists to mirror
/// `UnixStream::connect` for callers and tests, and unix has no such
/// distinction to mirror. A refusal is reported as `PermissionDenied` —
/// which is what it is, a decision about who owns the endpoint rather than
/// an I/O failure — and the message carries the owner so the reason is not
/// lost on the way through.
#[cfg(windows)]
pub fn connect(path: &Path) -> io::Result<LocalStream> {
    use windows_pipe::ClientConnectError;
    windows_pipe::open_client(path).map_err(|error| match error {
        ClientConnectError::Io(error) => error,
        refused @ ClientConnectError::ForeignOwner { .. } => {
            io::Error::new(io::ErrorKind::PermissionDenied, refused.to_string())
        }
    })
}

/// Returns the address users can act on for the local transport: the pipe
/// name on Windows, the path elsewhere.
pub fn display_endpoint(path: &Path) -> String {
    #[cfg(windows)]
    {
        display_endpoint_from(path, windows_pipe::pipe_name_for_path(path))
    }

    #[cfg(not(windows))]
    {
        path.display().to_string()
    }
}

#[cfg(windows)]
fn display_endpoint_from(path: &Path, endpoint: Result<String, String>) -> String {
    endpoint.unwrap_or_else(|_| path.display().to_string())
}
