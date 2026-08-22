//! The blocking control client used by `tillerctl` and by tests. Ported from
//! `TillerControl/ControlClient.swift`: a fresh connection per request, one
//! request line in, one response line out, with a receive timeout.

use std::io::{Read, Write};
#[cfg(unix)]
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::Duration;

use crate::protocol::{ControlRequest, ControlResponse, decode_response, encode_line};

/// A failure talking to the control socket.
#[derive(Debug)]
pub enum ClientError {
    /// The socket path exceeds `sun_path` capacity.
    PathTooLong { path: String },
    /// Connecting failed (socket missing, Tiller not running).
    Connect { detail: String },
    /// Reading or writing the socket failed.
    Io { detail: String },
    /// The server's reply was not a valid response line.
    BadResponse { detail: String },
    /// The server did not answer within the timeout.
    TimedOut { timeout: Duration },
}

impl std::fmt::Display for ClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ClientError::PathTooLong { path } => write!(f, "socket path too long: {path}"),
            ClientError::Connect { detail } => write!(f, "connect: {detail}"),
            ClientError::Io { detail } => write!(f, "io: {detail}"),
            ClientError::BadResponse { detail } => write!(f, "bad response: {detail}"),
            ClientError::TimedOut { timeout } => {
                write!(f, "no response within {timeout:?}")
            }
        }
    }
}

impl std::error::Error for ClientError {}

/// The default receive timeout, mirroring the Swift client's 3600 s default.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(3600);

/// Sends one request over a fresh connection and returns the response.
///
/// Blocking by design — `tillerctl` is a short-lived CLI. `timeout` bounds
/// the wait for the response line.
///
/// Unix-only, matching [`crate::server::ControlServer::start`]'s transport —
/// see its doc comment for why a named pipe is not a drop-in replacement.
#[cfg(unix)]
pub fn round_trip(
    socket_path: &Path,
    request: &ControlRequest,
    timeout: Duration,
) -> Result<ControlResponse, ClientError> {
    let path = socket_path.to_string_lossy();
    if path.len() > 104 {
        return Err(ClientError::PathTooLong {
            path: path.into_owned(),
        });
    }

    let mut stream = UnixStream::connect(socket_path).map_err(|error| ClientError::Connect {
        detail: error.to_string(),
    })?;
    stream
        .set_read_timeout(Some(timeout))
        .map_err(|error| ClientError::Io {
            detail: error.to_string(),
        })?;

    let line = encode_line(request).map_err(|error| ClientError::Io {
        detail: error.to_string(),
    })?;
    stream.write_all(&line).map_err(|error| ClientError::Io {
        detail: error.to_string(),
    })?;

    let mut buffer: Vec<u8> = Vec::new();
    let mut chunk = [0u8; 64 * 1024];
    loop {
        if let Some(newline) = buffer.iter().position(|byte| *byte == b'\n') {
            let line: Vec<u8> = buffer.drain(..=newline).collect();
            return decode_response(&line[..line.len() - 1]).map_err(|error| {
                ClientError::BadResponse {
                    detail: error.to_string(),
                }
            });
        }
        match stream.read(&mut chunk) {
            Ok(0) => {
                return Err(ClientError::BadResponse {
                    detail: "connection closed before a response line".to_string(),
                });
            }
            Ok(n) => buffer.extend_from_slice(&chunk[..n]),
            Err(error)
                if error.kind() == std::io::ErrorKind::WouldBlock
                    || error.kind() == std::io::ErrorKind::TimedOut =>
            {
                return Err(ClientError::TimedOut { timeout });
            }
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) => {
                return Err(ClientError::Io {
                    detail: error.to_string(),
                });
            }
        }
    }
}

/// A raw client-side control stream: the exact surface a real client (and
/// the integration tests, which hold idle connections open, half-write
/// requests and so on) needs. Connect it with [`connect_raw`].
#[cfg(unix)]
pub type RawStream = std::os::unix::net::UnixStream;
#[cfg(windows)]
pub type RawStream = crate::windows_pipe::PipeStream;

/// Connects a raw client stream to the control endpoint — the same call
/// [`round_trip`] makes internally.
#[cfg(unix)]
pub fn connect_raw(socket_path: &Path) -> std::io::Result<RawStream> {
    UnixStream::connect(socket_path)
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
pub fn connect_raw(socket_path: &Path) -> std::io::Result<RawStream> {
    use crate::windows_pipe::ClientConnectError;
    crate::windows_pipe::open_client(socket_path).map_err(|error| match error {
        ClientConnectError::Io(error) => error,
        refused @ ClientConnectError::ForeignOwner { .. } => {
            std::io::Error::new(std::io::ErrorKind::PermissionDenied, refused.to_string())
        }
    })
}

/// Named-pipe client transport. No path-length check here, unlike the unix
/// twin above: `sun_path`'s 104-byte cap has no Windows counterpart — the
/// derived pipe name is length-bounded by construction (see
/// [`crate::windows_pipe::pipe_name_for_path`]).
#[cfg(windows)]
pub fn round_trip(
    socket_path: &Path,
    request: &ControlRequest,
    timeout: Duration,
) -> Result<ControlResponse, ClientError> {
    let mut stream = crate::windows_pipe::open_client(socket_path).map_err(|error| {
        ClientError::Connect {
            detail: error.to_string(),
        }
    })?;
    stream
        .set_read_timeout(Some(timeout))
        .map_err(|error| ClientError::Io {
            detail: error.to_string(),
        })?;

    let line = encode_line(request).map_err(|error| ClientError::Io {
        detail: error.to_string(),
    })?;
    stream.write_all(&line).map_err(|error| ClientError::Io {
        detail: error.to_string(),
    })?;

    let mut buffer: Vec<u8> = Vec::new();
    let mut chunk = [0u8; 64 * 1024];
    loop {
        if let Some(newline) = buffer.iter().position(|byte| *byte == b'\n') {
            let line: Vec<u8> = buffer.drain(..=newline).collect();
            return decode_response(&line[..line.len() - 1]).map_err(|error| {
                ClientError::BadResponse {
                    detail: error.to_string(),
                }
            });
        }
        match stream.read(&mut chunk) {
            Ok(0) => {
                return Err(ClientError::BadResponse {
                    detail: "connection closed before a response line".to_string(),
                });
            }
            Ok(n) => buffer.extend_from_slice(&chunk[..n]),
            Err(error)
                if error.kind() == std::io::ErrorKind::WouldBlock
                    || error.kind() == std::io::ErrorKind::TimedOut =>
            {
                return Err(ClientError::TimedOut { timeout });
            }
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) => {
                return Err(ClientError::Io {
                    detail: error.to_string(),
                });
            }
        }
    }
}
