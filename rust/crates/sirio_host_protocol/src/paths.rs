//! Where a host keeps its endpoint, lock, state and binaries (spec §4.2,
//! §5.1). An absolute `SIRIO_DB` gets a host beside it, and debug builds use
//! `host-debug` so they do not adopt the release host. The default data-root
//! rule duplicates `sirio`'s `xdg_data_home_for` deliberately — leaf crates do
//! not depend on the app — minus its temp-dir fallback: a host on tmpfs loses
//! its state with the next reboot.

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
    /// Spec §7.1.1 (SP2): `$SIRIO_HOST_HOME` if absolute; else beside an
    /// absolute `SIRIO_DB` — an isolated Sirio gets an isolated host; else the
    /// per-user default, which a debug build names `host-debug` so it never
    /// adopts the user's own host and its terminals.
    pub fn from_environment(env: &BTreeMap<String, String>) -> Option<Self> {
        if let Some(root) = absolute(env, "SIRIO_HOST_HOME") {
            return Some(Self { root });
        }
        if let Some(database) = absolute(env, "SIRIO_DB")
            && let Some(parent) = database.parent()
        {
            return Some(Self {
                root: parent.join("host"),
            });
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
        let leaf = if cfg!(debug_assertions) {
            "host-debug"
        } else {
            "host"
        };
        Some(Self {
            root: data_home.join("Sirio").join(leaf),
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
    /// The directory name the default root ends in for this build profile.
    fn default_leaf() -> &'static str {
        if cfg!(debug_assertions) {
            "host-debug"
        } else {
            "host"
        }
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
        assert_eq!(p.root, PathBuf::from("/x/Sirio").join(default_leaf()));
    }
    #[cfg(not(windows))]
    #[test]
    fn home_is_the_fallback_on_unix() {
        let p = HostPaths::from_environment(&env(&[("HOME", "/home/u")])).unwrap();
        assert_eq!(
            p.root,
            PathBuf::from("/home/u/.local/share/Sirio").join(default_leaf())
        );
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

    #[cfg(unix)]
    #[test]
    fn an_absolute_sirio_host_home_wins_over_everything() {
        let p = HostPaths::from_environment(&env(&[
            ("SIRIO_HOST_HOME", "/h"),
            ("SIRIO_DB", "/db/sirio.sqlite"),
            ("HOME", "/home/u"),
        ]))
        .unwrap();
        assert_eq!(p.root, PathBuf::from("/h"));
    }

    #[cfg(unix)]
    #[test]
    fn an_isolated_database_gets_its_own_host_beside_it() {
        let p = HostPaths::from_environment(&env(&[
            ("SIRIO_DB", "/tmp/iso/sirio.sqlite"),
            ("HOME", "/home/u"),
        ]))
        .unwrap();
        assert_eq!(p.root, PathBuf::from("/tmp/iso/host"));
    }

    #[cfg(unix)]
    #[test]
    fn a_relative_sirio_db_does_not_move_the_root() {
        let p = HostPaths::from_environment(&env(&[
            ("SIRIO_DB", "rel.sqlite"),
            ("HOME", "/home/u"),
        ]))
        .unwrap();
        assert_eq!(
            p.root,
            PathBuf::from("/home/u/.local/share/Sirio").join(default_leaf())
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_relative_sirio_host_home_falls_through_to_the_database_rule() {
        let p = HostPaths::from_environment(&env(&[
            ("SIRIO_HOST_HOME", "rel"),
            ("SIRIO_DB", "/tmp/iso/sirio.sqlite"),
        ]))
        .unwrap();
        assert_eq!(p.root, PathBuf::from("/tmp/iso/host"));
    }

    #[cfg(unix)]
    #[test]
    fn a_debug_build_never_shares_the_release_root() {
        let p = HostPaths::from_environment(&env(&[("HOME", "/home/u")])).unwrap();
        assert_eq!(p.root.file_name().unwrap(), default_leaf());
        if cfg!(debug_assertions) {
            assert_ne!(p.root, PathBuf::from("/home/u/.local/share/Sirio/host"));
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_database_at_the_filesystem_root_has_no_parent_to_sit_beside() {
        // "/sirio.sqlite" has parent "/", which is a directory: the host goes
        // to "/host" — refused as a data root only if not writable, which is
        // the start's business, not this rule's.
        let p = HostPaths::from_environment(&env(&[("SIRIO_DB", "/sirio.sqlite")])).unwrap();
        assert_eq!(p.root, PathBuf::from("/host"));
    }
}

/// Spec §7.1.1, amended (fix batch 1, finding 10): the host root follows the
/// database the app actually uses, not the `SIRIO_DB` variable. `database` is
/// the resolved database path; `stable` is the user-wide database. A stable
/// database keeps the default root; any other database (an explicit
/// `SIRIO_DB`, or a checkout-local database) gets a `host` directory beside
/// it. `SIRIO_HOST_HOME`, when absolute, always wins.
pub fn root_for_database(
    environment: &BTreeMap<String, String>,
    database: &std::path::Path,
    stable: &std::path::Path,
) -> Option<HostPaths> {
    if absolute(environment, "SIRIO_HOST_HOME").is_some() {
        return HostPaths::from_environment(environment);
    }
    if database == stable {
        // The default root must not follow `SIRIO_DB`, which is not the
        // database in use here (it is the stable one, or unset).
        let mut without_database = environment.clone();
        without_database.remove("SIRIO_DB");
        return HostPaths::from_environment(&without_database);
    }
    if database.is_absolute()
        && let Some(parent) = database.parent()
    {
        return Some(HostPaths {
            root: parent.join("host"),
        });
    }
    HostPaths::from_environment(environment)
}

#[cfg(test)]
mod database_root_tests {
    use super::*;
    use std::path::Path;

    fn env(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[cfg(not(windows))]
    #[test]
    fn a_stable_database_keeps_the_default_root() {
        let environment = env(&[("XDG_DATA_HOME", "/x")]);
        let stable = Path::new("/s/sirio.sqlite");
        let paths = root_for_database(&environment, stable, stable).unwrap();
        assert_eq!(paths.root, Path::new("/x/Sirio").join(default_leaf()));
    }

    #[cfg(not(windows))]
    #[test]
    fn a_checkout_database_puts_the_host_beside_it() {
        let environment = env(&[("XDG_DATA_HOME", "/x")]);
        let database = Path::new("/c/.sirio/state/sirio-abc.sqlite");
        let stable = Path::new("/s/sirio.sqlite");
        let paths = root_for_database(&environment, database, stable).unwrap();
        assert_eq!(paths.root, Path::new("/c/.sirio/state/host"));
    }

    #[cfg(not(windows))]
    #[test]
    fn sirio_host_home_wins_over_the_database() {
        let environment = env(&[("SIRIO_HOST_HOME", "/h"), ("XDG_DATA_HOME", "/x")]);
        let database = Path::new("/c/.sirio/state/sirio-abc.sqlite");
        let stable = Path::new("/s/sirio.sqlite");
        let paths = root_for_database(&environment, database, stable).unwrap();
        assert_eq!(paths.root, Path::new("/h"));
    }

    #[cfg(not(windows))]
    #[test]
    fn an_explicit_sirio_db_puts_the_host_beside_it() {
        let environment = env(&[("SIRIO_DB", "/d/run/sirio.sqlite"), ("XDG_DATA_HOME", "/x")]);
        let database = Path::new("/d/run/sirio.sqlite");
        let stable = Path::new("/s/sirio.sqlite");
        let paths = root_for_database(&environment, database, stable).unwrap();
        assert_eq!(paths.root, Path::new("/d/run/host"));
    }

    #[cfg(not(windows))]
    fn default_leaf() -> &'static str {
        if cfg!(debug_assertions) { "host-debug" } else { "host" }
    }
}
