//! Spec §5.3 and §5.6: adopt the host of each major this client speaks,
//! start one of the current major when none exists, and never start a
//! second host or signal one on the strength of a silent endpoint.

use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use sirio_host_protocol::liveness::{LockState, Verdict};
use sirio_host_protocol::messages::{HelloReply, HostInfo, HostMode, Welcome, capability, method};
use sirio_host_protocol::paths::HostPaths;
use sirio_host_protocol::version::majors_spoken;

use crate::connection::Connection;
use crate::detach::{DetachMethod, DetachedSpawn, spawn_detached};
use crate::mux::Mux;
use crate::observe::observe;
use crate::stage::{host_binary_name, locate_packaged_host, prune_binaries, stage_binary};
use crate::term::TermClient;

const UNVERIFIABLE_WAIT: Duration = Duration::from_secs(10);
const START_WAIT: Duration = Duration::from_secs(10);
const CALL_TIMEOUT: Duration = Duration::from_secs(5);

pub struct EnsureOptions {
    pub paths: HostPaths,
    pub client_version: String,
    pub current_exe: PathBuf,
    pub environment: BTreeMap<String, String>,
    /// The environment a host this call starts runs in. The app passes the
    /// user environment (`sirio_env::for_child`); a caller that is not the app
    /// passes the process's own.
    pub host_environment: BTreeMap<String, String>,
    pub major: u32,
}

pub struct HostSession {
    pub connection: Connection,
    pub welcome: Welcome,
    pub major: u32,
    endpoint: PathBuf,
    client_version: String,
}

pub struct HostHandle {
    pub primary: HostSession,
    pub previous: Option<HostSession>,
    pub method: Option<DetachMethod>,
}

#[derive(Debug)]
pub enum EnsureError {
    NoDataRoot,
    BinaryMissing {
        looked: Vec<PathBuf>,
    },
    Stage(io::Error),
    Spawn(io::Error),
    /// The data root could not be created before anything was staged: no
    /// host can be started on it (fix batch 6, N6).
    DataRoot(io::Error),
    Unverifiable {
        waited: Duration,
        reason: Option<String>,
    },
    Refused {
        host_major: u32,
        client_majors: Vec<u32>,
    },
    StartTimeout,
    /// The endpoint path does not fit a unix socket address (spec §8): the
    /// host could never bind it, so none is started.
    EndpointPathTooLong {
        path: PathBuf,
        /// The longest path this platform's socket address holds.
        max: usize,
    },
    Io(io::Error),
}

impl std::fmt::Display for EnsureError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoDataRoot => write!(
                f,
                "no data directory (HOME / XDG_DATA_HOME / LOCALAPPDATA unset)"
            ),
            Self::BinaryMissing { looked } => write!(
                f,
                "sirio-host not found (looked in {})",
                looked
                    .iter()
                    .map(|p| p.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Self::Stage(e) => write!(f, "could not copy sirio-host: {e}"),
            Self::Spawn(e) => write!(f, "could not start sirio-host: {e}"),
            Self::DataRoot(e) => write!(f, "could not create the data directory: {e}"),
            Self::Unverifiable { waited, reason } => {
                write!(
                    f,
                    "a host holds the lock but does not answer (waited {}s)",
                    waited.as_secs()
                )?;
                if let Some(reason) = reason {
                    write!(f, " — {reason}")?;
                }
                Ok(())
            }
            Self::Refused {
                host_major,
                client_majors,
            } => write!(
                f,
                "host speaks protocol v{host_major}, this app speaks {client_majors:?}"
            ),
            Self::StartTimeout => write!(f, "sirio-host started but never answered"),
            Self::EndpointPathTooLong { path, max } => write!(
                f,
                "the host endpoint path is too long ({} bytes, at most {max}): {}",
                path.as_os_str().as_encoded_bytes().len(),
                path.display()
            ),
            Self::Io(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for EnsureError {}

impl EnsureError {
    /// Whether this failure leaves no host to lose, so a terminal may fall
    /// back to a local shell (spec §8). Every failure rooted in `Absent` says
    /// so: no host was found, and none could be started. So does `Refused`,
    /// a newer host that will not serve this client and holds nothing it
    /// could see. A host that answered `Live` and then failed to connect, or
    /// one that holds its lock without answering, may still be live beside
    /// the terminal, so the failure is one to retry (spec §8, finding 7).
    pub fn means_no_host(&self) -> bool {
        match self {
            Self::NoDataRoot
            | Self::BinaryMissing { .. }
            | Self::Stage(_)
            | Self::Spawn(_)
            | Self::DataRoot(_)
            | Self::Refused { .. }
            | Self::StartTimeout
            | Self::EndpointPathTooLong { .. } => true,
            Self::Unverifiable { .. } | Self::Io(_) => false,
        }
    }
}

pub fn ensure_host(options: &EnsureOptions) -> Result<HostHandle, EnsureError> {
    let majors = majors_spoken(options.major);
    // The previous major, if a host of it is live: kept for its sessions (§5.6).
    let previous = majors
        .get(1)
        .and_then(|&old| adopt_if_live(options, old, &majors).ok().flatten());
    let (primary, method) = ensure_major(options, options.major, &majors)?;
    prune_binaries(
        &options.paths,
        std::slice::from_ref(&options.client_version),
    );
    Ok(HostHandle {
        primary,
        previous,
        method,
    })
}

fn adopt_if_live(
    options: &EnsureOptions,
    major: u32,
    majors: &[u32],
) -> Result<Option<HostSession>, EnsureError> {
    let observation = observe(&options.paths, major, majors);
    if observation.verdict != Verdict::Live {
        return Ok(None);
    }
    connect(options, major, majors).map(Some)
}

fn ensure_major(
    options: &EnsureOptions,
    major: u32,
    majors: &[u32],
) -> Result<(HostSession, Option<DetachMethod>), EnsureError> {
    let started = Instant::now();
    loop {
        let observation = observe(&options.paths, major, majors);
        match observation.verdict {
            Verdict::Live => {
                if let Some(HelloReply::Refused(refused)) = observation.reply {
                    return Err(EnsureError::Refused {
                        host_major: refused.host_major,
                        client_majors: majors.to_vec(),
                    });
                }
                match connect(options, major, majors) {
                    Ok(mut session) => {
                        // An SP1 host (no `terminal` capability) with no
                        // session is replaced by the packaged host, so it is
                        // not held for the whole run (finding 3). When it
                        // cannot be replaced, the session is returned and the
                        // app drops it, so that host can idle out.
                        if major == options.major && !has_terminal(&session) && retire(&mut session, options, major, majors) {
                            continue;
                        }
                        return Ok((session, None));
                    }
                    // The host answered a moment ago and is gone or turning
                    // us away now (its idle exit, §5.5): the client that
                    // loses that race goes through §5.3 again, bounded by
                    // the same deadline as the wait for a silent host.
                    Err(error) if started.elapsed() >= UNVERIFIABLE_WAIT => return Err(error),
                    Err(_) => std::thread::sleep(Duration::from_millis(100)),
                }
            }
            Verdict::Unverifiable => {
                if started.elapsed() >= UNVERIFIABLE_WAIT {
                    return Err(EnsureError::Unverifiable {
                        waited: started.elapsed(),
                        reason: match observation.lock {
                            LockState::Unopenable(kind) => Some(format!(
                                "the lock file cannot be opened ({kind})"
                            )),
                            LockState::Held | LockState::Free => None,
                        },
                    });
                }
                std::thread::sleep(Duration::from_millis(250));
            }
            Verdict::Absent => {
                let method = start(options, major)?;
                // `Ok` from the spawn is not "serving" (on Linux a scope may
                // still fail after it): the endpoint never answering is the
                // real start failure.
                let deadline = Instant::now() + START_WAIT;
                while Instant::now() < deadline {
                    if let Ok(session) = connect(options, major, majors) {
                        return Ok((session, Some(method)));
                    }
                    std::thread::sleep(Duration::from_millis(100));
                }
                return Err(EnsureError::StartTimeout);
            }
        }
    }
}

/// How long a retired host may take to leave before it is treated as kept.
const RETIRE_WAIT: Duration = Duration::from_secs(10);

fn has_terminal(session: &HostSession) -> bool {
    session
        .welcome
        .capabilities
        .iter()
        .any(|name| name == capability::TERMINAL)
}

/// Asks a host that predates `terminal` to leave, but only when it holds no
/// session: a non-force `host.shutdown` is refused otherwise, and the answer
/// is the same as a host that refuses for any other reason (SP2 finding 3).
/// `true` once the host is `Absent`, within `RETIRE_WAIT`.
fn retire(session: &mut HostSession, options: &EnsureOptions, major: u32, majors: &[u32]) -> bool {
    let sessions = session
        .connection
        .call(method::INFO, serde_json::json!({}), None)
        .ok()
        .and_then(|info| serde_json::from_value::<HostInfo>(info).ok())
        .map(|info| info.sessions);
    if sessions != Some(0) {
        return false;
    }
    // Every request after the welcome carries the host's generation: a host
    // refuses a shutdown that names none (`stale_generation`).
    if session
        .connection
        .call(
            method::SHUTDOWN,
            serde_json::json!({ "force": false }),
            Some(&session.welcome.generation),
        )
        .is_err()
    {
        return false;
    }
    let deadline = Instant::now() + RETIRE_WAIT;
    while Instant::now() < deadline {
        if observe(&options.paths, major, majors).verdict == Verdict::Absent {
            return true;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    false
}

fn start(options: &EnsureOptions, major: u32) -> Result<DetachMethod, EnsureError> {
    // Before anything is staged or spawned: a host on this root would exit
    // at its bind, and the client would wait out START_WAIT for nothing.
    endpoint_fits(&options.paths.endpoint(major))?;
    let source =
        locate_packaged_host(&options.current_exe, &options.environment).ok_or_else(|| {
            EnsureError::BinaryMissing {
                looked: options
                    .current_exe
                    .parent()
                    .map(|d| vec![d.join(host_binary_name())])
                    .unwrap_or_default(),
            }
        })?;
    std::fs::create_dir_all(&options.paths.root).map_err(EnsureError::DataRoot)?;
    // A stale endpoint or state file of a host that is gone is not cleared
    // here: the host replaces a stale socket when it binds and rewrites its
    // state file when it is ready, and a client that removed them would, when
    // another client has just started a host, delete the files of that live one.
    let program = stage_binary(&source, &options.paths, &options.client_version)
        .map_err(EnsureError::Stage)?;
    spawn_detached(&DetachedSpawn {
        program,
        // The root is an argument, not the environment: the host's shells
        // inherit its environment, and the app's root must not reach them.
        args: vec![
            "--mode".into(),
            "on-demand".into(),
            "--root".into(),
            options.paths.root.as_os_str().to_owned(),
        ],
        cwd: options.paths.root.clone(),
        label: format!("sirio-host-v{major}"),
        launchd_dir: options.paths.launchd_dir(),
        env: Some(options.host_environment.clone()),
    })
    .map_err(EnsureError::Spawn)
}

/// Whether the platform can bind `endpoint`. A unix socket path must fit
/// `sun_path` together with its terminating NUL, so a path exactly as long
/// as `sirio_ipc::MAX_SOCKET_PATH_LENGTH` is already too long. A named pipe's
/// name is derived from the path and bounded by construction.
fn endpoint_fits(endpoint: &Path) -> Result<(), EnsureError> {
    #[cfg(unix)]
    if endpoint.as_os_str().as_encoded_bytes().len() >= sirio_ipc::MAX_SOCKET_PATH_LENGTH {
        return Err(EnsureError::EndpointPathTooLong {
            path: endpoint.to_path_buf(),
            max: sirio_ipc::MAX_SOCKET_PATH_LENGTH - 1,
        });
    }
    #[cfg(not(unix))]
    let _ = endpoint;
    Ok(())
}

fn connect(
    options: &EnsureOptions,
    major: u32,
    majors: &[u32],
) -> Result<HostSession, EnsureError> {
    let endpoint = options.paths.endpoint(major);
    let mut connection = Connection::open(&endpoint, CALL_TIMEOUT).map_err(EnsureError::Io)?;
    match connection.hello(&options.client_version, majors) {
        Ok(HelloReply::Welcome(welcome)) => Ok(HostSession {
            connection,
            welcome,
            major,
            endpoint,
            client_version: options.client_version.clone(),
        }),
        Ok(HelloReply::Refused(r)) => Err(EnsureError::Refused {
            host_major: r.host_major,
            client_majors: majors.to_vec(),
        }),
        Err(e) => Err(EnsureError::Io(io::Error::other(e.to_string()))),
    }
}

impl HostHandle {
    /// Opens the terminal capability on the primary host with a second
    /// connection, keeping the existing SP1 connection intact.
    pub fn term(&self) -> Option<TermClient> {
        if !self
            .primary
            .welcome
            .capabilities
            .iter()
            .any(|name| name == capability::TERMINAL)
        {
            return None;
        }
        let (mux, _) = Mux::open(
            &self.primary.endpoint,
            &self.primary.client_version,
            &majors_spoken(self.primary.major),
        )
        .ok()?;
        TermClient::new(Arc::new(mux))
    }

    /// A reconnect closure for `HostLink`: re-opens a `Mux` to the primary
    /// host's endpoint. `None` when the host has no `terminal` capability.
    pub fn term_reconnect(&self) -> Option<Arc<dyn Fn() -> Option<TermClient> + Send + Sync>> {
        if !self
            .primary
            .welcome
            .capabilities
            .iter()
            .any(|name| name == capability::TERMINAL)
        {
            return None;
        }
        let endpoint = self.primary.endpoint.clone();
        let client_version = self.primary.client_version.clone();
        let majors = majors_spoken(self.primary.major);
        Some(Arc::new(move || {
            let (mux, _) = Mux::open(&endpoint, &client_version, &majors).ok()?;
            TermClient::new(Arc::new(mux))
        }))
    }

    /// The diagnostic row (spec §7): version, pid, mode, verdict.
    pub fn status_line(&mut self) -> String {
        let info = self
            .primary
            .connection
            .call(method::INFO, serde_json::json!({}), None)
            .ok()
            .and_then(|v| serde_json::from_value::<HostInfo>(v).ok());
        compose_status(
            info.as_ref(),
            &self.primary.welcome.host_version,
            self.previous.as_ref().map(|prev| prev.major),
            self.method,
        )
    }
}

/// The row itself. A start that could not leave the app's job is named:
/// that host dies with the app, which is what the host exists to prevent.
fn compose_status(
    info: Option<&HostInfo>,
    welcomed_version: &str,
    previous_major: Option<u32>,
    method: Option<DetachMethod>,
) -> String {
    let mut line = match info {
        Some(i) => format!(
            "Live · v{} · pid {} · {} · protocol {}.{}",
            i.version,
            i.pid,
            mode_name(&i.mode),
            i.protocol.major,
            i.protocol.minor
        ),
        None => format!("Live · v{welcomed_version}"),
    };
    if let Some(major) = previous_major {
        line.push_str(&format!(" · draining v{major} host"));
    }
    if method == Some(DetachMethod::WindowsNoBreakaway) {
        line.push_str(" — will not survive the app (no job breakaway)");
    }
    line
}

/// The mode as `sirio-host --mode` and the spec spell it.
fn mode_name(mode: &HostMode) -> &'static str {
    match mode {
        HostMode::OnDemand => "on-demand",
        HostMode::Service => "service",
        HostMode::Other => "unknown mode",
    }
}

pub fn status_line_for_error(error: &EnsureError) -> String {
    format!("Not available · {error}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    fn path_of_len(len: usize) -> PathBuf {
        PathBuf::from(format!("/{}", "e".repeat(len - 1)))
    }

    #[cfg(unix)]
    #[test]
    fn an_endpoint_exactly_at_the_socket_capacity_is_refused() {
        // sun_path also holds the terminating NUL: a path as long as the
        // capacity does not fit, and the host could never bind it.
        let path = path_of_len(sirio_ipc::MAX_SOCKET_PATH_LENGTH);
        match endpoint_fits(&path) {
            Err(EnsureError::EndpointPathTooLong { path: named, .. }) => assert_eq!(named, path),
            other => panic!("expected EndpointPathTooLong, got {other:?}"),
        }
    }

    #[cfg(unix)]
    #[test]
    fn an_endpoint_one_byte_shorter_than_the_capacity_is_accepted() {
        let path = path_of_len(sirio_ipc::MAX_SOCKET_PATH_LENGTH - 1);
        assert!(endpoint_fits(&path).is_ok());
    }

    #[cfg(windows)]
    #[test]
    fn a_named_pipe_endpoint_is_never_refused_on_length() {
        let path = PathBuf::from(format!(
            r"C:\{}\host-v1.sock",
            "e".repeat(4 * sirio_ipc::MAX_SOCKET_PATH_LENGTH)
        ));
        assert!(endpoint_fits(&path).is_ok());
    }

    #[cfg(unix)]
    #[test]
    fn an_over_long_endpoint_is_refused_before_anything_is_staged_or_created() {
        // Neither the binary nor the root exists: a client that looked for
        // the binary first would say BinaryMissing, one that created the
        // root first would fail on it or leave it behind.
        let root = Path::new("/nonexistent-sirio-ensure-test").join("r".repeat(120));
        let options = EnsureOptions {
            paths: HostPaths { root: root.clone() },
            client_version: "0.0.0".into(),
            current_exe: "/nonexistent-sirio-ensure-test/sirio".into(),
            environment: BTreeMap::new(),
            host_environment: BTreeMap::new(),
            major: 1,
        };
        match ensure_host(&options) {
            Err(EnsureError::EndpointPathTooLong { path, max }) => {
                assert_eq!(path, options.paths.endpoint(1));
                let text = EnsureError::EndpointPathTooLong { path, max }.to_string();
                assert!(text.contains(&options.paths.endpoint(1).display().to_string()));
                assert!(text.contains(&max.to_string()));
            }
            Err(other) => panic!("expected EndpointPathTooLong, got {other}"),
            Ok(_) => panic!("an over-long endpoint was accepted"),
        }
        assert!(!root.exists());
    }

    fn info(mode: sirio_host_protocol::messages::HostMode) -> HostInfo {
        HostInfo {
            version: "0.31.0".into(),
            pid: 42,
            mode,
            sessions: 0,
            clients: 1,
            uptime_s: 3,
            generation: "g".into(),
            protocol: sirio_host_protocol::messages::ProtocolVersion { major: 1, minor: 0 },
        }
    }

    #[test]
    fn a_host_started_without_job_breakaway_is_said_not_to_survive_the_app() {
        use sirio_host_protocol::messages::HostMode;
        let degraded = compose_status(
            Some(&info(HostMode::OnDemand)),
            "0.31.0",
            None,
            Some(DetachMethod::WindowsNoBreakaway),
        );
        assert!(degraded.contains("will not survive the app"), "{degraded}");
        for method in [
            None,
            Some(DetachMethod::WindowsBreakaway),
            Some(DetachMethod::Setsid),
            Some(DetachMethod::SystemdScope),
            Some(DetachMethod::Launchd),
        ] {
            let line = compose_status(Some(&info(HostMode::OnDemand)), "0.31.0", None, method);
            assert!(!line.contains("will not survive"), "{method:?}: {line}");
        }
    }

    #[test]
    fn the_mode_is_named_as_the_host_is_started_with_it() {
        use sirio_host_protocol::messages::HostMode;
        let line = compose_status(Some(&info(HostMode::OnDemand)), "0.31.0", None, None);
        assert!(line.contains("on-demand"), "{line}");
        assert!(!line.contains("OnDemand"), "{line}");
    }

    #[test]
    fn unverifiable_error_includes_the_lock_open_reason() {
        let error = EnsureError::Unverifiable {
            waited: Duration::from_secs(10),
            reason: Some("the lock file cannot be opened (PermissionDenied)".into()),
        };
        assert_eq!(
            error.to_string(),
            concat!(
                "a host holds the lock but does not answer (waited 10s)",
                " — the lock file cannot be opened (PermissionDenied)"
            )
        );
    }
}

#[cfg(test)]
mod classification_tests {
    use super::*;

    /// Every variant, with what a terminal must make of it: `true` when the
    /// failure leaves no host to lose (a fallback), `false` when a host may be
    /// live beside the terminal (a failure to retry).
    fn every_variant() -> Vec<(EnsureError, bool)> {
        vec![
            (EnsureError::NoDataRoot, true),
            (EnsureError::BinaryMissing { looked: vec![] }, true),
            (EnsureError::Stage(io::Error::other("copy")), true),
            (EnsureError::Spawn(io::Error::other("spawn")), true),
            (EnsureError::DataRoot(io::Error::other("mkdir")), true),
            (
                EnsureError::Unverifiable {
                    waited: Duration::from_secs(10),
                    reason: None,
                },
                false,
            ),
            (
                EnsureError::Refused {
                    host_major: 2,
                    client_majors: vec![1],
                },
                true,
            ),
            (EnsureError::StartTimeout, true),
            (
                EnsureError::EndpointPathTooLong {
                    path: PathBuf::from("/r/host-v1.sock"),
                    max: 107,
                },
                true,
            ),
            // A connect that fails after a host answered: the host is live.
            (EnsureError::Io(io::Error::other("connect")), false),
        ]
    }

    #[test]
    fn every_failure_says_whether_it_leaves_no_host() {
        for (error, expected) in every_variant() {
            assert_eq!(error.means_no_host(), expected, "{error}");
        }
    }
}
