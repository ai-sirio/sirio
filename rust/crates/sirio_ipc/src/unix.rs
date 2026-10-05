//! The unix-domain-socket arm of the local transport: a private bind, the
//! post-bind check, and the per-connection owner check. Moved verbatim out
//! of `sirio_control`'s server; [`crate::LocalListener`] is the only caller.

use std::fs;
use std::io;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use crate::BindError;

/// Maximum encoded Unix-socket path length for the target platform: the
/// capacity of `sockaddr_un.sun_path` including the null terminator.
#[cfg(target_os = "linux")]
pub const MAX_SOCKET_PATH_LENGTH: usize = 108;
#[cfg(not(target_os = "linux"))]
pub const MAX_SOCKET_PATH_LENGTH: usize = 104;

/// How often the non-blocking listener is polled, so shutdown is detected
/// without depending on any wake-up mechanism.
const POLL_INTERVAL: Duration = Duration::from_millis(5);

/// Binds the socket at `path`: parent directory prepared, a stale file
/// replaced (a live one never stolen), bound private, re-chmodded and
/// checked. The listener is non-blocking, ready for [`accept`].
pub fn bind(path: &Path) -> Result<UnixListener, BindError> {
    prepare_parent_directory(path)?;
    prepare_path(path)?;

    // The socket must never exist at the umask-derived mode: between a
    // plain bind and a post-hoc chmod, the file sits at 0755 on a
    // default macOS umask — a window an attacker can race and connect
    // through (measured at 488/500 startups, and the captured connection
    // stays authorized after start() returns). The socket is therefore
    // bound under a private temporary name, chmodded, and atomically
    // renamed into place: the final path never exists in a permissive
    // state, because before the rename it does not exist at all and
    // after it it is already 0600.
    let listener = bind_private(path).map_err(|error| BindError::BindFailed {
        path: path.to_path_buf(),
        detail: error.to_string(),
    })?;

    // Belt and braces: re-assert 0600 on the final path and fail loudly
    // if it cannot be enforced.
    fs::set_permissions(path, fs::Permissions::from_mode(0o600)).map_err(|error| {
        BindError::ChmodFailed {
            path: path.to_path_buf(),
            detail: error.to_string(),
        }
    })?;

    post_bind_sanity_check(path)?;

    // Polled by `accept`; a failure here only means `accept` would block,
    // which the original accept loop tolerated the same way.
    let _ = listener.set_nonblocking(true);
    Ok(listener)
}

/// Waits for the next client whose peer is this process's own user and hands
/// it back; `None` once `shutdown` is set. A connection from a different
/// uid is refused and the wait continues.
pub fn accept(listener: &UnixListener, shutdown: &AtomicBool) -> Option<UnixStream> {
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
                return Some(stream);
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
    None
}

/// XDG runtime directories are normally provisioned by the login/session
/// manager, but the state-directory fallback and explicit test/override
/// paths may not exist yet. Create only the socket's leaf parent and keep
/// it private before binding anything inside it.
fn prepare_parent_directory(path: &Path) -> Result<(), BindError> {
    let parent = path.parent().ok_or_else(|| BindError::BindFailed {
        path: path.to_path_buf(),
        detail: "socket path has no parent directory".to_string(),
    })?;
    let parent_was_present = parent.is_dir();
    fs::create_dir_all(parent).map_err(|error| BindError::BindFailed {
        path: path.to_path_buf(),
        detail: format!("could not create socket directory: {error}"),
    })?;
    // Never chmod an existing directory supplied by SIRIO_SOCKET (it
    // may be a shared directory such as /tmp). New leaf directories are
    // ours, so make those private before binding the socket.
    if !parent_was_present {
        fs::set_permissions(parent, fs::Permissions::from_mode(0o700)).map_err(|error| {
            BindError::BindFailed {
                path: path.to_path_buf(),
                detail: format!("could not secure socket directory: {error}"),
            }
        })?;
    }
    Ok(())
}

/// Removes a stale socket file, or refuses if a live server owns it.
fn prepare_path(path: &Path) -> Result<(), BindError> {
    if path.as_os_str().as_encoded_bytes().len() > MAX_SOCKET_PATH_LENGTH {
        return Err(BindError::PathTooLong {
            path: path.to_path_buf(),
        });
    }

    // Probe: if a server is alive on this path, connecting succeeds and
    // the socket must not be stolen. If the connect fails, the path is
    // stale (crashed run) or not a socket — unlink and take it over.
    match UnixStream::connect(path) {
        Ok(_) => Err(BindError::AlreadyRunning {
            path: path.to_path_buf(),
        }),
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
fn post_bind_sanity_check(path: &Path) -> Result<(), BindError> {
    use std::os::unix::fs::{FileTypeExt, MetadataExt};
    let metadata = fs::metadata(path).map_err(|error| BindError::InsecureSocket {
        detail: format!("stat failed: {error}"),
    })?;
    if !metadata.file_type().is_socket() {
        return Err(BindError::InsecureSocket {
            detail: "not a socket".to_string(),
        });
    }
    if metadata.uid() != unsafe { libc::getuid() } {
        return Err(BindError::InsecureSocket {
            detail: "wrong owner".to_string(),
        });
    }
    Ok(())
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
