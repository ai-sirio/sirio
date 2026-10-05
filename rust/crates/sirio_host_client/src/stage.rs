//! Spec §4.3: run the host from a copy under the data root, never from the
//! installed package (Inno's Restart Manager, the AppImage's FUSE mount).

use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use sirio_host_protocol::paths::HostPaths;
use sirio_host_protocol::state_file::HostStateFile;

pub fn host_binary_name() -> &'static str {
    if cfg!(windows) {
        "sirio-host.exe"
    } else {
        "sirio-host"
    }
}

/// The packaged host beside the running executable. In debug builds
/// `SIRIO_HOST_BIN` names one explicitly (the E2E probe lives in
/// `target/debug/examples/`, where no sibling exists).
pub fn locate_packaged_host(current_exe: &Path, env: &BTreeMap<String, String>) -> Option<PathBuf> {
    if cfg!(debug_assertions)
        && let Some(path) = env
            .get("SIRIO_HOST_BIN")
            .map(PathBuf::from)
            .filter(|p| p.is_file())
    {
        return Some(path);
    }
    current_exe
        .parent()
        .map(|dir| dir.join(host_binary_name()))
        .filter(|p| p.is_file())
}

fn digest(path: &Path) -> io::Result<[u8; 32]> {
    Ok(Sha256::digest(std::fs::read(path)?).into())
}

pub fn stage_binary(source: &Path, paths: &HostPaths, version: &str) -> io::Result<PathBuf> {
    let target = paths.bin(version);
    if target.is_file() && digest(&target)? == digest(source)? {
        return Ok(target);
    }
    let dir = target.parent().expect("bin path has a parent");
    std::fs::create_dir_all(dir)?;
    let tmp = dir.join(format!(".{}.{}", host_binary_name(), std::process::id()));
    let copied = std::fs::copy(source, &tmp).map(|_| ()).and_then(|()| {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755))?;
        }
        Ok(())
    });
    if let Err(error) = copied {
        let _ = std::fs::remove_file(&tmp);
        return Err(error);
    }
    match std::fs::rename(&tmp, &target) {
        Ok(()) => Ok(target),
        Err(error) => {
            let _ = std::fs::remove_file(&tmp);
            // Two clients starting at once on Windows race to rename onto an
            // exe the winner already runs (a sharing violation); identical
            // bytes mean the copy that is there is good.
            match (digest(&target), digest(source)) {
                (Ok(there), Ok(ours)) if there == ours => Ok(target),
                _ => Err(error),
            }
        }
    }
}

/// Removes every `bin/<version>/` that is neither `keep` nor named by a
/// state file of a host that is still running (spec §4.3). A copy in use on
/// Windows cannot be removed; the error is ignored and the next prune tries
/// again.
pub fn prune_binaries(paths: &HostPaths, keep: &[String]) {
    let mut live: Vec<String> = keep.to_vec();
    if let Ok(entries) = std::fs::read_dir(&paths.root) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if !(name.starts_with("host-v") && name.ends_with(".json")) {
                continue;
            }
            if let Some(state) = std::fs::read(entry.path())
                .ok()
                .and_then(|bytes| serde_json::from_slice::<HostStateFile>(&bytes).ok())
                && sirio_ipc::process::start_time(state.pid) == Some(state.start_time)
            {
                live.push(state.version);
            }
        }
    }
    if let Ok(entries) = std::fs::read_dir(paths.root.join("bin")) {
        for entry in entries.flatten() {
            if !live.contains(&entry.file_name().to_string_lossy().to_string()) {
                let _ = std::fs::remove_dir_all(entry.path());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("sirio-stage-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }
    #[test]
    fn a_sibling_binary_is_found() {
        let d = tmp("sibling");
        std::fs::write(d.join(host_binary_name()), b"x").unwrap();
        assert_eq!(
            locate_packaged_host(&d.join("sirio"), &BTreeMap::new()),
            Some(d.join(host_binary_name()))
        );
    }
    #[test]
    fn no_sibling_is_none() {
        let d = tmp("none");
        assert_eq!(
            locate_packaged_host(&d.join("sirio"), &BTreeMap::new()),
            None
        );
    }
    #[test]
    fn staging_copies_once_and_recopies_a_changed_source() {
        let d = tmp("stage");
        let src = d.join("src-host");
        std::fs::write(&src, b"one").unwrap();
        let paths = HostPaths {
            root: d.join("root"),
        };
        let staged = stage_binary(&src, &paths, "9.9.9").unwrap();
        assert_eq!(std::fs::read(&staged).unwrap(), b"one");
        let first_modified = std::fs::metadata(&staged).unwrap().modified().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        stage_binary(&src, &paths, "9.9.9").unwrap();
        assert_eq!(
            std::fs::metadata(&staged).unwrap().modified().unwrap(),
            first_modified,
            "identical copy is not rewritten"
        );
        std::fs::write(&src, b"two").unwrap();
        stage_binary(&src, &paths, "9.9.9").unwrap();
        assert_eq!(
            std::fs::read(&staged).unwrap(),
            b"two",
            "a different source of the same version is recopied"
        );
    }
    #[cfg(unix)]
    #[test]
    fn the_staged_copy_is_executable() {
        use std::os::unix::fs::PermissionsExt;
        let d = tmp("exec");
        let src = d.join("h");
        std::fs::write(&src, b"x").unwrap();
        let staged = stage_binary(&src, &HostPaths { root: d.join("r") }, "1.0.0").unwrap();
        assert_eq!(
            std::fs::metadata(staged).unwrap().permissions().mode() & 0o111,
            0o111
        );
    }
    #[test]
    fn pruning_keeps_the_current_and_live_versions_only() {
        let d = tmp("prune");
        let paths = HostPaths { root: d.clone() };
        for v in ["0.1.0", "0.2.0", "0.3.0"] {
            std::fs::create_dir_all(d.join("bin").join(v)).unwrap();
        }
        // 0.2.0 is named by the state file of a live process: this one.
        let me = std::process::id();
        let state = serde_json::json!({"pid": me, "start_time": sirio_ipc::process::start_time(me).unwrap(),
            "version": "0.2.0", "protocol": {"major":1,"minor":0}, "mode": "on_demand", "endpoint": "e", "generation": "g"});
        std::fs::write(d.join("host-v1.json"), state.to_string()).unwrap();
        // 0.1.0 is named by a dead one.
        let dead = serde_json::json!({"pid": me, "start_time": 1, "version": "0.1.0",
            "protocol": {"major":2,"minor":0}, "mode": "on_demand", "endpoint": "e", "generation": "g"});
        std::fs::write(d.join("host-v2.json"), dead.to_string()).unwrap();
        prune_binaries(&paths, &["0.3.0".to_string()]);
        assert!(!d.join("bin/0.1.0").exists());
        assert!(d.join("bin/0.2.0").exists());
        assert!(d.join("bin/0.3.0").exists());
    }
}
