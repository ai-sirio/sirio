//! The app's side of the host: find it, start it detached, adopt it.
//! The app depends on this crate and never on `sirio_host` — that edge is
//! the boundary of spec §4.1.

pub mod connection;
pub mod detach;
pub mod ensure;
pub mod observe;
pub mod stage;

pub use ensure::{
    EnsureError, EnsureOptions, HostHandle, HostSession, ensure_host, status_line_for_error,
};
