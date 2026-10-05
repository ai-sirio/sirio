//! The app's side of the host: find it, start it detached, adopt it.
//! The app depends on this crate and never on `sirio_host` — that edge is
//! the boundary of spec §4.1.

pub mod detach;
