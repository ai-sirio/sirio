//! What Tiller has installed, where, and at which version.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::resolve::{InstalledAgent, Integrity};

pub struct InstallStore {
    root: PathBuf,
}

#[derive(Serialize, Deserialize)]
struct ManifestWire {
    id: String,
    version: String,
    executable: PathBuf,
    #[serde(default)]
    args: Vec<String>,
    /// "sha256" or "none". Recorded so an unverified install stays
    /// auditable after the fact: 47 of the registry's 95 binary artifacts
    /// publish no hash.
    integrity: String,
}

impl InstallStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// `$XDG_DATA_HOME/tiller/agents`, falling back to
    /// `$HOME/.local/share`.
    ///
    /// This deliberately duplicates the shape of
    /// `tiller_control::protocol::default_socket_path` (`protocol.rs:98-139`)
    /// rather than sharing it: that function resolves `$TILLER_SOCKET`
    /// first, then `$XDG_RUNTIME_DIR`, then the *state* directory, and
    /// carries a macOS branch — none of which applies here. The
    /// duplication is the absolute-path filter and nothing more; the other
    /// copy is named so a future change to XDG handling can find both.
    pub fn default_root(environment: &BTreeMap<String, String>) -> PathBuf {
        let data_home = absolute(environment, "XDG_DATA_HOME").unwrap_or_else(|| {
            absolute(environment, "HOME")
                .unwrap_or_else(|| PathBuf::from("/tmp"))
                .join(".local/share")
        });
        data_home.join("tiller").join("agents")
    }

    fn manifest_path(&self, id: &str) -> PathBuf {
        self.root.join(id).join("manifest.json")
    }

    /// A manifest that does not read or does not decode is absent, not an
    /// error: `resolve` turns that into an offer to reinstall.
    pub fn manifest(&self, id: &str) -> Option<InstalledAgent> {
        let body = std::fs::read_to_string(self.manifest_path(id)).ok()?;
        let wire: ManifestWire = serde_json::from_str(&body).ok()?;
        Some(InstalledAgent {
            id: wire.id,
            version: wire.version,
            executable: wire.executable,
            args: wire.args,
            integrity: match wire.integrity.as_str() {
                "sha256" => Integrity::Sha256,
                _ => Integrity::None,
            },
        })
    }

    /// The commit point of an install. Published atomically, so a crash
    /// here leaves the previously recorded version fully intact and two
    /// concurrent installs cannot take each other's scratch file away.
    pub fn write(&self, agent: &InstalledAgent) -> anyhow::Result<()> {
        let path = self.manifest_path(&agent.id);
        let wire = ManifestWire {
            id: agent.id.clone(),
            version: agent.version.clone(),
            executable: agent.executable.clone(),
            args: agent.args.clone(),
            integrity: match agent.integrity {
                Integrity::Sha256 => "sha256".to_string(),
                Integrity::None => "none".to_string(),
            },
        };
        crate::atomic::write_atomically(&path, &serde_json::to_vec_pretty(&wire)?)
    }

    pub fn remove(&self, id: &str) -> anyhow::Result<()> {
        let dir = self.root.join(id);
        if dir.exists() {
            std::fs::remove_dir_all(dir)?;
        }
        Ok(())
    }
}

fn absolute(environment: &BTreeMap<String, String>, key: &str) -> Option<PathBuf> {
    environment
        .get(key)
        .map(Path::new)
        .filter(|path| path.is_absolute())
        .map(Path::to_path_buf)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resolve::Integrity;
    use std::collections::BTreeMap;

    fn temp_root(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("tiller-store-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    fn agent() -> InstalledAgent {
        InstalledAgent {
            id: "codex-acp".into(),
            version: "1.6.2".into(),
            executable: PathBuf::from("/data/codex-acp/1.6.2/node_modules/.bin/codex-acp"),
            args: vec![],
            integrity: Integrity::None,
        }
    }

    #[test]
    fn an_unknown_agent_has_no_manifest() {
        let store = InstallStore::new(temp_root("unknown"));
        assert_eq!(store.manifest("codex-acp"), None);
    }

    #[test]
    fn a_written_manifest_reads_back_identically() {
        let store = InstallStore::new(temp_root("roundtrip"));
        store.write(&agent()).unwrap();
        assert_eq!(store.manifest("codex-acp"), Some(agent()));
    }

    #[test]
    fn writing_twice_replaces_rather_than_appends() {
        let store = InstallStore::new(temp_root("replace"));
        store.write(&agent()).unwrap();
        let newer = InstalledAgent {
            version: "1.7.0".into(),
            ..agent()
        };
        store.write(&newer).unwrap();
        assert_eq!(store.manifest("codex-acp").unwrap().version, "1.7.0");
    }

    #[test]
    fn an_unreadable_manifest_is_absent_rather_than_fatal() {
        let root = temp_root("corrupt");
        let store = InstallStore::new(root.clone());
        std::fs::create_dir_all(root.join("codex-acp")).unwrap();
        std::fs::write(root.join("codex-acp/manifest.json"), "{ truncated").unwrap();
        assert_eq!(store.manifest("codex-acp"), None);
    }

    #[test]
    fn removing_an_agent_clears_its_manifest() {
        let store = InstallStore::new(temp_root("remove"));
        store.write(&agent()).unwrap();
        store.remove("codex-acp").unwrap();
        assert_eq!(store.manifest("codex-acp"), None);
    }

    // The three default_root tests below build their inputs at runtime
    // (`std::env::temp_dir()` is absolute on every host) and compare
    // `PathBuf`s built with the same `.join()` chain the implementation
    // uses — only the *rules* are Tiller's; no POSIX literal is under
    // test. What is platform-defined (e.g. `/data` counting as absolute
    // on Unix) is deliberately not asserted here.
    #[test]
    fn the_default_root_follows_xdg_data_home() {
        let data_home = std::env::temp_dir();
        let env = BTreeMap::from([(
            "XDG_DATA_HOME".to_string(),
            data_home.to_string_lossy().into_owned(),
        )]);
        assert_eq!(
            InstallStore::default_root(&env),
            data_home.join("tiller").join("agents")
        );
    }

    #[test]
    fn the_default_root_falls_back_to_home_local_share() {
        let home = std::env::temp_dir();
        let env = BTreeMap::from([("HOME".to_string(), home.to_string_lossy().into_owned())]);
        assert_eq!(
            InstallStore::default_root(&env),
            home.join(".local/share").join("tiller").join("agents")
        );
    }

    #[test]
    fn a_relative_xdg_data_home_is_ignored() {
        // The XDG spec says a relative value is invalid and must be treated
        // as unset — the same filter `tiller_control` applies.
        // "relative/path" is relative on every host, so this holds
        // everywhere unchanged.
        let home = std::env::temp_dir();
        let env = BTreeMap::from([
            ("XDG_DATA_HOME".to_string(), "relative/path".to_string()),
            ("HOME".to_string(), home.to_string_lossy().into_owned()),
        ]);
        assert_eq!(
            InstallStore::default_root(&env),
            home.join(".local/share").join("tiller").join("agents")
        );
    }

    #[test]
    fn concurrent_writers_all_publish_their_manifest() {
        // A shared scratch name does not corrupt the manifest — it makes
        // the loser's rename fail with NotFound, because the winner
        // already moved the one temp file away. A good payload then
        // reports as a write failure.
        let root = temp_root("concurrent");
        let store = std::sync::Arc::new(InstallStore::new(root));

        let failures: Vec<String> = std::thread::scope(|scope| {
            let handles: Vec<_> = (0..4)
                .map(|_| {
                    let store = std::sync::Arc::clone(&store);
                    scope.spawn(move || {
                        let mut errors = Vec::new();
                        for _ in 0..25 {
                            if let Err(error) = store.write(&agent()) {
                                errors.push(error.to_string());
                            }
                        }
                        errors
                    })
                })
                .collect();
            handles
                .into_iter()
                .flat_map(|handle| handle.join().unwrap())
                .collect()
        });

        assert!(
            failures.is_empty(),
            "writers collided on a shared scratch file: {failures:?}"
        );
        assert_eq!(
            store.manifest("codex-acp").as_ref(),
            Some(&agent()),
            "the published manifest must be one writer's payload, whole"
        );
    }
}
