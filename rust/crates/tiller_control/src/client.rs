//! The blocking control client used by `tillerctl` and by tests. Ported from
//! `TillerControl/ControlClient.swift`: a fresh connection per request, one
//! request line in, one response line out, with a receive timeout.

#[cfg(unix)]
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
    /// This platform has no client transport implemented yet — see
    /// [`ServerError::Unsupported`](crate::server::ServerError::Unsupported)
    /// for the matching server-side gap and its rationale (named pipes).
    Unsupported { detail: String },
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
            ClientError::Unsupported { detail } => write!(f, "unsupported: {detail}"),
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

/// Named-pipe client not implemented (see the doc comment on the
/// `#[cfg(unix)]` twin above).
#[cfg(not(unix))]
pub fn round_trip(
    _socket_path: &Path,
    _request: &ControlRequest,
    _timeout: Duration,
) -> Result<ControlResponse, ClientError> {
    Err(ClientError::Unsupported {
        detail: "the control socket client is not implemented on this platform yet \
                 (Windows counterpart: a named pipe)"
            .to_string(),
    })
}
