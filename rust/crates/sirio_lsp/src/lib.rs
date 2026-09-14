//! A Language Server Protocol client over a child process's stdio.
//!
//! This is `sirio_acp`'s shape with one difference. Both spawn a child and
//! speak JSON-RPC over its stdio; ACP delimits messages with newlines,
//! LSP prefixes each payload with a `Content-Length` header, because an
//! LSP payload may contain raw newlines of its own (a hover's markdown
//! documentation is exactly where they turn up).
//!
//! The crate does not depend on gpui and does not spawn anything itself:
//! [`connection::Connection::new`] hands back the read loop as a future for
//! the caller to place on gpui's executor. That keeps the whole protocol
//! layer drivable from a plain `block_on` in tests, the same reason
//! `sirio_activity` stays window-free.

mod connection;
mod framing;
mod lifecycle;
mod message;
mod position;

pub use connection::{Client, Connection};
pub use lsp_types;
pub use message::{Incoming, RequestId, ResponseError};
pub use position::LineIndex;

/// Everything that can go wrong below the UI.
///
/// `Transport` is terminal for the connection that raised it: a framing
/// error means the byte stream is desynchronised, and there is no way to
/// find the next header that does not amount to trusting a stream which
/// has already lied.
#[derive(Debug)]
pub enum LspError {
    /// The child could not be launched, or its stdio could not be taken.
    Launch(String),
    /// The stream ended, or carried something that is not a framed message.
    Transport(String),
    /// The server answered with a JSON-RPC error object.
    Server { code: i64, message: String },
    /// The server accepted the request and never answered it.
    Timeout { method: &'static str },
}

impl std::fmt::Display for LspError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Launch(detail) => write!(f, "could not launch the language server: {detail}"),
            Self::Transport(detail) => write!(f, "language server transport failed: {detail}"),
            Self::Server { code, message } => {
                write!(f, "language server returned error {code}: {message}")
            }
            Self::Timeout { method } => write!(f, "language server did not answer `{method}`"),
        }
    }
}

impl std::error::Error for LspError {}
