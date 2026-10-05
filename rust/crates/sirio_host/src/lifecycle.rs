//! Spec §5: take the lock, bind, publish the state file, serve, idle out,
//! and leave in the order a client can trust.

use std::fs::{self, File, OpenOptions};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use sirio_host_protocol::messages::{HostMode, ProtocolVersion};
use sirio_host_protocol::state_file::HostStateFile;
use sirio_host_protocol::version::PROTOCOL_MINOR;
use sirio_ipc::{BindError, LocalListener};

use crate::HostConfig;
use crate::core::HostCore;
use crate::log::Log;
use crate::router::Router;

const LOCK_RETRY: Duration = Duration::from_millis(500);
/// How long a leaving host lets its connections write their last frame — the
/// answer to `host.shutdown` above all — before it exits under them.
const DRAIN: Duration = Duration::from_millis(250);

pub fn run(config: HostConfig) -> i32 {
    let root = &config.paths.root;
    if let Err(e) = create_private_dir(root) {
        eprintln!("sirio-host: cannot create data root: {e}");
        return 5;
    }
    let log = Arc::new(Log::new(config.paths.log(config.major)));
    // A client probing the lock holds it for an instant; retry briefly
    // before concluding another host won.
    let lock = match acquire_lock(&config.paths.lock(config.major)) {
        Some(lock) => lock,
        None => {
            log.line("start.lost_lock", "");
            return 3;
        }
    };
    let mut listener = match LocalListener::bind(&config.paths.endpoint(config.major)) {
        Ok(listener) => listener,
        Err(error) => {
            // The log names the kind of failure and never the path (spec
            // §5.8); stderr, which nobody reads once the host is detached,
            // keeps the whole message.
            let (kind, detail) = match &error {
                BindError::PathTooLong { .. } => (
                    "endpoint path too long",
                    "endpoint path too long".to_string(),
                ),
                BindError::AlreadyRunning { .. } => ("already_running", error.to_string()),
                BindError::BindFailed { .. } => ("bind_failed", error.to_string()),
                BindError::ChmodFailed { .. } => ("chmod_failed", error.to_string()),
                BindError::InsecureSocket { .. } => ("insecure_socket", error.to_string()),
            };
            log.line("start.bind_failed", kind);
            eprintln!("sirio-host: {detail}");
            return 4;
        }
    };
    // What this host bound, to tell its own endpoint from a successor's when
    // it leaves.
    #[cfg(unix)]
    let bound = file_identity(&config.paths.endpoint(config.major));
    let generation = random_token();
    let core = Arc::new(HostCore::new(
        config.version.clone(),
        config.major,
        config.mode.clone(),
        generation.clone(),
        random_token(),
    ));
    let state = HostStateFile {
        pid: std::process::id(),
        start_time: sirio_ipc::process::start_time(std::process::id()).unwrap_or(0),
        version: config.version.clone(),
        protocol: ProtocolVersion {
            major: config.major,
            minor: PROTOCOL_MINOR,
        },
        mode: config.mode.clone(),
        endpoint: sirio_ipc::display_endpoint(&config.paths.endpoint(config.major)),
        generation: generation.clone(),
    };
    let published = serde_json::to_vec_pretty(&state)
        .map_err(std::io::Error::other)
        .and_then(|bytes| write_atomic(&config.paths.state(config.major), &bytes));
    if let Err(e) = published {
        log.line("start.state_file_failed", &format!("{:?}", e.kind()));
        eprintln!("sirio-host: cannot write the state file: {e}");
        return 4;
    }
    log.line(
        "start.ready",
        &format!("major={} mode={:?}", config.major, config.mode),
    );
    ignore_sighup();

    let router = Arc::new(Router::new(Arc::clone(&core)));
    let accept_core = Arc::clone(&core);
    let accept_log = Arc::clone(&log);
    let accept = std::thread::spawn(move || {
        while let Some(stream) = listener.accept(&accept_core.shutdown) {
            let (c, r, l) = (
                Arc::clone(&accept_core),
                Arc::clone(&router),
                Arc::clone(&accept_log),
            );
            std::thread::spawn(move || crate::connection::serve(stream, c, r, l));
        }
    });

    // The decision is the core's (one critical section, measured from the last
    // client or session event), so a client that came and went between two
    // looks still restarts the window.
    while !core.shutdown.load(Ordering::SeqCst) {
        std::thread::sleep(Duration::from_millis(100));
        if config.mode == HostMode::OnDemand && core.try_begin_idle_exit(config.idle_grace) {
            log.line("stop.idle", "");
        }
    }
    // Leave in the order of spec §5.5: stop accepting, endpoint, state file,
    // lock. Only what is still this host's own goes: a root removed and
    // re-created under a running host holds a successor's files by now.
    let _ = accept.join();
    #[cfg(unix)]
    {
        let endpoint = config.paths.endpoint(config.major);
        if bound.is_some() && file_identity(&endpoint) == bound {
            let _ = fs::remove_file(&endpoint);
        }
    }
    let state_path = config.paths.state(config.major);
    let recorded = fs::read(&state_path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<HostStateFile>(&bytes).ok());
    if recorded.is_some_and(|recorded| recorded.generation == generation) {
        let _ = fs::remove_file(&state_path);
    }
    // Each connection closes itself once it sees the shutdown flag; give them
    // a moment so the answer to `host.shutdown` is on the wire before the
    // process, and with it every socket, goes.
    let deadline = Instant::now() + DRAIN;
    while core.clients() > 0 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    log.line("stop.done", "");
    drop(lock);
    0
}

/// The (device, inode) of the file at `path`, not following a symlink.
#[cfg(unix)]
fn file_identity(path: &Path) -> Option<(u64, u64)> {
    use std::os::unix::fs::MetadataExt;
    fs::symlink_metadata(path)
        .ok()
        .map(|meta| (meta.dev(), meta.ino()))
}

fn acquire_lock(path: &Path) -> Option<File> {
    let file = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(path)
        .ok()?;
    let deadline = Instant::now() + LOCK_RETRY;
    loop {
        match file.try_lock() {
            Ok(()) => return Some(file),
            Err(_) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(20)),
            Err(_) => return None,
        }
    }
}

fn create_private_dir(path: &Path) -> std::io::Result<()> {
    fs::create_dir_all(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let tmp = path.with_extension(format!("json.{}", std::process::id()));
    fs::write(&tmp, bytes)?;
    fs::rename(&tmp, path)
}

/// An identifier that only has to differ between host starts: two hashes of
/// the process id and the clock, each seeded by `RandomState`'s own
/// per-instance random keys, printed as 32 hex digits. It is not a secret and
/// not drawn straight from the OS's entropy source — nothing may treat it as
/// one — but needs no crate and behaves the same on every platform.
fn random_token() -> String {
    use std::hash::{BuildHasher, Hasher};
    let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
    hasher.write_u32(std::process::id());
    hasher.write_u128(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0),
    );
    let a = hasher.finish();
    let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
    hasher.write_u64(a);
    format!("{a:016x}{:016x}", hasher.finish())
}

#[cfg(unix)]
fn ignore_sighup() {
    // SAFETY: installing SIG_IGN for a signal is async-signal-safe and
    // touches no Rust state.
    unsafe {
        libc::signal(libc::SIGHUP, libc::SIG_IGN);
    }
}
#[cfg(not(unix))]
fn ignore_sighup() {}
