//! Where a `panel.*` command goes: the app when it answers, the host when no
//! app is listening, and nowhere else.
//!
//! SP1 §5 (`docs/superpowers/specs/2026-10-05-host-foundation-design.md`):
//! loss of contact is not evidence of death. A socket that accepts and does
//! not answer means the app is alive and busy, so the command waits for it;
//! it is never bypassed to the host.

use std::io;
use std::path::Path;
use std::time::Duration;

use crate::client::round_trip;
use crate::protocol::request::system_ping;

/// What a probe of the app's control socket found.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppSocket {
    /// The socket path does not exist: no app, or the control socket is off.
    Missing,
    /// The path exists but nothing accepts: a stale socket file.
    Refused,
    /// The app accepted and answered.
    Answered,
    /// The app accepted and did not answer in time, or answered garbage.
    Silent,
}

/// Where the command is answered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Route {
    App,
    Host,
    Wait,
}

/// Picks the route from the probe. A running app is always asked first.
pub fn decide(probe: AppSocket) -> Route {
    match probe {
        AppSocket::Answered => Route::App,
        AppSocket::Missing | AppSocket::Refused => Route::Host,
        AppSocket::Silent => Route::Wait,
    }
}

/// How long a probe waits for the app's answer before calling it silent.
pub const PROBE_TIMEOUT: Duration = Duration::from_secs(5);

/// Probes the app's control socket with `system.ping`. The connect error's
/// kind separates "no app" from "an app that is not answering": only a
/// refused or missing socket sends the command to the host.
pub fn probe(socket: &Path) -> AppSocket {
    match sirio_ipc::connect(socket) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => AppSocket::Missing,
        Err(error) if error.kind() == io::ErrorKind::ConnectionRefused => AppSocket::Refused,
        Err(_) => AppSocket::Silent,
        Ok(stream) => {
            drop(stream);
            match round_trip(socket, &system_ping(), PROBE_TIMEOUT) {
                Ok(response) if response.ok => AppSocket::Answered,
                _ => AppSocket::Silent,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_running_app_is_always_asked_first() {
        assert_eq!(decide(AppSocket::Answered), Route::App);
    }

    #[test]
    fn no_app_means_the_host() {
        assert_eq!(decide(AppSocket::Missing), Route::Host);
        assert_eq!(decide(AppSocket::Refused), Route::Host);
    }

    #[test]
    fn an_app_that_accepts_and_does_not_answer_is_waited_for_not_bypassed() {
        assert_eq!(decide(AppSocket::Silent), Route::Wait);
    }
}
