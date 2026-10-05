use std::collections::BTreeMap;
use std::time::Duration;

use sirio_host::{HostConfig, run};
use sirio_host_protocol::{messages::HostMode, paths::HostPaths, version::effective_major};

fn main() {
    let mut mode = HostMode::OnDemand;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match (arg.as_str(), args.next().as_deref()) {
            ("--mode", Some("on-demand")) => mode = HostMode::OnDemand,
            ("--mode", Some("service")) => mode = HostMode::Service,
            _ => {
                eprintln!("usage: sirio-host [--mode on-demand|service]");
                std::process::exit(2);
            }
        }
    }
    let env: BTreeMap<String, String> = std::env::vars().collect();
    let Some(paths) = HostPaths::from_environment(&env) else {
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
    std::process::exit(run(HostConfig {
        paths,
        major: effective_major(debug("SIRIO_HOST_PROTOCOL_MAJOR")),
        mode,
        idle_grace,
        version: env!("CARGO_PKG_VERSION").to_string(),
    }));
}
