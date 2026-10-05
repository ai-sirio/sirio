//! The host protocol as types and pure functions — no process, no socket.
//! Spec: docs/superpowers/specs/2026-10-05-host-foundation-design.md §6.

pub mod frame;
pub mod liveness;
pub mod messages;
pub mod paths;
pub mod state_file;
pub mod version;
