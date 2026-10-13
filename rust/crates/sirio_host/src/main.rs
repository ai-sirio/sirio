use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use sirio_host::{HostConfig, run};
use sirio_host_protocol::{messages::HostMode, paths::HostPaths, version::effective_major};

fn main() {
    let mut mode = HostMode::OnDemand;
    // The app names its root (`--root`): the host never reads a pane's root,
    // and the app's own root is never in the environment a shell inherits.
    let mut root: Option<PathBuf> = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match (arg.as_str(), args.next().as_deref()) {
            ("--mode", Some("on-demand")) => mode = HostMode::OnDemand,
            ("--mode", Some("service")) => mode = HostMode::Service,
            ("--root", Some(path)) if Path::new(path).is_absolute() => {
                root = Some(PathBuf::from(path));
            }
            _ => {
                eprintln!("usage: sirio-host [--mode on-demand|service] [--root DIR]");
                std::process::exit(2);
            }
        }
    }
    let env: BTreeMap<String, String> = std::env::vars().collect();
    let resolved = match root {
        Some(root) => Some(HostPaths { root }),
        None => HostPaths::from_app_environment(&env),
    };
    let Some(paths) = resolved else {
        eprintln!("sirio-host: no data root (set SIRIO_HOST_HOME, XDG_DATA_HOME or HOME)");
        std::process::exit(5);
    };
    // Test knobs exist in debug builds only, so a release binary cannot be
    // talked into another protocol major or a different idle grace.
    let debug = |key: &str| {
        if cfg!(debug_assertions) {
            env.get(key).map(String::as_str)
        } else {
            None
        }
    };
    let idle_grace = debug("SIRIO_HOST_IDLE_GRACE_MS")
        .and_then(|v| v.parse().ok())
        .map(Duration::from_millis)
        .unwrap_or(Duration::from_secs(60));
    let start_delay = debug("SIRIO_HOST_START_DELAY_MS")
        .and_then(|v| v.parse().ok())
        .map(Duration::from_millis)
        .unwrap_or(Duration::ZERO);
    std::process::exit(run(HostConfig {
        paths,
        major: effective_major(debug("SIRIO_HOST_PROTOCOL_MAJOR")),
        mode,
        idle_grace,
        start_delay,
        version: env!("CARGO_PKG_VERSION").to_string(),
    }));
}
