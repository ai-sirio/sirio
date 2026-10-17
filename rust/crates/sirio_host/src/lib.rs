//! `sirio-host`: the process that owns what runs (spec §4). SP1 serves only
//! the protocol's own methods.

mod connection;
mod core;
mod lifecycle;
mod log;
mod router;
mod sessions;

use std::time::Duration;

use sirio_host_protocol::{messages::HostMode, paths::HostPaths};

pub struct HostConfig {
    pub paths: HostPaths,
    pub major: u32,
    pub mode: HostMode,
    pub idle_grace: Duration,
    pub start_delay: Duration,
    pub version: String,
    /// Advertise `term.env_exact` (SP2 §3). Only a debug build turns it off.
    pub env_exact: bool,
}

pub use lifecycle::run;
