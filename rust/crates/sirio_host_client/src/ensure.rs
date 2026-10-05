//! Spec §5.3 and §5.6: adopt the host of each major this client speaks,
//! start one of the current major when none exists, and never start a
//! second host or signal one on the strength of a silent endpoint.

use std::collections::BTreeMap;
use std::io;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use sirio_host_protocol::liveness::Verdict;
use sirio_host_protocol::messages::{HelloReply, HostInfo, Welcome, method};
use sirio_host_protocol::paths::HostPaths;
use sirio_host_protocol::version::majors_spoken;

use crate::connection::Connection;
use crate::detach::{DetachMethod, DetachedSpawn, spawn_detached};
use crate::observe::observe;
use crate::stage::{host_binary_name, locate_packaged_host, prune_binaries, stage_binary};

const UNVERIFIABLE_WAIT: Duration = Duration::from_secs(10);
const START_WAIT: Duration = Duration::from_secs(10);
const CALL_TIMEOUT: Duration = Duration::from_secs(5);

pub struct EnsureOptions {
    pub paths: HostPaths,
    pub client_version: String,
    pub current_exe: PathBuf,
    pub environment: BTreeMap<String, String>,
    pub major: u32,
}

pub struct HostSession {
    pub connection: Connection,
    pub welcome: Welcome,
    pub major: u32,
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
    Unverifiable {
        waited: Duration,
    },
    Refused {
        host_major: u32,
        client_majors: Vec<u32>,
    },
    StartTimeout,
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
            Self::Unverifiable { waited } => write!(
                f,
                "a host holds the lock but does not answer (waited {}s)",
                waited.as_secs()
            ),
            Self::Refused {
                host_major,
                client_majors,
            } => write!(
                f,
                "host speaks protocol v{host_major}, this app speaks {client_majors:?}"
            ),
            Self::StartTimeout => write!(f, "sirio-host started but never answered"),
            Self::Io(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for EnsureError {}

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
                return Ok((connect(options, major, majors)?, None));
            }
            Verdict::Unverifiable => {
                if started.elapsed() >= UNVERIFIABLE_WAIT {
                    return Err(EnsureError::Unverifiable {
                        waited: started.elapsed(),
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

fn start(options: &EnsureOptions, major: u32) -> Result<DetachMethod, EnsureError> {
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
    std::fs::create_dir_all(&options.paths.root).map_err(EnsureError::Io)?;
    // A stale endpoint or state file of a host that is gone is not cleared
    // here: the host replaces a stale socket when it binds and rewrites its
    // state file when it is ready, and a client that removed them would, when
    // another client has just started a host, delete the files of that live one.
    let program = stage_binary(&source, &options.paths, &options.client_version)
        .map_err(EnsureError::Stage)?;
    spawn_detached(&DetachedSpawn {
        program,
        args: vec!["--mode".into(), "on-demand".into()],
        cwd: options.paths.root.clone(),
        label: format!("sirio-host-v{major}"),
        launchd_dir: options.paths.launchd_dir(),
    })
    .map_err(EnsureError::Spawn)
}

fn connect(
    options: &EnsureOptions,
    major: u32,
    majors: &[u32],
) -> Result<HostSession, EnsureError> {
    let mut connection =
        Connection::open(&options.paths.endpoint(major), CALL_TIMEOUT).map_err(EnsureError::Io)?;
    match connection.hello(&options.client_version, majors) {
        Ok(HelloReply::Welcome(welcome)) => Ok(HostSession {
            connection,
            welcome,
            major,
        }),
        Ok(HelloReply::Refused(r)) => Err(EnsureError::Refused {
            host_major: r.host_major,
            client_majors: majors.to_vec(),
        }),
        Err(e) => Err(EnsureError::Io(io::Error::other(e.to_string()))),
    }
}

impl HostHandle {
    /// The diagnostic row (spec §7): version, pid, mode, verdict.
    pub fn status_line(&mut self) -> String {
        let info = self
            .primary
            .connection
            .call(method::INFO, serde_json::json!({}), None)
            .ok()
            .and_then(|v| serde_json::from_value::<HostInfo>(v).ok());
        let mut line = match info {
            Some(i) => format!(
                "Live · v{} · pid {} · {:?} · protocol {}.{}",
                i.version, i.pid, i.mode, i.protocol.major, i.protocol.minor
            ),
            None => format!("Live · v{}", self.primary.welcome.host_version),
        };
        if let Some(prev) = &self.previous {
            line.push_str(&format!(" · draining v{} host", prev.major));
        }
        line
    }
}

pub fn status_line_for_error(error: &EnsureError) -> String {
    format!("Not available · {error}")
}
