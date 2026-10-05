//! Spec §5.2's three observations, read from the real files and processes,
//! handed to the pure verdict.

use std::fs::OpenOptions;
use std::io::ErrorKind;
use std::time::Duration;

use sirio_host_protocol::liveness::*;
use sirio_host_protocol::messages::HelloReply;
use sirio_host_protocol::paths::HostPaths;
use sirio_host_protocol::state_file::HostStateFile;

use crate::connection::Connection;

pub struct Observation {
    pub verdict: Verdict,
    pub state: Option<HostStateFile>,
    pub reply: Option<HelloReply>,
}

/// Asks the host itself (a handshake on its endpoint), then the lock and the
/// recorded process. A client probing the endpoint is, for an instant, one of
/// the host's clients; that is harmless against its 60 s idle grace.
pub fn observe(paths: &HostPaths, major: u32, client_majors: &[u32]) -> Observation {
    let state: Option<HostStateFile> = std::fs::read(paths.state(major))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok());
    let reply = Connection::open(&paths.endpoint(major), Duration::from_secs(2))
        .ok()
        .and_then(|mut c| c.hello(env!("CARGO_PKG_VERSION"), client_majors).ok());
    let handshake = if reply.is_some() {
        Handshake::Completed
    } else {
        Handshake::Failed
    };
    let lock = lock_state(paths, major);
    let process = match &state {
        None => ProcessObservation::NoRecord,
        Some(s) => observe_process(s.start_time, sirio_ipc::process::start_time(s.pid)),
    };
    Observation {
        verdict: verdict(handshake, lock, process),
        state,
        reply,
    }
}

fn lock_state(paths: &HostPaths, major: u32) -> LockState {
    let file = match OpenOptions::new()
        .read(true)
        .write(true)
        .open(paths.lock(major))
    {
        Ok(file) => file,
        // No lock file: no host ever took it.
        Err(e) if e.kind() == ErrorKind::NotFound => return LockState::Free,
        // A lock file that cannot be opened is not evidence of absence, and
        // only an `Absent` verdict permits a start.
        Err(_) => return LockState::Held,
    };
    match file.try_lock() {
        Ok(()) => {
            let _ = file.unlock();
            LockState::Free
        }
        Err(_) => LockState::Held,
    }
}
