//! The control socket: how external tools drive a running Tiller, and how
//! the agent hooks report status back. Ported from the Swift
//! `TillerControl` package.
//!
//! The server listens on a unix socket — `$TILLER_SOCKET` when set, otherwise
//! `$XDG_RUNTIME_DIR/TillerRust/control.sock`, falling back to the XDG state
//! directory when no runtime directory exists (see [`protocol`]). The Swift
//! original defaulted to `~/Library/Application Support/Tiller/control.sock`;
//! that path is history, not current behaviour. It dispatches line-delimited
//! JSON requests
//! ([`protocol::ControlRequest`]) to a [`server::ControlHandler`] the app
//! implements — this crate carries the transport and the protocol and knows
//! nothing about GPUI.
//!
//! `tillerctl` is the CLI client that speaks the same protocol with the same
//! subcommand names as the Swift CLI: `ping`, `capabilities`, `identify`,
//! `list-workspaces`, `new-workspace`, `select-workspace`,
//! `current-workspace`, `close-workspace`, `notify`, `session-ref`,
//! `list-notifications`, `clear-notifications`.

pub mod client;
pub mod extract;
pub mod panel;
pub mod protocol;
pub mod server;

pub use client::{ClientError, round_trip};
pub use extract::{session_ref_from_json, session_ref_from_payload_arguments};
pub use panel::{
    PaneError, PaneExitStatus, PaneInfo, PaneRegistry, PaneStateSnapshot, base64_encode,
    terminal_key_bytes,
};
pub use protocol::{
    ControlRequest, ControlResponse, decode_request, decode_response, default_socket_path,
    encode_line,
};
pub use server::{ControlHandler, ControlServer, ServerError};
