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

    /// The commit point of an install. Temp + rename, so a crash here
    /// leaves the previously recorded version fully intact.
    pub fn write(&self, agent: &InstalledAgent) -> anyhow::Result<()> {
        let path = self.manifest_path(&agent.id);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
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
        let temp = path.with_extension("json.tmp");
        std::fs::write(&temp, serde_json::to_vec_pretty(&wire)?)?;
        std::fs::rename(&temp, &path)?;
        Ok(())
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
    // Only the Linux default_root tests below need this. Gating the import
    // the same way those tests are gated keeps other platforms
    // warning-clean instead of importing a name nothing there can use.
    #[cfg(target_os = "linux")]
    use std::collections::BTreeMap;

    fn temp_root(name: &str) -> PathBuf {
        let root = std::env::temp_dir()
            .join(format!("tiller-store-{name}-{}", std::process::id()));
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
        let newer = InstalledAgent { version: "1.7.0".into(), ..agent() };
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

    // The default_root tests below assert POSIX spellings (`/data`,
    // `/home/…`), which `Path::is_absolute` only recognises on Unix. Gating
    // them like `tiller_control::protocol`'s socket-path test keeps
    // non-Linux hosts warning-clean instead of failing on path semantics
    // that do not apply there.
    #[cfg(target_os = "linux")]
    #[test]
    fn the_default_root_follows_xdg_data_home() {
        let env = BTreeMap::from([("XDG_DATA_HOME".to_string(), "/data".to_string())]);
        assert_eq!(InstallStore::default_root(&env), PathBuf::from("/data/tiller/agents"));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn the_default_root_falls_back_to_home_local_share() {
        let env = BTreeMap::from([("HOME".to_string(), "/home/enzo".to_string())]);
        assert_eq!(
            InstallStore::default_root(&env),
            PathBuf::from("/home/enzo/.local/share/tiller/agents")
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn a_relative_xdg_data_home_is_ignored() {
        // The XDG spec says a relative value is invalid and must be treated
        // as unset — the same filter `tiller_control` applies.
        let env = BTreeMap::from([
            ("XDG_DATA_HOME".to_string(), "relative/path".to_string()),
            ("HOME".to_string(), "/home/enzo".to_string()),
        ]);
        assert_eq!(
            InstallStore::default_root(&env),
            PathBuf::from("/home/enzo/.local/share/tiller/agents")
        );
    }
}
