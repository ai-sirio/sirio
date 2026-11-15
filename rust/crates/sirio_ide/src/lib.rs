//! Claude Code's IDE protocol, served by Sirio: the endpoint (token, bind), the lockfile Claude
//! discovers it by, and the WebSocket MCP server. A leaf: no GPUI, and it never touches
//! user-global Claude config.

pub mod endpoint;
pub mod lockfile;
mod mcp;
mod pending;
mod server;
mod socket;
mod untrusted;
pub mod uri;

pub use endpoint::{PORT_RANGE, bind, new_token};
pub use lockfile::{LockBody, Lockfile};
pub use mcp::{Diagnostic, FileDiagnostics, Notification, Pos16, Severity};
pub use untrusted::{ReadOutcome, Refusal, read_regular_file};
pub use server::{
    Bridge, BridgeEvent, BridgeOptions, ConnId, DIAGNOSTICS_DEADLINE, DiagId, DiffAnswer, DiffId,
    Gone,
};
