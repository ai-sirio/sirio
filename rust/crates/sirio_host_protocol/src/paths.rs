//! Where a host keeps its endpoint, lock, state and binaries (spec §4.2,
//! §5.1). The data-root rule duplicates `sirio`'s `xdg_data_home_for`
//! deliberately — leaf crates do not depend on the app — minus its temp-dir
//! fallback: a host on tmpfs loses its state with the next reboot.

use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostPaths {
    pub root: PathBuf,
}

fn absolute(env: &BTreeMap<String, String>, key: &str) -> Option<PathBuf> {
    env.get(key)
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
}

impl HostPaths {
    pub fn from_environment(env: &BTreeMap<String, String>) -> Option<Self> {
        if let Some(root) = absolute(env, "SIRIO_HOST_HOME") {
            return Some(Self { root });
        }
        let data_home = absolute(env, "XDG_DATA_HOME").or_else(|| {
            #[cfg(windows)]
            {
                absolute(env, "LOCALAPPDATA")
            }
            #[cfg(not(windows))]
            {
                absolute(env, "HOME").map(|home| home.join(".local/share"))
            }
        })?;
        Some(Self {
            root: data_home.join("Sirio").join("host"),
        })
    }

    pub fn endpoint(&self, major: u32) -> PathBuf {
        self.root.join(format!("host-v{major}.sock"))
    }

    pub fn lock(&self, major: u32) -> PathBuf {
        self.root.join(format!("host-v{major}.lock"))
    }

    pub fn state(&self, major: u32) -> PathBuf {
        self.root.join(format!("host-v{major}.json"))
    }

    pub fn log(&self, major: u32) -> PathBuf {
        self.root.join("log").join(format!("host-v{major}.log"))
    }

    pub fn bin(&self, version: &str) -> PathBuf {
        let name = if cfg!(windows) {
            "sirio-host.exe"
        } else {
            "sirio-host"
        };
        self.root.join("bin").join(version).join(name)
    }

    pub fn launchd_dir(&self) -> PathBuf {
        self.root.join("launchd")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    fn env(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }
    // `/srv/h` and `/x` are absolute on unix only; on Windows they are rooted
    // but not absolute, so the tests that rely on them are unix-only.
    #[cfg(not(windows))]
    #[test]
    fn an_absolute_sirio_host_home_wins() {
        let p = HostPaths::from_environment(&env(&[
            ("SIRIO_HOST_HOME", "/srv/h"),
            ("XDG_DATA_HOME", "/x"),
        ]))
        .unwrap();
        assert_eq!(p.root, PathBuf::from("/srv/h"));
    }
    #[cfg(not(windows))]
    #[test]
    fn a_relative_sirio_host_home_is_ignored() {
        let p = HostPaths::from_environment(&env(&[
            ("SIRIO_HOST_HOME", "rel"),
            ("XDG_DATA_HOME", "/x"),
        ]))
        .unwrap();
        assert_eq!(p.root, PathBuf::from("/x/Sirio/host"));
    }
    #[cfg(not(windows))]
    #[test]
    fn home_is_the_fallback_on_unix() {
        let p = HostPaths::from_environment(&env(&[("HOME", "/home/u")])).unwrap();
        assert_eq!(p.root, PathBuf::from("/home/u/.local/share/Sirio/host"));
    }
    #[test]
    fn no_usable_root_is_none_never_a_temp_dir() {
        assert_eq!(
            HostPaths::from_environment(&env(&[("HOME", "relative")])),
            None
        );
    }
    #[test]
    fn files_are_namespaced_by_major() {
        let p = HostPaths {
            root: PathBuf::from("/r"),
        };
        assert_eq!(p.endpoint(2), PathBuf::from("/r/host-v2.sock"));
        assert_eq!(p.lock(2), PathBuf::from("/r/host-v2.lock"));
        assert_eq!(p.state(2), PathBuf::from("/r/host-v2.json"));
        assert_eq!(p.log(2), PathBuf::from("/r/log/host-v2.log"));
        assert!(p.bin("0.32.0").starts_with("/r/bin/0.32.0"));
    }
}
